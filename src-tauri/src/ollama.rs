use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, State};
use crate::project::OpenProject;
use crate::repository::{self, Activity, PendingChanges, PendingProposal, ProposedReplacement, ToolRequest};

const BASE: &str = "http://127.0.0.1:11434";
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
struct TraceBuffer { lines: Vec<String>, next_turn: usize }
type Trace = Arc<Mutex<TraceBuffer>>;

fn debug_log(trace: Option<&Trace>, category: &str, message: impl AsRef<str>) {
    let Some(trace) = trace else { return };
    let line = format!("[AIIDE][{category}] {}", message.as_ref());
    eprintln!("{line}");
    if let Ok(mut buffer) = trace.lock() { buffer.lines.push(line); }
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
pub struct ModelInfo { id: String, name: String }

#[derive(Serialize)]
pub struct ProviderError { code: &'static str, message: &'static str }

#[derive(Deserialize)]
struct VersionResponse { version: String }

#[derive(Deserialize)]
struct TagsResponse { models: Vec<TaggedModel> }

#[derive(Deserialize)]
struct TaggedModel { name: String }

#[derive(Deserialize, Serialize)]
pub struct ChatMessage { role: String, content: String }

#[derive(Serialize)]
struct ChatPayload<'a> { model: &'a str, messages: &'a [ChatMessage], stream: bool, format: Value, options: ChatOptions }

#[derive(Serialize)]
struct ChatOptions { temperature: f32, num_predict: u16 }

#[derive(Deserialize)]
struct ChatPayloadResponse { model: String, message: ChatMessage, done: bool, done_reason: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse { model: String, content: String, activity: Vec<Activity>, proposal: Option<PendingProposal> }

const CORE_AGENT_INSTRUCTIONS: &str = "You are Elma, a local-first AI coding companion inside AIIDE. You may inspect the opened project through AIIDE's bounded tools and propose focused replacements to existing text files. You cannot apply changes, write files, execute commands, commit, or push. Inspect every target file with read_file before proposing a change. Preserve its style and avoid unrelated cleanup or whole-file rewrites. For a conversational response, return exactly {\"action\":\"answer\",\"answer\":\"<response>\"}; never put conversational answer text in summary. A propose_change action needs a concise summary and changes containing project-relative path, exact old_text copied from file content without the displayed line-number prefix, and replacement new_text. AIIDE validates and previews it; only the user's Apply button can write it. Never claim a proposal was applied. Questions and reviews may be answered without proposing changes. Never invent files, code, Git state, tool results, or commands. list_files returns exact paths; search_files searches one literal substring. Failed tools do not prove absence. Return exactly one JSON object matching the provided schema.";

const DEFAULT_PERSONALITY: &str = "Elma is calm, clever, trustworthy, and down-to-earth, with a cute exterior and a dry sense of humour. Sound moderately casual and task-focused. Occasional mild sarcasm, playful comments, and natural emoji are welcome when they do not obscure technical facts or errors. Lightly mirror the user's casual language without forcing slang or caricature. Be concise by default: give the shortest complete answer, usually a few sentences for simple questions. Start with the answer; do not restate the question or add generic introductions, conclusions, or unnecessary headings. Assume normal software-development basics, explain important details briefly, and expand only when useful or requested.";

const MAX_REPAIRS: usize = 2;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AgentReply { action: String, path: Option<String>, query: Option<String>, answer: Option<String>, summary: Option<String>, changes: Option<Vec<ProposedReplacement>> }

#[derive(Debug, PartialEq)]
enum AgentAction { List(String), Search(String), Read(String), Answer(String), Propose(String, Vec<ProposedReplacement>) }

fn agent_schema(project_open: bool, allow_proposal: bool) -> Value {
    let actions = if !project_open { json!(["answer"]) }
        else if allow_proposal { json!(["list_files","search_files","read_file","answer","propose_change"]) }
        else { json!(["list_files","search_files","read_file","answer"]) };
    json!({"type":"object","properties":{
        "action":{"type":"string","enum":actions,"description":"Choose answer for conversational responses. For action=answer, use the answer field and never summary."},
        "path":{"type":"string"},"query":{"type":"string"},
        "answer":{"type":"string","description":"Required conversational response text when action is answer."},
        "summary":{"type":"string","description":"A concise change summary for propose_change only. Never use this for action=answer."},
        "changes":{"type":"array","maxItems":4,"items":{"type":"object","properties":{"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}},"required":["path","old_text","new_text"],"additionalProperties":false}}
    },"required":["action"],"additionalProperties":false})
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
        "propose_change" if reply.path.is_none() && reply.query.is_none() && reply.answer.is_none() && reply.summary.as_ref().is_some_and(|value| !value.trim().is_empty()) && reply.changes.as_ref().is_some_and(|value| !value.is_empty()) => Ok(AgentAction::Propose(reply.summary.unwrap(), reply.changes.unwrap())),
        "propose_change" => Err("propose_change requires summary and one or more exact changes"),
        _ => Err("invalid or ambiguous action fields"),
    }
}

async fn chat_turn(client: &reqwest::Client, model: &str, messages: &[ChatMessage], format: Value, temperature: f32, stage: &str, trace: Option<&Trace>) -> Result<ChatPayloadResponse, String> {
    let turn = trace_turn(trace);
    debug_log(trace, "protocol", format!("Turn {turn} — {stage}\nTemperature: {temperature}\nStructured format supplied: yes\nMessage count: {}\nMessage roles: {}\nSchema/format: {}", messages.len(), messages.iter().map(|message| message.role.as_str()).collect::<Vec<_>>().join(", "), format));
    if trace.is_some() {
        for (index, message) in messages.iter().enumerate() {
            debug_log(trace, "agent", format!("Message {} ({})\n--- MESSAGE START ---\n{}\n--- MESSAGE END ---", index + 1, message.role, message.content));
        }
    }
    let started = Instant::now();
    debug_log(trace, "ollama", format!("Turn {turn} request started"));
    let response = client.post(format!("{BASE}/api/chat"))
        .json(&ChatPayload { model, messages, stream: false, format, options: ChatOptions { temperature, num_predict: 2_048 } })
        .send().await.map_err(request_error)?;
    if !response.status().is_success() { return Err("Ollama could not complete the chat request. Try again.".into()); }
    let result = response.json::<ChatPayloadResponse>().await.map_err(|_| "Ollama returned an invalid chat response.".to_owned())?;
    debug_log(trace, "ollama", format!("Turn {turn} response received in {}ms\n--- RAW OLLAMA RESPONSE START ---\n{}\n--- RAW OLLAMA RESPONSE END ---\nMetadata: model={}, done={}, done_reason={:?}", started.elapsed().as_millis(), result.message.content, result.model, result.done, result.done_reason));
    if !result.done || result.message.role != "assistant" || result.model.is_empty() { return Err("Ollama returned an incomplete chat response.".into()); }
    Ok(result)
}

async fn agent_turn(client: &reqwest::Client, model: &str, exchange: &mut Vec<ChatMessage>, project_open: bool, allow_proposal: bool, app: Option<&tauri::AppHandle>, trace: Option<&Trace>) -> Result<(ChatPayloadResponse, AgentAction), String> {
    for attempt in 0..=MAX_REPAIRS {
        let stage = if attempt == 0 { "STRUCTURED".to_owned() } else { format!("REPAIR {attempt}/{MAX_REPAIRS}") };
        let result = chat_turn(client, model, exchange, agent_schema(project_open, allow_proposal), PROTOCOL_TEMPERATURE, &stage, trace).await?;
        let parsed = if result.done_reason.as_deref() == Some("length") { Err("response exceeded the model output limit") }
            else { parse_action(&result.message.content) };
        match parsed {
            Ok(action) => { debug_log(trace, "protocol", format!("JSON parsing: PASSED\nSchema validation: PASSED\nSemantic validation: PASSED\nSelected action: {}", action_name(&action))); return Ok((result, action)); }
            Err(reason) => {
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
                let repair = format!("Your previous response was invalid ({reason}). Return exactly one JSON object matching the schema. No prose or markdown.");
                debug_log(trace, "repair", format!("Attempt {}/{}\nOriginal failure: {reason}\n--- REPAIR INSTRUCTION START ---\n{repair}\n--- REPAIR INSTRUCTION END ---", attempt + 1, MAX_REPAIRS));
                exchange.push(ChatMessage { role: "user".into(), content: repair });
            }
        }
    }
    Err("The local model could not produce a valid structured response after two retries.".into())
}

fn action_name(action: &AgentAction) -> &'static str {
    match action { AgentAction::List(_) => "list_files", AgentAction::Search(_) => "search_files", AgentAction::Read(_) => "read_file", AgentAction::Answer(_) => "answer", AgentAction::Propose(_, _) => "propose_change" }
}

async fn requires_inspection(client: &reqwest::Client, model: &str, prompt: &str, trace: Option<&Trace>) -> Result<bool, String> {
    let mut messages = vec![
        ChatMessage { role: "system".into(), content: "Classify whether answering this user prompt requires inspecting the currently opened project. Use repository for questions about this project's behavior, structure, UI, bugs, or improvements, including indirect references like 'the menu'. Use general for conceptual questions such as 'What is a JavaScript closure?'. Return only a JSON object with scope general or repository.".into() },
        ChatMessage { role: "user".into(), content: prompt.into() },
    ];
    for attempt in 0..=MAX_REPAIRS {
        let result = chat_turn(client, model, &messages, scope_schema(), PROTOCOL_TEMPERATURE, "SCOPE CLASSIFICATION", trace).await?;
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

async fn final_turn(client: &reqwest::Client, model: &str, prompt: &str, project_info: &str, evidence: &[(String, String)], read_paths: &[String], trace: Option<&Trace>) -> Result<String, String> {
    let mut exchange = vec![
        ChatMessage { role: "system".into(), content: format!("You are Elma inside AIIDE. {project_info} Answer the user's original question using only the repository evidence below. Do not respond to a prior tool search or infer missing files from a failed tool. Do not invent filenames: HTML sections are not separate files. Do not list inspected filenames in your answer; the application appends the verified list. No more tools are available. Keep the answer under 120 words and address every part of the user's request. Avoid quoted code snippets or HTML attributes. Return only a JSON object with action answer and answer text.") },
        ChatMessage { role: "system".into(), content: DEFAULT_PERSONALITY.into() },
        ChatMessage { role: "user".into(), content: format!("Files actually read: {}\n\nRepository evidence:\n{}\n\nOriginal user request: {prompt}\n\nAnswer this original request directly, using the evidence above. If it asks for a review, give exactly three concrete improvements. For each, say what to change, which of the actual files would be affected, and why. Avoid speculative claims, generic advice, invented files, code snippets, and unverified accessibility findings. The app separately reports inspected files.", read_paths.join(", "), evidence.iter().map(|(name, value)| format!("[{name}]\n{value}")).collect::<Vec<_>>().join("\n\n")) },
    ];
    for attempt in 0..=MAX_REPAIRS {
        debug_log(trace, "final", "Grounded personality answer started; personality included: yes");
        let result = chat_turn(client, model, &exchange, answer_schema(), CONVERSATIONAL_TEMPERATURE, "GROUNDED FINAL ANSWER", trace).await?;
        if result.done_reason.as_deref() != Some("length") {
            if let Ok(AgentAction::Answer(answer)) = parse_action(&result.message.content) { debug_log(trace, "final", "Grounded personality answer accepted"); return Ok(answer); }
        }
        if attempt == MAX_REPAIRS { break; }
        exchange.last_mut().unwrap().content.push_str("\n\nYour previous response was invalid or too long. Answer the ORIGINAL REQUEST above in at most 90 words. For a review, give three short numbered improvements with actual affected files and reasons. Return only the JSON answer object.");
    }
    Err("The local model could not produce a final structured answer after two retries.".into())
}

async fn conversational_turn(client: &reqwest::Client, model: &str, prompt: &str, draft: &str, trace: Option<&Trace>) -> Result<String, String> {
    let mut exchange = vec![
        ChatMessage { role: "system".into(), content: format!("You are Elma inside AIIDE. {DEFAULT_PERSONALITY} Preserve the draft's factual meaning. Return only a JSON object with action answer and answer text.") },
        ChatMessage { role: "user".into(), content: format!("Original question: {prompt}\n\nDraft answer: {draft}\n\nGive the shortest complete answer in Elma's natural voice. Do not add facts that are absent from the draft.") },
    ];
    for attempt in 0..=MAX_REPAIRS {
        debug_log(trace, "final", "Personality rewrite started; personality included: yes");
        let result = chat_turn(client, model, &exchange, answer_schema(), CONVERSATIONAL_TEMPERATURE, "PERSONALITY REWRITE", trace).await?;
        if result.done_reason.as_deref() != Some("length") {
            if let Ok(AgentAction::Answer(answer)) = parse_action(&result.message.content) { debug_log(trace, "final", "Personality rewrite accepted"); return Ok(answer); }
        }
        if attempt == MAX_REPAIRS { break; }
        exchange.last_mut().unwrap().content.push_str("\n\nReturn one valid JSON answer object in at most 80 words.");
    }
    Err("The local model could not produce a concise conversational answer after two retries.".into())
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder().no_proxy().timeout(timeout).build()
        .map_err(|_| "Could not prepare the local Ollama connection.".to_owned())
}

fn request_error(error: reqwest::Error) -> String {
    if error.is_timeout() { "Ollama took too long to respond. Try again.".to_owned() }
    else if error.is_connect() { "Ollama is offline. Start Ollama and retry.".to_owned() }
    else { "The local Ollama request failed. Try again.".to_owned() }
}

#[tauri::command]
pub async fn ollama_status() -> ProviderStatus {
    let offline = || ProviderStatus { state: "offline", models: vec![], error: Some(ProviderError { code: "unavailable", message: "Ollama not detected. Start Ollama and try again." }) };
    let Ok(client) = client(Duration::from_secs(4)) else { return offline() };
    let Ok(response) = client.get(format!("{BASE}/api/version")).send().await else { return offline() };
    if !response.status().is_success() { return offline(); }
    let Ok(version) = response.json::<VersionResponse>().await else { return offline() };
    if version.version.is_empty() { return offline(); }
    let Ok(response) = client.get(format!("{BASE}/api/tags")).send().await else {
        return ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "model_list_failed", message: "Ollama is connected, but its model list could not be loaded." }) };
    };
    if !response.status().is_success() {
        return ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "model_list_failed", message: "Ollama is connected, but its model list could not be loaded." }) };
    }
    match response.json::<TagsResponse>().await {
        Ok(tags) => ProviderStatus { state: "connected", models: tags.models.into_iter().filter(|model| !model.name.is_empty()).map(|model| ModelInfo { id: model.name.clone(), name: model.name }).collect(), error: None },
        Err(_) => ProviderStatus { state: "error", models: vec![], error: Some(ProviderError { code: "malformed_response", message: "Ollama returned an invalid model list." }) },
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
            (state.next_request, Some(Arc::new(Mutex::new(TraceBuffer::default()))))
        } else { (0, None) }
    };
    if let Some(trace) = trace.as_ref() {
        let started_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis()).unwrap_or(0);
        debug_log(Some(trace), "agent", format!("==================================================\nAIIDE AGENT DEBUG TRACE\nTrace version: 1\nRequest: #{request}\nStarted at Unix ms: {started_at}\nModel: {model}\nProject: {}\nCore agent instructions: included on agent turns\nDefault personality: included only on final conversational turns\n==================================================", root.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy()).unwrap_or_else(|| "none".into())));
    }
    let result = run_agent(model, messages, root, Some(&pending), Some(&app), trace.as_ref()).await;
    if let Some(trace) = trace {
        if let Err(error) = &result { debug_log(Some(&trace), "final", format!("FAILED\n{error}")); }
        debug_log(Some(&trace), "final", format!("Result: {}\nTotal duration: {}ms\n==================================================", if result.is_ok() { "SUCCESS" } else { "FAILED" }, started.elapsed().as_millis()));
        let report = trace.lock().map(|buffer| buffer.lines.join("\n\n")).unwrap_or_else(|_| "Trace unavailable".into());
        let mut state = debug.0.lock().map_err(|_| "Debug state unavailable")?;
        if state.enabled { state.latest = Some(report); }
    }
    result
}

async fn run_agent(model: String, messages: Vec<ChatMessage>, root: Option<std::path::PathBuf>, pending: Option<&PendingChanges>, app: Option<&tauri::AppHandle>, debug_trace: Option<&Trace>) -> Result<ChatResponse, String> {
    if model.is_empty() || messages.is_empty() || messages.len() > MAX_MESSAGES || messages.iter().any(|message| {
        !matches!(message.role.as_str(), "user" | "assistant") || message.content.is_empty() || message.content.chars().count() > MAX_MESSAGE_CHARS
    }) || messages.last().is_none_or(|message| message.role != "user") {
        return Err("The chat request is invalid or too long.".to_owned());
    }
    let client = client(Duration::from_secs(120))?;
    let tags = client.get(format!("{BASE}/api/tags")).send().await.map_err(request_error)?;
    if !tags.status().is_success() { return Err("Could not verify installed models. Retry the connection.".to_owned()); }
    let tags = tags.json::<TagsResponse>().await.map_err(|_| "Ollama returned an invalid model list.".to_owned())?;
    if !tags.models.iter().any(|item| item.name == model) { return Err("This model is no longer installed. Retry to refresh the model list.".to_owned()); }
    let last_prompt = messages.last().map_or("", |message| message.content.trim()).to_owned();
    if last_prompt.eq_ignore_ascii_case("what is your name?") || last_prompt.eq_ignore_ascii_case("what is your name") {
        return Ok(ChatResponse { model, content: "I'm Elma, your local coding companion in AIIDE. My responses are generated by the selected Ollama model.".into(), activity: vec![], proposal: None });
    }
    if last_prompt.eq_ignore_ascii_case("do you have access to my files?") || last_prompt.eq_ignore_ascii_case("do you have access to my files") {
        return Ok(ChatResponse { model, content: if root.is_some() { "I can inspect this project and prepare focused changes for your review. Only the Apply button can write them." } else { "No project is open, so I cannot inspect or propose changes to files." }.into(), activity: vec![], proposal: None });
    }
    let mut exchange = Vec::new();
    exchange.push(ChatMessage { role: "system".into(), content: CORE_AGENT_INSTRUCTIONS.into() });
    if let Some(ref root) = root {
        let info = super::project::inspect_metadata(root);
        exchange.push(ChatMessage { role: "system".into(), content: info });
    } else {
        exchange.push(ChatMessage { role: "system".into(), content: "No project is open. Repository tools are unavailable; answer normal chat directly.".into() });
    }
    exchange.extend(messages.into_iter().rev().take(12).collect::<Vec<_>>().into_iter().rev());
    let mut activity = Vec::new();
    let mut seen = HashSet::new();
    let mut context_bytes = 0;
    let mut successful_inspections = 0;
    let mut successful_reads = 0;
    let mut read_paths = Vec::new();
    let mut evidence: Vec<(String, String)> = Vec::new();
    let mut consecutive_repeats = 0;
    let mut unresolved_failure = false;
    let mut known_paths = String::new();
    let mut known_file_paths: Vec<String> = Vec::new();
    let mut needs_inspection: Option<bool> = None;
    let mut inspection_reminders = 0;
    for iteration in 0..=repository::MAX_TOOL_CALLS {
        let (result, action) = agent_turn(&client, &model, &mut exchange, root.is_some(), successful_reads > 0, app, debug_trace).await?;
        if let AgentAction::Propose(summary, edits) = action {
            let root = root.as_ref().ok_or_else(|| "Open a project before proposing changes.".to_owned())?;
            if edits.iter().any(|edit| !read_paths.contains(&edit.path)) {
                exchange.push(result.message);
                exchange.push(ChatMessage { role: "user".into(), content: "Every proposed target must first be inspected with read_file. Inspect the exact target path, then propose the focused replacement.".into() });
                continue;
            }
            if let Some(app) = app { let _ = app.emit("proposal-validation-start", ()); }
            debug_log(debug_trace, "tool", format!("propose_change selected\nPath: {}\nReplacements: {}\nValidation: started", edits.first().map(|edit| edit.path.as_str()).unwrap_or("none"), edits.len()));
            let proposal = repository::validate_proposal(root, summary, edits)?;
            if let Some(pending) = pending {
                *pending.0.lock().map_err(|_| "Pending change state unavailable")? = Some(proposal.clone());
            }
            debug_log(debug_trace, "tool", "Proposal validation: passed\nPending change creation: passed");
            return Ok(ChatResponse { model: result.model, content: "Done — have a wee look in Changes 👀".into(), activity, proposal: Some(proposal) });
        }
        if let AgentAction::Answer(answer) = action {
            if root.is_some() && successful_reads == 0 {
                let needed = match needs_inspection {
                    Some(needed) => needed,
                    None => {
                        let needed = requires_inspection(&client, &model, &last_prompt, debug_trace).await?;
                        needs_inspection = Some(needed);
                        needed
                    }
                };
                if needed {
                    if inspection_reminders < 2 {
                        inspection_reminders += 1;
                        exchange.push(result.message);
                        exchange.push(ChatMessage { role: "user".into(), content: "This question needs evidence from the opened project. Use list_files to find exact paths, then read_file on relevant files before answering. A failed tool or no matches does not prove files are absent.".into() });
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
                let answer = final_turn(&client, &model, &last_prompt, &info, &evidence, &read_paths, debug_trace).await?;
                format!("{answer}\n\nFiles actually inspected: {}", read_paths.join(", "))
            } else { conversational_turn(&client, &model, &last_prompt, &answer, debug_trace).await? };
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
        let key = format!("{}|{}|{}", request.tool, request.path, request.query);
        let repeated = seen.contains(&key);
        if !repeated {
            if let Some(app) = app { let _ = app.emit("repository-inspection-start", ()); }
        }
        let tool_started = Instant::now();
        debug_log(debug_trace, "tool", format!("{} requested\nPath: {}\nQuery: {}", request.tool, request.path, request.query));
        let (output, event) = if seen.insert(key) { repository::execute(root.as_deref().unwrap(), &request) }
            else { ("This tool request was already answered in this turn; use the earlier result.".into(), Activity { label: "Repeated inspection skipped".into() }) };
        let remaining = repository::MAX_CONTEXT_BYTES.saturating_sub(context_bytes);
        if remaining == 0 { break; }
        let output = output.chars().scan(0_usize, |used, character| {
            let next = *used + character.len_utf8();
            if next > remaining { None } else { *used = next; Some(character) }
        }).collect::<String>();
        context_bytes += output.len();
        if repeated { consecutive_repeats += 1; } else { consecutive_repeats = 0; }
        let useful = !output.starts_with("Error:") && output != "No matches." && event.label != "Repeated inspection skipped";
        if request.tool == "list_files" && useful && (request.path.is_empty() || request.path == ".") {
            known_paths = output.lines().take(120).map(|line| line.split(" (").next().unwrap_or(line)).collect::<Vec<_>>().join(", ");
            known_file_paths = output.lines().filter_map(|line| line.strip_suffix(" (file)").filter(|path| !std::path::Path::new(path).file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'))).map(str::to_owned)).collect();
        }
        if useful {
            successful_inspections += 1;
            if request.tool == "read_file" { successful_reads += 1; read_paths.push(request.path.clone()); }
            evidence.push((format!("{} {}", request.tool, request.path), output.clone()));
            unresolved_failure = false;
        } else { unresolved_failure = true; }
        debug_log(debug_trace, "tool", format!("{}\nValidation/execution: {}\nReturned: {} bytes\nDuration: {}ms\nNext stage: agent turn", request.tool, if output.starts_with("Error:") { &output } else { "passed" }, output.len(), tool_started.elapsed().as_millis()));
        if let Some(app) = app { let _ = app.emit("repository-activity", &event); }
        activity.push(event);
        exchange.push(result.message);
        let unread = known_file_paths.iter().filter(|path| !read_paths.contains(path)).cloned().collect::<Vec<_>>().join(", ");
        exchange.push(ChatMessage { role: "user".into(), content: format!("Tool result for {} ({}):\n{}\n{}{}{}", request.tool, if useful { "success" } else { "no evidence" }, output,
            if useful && request.tool == "read_file" { "The target file is now inspected. If the original request asks you to implement, fix, add, remove, or otherwise change code, use propose_change with focused exact old_text/new_text now. Otherwise continue inspection or answer. Use exact listed paths." }
            else if useful { "Continue with another structured tool request or a grounded final answer. Use exact listed paths." }
            else { "This result does not establish that files are absent. Try list_files or another exact project-relative path before answering. If this request was repeated, choose an unread listed file." },
            if !known_paths.is_empty() { format!("\nExact paths from the project listing: {known_paths}") } else { String::new() },
            if !unread.is_empty() { format!("\nListed files not yet read: {unread}") } else { String::new() }) });
        if consecutive_repeats >= 2 { break; }
    }
    if !read_paths.is_empty() {
        let info = root.as_ref().map(|path| super::project::inspect_metadata(path)).unwrap_or_default();
        let answer = final_turn(&client, &model, &last_prompt, &info, &evidence, &read_paths, debug_trace).await?;
        let content = format!("{answer}\n\nFiles actually inspected: {}", read_paths.join(", "));
        return Ok(ChatResponse { model, content, activity, proposal: None });
    }
    Ok(ChatResponse { model, content: "I reached the repository inspection limit before I could read a relevant file. Please ask a narrower question.".into(), activity, proposal: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personality_is_separate_from_protocol_defaults() {
        assert!(CORE_AGENT_INSTRUCTIONS.contains("cannot apply changes"));
        assert!(CORE_AGENT_INSTRUCTIONS.contains("exact old_text"));
        assert!(DEFAULT_PERSONALITY.contains("concise by default"));
        assert!(DEFAULT_PERSONALITY.contains("dry sense of humour"));
        assert!(!DEFAULT_PERSONALITY.contains("propose_change"));
        assert_eq!(PROTOCOL_TEMPERATURE, 0.0);
        assert_eq!(CONVERSATIONAL_TEMPERATURE, 0.2);
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
        let schema = agent_schema(false, false);
        assert!(schema["properties"]["answer"]["description"].as_str().unwrap().contains("Required conversational response"));
        assert!(schema["properties"]["summary"]["description"].as_str().unwrap().contains("Never use this for action=answer"));
    }

    #[test]
    fn agent_action_availability_matches_project_and_inspection_state() {
        let actions = |project_open, allow_proposal| agent_schema(project_open, allow_proposal)["properties"]["action"]["enum"].as_array().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(actions(false, false), vec!["answer"]);
        assert!(!actions(true, false).contains(&"propose_change".to_owned()));
        assert!(actions(true, true).contains(&"propose_change".to_owned()));
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
    #[ignore = "requires local Ollama with qwen2.5-coder:7b and aiide-sandbox"]
    fn local_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../aiide-sandbox"))
            .expect("aiide-sandbox must exist beside AIIDE");
        let prompts = [
            "What files are in this project and what does the project appear to be?",
            "Review this website as a front-end developer. Identify 3-5 meaningful UX, accessibility or code-quality improvements. Do not modify anything. Tell me which files you inspected.",
            "Why doesn't the navigation work on mobile?",
            "what is a javascript closure",
        ];
        for (index, prompt) in prompts.iter().enumerate() {
            if let Ok(selected) = std::env::var("AIIDE_ACCEPTANCE_CASE") {
                if selected != (index + 1).to_string() { continue; }
            }
            let debug_trace = Arc::new(Mutex::new(TraceBuffer::default()));
            let response = tauri::async_runtime::block_on(run_agent(
                "qwen2.5-coder:7b".into(),
                vec![ChatMessage { role: "user".into(), content: (*prompt).into() }],
                Some(root.clone()), None, None, Some(&debug_trace),
            )).expect("agent request should complete");
            println!("{}", debug_trace.lock().unwrap().lines.join("\n\n"));
            println!("case {} activity: {:?}; answer: {}", index + 1,
                response.activity.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), response.content);
            if index < 3 { assert!(!response.activity.is_empty(), "case {} did not inspect the repository", index + 1); }
            else { assert!(response.activity.is_empty(), "general question unnecessarily inspected repository"); }
            if index == 1 || index == 2 {
                for path in ["src/index.html", "styles.css", "script.js"] {
                    assert!(response.activity.iter().any(|item| item.label == format!("Read: {path}")), "case {} did not read {path}", index + 1);
                }
            }
            assert!(!response.content.contains("inspection limit"));
        }
    }

    #[test]
    #[ignore = "requires local Ollama with qwen2.5-coder:7b and clean aiide-sandbox"]
    fn local_proposal_acceptance() {
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../aiide-sandbox"))
            .expect("aiide-sandbox must exist beside AIIDE");
        let before = std::fs::read_to_string(root.join("src/index.html")).expect("sandbox signup page must exist");
        let response = tauri::async_runtime::block_on(run_agent(
            "qwen2.5-coder:7b".into(),
            vec![ChatMessage { role: "user".into(), content: "Improve the signup form accessibility. Make a focused change and let me review it before anything is applied.".into() }],
            Some(root.clone()), None, None, None,
        )).expect("agent request should complete");
        let proposal = response.proposal.expect("agent should produce a validated proposal");
        assert_eq!(proposal.changes[0].path, "src/index.html");
        assert_ne!(proposal.changes[0].before, proposal.changes[0].after);
        assert_eq!(std::fs::read_to_string(root.join("src/index.html")).unwrap(), before, "proposal must not write");
    }
}
