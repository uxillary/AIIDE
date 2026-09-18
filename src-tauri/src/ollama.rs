use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, State};
use crate::model_profiles;
use crate::model_provider::{InferenceRequest, InferenceResponse, ModelMessage, ModelProvider, OllamaProvider, ProviderErrorKind, SelectedProvider, OLLAMA_PROVIDER_ID};
use crate::project::OpenProject;
use crate::repository::{self, Activity, PendingChanges, PendingProposal, ProposedReplacement, ToolRequest};
use crate::repository::candidates::{CandidateRegistry, CandidateView};

const MAX_MESSAGES: usize = 40;
const MAX_MESSAGE_CHARS: usize = 12_000;
const PROTOCOL_TEMPERATURE: f32 = 0.0;
const CONVERSATIONAL_TEMPERATURE: f32 = 0.2;

#[derive(Default)]
struct DebugData { enabled: bool, latest: Option<String>, next_request: u64 }

#[derive(Default)]
pub struct AgentDebug(Mutex<DebugData>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus { enabled: bool, has_trace: bool }

#[derive(Default)]
struct TraceBuffer { lines: Vec<String>, next_turn: usize, echo: bool }
type Trace = Arc<Mutex<TraceBuffer>>;

fn debug_log(trace: Option<&Trace>, category: &str, message: impl AsRef<str>) {
    let Some(trace) = trace else { return };
    let line = format!("[AIIDE][{category}] {}", message.as_ref());
    if let Ok(mut buffer) = trace.lock() {
        if buffer.echo { eprintln!("{line}"); }
        buffer.lines.push(line);
    }
}

fn trace_turn(trace: Option<&Trace>) -> usize {
    let Some(trace) = trace else { return 0 };
    let Ok(mut buffer) = trace.lock() else { return 0 };
    buffer.next_turn += 1;
    buffer.next_turn
}

#[tauri::command]
pub fn set_agent_debug(enabled: bool, debug: State<'_, AgentDebug>) -> Result<DebugStatus, String> {
    let mut state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
    state.enabled = enabled;
    if !enabled { state.latest = None; }
    Ok(DebugStatus { enabled, has_trace: state.latest.is_some() })
}

#[tauri::command]
pub fn agent_debug_status(debug: State<'_, AgentDebug>) -> Result<DebugStatus, String> {
    let state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
    Ok(DebugStatus { enabled: state.enabled, has_trace: state.latest.is_some() })
}

#[tauri::command]
pub fn latest_agent_trace(debug: State<'_, AgentDebug>) -> Result<Option<String>, String> {
    let state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
    Ok(if state.enabled { state.latest.clone() } else { None })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    state: &'static str,
    models: Vec<ModelInfo>,
    error: Option<ProviderError>,
}

#[derive(Serialize)]
pub struct ModelInfo { id: String, name: String, profile: model_profiles::ModelProfile }

#[derive(Serialize)]
pub struct ProviderError { code: &'static str, message: &'static str }

pub type ChatMessage = ModelMessage;
type ChatPayloadResponse = InferenceResponse;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse { model: String, content: String, activity: Vec<Activity>, proposal: Option<PendingProposal> }

const CORE_AGENT_INSTRUCTIONS: &str = "You are Elma, a local-first AI coding companion inside AIIDE. You may inspect the opened project through AIIDE's bounded tools and propose focused replacements to existing text files. You cannot apply changes, write files, execute commands, commit, or push. Questions about project files, source code, directories, structure, or repository content require relevant repository evidence before answering. If the user gives only a filename, use list_files to discover its exact project-relative path, then read_file when its contents are needed. Never ask the user to provide project content that AIIDE's tools can inspect, and do not treat recognizing missing evidence as a final answer. Use the least expensive relevant tool and do not inspect unrelated files. answer is only for general conversation or a repository answer supported by sufficient evidence. Inspect every target file with read_file before proposing a change; do not return propose_change before that read succeeds. Preserve its style and avoid unrelated cleanup or whole-file rewrites. For a conversational response, return exactly {\"action\":\"answer\",\"answer\":\"<response>\"}; never put conversational answer text in summary. list_files.path is a project-relative directory, never a glob: use path=\"\" for the project root, then copy exact returned paths into read_file. After inspection, a proposal has this shape: {\"action\":\"propose_change\",\"summary\":\"<short edit summary>\",\"changes\":[{\"path\":\"<exact path>\",\"old_text\":\"<unique exact inspected text>\",\"new_text\":\"<replacement>\"}]}. old_text must be copied exactly from the inspected file, match exactly one location, and omit displayed line-number prefixes. If a fragment repeats, include enough exact unchanged surrounding context to make it unique. Never invent old_text. Keep it as small as practical while still unique; for example, if `target` repeats, use a unique exact span such as `unique-prefix target`. AIIDE validates and previews the proposal; only the user's Apply button can write it. Never claim a proposal was applied. Questions and reviews may be answered without proposing changes. Never invent files, code, Git state, tool results, or commands. search_files searches one literal substring. Failed tools do not prove absence. Return exactly one JSON object matching the provided schema.";

const DEFAULT_PERSONALITY: &str = "Elma is calm, clever, trustworthy, and down-to-earth, with a cute exterior and a dry sense of humour. Sound moderately casual and task-focused. Occasional mild sarcasm, playful comments, and natural emoji are welcome when they do not obscure technical facts or errors. Lightly mirror the user's casual language without forcing slang or caricature. Be concise by default: give the shortest complete answer, usually a few sentences for simple questions. Start with the answer; do not restate the question or add generic introductions, conclusions, or unnecessary headings. Assume normal software-development basics, explain important details briefly, and expand only when useful or requested.";

const MAX_REPAIRS: usize = 2;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AgentReply { action: String, path: Option<String>, query: Option<String>, answer: Option<String>, summary: Option<String>, changes: Option<Vec<ProposedReplacement>> }

#[derive(Debug, PartialEq)]
enum AgentAction { List(String), Search(String), Read(String), Answer(String), Propose(String, Vec<ProposedReplacement>) }

#[derive(Clone, Copy, Debug, PartialEq)]
enum RequestScope { General, Repository, Unknown }

#[derive(Clone, Copy, Debug, PartialEq)]
enum RequestIntent { Answer, Edit }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionReply { result: String, candidate_id: Option<String> }

#[derive(Debug)]
enum SelectionOutcome { Selected(CandidateView), Ambiguous, NoMatch }

fn requests_candidate_selection(prompt: &str, intent: RequestIntent) -> bool {
    if intent != RequestIntent::Answer { return false; }
    let lower = prompt.trim().to_ascii_lowercase();
    lower.starts_with("select ") || lower.starts_with("identify the target")
        || lower.starts_with("find the target") || (lower.starts_with("which ") && lower.contains(" should ") && (lower.contains(" edit") || lower.contains(" change")))
}

fn is_html_path(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm"))
}

fn selection_schema() -> Value {
    json!({"type":"object","properties":{
        "result":{"type":"string","enum":["selected","ambiguous","no_match"]},
        "candidate_id":{"type":"string","description":"Required only for selected; copy one listed ID exactly."}
    },"required":["result"],"additionalProperties":false})
}

fn resolve_selection(raw: &str, registry: &CandidateRegistry, root: &std::path::Path) -> Result<SelectionOutcome, String> {
    let reply: SelectionReply = serde_json::from_str(raw.trim()).map_err(|_| "Candidate selection response was malformed.")?;
    match (reply.result.as_str(), reply.candidate_id) {
        ("selected", Some(id)) if !id.is_empty() => {
            let candidate = registry.verify_current(root, &id)?;
            let view = registry.model_view().into_iter().find(|view| view.id == candidate.id()).ok_or("Unknown candidate ID.")?;
            Ok(SelectionOutcome::Selected(view))
        }
        ("ambiguous", None) => Ok(SelectionOutcome::Ambiguous),
        ("no_match", None) => Ok(SelectionOutcome::NoMatch),
        _ => Err("Candidate selection response was malformed.".into()),
    }
}

async fn select_candidate(provider: &impl ModelProvider, model: &str, prompt: &str, root: &std::path::Path, registry: &CandidateRegistry, trace: Option<&Trace>) -> Result<SelectionOutcome, String> {
    let views = registry.model_view();
    let candidates = serde_json::to_string(&views).map_err(|_| "Could not prepare candidate views.")?;
    let schema = selection_schema();
    let messages = [
        ChatMessage { role: "system".into(), content: "Select a verified source target for the user's request using only the listed candidate IDs. Compare each candidate's role, roleIndex (its position among candidates of that role), path, lines, excerpt, and bounded preceding context with the request. A document title, visible h1 heading, and paragraph are distinct targets. Return exactly one JSON object: {\"result\":\"selected\",\"candidate_id\":\"<listed ID>\"}, {\"result\":\"ambiguous\"}, or {\"result\":\"no_match\"}. If several plausible targets remain and the request does not distinguish them, return ambiguous. Do not invent an ID, source text, replacement code, or a proposal.".into() },
        ChatMessage { role: "user".into(), content: format!("Request: {prompt}\nVerified candidates: {candidates}") },
    ];
    // Keep candidate excerpts, source context, and the user's request out of the debug trace.
    debug_log(trace, "selection", format!("Selection call started\nCandidate count: {}\nSchema supplied: {schema}", views.len()));
    let started = Instant::now();
    let response = provider.infer(InferenceRequest { model, messages: &messages, format: schema, temperature: PROTOCOL_TEMPERATURE }).await.map_err(|error| {
        debug_log(trace, "selection", format!("Selection call failed after {}ms\nValidation: not run\nProvider error kind: {:?}", started.elapsed().as_millis(), error.kind));
        error.to_string()
    })?;
    debug_log(trace, "selection", format!("Selection response received in {}ms", started.elapsed().as_millis()));
    if response.done_reason.as_deref() == Some("length") {
        debug_log(trace, "selection", "Structured action: unavailable\nValidation: failed\nError: Candidate selection response was malformed.");
        return Err("Candidate selection response was malformed.".into());
    }
    let action = serde_json::from_str::<SelectionReply>(response.message.content.trim()).ok();
    let action_name = action.as_ref().map_or("malformed", |reply| match reply.result.as_str() {
        "selected" => "selected", "ambiguous" => "ambiguous", "no_match" => "no_match", _ => "invalid",
    });
    let id = if action_name == "selected" {
        action.as_ref().and_then(|reply| reply.candidate_id.as_deref())
            .filter(|id| registry.candidate(id).is_some()).unwrap_or("unverified")
    } else { "none" };
    debug_log(trace, "selection", format!("Structured action: {action_name}\nCandidate ID: {id}"));
    let outcome = resolve_selection(&response.message.content, registry, root).map_err(|error| {
        debug_log(trace, "selection", format!("Validation: failed\nError: {error}"));
        error
    })?;
    debug_log(trace, "selection", match &outcome {
        SelectionOutcome::Selected(view) => format!("Validation: passed\nSelected ID: {} at {}:{}", view.id, view.path, view.start_line),
        SelectionOutcome::Ambiguous => "Validation: passed\nAmbiguous candidates; clarification required".into(),
        SelectionOutcome::NoMatch => "Validation: passed\nNo matching candidate".into(),
    });
    Ok(outcome)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RequestPlan { scope: RequestScope, intent: RequestIntent }

#[derive(Clone, Debug, PartialEq)]
enum EvidenceKind { None, Listing, Inspection, Read }

#[derive(Clone, Debug, PartialEq)]
struct EvidenceRequirement { kind: EvidenceKind, filename: Option<String> }

impl EvidenceRequirement {
    fn none() -> Self { Self { kind: EvidenceKind::None, filename: None } }

    fn satisfied(&self, listings: usize, searches: usize, read_paths: &[String]) -> bool {
        match self.kind {
            EvidenceKind::None => true,
            EvidenceKind::Listing => listings > 0,
            EvidenceKind::Inspection => searches > 0 || !read_paths.is_empty(),
            EvidenceKind::Read => self.filename.as_ref().map_or(!read_paths.is_empty(), |filename| {
                read_paths.iter().any(|path| std::path::Path::new(path).file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(filename)))
            }),
        }
    }
}

fn is_edit_clause(clause: &str) -> bool {
    let mut clause = clause.trim();
    loop {
        let stripped = ["please ", "can you ", "could you ", "okay ", "ok ", "now "]
            .iter().find_map(|prefix| clause.strip_prefix(prefix));
        if let Some(value) = stripped { clause = value.trim_start(); } else { break; }
    }
    ["change", "edit", "modify", "fix", "implement", "add", "remove", "rename", "update", "replace", "refactor"]
        .iter().any(|verb| clause == *verb || clause.starts_with(&format!("{verb} ")))
        || ["make this change", "make only that change", "prepare this change for review", "prepare it for review"]
            .iter().any(|phrase| clause.starts_with(phrase))
}

fn classify_current_request(prompt: &str) -> RequestPlan {
    let text = prompt.trim().to_ascii_lowercase();
    let edit = text.split(['.', '!', '?', ';', '\n']).any(is_edit_clause);
    let selection = requests_candidate_selection(prompt, if edit { RequestIntent::Edit } else { RequestIntent::Answer });
    let repository = edit || selection || file_reference(prompt).is_some() || has_path_reference(prompt)
        || ["this project", "the project", "look at the css", "look at the code", "source code", "codebase", "project structure", "repository structure", "repository", "what files", "which files", "list files", "files in ", "files are in ", "directory", "folder", "page heading", "mobile menu", "main heading", "signup form"].iter().any(|phrase| text.contains(phrase));
    let general = matches!(text.as_str(), "hey" | "hello" | "hi" | "how are you" | "how are you?")
        || text.starts_with("what is ") || text.starts_with("what's ") || text.starts_with("explain ");
    RequestPlan { scope: if repository { RequestScope::Repository } else if general { RequestScope::General } else { RequestScope::Unknown }, intent: if edit { RequestIntent::Edit } else { RequestIntent::Answer } }
}

fn has_path_reference(prompt: &str) -> bool {
    prompt.split_whitespace().any(|word| {
        let token = word.trim_matches(|character: char| matches!(character, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '!' | '?'));
        !token.contains("://") && (token.starts_with("./") || token.contains('/') || token.contains('\\'))
    })
}

fn file_reference(prompt: &str) -> Option<String> {
    const EXTENSIONS: &[&str] = &["html", "htm", "css", "scss", "js", "jsx", "ts", "tsx", "rs", "py", "go", "java", "c", "h", "cpp", "json", "toml", "yaml", "yml", "md", "txt", "xml", "sql", "sh", "ps1"];
    const FILENAMES: &[&str] = &["dockerfile", "makefile", "readme", "license"];
    prompt.split_whitespace().filter_map(|word| {
        let token = word.trim_matches(|character: char| matches!(character, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '!' | '?'));
        let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
        let lower = name.to_ascii_lowercase();
        let extension = lower.rsplit_once('.').map(|(_, extension)| extension);
        ((!name.is_empty()) && (FILENAMES.contains(&lower.as_str()) || extension.is_some_and(|value| EXTENSIONS.contains(&value))))
            .then_some(name.to_owned())
    }).next()
}

fn evidence_requirement(prompt: &str, scope: RequestScope) -> EvidenceRequirement {
    if scope != RequestScope::Repository { return EvidenceRequirement::none(); }
    let text = prompt.trim().to_ascii_lowercase();
    if ["what files", "which files", "list files", "files are in ", "files in ", "directory contents", "folder contents"]
        .iter().any(|phrase| text.contains(phrase)) {
        EvidenceRequirement { kind: EvidenceKind::Listing, filename: None }
    } else if let Some(filename) = file_reference(prompt) {
        EvidenceRequirement { kind: EvidenceKind::Read, filename: Some(filename) }
    } else if ["find ", "locate ", "search ", "where is ", "where's "].iter().any(|phrase| text.starts_with(phrase)) {
        EvidenceRequirement { kind: EvidenceKind::Inspection, filename: None }
    } else {
        EvidenceRequirement { kind: EvidenceKind::Read, filename: None }
    }
}

fn agent_schema(project_open: bool, edit_intent: bool, has_read_evidence: bool, answer_allowed: bool) -> Value {
    let actions = if !project_open { json!(["answer"]) }
        else if edit_intent && has_read_evidence { json!(["propose_change"]) }
        else if edit_intent { json!(["list_files","search_files","read_file"]) }
        else if !answer_allowed { json!(["list_files","search_files","read_file"]) }
        else { json!(["list_files","search_files","read_file","answer"]) };
    let mut properties = serde_json::Map::new();
    properties.insert("action".into(), json!({"type":"string","enum":actions,"description":"Use the least expensive relevant repository action when project evidence is needed. Choose answer only for general conversation or after sufficient evidence."}));
    if project_open && !(edit_intent && has_read_evidence) {
        properties.insert("path".into(), json!({"type":"string","description":"For list_files, the project-relative directory to enumerate, such as '' or 'src', never a glob. Use it to discover exact paths. For read_file, an exact project-relative file path returned by list_files; use it when file contents are needed."}));
        properties.insert("query".into(), json!({"type":"string","description":"For search_files, one literal text substring to find in project files; use it to locate content, not filenames."}));
    }
    if !project_open || (!edit_intent && answer_allowed) {
        properties.insert("answer".into(), json!({"type":"string","description":"Required response text when action is answer. Do not ask for project content that repository tools can obtain."}));
    }
    if project_open && edit_intent && has_read_evidence {
        properties.insert("summary".into(), json!({"type":"string","maxLength":160,"description":"For propose_change only: one short sentence describing the edit."}));
        properties.insert("changes".into(), json!({"type":"array","minItems":1,"maxItems":4,"description":"Focused exact replacements for propose_change.","items":{"type":"object","properties":{"path":{"type":"string","description":"Exact project-relative path previously read."},"old_text":{"type":"string","description":"Exact text copied from the inspected file that matches exactly once. Include unchanged surrounding context when a smaller fragment repeats. Never include displayed line numbers."},"new_text":{"type":"string","description":"Replacement text for that unique old_text anchor."}},"required":["path","old_text","new_text"],"additionalProperties":false}}));
    }
    let required = if project_open && edit_intent && has_read_evidence {
        json!(["action", "summary", "changes"])
    } else {
        json!(["action"])
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn scope_schema() -> Value {
    json!({"type":"object","properties":{"scope":{"type":"string","enum":["general","repository"]}},"required":["scope"],"additionalProperties":false})
}

fn answer_schema() -> Value {
    json!({"type":"object","properties":{
        "action":{"type":"string","enum":["answer"]},"answer":{"type":"string"}
    },"required":["action","answer"],"additionalProperties":false})
}

fn parse_action(raw: &str) -> Result<AgentAction, &'static str> {
    let reply: AgentReply = serde_json::from_str(raw.trim()).map_err(|_| "invalid JSON object")?;
    match reply.action.as_str() {
        "list_files" if reply.answer.is_none() && reply.query.is_none() && reply.summary.is_none() && reply.changes.is_none() => Ok(AgentAction::List(reply.path.unwrap_or_default())),
        "list_files" => Err("list_files accepts path, not query or answer; use {\"action\":\"list_files\",\"path\":\"src\"}"),
        "search_files" if reply.answer.is_none() && reply.path.is_none() && reply.summary.is_none() && reply.changes.is_none() && reply.query.as_ref().is_some_and(|query| !query.trim().is_empty()) => Ok(AgentAction::Search(reply.query.unwrap())),
        "search_files" => Err("search_files requires a nonempty query and no path or answer"),
        "read_file" if reply.answer.is_none() && reply.query.is_none() && reply.summary.is_none() && reply.changes.is_none() && reply.path.as_ref().is_some_and(|path| !path.trim().is_empty()) => Ok(AgentAction::Read(reply.path.unwrap())),
        "read_file" => Err("read_file requires a nonempty path and no query or answer"),
        "answer" if reply.path.is_none() && reply.query.is_none() && reply.summary.is_none() && reply.changes.is_none() && reply.answer.as_ref().is_some_and(|answer| !answer.trim().is_empty()) => Ok(AgentAction::Answer(reply.answer.unwrap())),
        "answer" => Err("For action='answer', put the response text in the 'answer' field. Do not put it in 'summary', path, query, or changes."),
        "propose_change" if reply.path.is_none() && reply.query.is_none() && reply.answer.is_none() && reply.summary.as_ref().is_some_and(|value| !value.trim().is_empty() && value.chars().count() <= 160) && reply.changes.as_ref().is_some_and(|value| !value.is_empty()) => Ok(AgentAction::Propose(reply.summary.unwrap(), reply.changes.unwrap())),
        "propose_change" => Err("For action='propose_change', include changes; each change needs path, old_text, and new_text. Copy old_text exactly from inspected content and keep summary to one short sentence."),
        _ => Err("invalid or ambiguous action fields"),
    }
}

fn repair_instruction(reason: &str, edit_intent: bool, has_read_evidence: bool) -> String {
    if edit_intent && !has_read_evidence {
        format!("Your previous response was invalid ({reason}). Do not propose the edit yet. Inspect first by returning exactly one tool object: {{\"action\":\"list_files\",\"path\":\"\"}}, {{\"action\":\"search_files\",\"query\":\"literal text\"}}, or {{\"action\":\"read_file\",\"path\":\"exact/project/path\"}}. No summary, changes, prose, or markdown.")
    } else if edit_intent && has_read_evidence && (reason.contains("propose_change") || reason.contains("output limit")) {
        format!("Your previous response was invalid ({reason}). Return one minimal propose_change object. Include changes; each change needs path, old_text copied exactly from inspected content, and new_text. Keep summary to one short sentence. Do not repeat rationale. No prose or markdown.")
    } else { format!("Your previous response was invalid ({reason}). Return exactly one JSON object matching the schema. No prose or markdown.") }
}

fn response_shape(raw: &str) -> String {
    let trimmed = raw.trim();
    let fenced = trimmed.starts_with("```") && trimmed.ends_with("```");
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(object)) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let action = object.get("action").and_then(Value::as_str).unwrap_or("missing-or-non-string");
            format!("json-object(action={action}, keys={}, chars={})", keys.join(","), trimmed.chars().count())
        }
        Ok(value) => format!("json-{}(chars={})", match value {
            Value::Null => "null", Value::Bool(_) => "boolean", Value::Number(_) => "number",
            Value::String(_) => "string", Value::Array(_) => "array", Value::Object(_) => unreachable!(),
        }, trimmed.chars().count()),
        Err(_) => format!("non-json(fenced={fenced}, chars={})", trimmed.chars().count()),
    }
}

fn ambiguous_anchor_guidance() -> &'static str {
    "Proposal validation rejected old_text because it matches more than one location. Retry propose_change with old_text copied exactly from the inspected file and expanded with enough unchanged surrounding context to match exactly once. Do not invent text, include displayed line numbers, or guess which occurrence to replace. Keep the anchor as small as practical while still unique."
}

fn anchor_shape(edits: &[ProposedReplacement]) -> String {
    edits.iter().enumerate().map(|(index, edit)| {
        let has_line_prefix = edit.old_text.lines().any(|line| line.split_once(": ")
            .is_some_and(|(prefix, _)| prefix.parse::<usize>().is_ok()));
        format!("{}:chars={},lines={},displayedLinePrefix={has_line_prefix}", index + 1,
            edit.old_text.chars().count(), edit.old_text.lines().count())
    }).collect::<Vec<_>>().join("; ")
}

fn provider_error(provider: &impl ModelProvider, ollama: &'static str, remote: &'static str) -> String {
    if provider.metadata().id == OLLAMA_PROVIDER_ID { ollama } else { remote }.to_owned()
}

async fn chat_turn(provider: &impl ModelProvider, model: &str, messages: &[ChatMessage], format: Value, temperature: f32, stage: &str, trace: Option<&Trace>) -> Result<ChatPayloadResponse, String> {
    let turn = trace_turn(trace);
    debug_log(trace, "protocol", format!("Turn {turn} — {stage}\nTemperature: {temperature}\nStructured format supplied: yes\nMessage count: {}\nMessage roles: {}\nSchema/format: {}", messages.len(), messages.iter().map(|message| message.role.as_str()).collect::<Vec<_>>().join(", "), format));
    if trace.is_some() {
        for (index, message) in messages.iter().enumerate() {
            debug_log(trace, "agent", format!("Message {} ({})\n--- MESSAGE START ---\n{}\n--- MESSAGE END ---", index + 1, message.role, message.content));
        }
    }
    let started = Instant::now();
    let provider_id = provider.metadata().id;
    let response_label = provider_id.to_ascii_uppercase();
    debug_log(trace, provider_id, format!("Turn {turn} request started"));
    let result = provider.infer(InferenceRequest { model, messages, format, temperature }).await.map_err(|error| error.to_string())?;
    debug_log(trace, provider_id, format!("Turn {turn} response received in {}ms\n--- RAW {response_label} RESPONSE START ---\n{}\n--- RAW {response_label} RESPONSE END ---\nMetadata: model={}, done={}, done_reason={:?}", started.elapsed().as_millis(), result.message.content, result.model, result.done, result.done_reason));
    Ok(result)
}

async fn agent_turn(provider: &impl ModelProvider, model: &str, exchange: &mut Vec<ChatMessage>, project_open: bool, edit_intent: bool, has_read_evidence: bool, answer_allowed: bool, app: Option<&tauri::AppHandle>, trace: Option<&Trace>) -> Result<(ChatPayloadResponse, AgentAction), String> {
    let temperature = model_profiles::adaptation(model).protocol_temperature.unwrap_or(PROTOCOL_TEMPERATURE);
    let mut last_failure = None;
    for attempt in 0..=MAX_REPAIRS {
        let stage = if attempt == 0 { "STRUCTURED".to_owned() } else { format!("REPAIR {attempt}/{MAX_REPAIRS}") };
        let result = chat_turn(provider, model, exchange, agent_schema(project_open, edit_intent, has_read_evidence, answer_allowed), temperature, &stage, trace).await?;
        let parsed = if result.done_reason.as_deref() == Some("length") { Err("response exceeded the model output limit") }
            else { parse_action(&result.message.content) };
        match parsed {
            Ok(action) => { debug_log(trace, "protocol", format!("JSON parsing: PASSED\nSchema validation: PASSED\nSemantic validation: PASSED\nSelected action: {}", action_name(&action))); return Ok((result, action)); }
            Err(reason) => {
                last_failure = Some((reason, response_shape(&result.message.content)));
                let diagnostic = match serde_json::from_str::<Value>(&result.message.content) {
                    Err(error) => format!("JSON parsing: FAILED — {error}\nSchema validation: not run\nSemantic validation: not run"),
                    Ok(_) => match serde_json::from_str::<AgentReply>(&result.message.content) {
                        Err(error) => format!("JSON parsing: PASSED\nSchema validation: FAILED — {error}\nSemantic validation: not run"),
                        Ok(_) => format!("JSON parsing: PASSED\nSchema validation: PASSED\nSemantic validation: FAILED — {reason}"),
                    },
                };
                debug_log(trace, "protocol", diagnostic);
                if attempt == MAX_REPAIRS { break; }
                if let Some(app) = app { let _ = app.emit("repository-retry", ()); }
                let repair = repair_instruction(reason, edit_intent, has_read_evidence);
                debug_log(trace, "repair", format!("Attempt {}/{}\nOriginal failure: {reason}\n--- REPAIR INSTRUCTION START ---\n{repair}\n--- REPAIR INSTRUCTION END ---", attempt + 1, MAX_REPAIRS));
                exchange.push(ChatMessage { role: "user".into(), content: repair });
            }
        }
    }
    if provider.metadata().id == OLLAMA_PROVIDER_ID {
        let detail = last_failure.map(|(reason, shape)| format!(" Last failure: {reason}. Response shape: {shape}.")).unwrap_or_default();
        let state = if edit_intent && has_read_evidence { "post-read proposal" } else if edit_intent { "pre-read inspection" } else { "answer/tool" };
        Err(format!("The local model could not produce a valid structured response after two retries. Failed state: {state}.{detail}"))
    } else {
        Err("The selected model could not produce a valid structured response after two retries.".into())
    }
}

fn action_name(action: &AgentAction) -> &'static str {
    match action { AgentAction::List(_) => "list_files", AgentAction::Search(_) => "search_files", AgentAction::Read(_) => "read_file", AgentAction::Answer(_) => "answer", AgentAction::Propose(_, _) => "propose_change" }
}

fn tool_key(request: &ToolRequest) -> String { format!("{}|{}|{}", request.tool, request.path, request.query) }

fn useful_tool_result(output: &str) -> bool { !output.starts_with("Error:") && output != "No matches." }

fn execute_cached(root: &std::path::Path, request: &ToolRequest, cache: &mut HashMap<String, (String, Activity)>) -> (String, Activity, bool) {
    let key = tool_key(request);
    if let Some((output, activity)) = cache.get(&key) { return (output.clone(), activity.clone(), true); }
    let (output, activity) = repository::execute(root, request);
    if useful_tool_result(&output) { cache.insert(key, (output.clone(), activity.clone())); }
    (output, activity, false)
}

fn tool_result_message(request: &ToolRequest, useful: bool, output: &str, known_paths: &str, unread: &str) -> String {
    format!("Tool result for {} ({}):\n{}\n{}{}{}", request.tool, if useful { "success" } else { "no evidence" }, output,
        if useful && request.tool == "read_file" { "The target file is now inspected. If the original request asks you to implement, fix, add, remove, or otherwise change code, use propose_change with focused exact old_text/new_text now. Otherwise continue inspection or answer. Use exact listed paths." }
        else if useful { "Continue with another structured tool request or a grounded final answer. Use the actual result above." }
        else { "This result does not establish that files are absent. Correct the request using the actionable error above, or use list_files with path='' to discover exact project-relative paths." },
        if !known_paths.is_empty() { format!("\nExact paths from the project listing: {known_paths}") } else { String::new() },
        if !unread.is_empty() { format!("\nListed files not yet read: {unread}") } else { String::new() })
}

async fn requires_inspection(provider: &impl ModelProvider, model: &str, prompt: &str, trace: Option<&Trace>) -> Result<bool, String> {
    let mut messages = vec![
        ChatMessage { role: "system".into(), content: "Classify whether answering this user prompt requires inspecting the currently opened project. Use repository for questions about this project's behavior, structure, UI, bugs, or improvements, including indirect references like 'the menu'. Use general for conceptual questions such as 'What is a JavaScript closure?'. Return only a JSON object with scope general or repository.".into() },
        ChatMessage { role: "user".into(), content: prompt.into() },
    ];
    for attempt in 0..=MAX_REPAIRS {
        let result = chat_turn(provider, model, &messages, scope_schema(), PROTOCOL_TEMPERATURE, "SCOPE CLASSIFICATION", trace).await?;
        if let Some(needed) = parse_scope(&result.message.content) { return Ok(needed); }
        if attempt == MAX_REPAIRS { break; }
        messages.push(ChatMessage { role: "user".into(), content: "Return exactly one JSON object: {\"scope\":\"repository\"} or {\"scope\":\"general\"}.".into() });
    }
    Ok(true)
}

fn parse_scope(raw: &str) -> Option<bool> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope { scope: String }
    let scope: Scope = serde_json::from_str(raw.trim()).ok()?;
    match scope.scope.as_str() { "repository" => Some(true), "general" => Some(false), _ => None }
}

async fn final_turn(provider: &impl ModelProvider, model: &str, prompt: &str, project_info: &str, evidence: &[(String, String)], read_paths: &[String], trace: Option<&Trace>) -> Result<String, String> {
    let mut exchange = vec![
        ChatMessage { role: "system".into(), content: format!("You are Elma inside AIIDE. {project_info} Answer the user's original question using only the repository evidence below. Do not respond to a prior tool search or infer missing files from a failed tool. Do not invent filenames: HTML sections are not separate files. Do not list inspected filenames in your answer; the application appends the verified list. No more tools are available. Keep the answer under 120 words and address every part of the user's request. Avoid quoted code snippets or HTML attributes. Return only a JSON object with action answer and answer text.") },
        ChatMessage { role: "system".into(), content: DEFAULT_PERSONALITY.into() },
        ChatMessage { role: "user".into(), content: format!("Files actually read: {}\n\nRepository evidence:\n{}\n\nOriginal user request: {prompt}\n\nAnswer this original request directly, using the evidence above. If it asks for a review, give exactly three concrete improvements. For each, say what to change, which of the actual files would be affected, and why. Avoid speculative claims, generic advice, invented files, code snippets, and unverified accessibility findings. The app separately reports inspected files.", read_paths.join(", "), evidence.iter().map(|(name, value)| format!("[{name}]\n{value}")).collect::<Vec<_>>().join("\n\n")) },
    ];
    for attempt in 0..=MAX_REPAIRS {
        debug_log(trace, "final", "Grounded personality answer started; personality included: yes");
        let result = chat_turn(provider, model, &exchange, answer_schema(), CONVERSATIONAL_TEMPERATURE, "GROUNDED FINAL ANSWER", trace).await?;
        if result.done_reason.as_deref() != Some("length") {
            if let Ok(AgentAction::Answer(answer)) = parse_action(&result.message.content) { debug_log(trace, "final", "Grounded personality answer accepted"); return Ok(answer); }
        }
        if attempt == MAX_REPAIRS { break; }
        exchange.last_mut().unwrap().content.push_str("\n\nYour previous response was invalid or too long. Answer the ORIGINAL REQUEST above in at most 90 words. For a review, give three short numbered improvements with actual affected files and reasons. Return only the JSON answer object.");
    }
    Err(provider_error(provider,
        "The local model could not produce a final structured answer after two retries.",
        "The selected model could not produce a final structured answer after two retries."))
}

async fn conversational_turn(provider: &impl ModelProvider, model: &str, prompt: &str, draft: &str, trace: Option<&Trace>) -> Result<String, String> {
    let mut exchange = vec![
        ChatMessage { role: "system".into(), content: format!("You are Elma inside AIIDE. {DEFAULT_PERSONALITY} Preserve the draft's factual meaning. Return only a JSON object with action answer and answer text.") },
        ChatMessage { role: "user".into(), content: format!("Original question: {prompt}\n\nDraft answer: {draft}\n\nGive the shortest complete answer in Elma's natural voice. Do not add facts that are absent from the draft.") },
    ];
    for attempt in 0..=MAX_REPAIRS {
        debug_log(trace, "final", "Personality rewrite started; personality included: yes");
        let result = chat_turn(provider, model, &exchange, answer_schema(), CONVERSATIONAL_TEMPERATURE, "PERSONALITY REWRITE", trace).await?;
        if result.done_reason.as_deref() != Some("length") {
            if let Ok(AgentAction::Answer(answer)) = parse_action(&result.message.content) { debug_log(trace, "final", "Personality rewrite accepted"); return Ok(answer); }
        }
        if attempt == MAX_REPAIRS { break; }
        exchange.last_mut().unwrap().content.push_str("\n\nReturn one valid JSON answer object in at most 80 words.");
    }
    Err(provider_error(provider,
        "The local model could not produce a concise conversational answer after two retries.",
        "The selected model could not produce a concise conversational answer after two retries."))
}

#[tauri::command]
pub async fn ollama_status() -> ProviderStatus {
    let offline = || ProviderStatus { state: "offline", models: vec![], error: Some(ProviderError { code: "unavailable", message: "Ollama not detected. Start Ollama and try again." }) };
    let Ok(provider) = OllamaProvider::new(Duration::from_secs(4)) else { return offline() };
    if !provider.is_available().await { return offline(); }
    match provider.installed_models().await {
        Ok(models) => ProviderStatus { state: "connected", models: models.into_iter().filter(|model| !model.is_empty()).map(|model| ModelInfo { id: model.clone(), profile: model_profiles::resolve(&model), name: model }).collect(), error: None },
        Err(error) if error.kind == ProviderErrorKind::MalformedResponse => ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "malformed_response", message: "Ollama returned an invalid model list." }) },
        Err(_) => ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "model_list_failed", message: "Ollama is connected, but its model list could not be loaded." }) },
    }
}

#[tauri::command]
pub async fn ollama_chat(model: String, messages: Vec<ChatMessage>, open_project: State<'_, OpenProject>, pending: State<'_, PendingChanges>, debug: State<'_, AgentDebug>, app: tauri::AppHandle) -> Result<ChatResponse, String> {
    let root = open_project.0.lock().map_err(|_| "Project state unavailable")?.clone();
    let started = Instant::now();
    let (request, trace) = {
        let mut state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
        if state.enabled {
            state.next_request += 1;
            (state.next_request, Some(Arc::new(Mutex::new(TraceBuffer { echo: true, ..TraceBuffer::default() }))))
        } else { (0, None) }
    };
    if let Some(trace) = trace.as_ref() {
        let started_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis()).unwrap_or(0);
        debug_log(Some(trace), "agent", format!("==================================================\nAIIDE AGENT DEBUG TRACE\nTrace version: 1\nRequest: #{request}\nStarted at Unix ms: {started_at}\nModel: {model}\nProject: {}\nCore agent instructions: included on agent turns\nDefault personality: included only on final conversational turns\n==================================================", root.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy()).unwrap_or_else(|| "none".into())));
    }
    let result = run_agent(OLLAMA_PROVIDER_ID, model, messages, root, Some(&pending), Some(&app), trace.as_ref()).await;
    if let Some(trace) = trace {
        if let Err(error) = &result { debug_log(Some(&trace), "final", format!("FAILED\n{error}")); }
        debug_log(Some(&trace), "final", format!("Result: {}\nTotal duration: {}ms\n==================================================", if result.is_ok() { "SUCCESS" } else { "FAILED" }, started.elapsed().as_millis()));
        let report = trace.lock().map(|buffer| buffer.lines.join("\n\n")).unwrap_or_else(|_| "Trace unavailable".into());
        let mut state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
        if state.enabled { state.latest = Some(report); }
    }
    result
}

async fn run_agent(provider_id: &str, model: String, messages: Vec<ChatMessage>, root: Option<std::path::PathBuf>, pending: Option<&PendingChanges>, app: Option<&tauri::AppHandle>, debug_trace: Option<&Trace>) -> Result<ChatResponse, String> {
    let provider = SelectedProvider::from_id(provider_id, Duration::from_secs(120)).map_err(|error| error.to_string())?;
    run_agent_with_provider(&provider, model, messages, root, pending, app, debug_trace).await
}

async fn run_agent_with_provider(provider: &impl ModelProvider, model: String, messages: Vec<ChatMessage>, root: Option<std::path::PathBuf>, pending: Option<&PendingChanges>, app: Option<&tauri::AppHandle>, debug_trace: Option<&Trace>) -> Result<ChatResponse, String> {
    if model.is_empty() || messages.is_empty() || messages.len() > MAX_MESSAGES || messages.iter().any(|message| {
        !matches!(message.role.as_str(), "user" | "assistant") || message.content.is_empty() || message.content.chars().count() > MAX_MESSAGE_CHARS
    }) || messages.last().is_none_or(|message| message.role != "user") {
        return Err("The chat request is invalid or too long.".to_owned());
    }
    let installed_models = provider.installed_models().await.map_err(|error| error.to_string())?;
    if !installed_models.iter().any(|item| item == &model) {
        return Err(provider_error(provider,
            "This model is no longer installed. Retry to refresh the model list.",
            "The selected OpenRouter model is unavailable or invalid."));
    }
    let last_prompt = messages.last().map_or("", |message| message.content.trim()).to_owned();
    if last_prompt.eq_ignore_ascii_case("what is your name?") || last_prompt.eq_ignore_ascii_case("what is your name") {
        let provider_name = if provider.metadata().id == OLLAMA_PROVIDER_ID { "Ollama" } else { "OpenRouter" };
        return Ok(ChatResponse { model, content: format!("I'm Elma, your local coding companion in AIIDE. My responses are generated by the selected {provider_name} model."), activity: vec![], proposal: None });
    }
    if last_prompt.eq_ignore_ascii_case("do you have access to my files?") || last_prompt.eq_ignore_ascii_case("do you have access to my files") {
        return Ok(ChatResponse { model, content: if root.is_some() { "I can inspect this project and prepare focused changes for your review. Only the Apply button can write them." } else { "No project is open, so I cannot inspect or propose changes to files." }.into(), activity: vec![], proposal: None });
    }
    let plan = classify_current_request(&last_prompt);
    let selection_requested = requests_candidate_selection(&last_prompt, plan.intent);
    debug_log(debug_trace, "agent", format!("Current request plan: scope={:?}, intent={:?}", plan.scope, plan.intent));
    let mut exchange = Vec::new();
    exchange.push(ChatMessage { role: "system".into(), content: CORE_AGENT_INSTRUCTIONS.into() });
    if let Some(hint) = model_profiles::adaptation(&model).protocol_hint {
        exchange.push(ChatMessage { role: "system".into(), content: format!("Model protocol adaptation: {hint}") });
        debug_log(debug_trace, "agent", "Model protocol adaptation included; repository validation remains unchanged");
    }
    if let Some(ref root) = root {
        let info = super::project::inspect_metadata(root);
        exchange.push(ChatMessage { role: "system".into(), content: info });
    } else {
        exchange.push(ChatMessage { role: "system".into(), content: "No project is open. Repository tools are unavailable; answer normal chat directly.".into() });
    }
    exchange.push(ChatMessage { role: "system".into(), content: format!("The final user message is the authoritative current task. Current request intent: {}. {}", if plan.intent == RequestIntent::Edit { "edit" } else { "inspect/answer only" }, if selection_requested && root.is_some() { "This is a read-only target selection request. Identify and read the relevant HTML file with repository tools; AIIDE will discover verified candidates after the read. Do not propose a change." } else if plan.intent == RequestIntent::Edit && root.is_some() { "Inspect the relevant target, then return a focused propose_change. Do not claim completion without a validated proposal." } else if plan.intent == RequestIntent::Edit { "No project is open, so explain that the requested edit cannot be prepared yet." } else { "Do not propose or prepare a change; answer the current request after any required inspection." }) });
    exchange.extend(messages.into_iter().rev().take(12).collect::<Vec<_>>().into_iter().rev());
    let mut activity = Vec::new();
    let mut tool_cache: HashMap<String, (String, Activity)> = HashMap::new();
    let mut context_bytes = 0;
    let mut successful_inspections = 0;
    let mut successful_listings = 0;
    let mut successful_searches = 0;
    let mut successful_reads = 0;
    let mut read_paths = Vec::new();
    let mut evidence: Vec<(String, String)> = Vec::new();
    let mut consecutive_repeats = 0;
    let mut unresolved_failure = false;
    let mut known_paths = String::new();
    let mut known_file_paths: Vec<String> = Vec::new();
    let mut needs_inspection = match plan.scope { RequestScope::General => Some(false), RequestScope::Repository => Some(true), RequestScope::Unknown => None };
    let mut grounding = evidence_requirement(&last_prompt, plan.scope);
    let mut inspection_reminders = 0;
    let mut proposal_repairs = 0;
    for iteration in 0..=repository::MAX_TOOL_CALLS {
        let evidence_sufficient = grounding.satisfied(successful_listings, successful_searches, &read_paths);
        let answer_allowed = plan.intent == RequestIntent::Answer && (needs_inspection != Some(true) || evidence_sufficient);
        let (result, action) = agent_turn(provider, &model, &mut exchange, root.is_some(), plan.intent == RequestIntent::Edit, successful_reads > 0, answer_allowed, app, debug_trace).await?;
        if let AgentAction::Propose(summary, edits) = action {
            let root = root.as_ref().ok_or_else(|| "Open a project before proposing changes.".to_owned())?;
            if edits.iter().any(|edit| !read_paths.contains(&edit.path)) {
                exchange.push(result.message);
                exchange.push(ChatMessage { role: "user".into(), content: "Every proposed target must first be inspected with read_file. Inspect the exact target path, then propose the focused replacement.".into() });
                continue;
            }
            if let Some(app) = app { let _ = app.emit("proposal-validation-start", ()); }
            debug_log(debug_trace, "tool", format!("propose_change selected\nPath: {}\nReplacements: {}\nValidation: started", edits.first().map(|edit| edit.path.as_str()).unwrap_or("none"), edits.len()));
            let shape = anchor_shape(&edits);
            let proposal = match repository::validate_proposal(root, summary, edits) {
                Ok(proposal) => proposal,
                Err(error) if error == repository::AMBIGUOUS_OLD_TEXT_ERROR && proposal_repairs < MAX_REPAIRS => {
                    proposal_repairs += 1;
                    debug_log(debug_trace, "repair", format!("Ambiguous proposal anchor rejected\nAttempt {proposal_repairs}/{MAX_REPAIRS}\nAnchor shape: {shape}"));
                    exchange.push(result.message);
                    exchange.push(ChatMessage { role: "user".into(), content: ambiguous_anchor_guidance().into() });
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(pending) = pending {
                *pending.0.lock().map_err(|_| "Pending change state unavailable")? = Some(proposal.clone());
            }
            debug_log(debug_trace, "tool", "Proposal validation: passed\nPending change creation: passed");
            return Ok(ChatResponse { model: result.model, content: "Done — have a wee look in Changes 👀".into(), activity, proposal: Some(proposal) });
        }
        if let AgentAction::Answer(answer) = action {
            if plan.intent == RequestIntent::Edit && root.is_some() {
                exchange.push(result.message);
                exchange.push(ChatMessage { role: "user".into(), content: if successful_reads == 0 { "This edit request cannot finish with an answer. Inspect the relevant file first, then prepare a focused proposal.".into() } else { "This edit request cannot finish with an answer or claim review readiness. Return a minimal propose_change using the inspected file content.".into() } });
                continue;
            }
            if root.is_some() && (needs_inspection.is_none() || (needs_inspection == Some(true) && !evidence_sufficient)) {
                let needed = match needs_inspection {
                    Some(needed) => needed,
                    None => {
                        let needed = requires_inspection(provider, &model, &last_prompt, debug_trace).await?;
                        needs_inspection = Some(needed);
                        needed
                    }
                };
                if needed {
                    if grounding.kind == EvidenceKind::None {
                        grounding = evidence_requirement(&last_prompt, RequestScope::Repository);
                    }
                    if inspection_reminders < 2 {
                        debug_log(debug_trace, "protocol", "Grounding validation: FAILED — repository evidence required before answer");
                        inspection_reminders += 1;
                        exchange.push(result.message);
                        exchange.push(ChatMessage { role: "user".into(), content: match grounding.kind {
                            EvidenceKind::Listing => "This question needs a project directory listing before it can be answered. Use list_files with the relevant project-relative directory. Do not ask the user to provide information AIIDE can inspect.".into(),
                            EvidenceKind::Inspection => "This question needs repository evidence before it can be answered. Use the least expensive relevant list_files, search_files, or read_file action. Do not ask the user to provide information AIIDE can inspect.".into(),
                            EvidenceKind::Read => "This question needs evidence from the opened project. Use list_files to discover exact paths, then read_file on the relevant file before answering. Do not ask the user to provide content AIIDE can inspect.".into(),
                            EvidenceKind::None => unreachable!(),
                        } });
                        continue;
                    }
                    return Ok(ChatResponse { model: result.model, content: "I couldn't inspect the opened project, so I can't give a file-based answer. Please try again.".into(), activity, proposal: None });
                }
            }
            if unresolved_failure && successful_inspections > 0 {
                if inspection_reminders < 2 {
                    inspection_reminders += 1;
                    exchange.push(result.message);
                    exchange.push(ChatMessage { role: "user".into(), content: "Your last repository tool did not provide evidence. Inspect another relevant file or correct the path before answering. Use exact paths from list_files.".into() });
                    continue;
                }
                return Ok(ChatResponse { model: result.model, content: "I couldn't verify the relevant project files, so I can't give a grounded answer. Please try again.".into(), activity, proposal: None });
            }
            let content = if !read_paths.is_empty() {
                let info = root.as_ref().map(|path| super::project::inspect_metadata(path)).unwrap_or_default();
                let answer = final_turn(provider, &model, &last_prompt, &info, &evidence, &read_paths, debug_trace).await?;
                format!("{answer}\n\nFiles actually inspected: {}", read_paths.join(", "))
            } else { conversational_turn(provider, &model, &last_prompt, &answer, debug_trace).await? };
            return Ok(ChatResponse { model: result.model, content, activity, proposal: None });
        }
        if root.is_none() { return Ok(ChatResponse { model: result.model, content: "Open a project to use repository tools.".into(), activity, proposal: None }); }
        if iteration == repository::MAX_TOOL_CALLS { break; }
        let request = match action {
            AgentAction::List(path) => ToolRequest { tool: "list_files".into(), path, query: String::new() },
            AgentAction::Search(query) => ToolRequest { tool: "search_files".into(), path: String::new(), query },
            AgentAction::Read(path) => ToolRequest { tool: "read_file".into(), path, query: String::new() },
            AgentAction::Answer(_) | AgentAction::Propose(_, _) => unreachable!(),
        };
        let repeated = tool_cache.contains_key(&tool_key(&request));
        if !repeated {
            if let Some(app) = app { let _ = app.emit("repository-inspection-start", ()); }
        }
        let tool_started = Instant::now();
        debug_log(debug_trace, "tool", format!("{} requested\nPath: {}\nQuery: {}", request.tool, request.path, request.query));
        let (output, event, reused) = execute_cached(root.as_deref().unwrap(), &request, &mut tool_cache);
        let remaining = repository::MAX_CONTEXT_BYTES.saturating_sub(context_bytes);
        if remaining == 0 { break; }
        let output = output.chars().scan(0_usize, |used, character| {
            let next = *used + character.len_utf8();
            if next > remaining { None } else { *used = next; Some(character) }
        }).collect::<String>();
        context_bytes += output.len();
        if repeated { consecutive_repeats += 1; } else { consecutive_repeats = 0; }
        let useful = useful_tool_result(&output);
        if request.tool == "list_files" && useful && (request.path.is_empty() || request.path == ".") {
            known_paths = output.lines().take(120).map(|line| line.split(" (").next().unwrap_or(line)).collect::<Vec<_>>().join(", ");
            known_file_paths = output.lines().filter_map(|line| line.strip_suffix(" (file)").filter(|path| !std::path::Path::new(path).file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'))).map(str::to_owned)).collect();
        }
        if useful {
            successful_inspections += 1;
            if request.tool == "list_files" { successful_listings += 1; }
            if request.tool == "search_files" { successful_searches += 1; }
            if request.tool == "read_file" { successful_reads += 1; read_paths.push(request.path.clone()); }
            evidence.push((format!("{} {}", request.tool, request.path), output.clone()));
            unresolved_failure = false;
        } else { unresolved_failure = true; }
        debug_log(debug_trace, "tool", format!("{}\nValidation/execution: {}\nCache: {}\nReturned: {} bytes\nDuration: {}ms\nNext stage: agent turn", request.tool, if output.starts_with("Error:") { &output } else { "passed" }, if reused { "reused successful result" } else { "executed" }, output.len(), tool_started.elapsed().as_millis()));
        if let Some(app) = app { let _ = app.emit("repository-activity", &event); }
        activity.push(event);
        if selection_requested && useful && request.tool == "read_file" && is_html_path(&request.path)
            && file_reference(&last_prompt).is_none_or(|name| std::path::Path::new(&request.path).file_name().is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(&name))) {
            let root = root.as_deref().expect("project is open for repository tools");
            let mut registry = CandidateRegistry::new();
            registry.discover_html(root, &request.path)?;
            let outcome = select_candidate(provider, &model, &last_prompt, root, &registry, debug_trace).await?;
            let content = match outcome {
                SelectionOutcome::Selected(view) => format!("Verified target: {} ({}) in {} at lines {}–{}. This selection is read-only; no change is prepared.", view.id, view.description, view.path, view.start_line, view.end_line),
                SelectionOutcome::Ambiguous => "I found multiple plausible targets. Please clarify which element or location you mean before I select one.".into(),
                SelectionOutcome::NoMatch => "I found no verified target matching that request in the inspected file. Please specify another file or target.".into(),
            };
            return Ok(ChatResponse { model, content, activity, proposal: None });
        }
        exchange.push(result.message);
        let unread = known_file_paths.iter().filter(|path| !read_paths.contains(path)).cloned().collect::<Vec<_>>().join(", ");
        exchange.push(ChatMessage { role: "user".into(), content: format!("{}\nCurrent request intent: {}.", tool_result_message(&request, useful, &output, &known_paths, &unread), if plan.intent == RequestIntent::Edit { "edit — inspect, then propose the focused change" } else { "inspect/answer only — do not propose a change" }) });
        if consecutive_repeats >= 2 { break; }
    }
    if !read_paths.is_empty() {
        let info = root.as_ref().map(|path| super::project::inspect_metadata(path)).unwrap_or_default();
        let answer = final_turn(provider, &model, &last_prompt, &info, &evidence, &read_paths, debug_trace).await?;
        let content = format!("{answer}\n\nFiles actually inspected: {}", read_paths.join(", "));
        return Ok(ChatResponse { model, content, activity, proposal: None });
    }
    Ok(ChatResponse { model, content: "I reached the repository inspection limit before I could read a relevant file. Please ask a narrower question.".into(), activity, proposal: None })
}

pub(crate) struct BenchmarkAgentOutput {
    pub content: String,
    pub activity: Vec<String>,
    pub proposal: Option<PendingProposal>,
    pub trace: String,
}

pub(crate) struct BenchmarkAgentFailure {
    pub error: String,
    pub trace: String,
}

pub(crate) async fn run_benchmark_agent(provider: &str, model: &str, root: std::path::PathBuf, prompt: &str) -> Result<BenchmarkAgentOutput, BenchmarkAgentFailure> {
    let trace = Arc::new(Mutex::new(TraceBuffer::default()));
    let response = run_agent(provider,
        model.to_owned(),
        vec![ChatMessage { role: "user".into(), content: prompt.to_owned() }],
        Some(root), None, None, Some(&trace),
    ).await;
    let report = trace.lock().map(|buffer| buffer.lines.join("\n\n")).unwrap_or_default();
    let response = response.map_err(|error| BenchmarkAgentFailure { error, trace: report.clone() })?;
    Ok(BenchmarkAgentOutput {
        content: response.content,
        activity: response.activity.into_iter().map(|item| item.label).collect(),
        proposal: response.proposal,
        trace: report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_provider::{ProviderFailure, ProviderLocality, ProviderMetadata};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubProvider {
        models: Vec<String>,
        responses: Mutex<VecDeque<InferenceResponse>>,
        formats: Mutex<Vec<Value>>,
        requests: Mutex<Vec<Vec<ChatMessage>>>,
        inference_calls: AtomicUsize,
    }

    impl StubProvider {
        fn new(models: &[&str], responses: &[&str]) -> Self {
            Self {
                models: models.iter().map(|value| (*value).to_owned()).collect(),
                responses: Mutex::new(responses.iter().map(|content| InferenceResponse {
                    model: "test-model".into(),
                    message: ChatMessage { role: "assistant".into(), content: (*content).into() },
                    done: true,
                    done_reason: Some("stop".into()),
                }).collect()),
                formats: Mutex::new(Vec::new()),
                requests: Mutex::new(Vec::new()),
                inference_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ModelProvider for StubProvider {
        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata { id: OLLAMA_PROVIDER_ID, locality: ProviderLocality::Local }
        }

        async fn is_available(&self) -> bool { true }

        async fn installed_models(&self) -> Result<Vec<String>, ProviderFailure> { Ok(self.models.clone()) }

        async fn infer(&self, request: InferenceRequest<'_>) -> Result<InferenceResponse, ProviderFailure> {
            self.inference_calls.fetch_add(1, Ordering::SeqCst);
            self.formats.lock().unwrap().push(request.format.clone());
            self.requests.lock().unwrap().push(request.messages.to_vec());
            let mut response = self.responses.lock().unwrap().pop_front().ok_or_else(|| ProviderFailure::new(ProviderErrorKind::Api, "No stub response configured."))?;
            if matches!(response.message.content.as_str(), "__SELECT_FIRST__" | "__SELECT_HEADING__" | "__SELECT_PARAGRAPH__") {
                let payload = request.messages.last().unwrap().content.split_once("Verified candidates: ").unwrap().1;
                let views: Value = serde_json::from_str(payload).unwrap();
                let selected = match response.message.content.as_str() {
                    "__SELECT_HEADING__" => views.as_array().unwrap().iter().find(|view| view["role"] == "heading_one").unwrap(),
                    "__SELECT_PARAGRAPH__" => views.as_array().unwrap().iter().find(|view| view["role"] == "paragraph" && view["roleIndex"] == 1).unwrap(),
                    _ => &views[0],
                };
                response.message.content = format!("{{\"result\":\"selected\",\"candidate_id\":\"{}\"}}", selected["id"].as_str().unwrap());
            }
            Ok(response)
        }
    }

    #[test]
    fn personality_is_separate_from_protocol_defaults() {
        assert!(CORE_AGENT_INSTRUCTIONS.contains("cannot apply changes"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("old_text must be copied exactly"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("Never ask the user to provide project content"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("least expensive relevant tool"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("match exactly one location"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("Never invent old_text"));
        assert!(DEFAULT_PERSONALITY.contains("concise by default"));
        assert!(DEFAULT_PERSONALITY.contains("dry sense of humour"));
        assert!(!DEFAULT_PERSONALITY.contains("propose_change"));
        assert_eq!(PROTOCOL_TEMPERATURE, 0.0);
        assert_eq!(CONVERSATIONAL_TEMPERATURE, 0.2);
    }

    #[test]
    fn provider_responses_enter_the_existing_validation_and_repair_pipeline() {
        let provider = StubProvider::new(&["test-model"], &["plain prose", r#"{"action":"answer","answer":"repaired"}"#]);
        let mut exchange = vec![ChatMessage { role: "user".into(), content: "hello".into() }];
        let (_, action) = tauri::async_runtime::block_on(agent_turn(
            &provider, "test-model", &mut exchange, false, false, false, true, None, None,
        )).unwrap();
        assert_eq!(action, AgentAction::Answer("repaired".into()));
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 2);
        assert!(exchange.last().unwrap().content.contains("previous response was invalid"));
    }

    #[test]
    fn agent_model_selection_uses_provider_discovery() {
        let provider = StubProvider::new(&["installed-model"], &[]);
        let error = match tauri::async_runtime::block_on(run_agent_with_provider(
            &provider,
            "missing-model".into(),
            vec![ChatMessage { role: "user".into(), content: "hello".into() }],
            None, None, None, None,
        )) {
            Err(error) => error,
            Ok(_) => panic!("missing model should be rejected"),
        };
        assert_eq!(error, "This model is no longer installed. Retry to refresh the model list.");
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn debug_defaults_off_and_trace_is_chronological() {
        let debug = AgentDebug::default();
        let state = debug.0.lock().unwrap();
        assert!(!state.enabled);
        assert!(state.latest.is_none());
        drop(state);

        let trace = Arc::new(Mutex::new(TraceBuffer::default()));
        debug_log(Some(&trace), "ollama", "--- RAW OLLAMA RESPONSE START ---\nplain prose\n--- RAW OLLAMA RESPONSE END ---");
        debug_log(Some(&trace), "protocol", "Parse or semantic validation: FAILED — expected value");
        debug_log(Some(&trace), "repair", "Attempt 1/2");
        debug_log(Some(&trace), "final", "Personality rewrite started");
        let report = trace.lock().unwrap().lines.join("\n");
        assert!(report.find("plain prose").unwrap() < report.find("FAILED").unwrap());
        assert!(report.find("FAILED").unwrap() < report.find("Attempt 1/2").unwrap());
        assert!(report.find("Attempt 1/2").unwrap() < report.find("Personality rewrite").unwrap());
    }

    #[test]
    fn debug_off_retains_nothing() {
        debug_log(None, "ollama", "raw response");
        assert_eq!(trace_turn(None), 0);
    }

    #[test]
    fn valid_actions() {
        assert_eq!(parse_action(r#"{"action":"list_files"}"#), Ok(AgentAction::List(String::new())));
        assert_eq!(parse_action(r#"{"action":"list_files","path":"src"}"#), Ok(AgentAction::List("src".into())));
        assert_eq!(parse_action(r#"{"action":"search_files","query":"menu"}"#), Ok(AgentAction::Search("menu".into())));
        assert_eq!(parse_action(r#"{"action":"read_file","path":"src/index.html"}"#), Ok(AgentAction::Read("src/index.html".into())));
        assert_eq!(parse_action(r#"{"action":"answer","answer":"A direct response."}"#), Ok(AgentAction::Answer("A direct response.".into())));
        assert!(matches!(parse_action(r#"{"action":"propose_change","summary":"Improve label","changes":[{"path":"index.html","old_text":"<input>","new_text":"<label>Name</label><input>"}]}"#), Ok(AgentAction::Propose(_, _))));
    }

    #[test]
    fn answer_field_contract_and_repair_are_explicit() {
        assert_eq!(parse_action(r#"{"action":"answer","answer":"A closure retains lexical scope."}"#), Ok(AgentAction::Answer("A closure retains lexical scope.".into())));
        let error = parse_action(r#"{"action":"answer","summary":"Wrong field."}"#).unwrap_err();
        assert!(error.contains("put the response text in the 'answer' field"));
        assert!(error.contains("Do not put it in 'summary'"));
        let schema = agent_schema(false, false, false, true);
        assert!(schema["properties"]["answer"]["description"].as_str().unwrap().contains("Required response text"));
        assert!(schema["properties"].get("summary").is_none());
    }

    #[test]
    fn structured_failure_diagnostics_report_shape_without_content() {
        let private = "private repository content";
        let json = format!(r#"{{"action":"propose_change","summary":"{private}"}}"#);
        let shape = response_shape(&json);
        assert!(shape.contains("action=propose_change"));
        assert!(shape.contains("keys=action,summary"));
        assert!(!shape.contains(private));
        assert_eq!(response_shape("```json\n{}\n```"), "non-json(fenced=true, chars=14)");
    }

    #[test]
    fn agent_action_availability_matches_project_and_inspection_state() {
        let actions = |project_open, edit, evidence, answer_allowed| agent_schema(project_open, edit, evidence, answer_allowed)["properties"]["action"]["enum"].as_array().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(actions(false, false, false, false), vec!["answer"]);
        assert!(actions(true, false, true, true).contains(&"answer".to_owned()));
        assert!(!actions(true, false, true, false).contains(&"answer".to_owned()));
        assert!(!actions(true, false, true, true).contains(&"propose_change".to_owned()));
        assert!(!actions(true, true, false, true).contains(&"answer".to_owned()));
        assert!(!actions(true, true, false, true).contains(&"propose_change".to_owned()));
        assert_eq!(actions(true, true, true, true), vec!["propose_change"]);
        let before_read = agent_schema(true, true, false, false);
        assert!(before_read["properties"].get("summary").is_none());
        assert!(before_read["properties"].get("changes").is_none());
        let after_read = agent_schema(true, true, true, false);
        assert_eq!(after_read["properties"]["changes"]["minItems"], 1);
        assert!(after_read["properties"]["changes"]["items"]["properties"]["old_text"]["description"].as_str().unwrap().contains("matches exactly once"));
        assert_eq!(after_read["required"], json!(["action", "summary", "changes"]));
        assert!(after_read["properties"].get("path").is_none());
    }

    #[test]
    fn current_request_scope_and_intent_are_separate() {
        for prompt in ["what is a javascript closure"] {
            assert_eq!(classify_current_request(prompt), RequestPlan { scope: RequestScope::General, intent: RequestIntent::Answer });
        }
        for prompt in ["tell me about this project", "look at the css and tell me one improvement", "tell me about this project and describe a change you'd make"] {
            assert_eq!(classify_current_request(prompt), RequestPlan { scope: RequestScope::Repository, intent: RequestIntent::Answer }, "{prompt}");
        }
        for prompt in ["describe one change you'd make", "what would you change here?", "review this page and suggest an improvement", "tell me how you'd fix this"] {
            assert_eq!(classify_current_request(prompt).intent, RequestIntent::Answer, "{prompt}");
        }
        for prompt in [
            "change the main heading to Welcome",
            "fix the heading",
            "rename this button to Save",
            "add a footer",
            "remove the old paragraph",
            "implement that",
            "make only that change and prepare it for review",
            "change the main heading to \"Welcome to the AIIDE Sandbox\". make only that change and prepare it for review",
        ] {
            assert_eq!(classify_current_request(prompt), RequestPlan { scope: RequestScope::Repository, intent: RequestIntent::Edit }, "{prompt}");
        }
        let earlier_edit = "change the heading";
        let current_answer = "actually just tell me what you'd change";
        assert_eq!(classify_current_request(current_answer).intent, RequestIntent::Answer);
        assert_eq!(classify_current_request(earlier_edit).intent, RequestIntent::Edit);
        let earlier_answer = "tell me what you'd change";
        let current_edit = "okay implement that";
        assert_eq!(classify_current_request(current_edit).intent, RequestIntent::Edit);
        assert_eq!(classify_current_request(earlier_answer).intent, RequestIntent::Answer);
    }

    #[test]
    fn natural_repository_requests_and_general_chat_are_classified_conservatively() {
        for prompt in ["summarize index.html", "what does styles.css do?", "find the page heading", "what files are in src?"] {
            assert_eq!(classify_current_request(prompt).scope, RequestScope::Repository, "{prompt}");
        }
        for prompt in ["hey", "how are you?", "explain what HTML is", "what is CSS?"] {
            assert_ne!(classify_current_request(prompt).scope, RequestScope::Repository, "{prompt}");
        }
        assert_eq!(evidence_requirement("summarize index.html", RequestScope::Repository), EvidenceRequirement { kind: EvidenceKind::Read, filename: Some("index.html".into()) });
        let named = evidence_requirement("what does styles.css do?", RequestScope::Repository);
        assert!(!named.satisfied(1, 1, &["src/index.html".into()]));
        assert!(named.satisfied(0, 0, &["src/styles.css".into()]));
        assert_eq!(evidence_requirement("find the page heading", RequestScope::Repository).kind, EvidenceKind::Inspection);
        assert!(!evidence_requirement("find the page heading", RequestScope::Repository).satisfied(1, 0, &[]));
        assert!(evidence_requirement("find the page heading", RequestScope::Repository).satisfied(0, 1, &[]));
        assert_eq!(evidence_requirement("what files are in src?", RequestScope::Repository).kind, EvidenceKind::Listing);
        assert_eq!(classify_current_request("inspect src/components").scope, RequestScope::Repository);
    }

    fn grounding_fixture(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("aiide-grounding-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/index.html"), "<h1>Welcome</h1>").unwrap();
        std::fs::write(root.join("src/styles.css"), "body { color: black; }").unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn selection_is_reachable_after_html_read_and_returns_a_verified_id_without_a_proposal() {
        let root = grounding_fixture("selection-valid");
        std::fs::write(root.join("src/index.html"), "<title>Welcome</title><h1>Welcome</h1>").unwrap();
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"read_file","path":"src/index.html"}"#,
            "__SELECT_FIRST__",
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "Select the document title in index.html".into() }],
            Some(root.clone()), None, None, None,
        )).unwrap();
        assert!(response.content.contains("Verified target: c"));
        assert!(response.content.contains("Document title"));
        assert!(response.proposal.is_none());
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 2);
        assert_eq!(classify_current_request("Select the document title in index.html").scope, RequestScope::Repository);
        assert!(!provider.formats.lock().unwrap()[0]["properties"]["action"]["enum"].as_array().unwrap().iter().any(|action| action == "answer"));
        let requests = provider.requests.lock().unwrap();
        let presented = &requests[1][1].content;
        assert!(presented.contains("\"role\":\"document_title\""));
        assert!(presented.contains("\"path\":\"src/index.html\""));
        assert!(presented.contains("\"startLine\":1"));
        assert!(presented.contains("\"description\":\"Document title\""));
        assert!(presented.contains("\"excerpt\":\"Welcome\""));
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), "<title>Welcome</title><h1>Welcome</h1>");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_heading_selection_uses_active_registry_and_traces_structured_outcome() {
        let root = grounding_fixture("selection-nested-heading");
        let original = "<title>Notes</title>\n<h1>\n Notes <span>for everyone</span>\n</h1>\n<p class=\"hero-text\">An introduction.</p>";
        std::fs::write(root.join("src/index.html"), original).unwrap();
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"read_file","path":"src/index.html"}"#,
            "__SELECT_HEADING__",
        ]);
        let trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "Select the main visible page heading in src/index.html".into() }],
            Some(root.clone()), None, None, Some(&trace),
        )).unwrap();
        assert!(response.content.contains("Visible h1 heading"));
        assert!(response.content.contains("lines 2–3"));
        assert!(response.proposal.is_none());
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 2);
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), original);
        let report = trace.lock().unwrap().lines.join("\n");
        assert!(report.contains("Selection call started"));
        assert!(report.contains("Schema supplied:"));
        assert!(report.contains("Selection response received in"));
        assert!(report.contains("Structured action: selected"));
        assert!(report.contains("Candidate ID: c"));
        assert!(report.contains("Validation: passed"));
        assert!(!report.contains("Notes <span>for everyone</span>"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn introductory_paragraph_selection_uses_role_order_without_writing() {
        let root = grounding_fixture("selection-intro-paragraph");
        let original = "<title>OrbitNote — Notes that stay out of your way</title>\n<h1>\n Notes that don't become\n <span>another unfinished project.</span>\n</h1>\n<p class=\"hero-text\">OrbitNote is a simple place to capture ideas.</p>\n<p>Later copy.</p>";
        std::fs::write(root.join("src/index.html"), original).unwrap();
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"read_file","path":"src/index.html"}"#,
            "__SELECT_PARAGRAPH__",
        ]);
        let trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "Select the introductory paragraph in src/index.html".into() }],
            Some(root.clone()), None, None, Some(&trace),
        )).unwrap();
        assert!(response.content.contains("Paragraph text"));
        assert!(response.content.contains("lines 6–6"));
        assert!(response.proposal.is_none());
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 2);
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), original);
        let requests = provider.requests.lock().unwrap();
        let presented = &requests[1][1].content;
        assert!(presented.contains("\"role\":\"paragraph\",\"roleIndex\":1"));
        assert!(presented.contains("\"role\":\"paragraph\",\"roleIndex\":2"));
        assert!(presented.contains("OrbitNote is a simple place"));
        let report = trace.lock().unwrap().lines.join("\n");
        assert!(report.contains("Structured action: selected"));
        assert!(report.contains("Validation: passed"));
        assert!(!report.contains("OrbitNote is a simple place"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_ambiguity_and_no_match_request_clarification_without_editing() {
        for (label, reply, expected) in [
            ("ambiguous", r#"{"result":"ambiguous"}"#, "Please clarify"),
            ("no-match", r#"{"result":"no_match"}"#, "no verified target"),
        ] {
            let root = grounding_fixture(label);
            let original = "<h1>Welcome</h1><h1>Welcome</h1>";
            std::fs::write(root.join("src/index.html"), original).unwrap();
            let provider = StubProvider::new(&["test-model"], &[
                r#"{"action":"read_file","path":"src/index.html"}"#, reply,
            ]);
            let response = tauri::async_runtime::block_on(run_agent_with_provider(
                &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "Select the target in index.html".into() }],
                Some(root.clone()), None, None, None,
            )).unwrap();
            assert!(response.content.contains(expected));
            assert!(response.proposal.is_none());
            assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), original);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn selection_rejects_unknown_ids_malformed_responses_other_requests_and_stale_source() {
        let root = grounding_fixture("selection-rejection");
        let mut first = CandidateRegistry::new();
        first.discover_html(&root, "src/index.html").unwrap();
        let id = first.model_view()[0].id.clone();
        assert!(matches!(resolve_selection(&format!(r#"{{"result":"selected","candidate_id":"{id}"}}"#), &first, &root), Ok(SelectionOutcome::Selected(_))));
        assert_eq!(resolve_selection(r#"{"result":"selected","candidate_id":"unknown"}"#, &first, &root).unwrap_err(), "Unknown candidate ID.");
        for raw in ["not json", r#"{"result":"selected"}"#, r#"{"result":"ambiguous","candidate_id":"x"}"#, r#"{"result":"no_match","extra":1}"#] {
            assert_eq!(resolve_selection(raw, &first, &root).unwrap_err(), "Candidate selection response was malformed.");
        }
        let mut second = CandidateRegistry::new();
        second.discover_html(&root, "src/index.html").unwrap();
        assert_eq!(resolve_selection(&format!(r#"{{"result":"selected","candidate_id":"{id}"}}"#), &second, &root).unwrap_err(), "Unknown candidate ID.");
        std::fs::write(root.join("src/index.html"), "<h1>Changed</h1>").unwrap();
        assert_eq!(resolve_selection(&format!(r#"{{"result":"selected","candidate_id":"{id}"}}"#), &first, &root).unwrap_err(), "Candidate source changed since discovery. Inspect it again.");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_trace_reports_validation_failure_without_raw_model_content() {
        let root = grounding_fixture("selection-trace-error");
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "src/index.html").unwrap();
        let provider = StubProvider::new(&["test-model"], &[r#"{"result":"selected","candidate_id":"unknown"}"#]);
        let trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let error = tauri::async_runtime::block_on(select_candidate(
            &provider, "test-model", "Select the heading", &root, &registry, Some(&trace),
        )).unwrap_err();
        assert_eq!(error, "Unknown candidate ID.");
        let report = trace.lock().unwrap().lines.join("\n");
        assert!(report.contains("Structured action: selected"));
        assert!(report.contains("Candidate ID: unverified"));
        assert!(report.contains("Validation: failed\nError: Unknown candidate ID."));
        assert!(!report.contains("\"candidate_id\":\"unknown\""));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_trace_reports_provider_failure_without_response_content() {
        let root = grounding_fixture("selection-provider-error");
        let mut registry = CandidateRegistry::new();
        registry.discover_html(&root, "src/index.html").unwrap();
        let provider = StubProvider::new(&["test-model"], &[]);
        let trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let error = tauri::async_runtime::block_on(select_candidate(
            &provider, "test-model", "Select the heading", &root, &registry, Some(&trace),
        )).unwrap_err();
        assert_eq!(error, "No stub response configured.");
        let report = trace.lock().unwrap().lines.join("\n");
        assert!(report.contains("Selection call started"));
        assert!(report.contains("Schema supplied:"));
        assert!(report.contains("Selection call failed after"));
        assert!(report.contains("Validation: not run"));
        assert!(report.contains("Provider error kind: Api"));
        assert!(!report.contains(&error));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_file_refusal_cannot_complete_before_discovery_and_relevant_read() {
        let root = grounding_fixture("named-file");
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"answer","answer":"I cannot summarize it until you provide the file."}"#,
            r#"{"action":"list_files","path":""}"#,
            r#"{"action":"read_file","path":"src/index.html"}"#,
            r#"{"action":"answer","answer":"The page contains a Welcome heading."}"#,
            r#"{"action":"answer","answer":"It is a small page headed Welcome."}"#,
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "summarize index.html".into() }],
            Some(root.clone()), None, None, None,
        )).unwrap();
        assert!(response.activity.iter().any(|item| item.label == "Project structure"));
        assert!(response.activity.iter().any(|item| item.label == "Read: src/index.html"));
        assert!(response.content.contains("small page"));
        assert!(!response.content.contains("provide the file"));
        let first_actions = provider.formats.lock().unwrap()[0]["properties"]["action"]["enum"].as_array().unwrap().clone();
        assert!(!first_actions.iter().any(|action| action == "answer"), "answer must be unavailable before evidence");
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 5);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_listing_is_sufficient_without_unnecessary_reads() {
        let root = grounding_fixture("directory-listing");
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"list_files","path":"src"}"#,
            r#"{"action":"answer","answer":"src contains index.html and styles.css."}"#,
            r#"{"action":"answer","answer":"src contains index.html and styles.css."}"#,
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "what files are in src?".into() }],
            Some(root.clone()), None, None, None,
        )).unwrap();
        assert_eq!(response.activity.iter().filter(|item| item.label.starts_with("Read: ")).count(), 0);
        assert_eq!(response.activity.iter().filter(|item| item.label == "Project structure").count(), 1);
        assert!(response.content.contains("index.html"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ordinary_conversation_answers_without_repository_tools() {
        let root = grounding_fixture("general-chat");
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"answer","answer":"Hey!"}"#,
            r#"{"action":"answer","answer":"Hey!"}"#,
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "hey".into() }],
            Some(root.clone()), None, None, None,
        )).unwrap();
        assert!(response.activity.is_empty());
        assert_eq!(response.content, "Hey!");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn premature_summary_only_edit_is_repaired_into_inspection_then_validated_proposal() {
        let root = grounding_fixture("structured-edit");
        let pending = PendingChanges::default();
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"propose_change","summary":"Change the heading."}"#,
            r#"{"action":"read_file","path":"src/index.html"}"#,
            r#"{"action":"propose_change","summary":"Change the heading.","changes":[{"path":"src/index.html","old_text":"<h1>Welcome</h1>","new_text":"<h1>Changed</h1>"}]}"#,
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "change the heading to Changed".into() }],
            Some(root.clone()), Some(&pending), None, None,
        )).unwrap();
        let proposal = response.proposal.expect("valid edit should create a proposal");
        assert_eq!(proposal.changes[0].path, "src/index.html");
        assert_eq!(proposal.changes[0].replacements, 1);
        let stored = pending.0.lock().unwrap();
        let stored = stored.as_ref().expect("validated edit should remain pending for approval");
        assert_eq!(stored.changes[0].after, proposal.changes[0].after);
        assert!(response.activity.iter().any(|item| item.label == "Read: src/index.html"));
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), "<h1>Welcome</h1>");
        let formats = provider.formats.lock().unwrap();
        assert!(formats[0]["properties"].get("changes").is_none());
        assert!(formats[2]["properties"].get("changes").is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ambiguous_edit_is_retried_with_unique_exact_context() {
        let root = grounding_fixture("ambiguous-edit");
        std::fs::write(root.join("src/index.html"), "<title>Welcome</title>\n<h1>Welcome</h1>").unwrap();
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"read_file","path":"src/index.html"}"#,
            r#"{"action":"propose_change","summary":"Change visible heading.","changes":[{"path":"src/index.html","old_text":"Welcome","new_text":"Changed"}]}"#,
            r#"{"action":"propose_change","summary":"Change visible heading.","changes":[{"path":"src/index.html","old_text":"<h1>Welcome</h1>","new_text":"<h1>Changed</h1>"}]}"#,
        ]);
        let response = tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "change the visible heading to Changed".into() }],
            Some(root.clone()), None, None, None,
        )).unwrap();
        let proposal = response.proposal.expect("unique retry should produce a proposal");
        assert_eq!(proposal.changes[0].after, "<title>Welcome</title>\n<h1>Changed</h1>");
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), "<title>Welcome</title>\n<h1>Welcome</h1>");
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ambiguous_edit_repair_is_bounded_and_never_selects_an_occurrence() {
        let root = grounding_fixture("bounded-ambiguous-edit");
        let original = "first value\nsecond value";
        std::fs::write(root.join("src/index.html"), original).unwrap();
        let ambiguous = r#"{"action":"propose_change","summary":"Change value.","changes":[{"path":"src/index.html","old_text":"value","new_text":"changed"}]}"#;
        let provider = StubProvider::new(&["test-model"], &[
            r#"{"action":"read_file","path":"src/index.html"}"#,
            ambiguous, ambiguous, ambiguous,
        ]);
        let error = match tauri::async_runtime::block_on(run_agent_with_provider(
            &provider, "test-model".into(), vec![ChatMessage { role: "user".into(), content: "change the second value".into() }],
            Some(root.clone()), None, None, None,
        )) { Err(error) => error, Ok(_) => panic!("ambiguous proposals must remain rejected") };
        assert_eq!(error, repository::AMBIGUOUS_OLD_TEXT_ERROR);
        assert_eq!(provider.inference_calls.load(Ordering::SeqCst), 4);
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), original);
        assert!(ambiguous_anchor_guidance().contains("matches more than one location"));
        assert!(ambiguous_anchor_guidance().contains("Do not invent text"));
        assert!(ambiguous_anchor_guidance().contains("displayed line numbers") && ambiguous_anchor_guidance().contains("guess"));
        assert_eq!(anchor_shape(&[ProposedReplacement { path: "x".into(), old_text: "10: repeated".into(), new_text: "x".into() }]), "1:chars=12,lines=1,displayedLinePrefix=true");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_or_oversized_proposals_get_minimal_guidance() {
        for raw in [
            r#"{"action":"propose_change","summary":"Update heading."}"#.to_owned(),
            format!(r#"{{"action":"propose_change","summary":"{}","changes":[{{"path":"x","old_text":"a","new_text":"b"}}]}}"#, "x".repeat(161)),
        ] {
            let error = parse_action(&raw).unwrap_err();
            assert!(error.contains("include changes"));
            assert!(error.contains("old_text"));
            assert!(error.contains("one short sentence"));
        }
        assert_eq!(agent_schema(true, true, true, false)["properties"]["summary"]["maxLength"], 160);
        let inspect_first = repair_instruction("invalid proposal", true, false);
        assert!(inspect_first.contains("Do not propose the edit yet"));
        assert!(inspect_first.contains("read_file"));
        assert!(inspect_first.contains("No summary, changes"));
        let repair = repair_instruction("response exceeded the model output limit", true, true);
        assert!(repair.contains("minimal propose_change"));
        assert!(repair.contains("Do not repeat rationale"));
    }

    #[test]
    fn tool_results_are_delivered_and_only_successes_are_cached() {
        let root = std::env::temp_dir().join(format!("aiide-tool-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("styles.css"), "body { color: red; }").unwrap();
        let root = root.canonicalize().unwrap();
        let mut cache = HashMap::new();

        let missing = ToolRequest { tool: "list_files".into(), path: "src".into(), query: String::new() };
        let (failed, _, reused) = execute_cached(&root, &missing, &mut cache);
        assert!(failed.starts_with("Error:"));
        assert!(!reused);
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/app.css"), "main {}").unwrap();
        let (recovered, _, reused) = execute_cached(&root, &missing, &mut cache);
        assert!(recovered.contains("src/app.css"));
        assert!(!reused, "failed calls must not be cached");
        let (cached, _, reused) = execute_cached(&root, &missing, &mut cache);
        assert!(reused);
        assert_eq!(cached, recovered);

        for request in [
            ToolRequest { tool: "list_files".into(), path: String::new(), query: String::new() },
            ToolRequest { tool: "read_file".into(), path: "styles.css".into(), query: String::new() },
            ToolRequest { tool: "search_files".into(), path: String::new(), query: "color: red".into() },
        ] {
            let (output, _, _) = execute_cached(&root, &request, &mut cache);
            assert!(useful_tool_result(&output));
            let delivered = tool_result_message(&request, true, &output, "", "");
            assert!(delivered.contains(&output), "actual bounded result must reach the next turn");
            assert!(delivered.contains("(success)"));
        }
        let unsupported = tool_result_message(&missing, false, "Error: missing", "", "");
        assert!(unsupported.contains("(no evidence)"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_actions() {
        for raw in [
            "not json",
            "Here is the answer: {\"action\":\"answer\",\"answer\":\"yes\"}",
            r#"{"action":"execute_command","path":"dir"}"#,
            r#"{"action":"search_files"}"#,
            r#"{"action":"search_files","query":" "}"#,
            r#"{"action":"read_file"}"#,
            r#"{"action":"read_file","path":""}"#,
            r#"{"action":"answer","answer":"yes","path":"x"}"#,
            r#"{"action":"answer","answer":""}"#,
            r#"{"action":"answer","answer":"yes","tool":"read_file"}"#,
        ] { assert!(parse_action(raw).is_err(), "accepted: {raw}"); }
    }

    #[test]
    fn scope_validation() {
        assert_eq!(parse_scope(r#"{"scope":"repository"}"#), Some(true));
        assert_eq!(parse_scope(r#"{"scope":"general"}"#), Some(false));
        assert_eq!(parse_scope(r#"{"scope":"unknown"}"#), None);
        assert_eq!(parse_scope("general"), None);
        assert_eq!(parse_scope(r#"{"scope":"general","tool":"read_file"}"#), None);
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b"]
    fn local_grounding_calibration_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/agent-benchmark"))
            .expect("agent benchmark fixture must exist");
        let response = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
            "qwen2.5-coder:7b".into(),
            vec![ChatMessage { role: "user".into(), content: "summarize index.html".into() }],
            Some(root), None, None, None,
        )).expect("named-file summary should complete");
        assert!(response.activity.iter().any(|item| item.label == "Read: src/index.html"));
        assert!(!response.content.to_ascii_lowercase().contains("provide the file"));
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b"]
    fn local_structured_edit_calibration_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/agent-benchmark"))
            .expect("agent benchmark fixture must exist");
        let path = root.join("src/index.html");
        let before = std::fs::read_to_string(&path).expect("fixture target must exist");
        let response = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
            "qwen2.5-coder:7b".into(),
            vec![ChatMessage { role: "user".into(), content: "change the main heading to \"Welcome to the AIIDE Sandbox\". make only that change and prepare it for review".into() }],
            Some(root), None, None, None,
        )).expect("structured edit should complete");
        let proposal = response.proposal.expect("edit must produce a validated proposal");
        assert_eq!(proposal.changes.len(), 1);
        assert_eq!(proposal.changes[0].path, "src/index.html");
        assert_eq!(proposal.changes[0].replacements, 1);
        assert_eq!(proposal.changes[0].before, before);
        assert_eq!(proposal.changes[0].after, before.replacen("<h1>OrbitNote</h1>", "<h1>Welcome to the AIIDE Sandbox</h1>", 1));
        assert_eq!(std::fs::read_to_string(path).unwrap(), before, "proposal must not write before Apply");
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b and aiide-sandbox"]
    fn local_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../aiide-sandbox"))
            .expect("aiide-sandbox must exist beside AIIDE");
        let prompts = [
            "What files are in this project and what does the project appear to be?",
            "Review this website as a front-end developer. Identify 3-5 meaningful UX, accessibility or code-quality improvements. Do not modify anything. Tell me which files you inspected.",
            "Why doesn't the navigation work on mobile?",
            "what is a javascript closure",
            "have a look at the css and tell me the one thing you'd improve first",
        ];
        for (index, prompt) in prompts.iter().enumerate() {
            if let Ok(selected) = std::env::var("AIIDE_ACCEPTANCE_CASE") {
                if selected != (index + 1).to_string() { continue; }
            }
            let debug_trace = Arc::new(Mutex::new(TraceBuffer::default()));
            let response = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
                "qwen2.5-coder:7b".into(),
                vec![ChatMessage { role: "user".into(), content: (*prompt).into() }],
                Some(root.clone()), None, None, Some(&debug_trace),
            )).expect("agent request should complete");
            println!("{}", debug_trace.lock().unwrap().lines.join("\n\n"));
            println!("case {} activity: {:?}; answer: {}", index + 1,
                response.activity.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), response.content);
            if index != 3 { assert!(!response.activity.is_empty(), "case {} did not inspect the repository", index + 1); }
            else { assert!(response.activity.is_empty(), "general question unnecessarily inspected repository"); }
            if index == 1 || index == 2 {
                for path in ["src/index.html", "styles.css", "script.js"] {
                    assert!(response.activity.iter().any(|item| item.label == format!("Read: {path}")), "case {} did not read {path}", index + 1);
                }
            }
            if index == 4 { assert!(response.activity.iter().any(|item| item.label == "Read: styles.css"), "CSS review did not read styles.css"); }
            assert!(!response.content.contains("inspection limit"));
        }
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b and clean aiide-sandbox"]
    fn local_proposal_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../aiide-sandbox"))
            .expect("aiide-sandbox must exist beside AIIDE");
        let before = std::fs::read_to_string(root.join("src/index.html")).expect("sandbox signup page must exist");
        let response = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
            "qwen2.5-coder:7b".into(),
            vec![ChatMessage { role: "user".into(), content: "Improve the signup form accessibility. Make a focused change and let me review it before anything is applied.".into() }],
            Some(root.clone()), None, None, None,
        )).expect("agent request should complete");
        let proposal = response.proposal.expect("agent should produce a validated proposal");
        assert_eq!(proposal.changes[0].path, "src/index.html");
        assert_ne!(proposal.changes[0].before, proposal.changes[0].after);
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), before, "proposal must not write");
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b and clean aiide-sandbox"]
    fn local_intent_sequence_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../aiide-sandbox"))
            .expect("aiide-sandbox must exist beside AIIDE");
        let path = root.join("src/index.html");
        let before = std::fs::read_to_string(&path).unwrap();
        let first_prompt = "tell me about this project and describe one change you'd make";
        let first_trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let first = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
            "qwen2.5-coder:7b".into(),
            vec![ChatMessage { role: "user".into(), content: first_prompt.into() }],
            Some(root.clone()), None, None, Some(&first_trace),
        )).expect("inspect/answer request should complete");
        println!("TEST A\n{}", first_trace.lock().unwrap().lines.join("\n\n"));
        assert!(first.proposal.is_none());
        assert!(!first.activity.is_empty());
        assert!(first.content.to_ascii_lowercase().contains("orbitnote"));

        let second_prompt = "change the main heading to \"Welcome to the AIIDE Sandbox\". make only that change and prepare it for review";
        let second_trace = Arc::new(Mutex::new(TraceBuffer::default()));
        let second = tauri::async_runtime::block_on(run_agent(OLLAMA_PROVIDER_ID,
            "qwen2.5-coder:7b".into(),
            vec![
                ChatMessage { role: "user".into(), content: first_prompt.into() },
                ChatMessage { role: "assistant".into(), content: first.content },
                ChatMessage { role: "user".into(), content: second_prompt.into() },
            ],
            Some(root), None, None, Some(&second_trace),
        )).expect("edit request should complete");
        println!("TEST B\n{}", second_trace.lock().unwrap().lines.join("\n\n"));
        let proposal = second.proposal.expect("edit request should create a proposal");
        assert!(proposal.summary.chars().count() <= 160);
        assert_eq!(proposal.changes[0].path, "src/index.html");
        assert_eq!(std::fs::read_to_string(path).unwrap(), before, "proposal must not write before Apply");
    }
}
