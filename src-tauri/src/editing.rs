//! Provider-neutral ownership and contracts for application-led editing.
//!
//! Model providers receive bounded candidate views and return these semantic
//! result shapes. Candidate IDs, source ranges, validation, and proposal bytes
//! remain owned by AIIDE's repository layer.

use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;
use tauri::Emitter;
use crate::model_provider::ModelMessage;

use crate::repository;
use crate::repository::candidates::CandidateRole;
use crate::repository::candidates::{CandidateRegistry, CandidateView};

pub(crate) struct StructuredResponse { pub content: String, pub done_reason: Option<String> }

/// AIIDE semantic orchestration depends on structured inference, not a provider protocol.
pub(crate) trait SemanticInference {
    async fn infer_structured(&self, model: &str, messages: &[ModelMessage], schema: Value, stage: &str) -> Result<StructuredResponse, String>;
    fn log(&self, category: &str, message: &str);
}

#[derive(Debug)]
pub(crate) enum SelectionOutcome { Selected(CandidateView), Ambiguous, NoMatch }

pub(crate) struct SemanticEditResponse { pub content: String, pub activity: Vec<repository::Activity>, pub proposal: Option<repository::PendingProposal> }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RequestIntent { Answer, Edit }

pub(crate) fn classify_request_intent(prompt: &str) -> RequestIntent {
    let text = prompt.trim().to_ascii_lowercase();
    if text.split(['.', '!', '?', ';', ',', '\n']).any(is_edit_clause) { RequestIntent::Edit } else { RequestIntent::Answer }
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
        || ["make this change", "make only that change", "prepare this change for review", "prepare it for review", "propose this change for review", "propose the change for review"]
            .iter().any(|phrase| clause.starts_with(phrase))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SemanticEditRoute {
    ReplaceCandidate { role: CandidateRole },
    InsertRelative { anchor: CandidateRole, position: repository::CandidatePosition, element: repository::InsertedElement },
}

/// Semantic covers the supported HTML primitives. Other exact-source edits stay
/// on the guarded Legacy path; unbounded structural requests are Unsupported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EditDispatch { NotEdit, Semantic(SemanticEditRoute), Legacy, Unsupported }

pub(crate) fn dispatch_edit_request(prompt: &str, intent: RequestIntent) -> EditDispatch {
    if intent != RequestIntent::Edit { return EditDispatch::NotEdit; }
    let lower = prompt.trim().to_ascii_lowercase();
    let insertion_verbs = lower.split(|character: char| !character.is_ascii_alphanumeric()).filter(|word| !word.is_empty()).collect::<Vec<_>>();
    if insertion_verbs.iter().any(|word| matches!(*word, "add" | "insert" | "append" | "create")) {
        if insertion_verbs.iter().any(|word| matches!(*word, "append" | "create")) { return EditDispatch::Unsupported; }
        let relations = [("after", repository::CandidatePosition::After), ("below", repository::CandidatePosition::After), ("beneath", repository::CandidatePosition::After), ("before", repository::CandidatePosition::Before), ("above", repository::CandidatePosition::Before)];
        let Some((position_at, relation, position)) = relations.iter().filter_map(|(relation, position)| lower.find(relation).map(|at| (at, *relation, *position))).min_by_key(|(at, _, _)| *at) else {
            return EditDispatch::Unsupported;
        };
        let content_request = &lower[..position_at];
        let anchor_request = &lower[position_at + relation.len()..];
        let element = if content_request.contains("paragraph") { Some(repository::InsertedElement::Paragraph) }
            else if content_request.contains("link") { Some(repository::InsertedElement::Link) }
            else if content_request.contains("heading") { Some(repository::InsertedElement::Heading) }
            else { None };
        let anchor = if anchor_request.contains("heading") { Some(CandidateRole::HeadingOne) }
            else if anchor_request.contains("paragraph") { Some(CandidateRole::Paragraph) }
            else if anchor_request.contains("form") { Some(CandidateRole::Form) }
            else { None };
        return match (anchor, element) {
            (Some(anchor), Some(element)) => EditDispatch::Semantic(SemanticEditRoute::InsertRelative { anchor, position, element }),
            _ => EditDispatch::Unsupported,
        };
    }
    let attribute_role = if lower.contains("aria-label") { Some(CandidateRole::AttributeAriaLabel) }
    else if lower.contains("placeholder") { Some(CandidateRole::AttributePlaceholder) }
    else if lower.contains("alt text") || lower.contains("image alt") { Some(CandidateRole::AttributeAlt) }
    else if lower.contains("image source") || lower.contains("src attribute") || lower.contains("image src")
        || lower.split_whitespace().any(|word| word == "src") { Some(CandidateRole::AttributeSrc) }
    else if lower.contains("href") || (lower.contains("link") && ["point to", "destination", "url"].iter().any(|term| lower.contains(term))) { Some(CandidateRole::AttributeHref) }
    else if lower.contains("title attribute") || lower.contains("tooltip") { Some(CandidateRole::AttributeTitle) }
    else { None };
    let role = if let Some(role) = attribute_role { role }
    else if ["main page heading", "main heading", "visible h1", "visible heading"].iter().any(|target| lower.contains(target)) {
        CandidateRole::HeadingOne
    } else if ["document title", "page title", "browser title"].iter().any(|target| lower.contains(target)) {
        CandidateRole::DocumentTitle
    } else if ["paragraph", "intro text", "introductory text"].iter().any(|target| lower.contains(target)) {
        CandidateRole::Paragraph
    } else if ["button text", "button label", "button caption"].iter().any(|target| lower.contains(target)) {
        CandidateRole::Button
    } else if ["link text", "anchor text"].iter().any(|target| lower.contains(target)) {
        CandidateRole::Link
    } else if ["label", "label text"].iter().any(|target| lower.contains(target)) {
        CandidateRole::Label
    } else if ["list item", "list-item", "bullet text"].iter().any(|target| lower.contains(target)) {
        CandidateRole::ListItem
    } else { return EditDispatch::Legacy; };
    let structural = ["preserve", "retain", "span", "markup", "<h1", "html", " tag", " class", "style", "color", "background", "font", "size", "alignment", "spacing", "format", "bold", "emphasis"]
        .iter().any(|term| lower.contains(term));
    let direct_replacement = ["change", "replace", "rename", "set", "update"].iter().any(|verb| lower.contains(verb))
        && (lower.contains(" to ") || lower.contains(" with "));
    let attribute_keyword_without_attribute_target = !role.is_attribute() && lower.contains("attribute");
    if structural || attribute_keyword_without_attribute_target || !direct_replacement { EditDispatch::Unsupported }
    else { EditDispatch::Semantic(SemanticEditRoute::ReplaceCandidate { role }) }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CandidateSelectionResult { Selected, Ambiguous, NoMatch }

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidateSelection { pub result: CandidateSelectionResult, pub candidate_id: Option<String> }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplacementGeneration { pub replacement: String }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InsertionGeneration { pub element_type: String, pub text: String, pub href: Option<String> }

fn selection_schema() -> Value {
    json!({"type":"object","properties":{
        "result":{"type":"string","enum":["selected","ambiguous","no_match"]},
        "candidate_id":{"type":"string","description":"Required only for selected; copy one listed ID exactly."}
    },"required":["result"],"additionalProperties":false})
}

fn replacement_schema() -> Value {
    json!({"type":"object","properties":{
        "replacement":{"type":"string","minLength":1,"maxLength":repository::MAX_PROPOSAL_BYTES}
    },"required":["replacement"],"additionalProperties":false})
}

fn insertion_schema(element: repository::InsertedElement) -> Value {
    let element_type = match element {
        repository::InsertedElement::Paragraph => "paragraph",
        repository::InsertedElement::Heading => "heading",
        repository::InsertedElement::Link => "link",
    };
    let mut properties = json!({
        "element_type":{"type":"string","enum":[element_type]},
        "text":{"type":"string","minLength":1,"maxLength":repository::candidates::MAX_CANDIDATE_SOURCE_BYTES}
    });
    let mut required = vec!["element_type", "text"];
    if element == repository::InsertedElement::Link {
        properties["href"] = json!({"type":"string","minLength":1,"maxLength":repository::candidates::MAX_CANDIDATE_SOURCE_BYTES});
        required.push("href");
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub(crate) fn resolve_selection(raw: &str, registry: &CandidateRegistry, root: &std::path::Path) -> Result<SelectionOutcome, String> {
    let reply: CandidateSelection = serde_json::from_str(raw.trim()).map_err(|_| "Candidate selection response was malformed.")?;
    match (reply.result, reply.candidate_id) {
        (CandidateSelectionResult::Selected, Some(id)) if !id.is_empty() => {
            let candidate = registry.verify_current(root, &id)?;
            let view = registry.model_view().into_iter().find(|view| view.id == candidate.id()).ok_or("Unknown candidate ID.")?;
            Ok(SelectionOutcome::Selected(view))
        }
        (CandidateSelectionResult::Ambiguous, None) => Ok(SelectionOutcome::Ambiguous),
        (CandidateSelectionResult::NoMatch, None) => Ok(SelectionOutcome::NoMatch),
        _ => Err("Candidate selection response was malformed.".into()),
    }
}

pub(crate) async fn select_candidate(inference: &impl SemanticInference, model: &str, prompt: &str, root: &std::path::Path, registry: &CandidateRegistry, required_role: Option<CandidateRole>) -> Result<SelectionOutcome, String> {
    let views = registry.model_view().into_iter().filter(|view| required_role.is_none_or(|role| view.role == role)).collect::<Vec<_>>();
    let candidates = serde_json::to_string(&views).map_err(|_| "Could not prepare candidate views.")?;
    let schema = selection_schema();
    let messages = [
        ModelMessage { role: "system".into(), content: "Select a verified source target for the user's request using only the listed candidate IDs. Compare each candidate's role and description, roleIndex (its position among candidates of that role), path, lines, excerpt, and bounded safe context with the request. Text content and existing attribute values are distinct targets. When exactly one listed candidate matches the requested target, select it rather than returning ambiguous. Return exactly one JSON object: {\"result\":\"selected\",\"candidate_id\":\"<listed ID>\"}, {\"result\":\"ambiguous\"}, or {\"result\":\"no_match\"}. If several plausible targets remain and the request does not distinguish them, return ambiguous. Do not invent an ID, source text, replacement code, byte offset, or a proposal.".into() },
        ModelMessage { role: "user".into(), content: format!("Request: {prompt}\nVerified candidates: {candidates}") },
    ];
    inference.log("semantic_selection", &format!("Selection call started\nCandidate count: {}\nSchema supplied: {schema}", views.len()));
    let started = Instant::now();
    let response = inference.infer_structured(model, &messages, schema, "CANDIDATE SELECTION").await.map_err(|error| {
        inference.log("semantic_selection", &format!("Selection call failed after {}ms\nValidation: not run", started.elapsed().as_millis()));
        error
    })?;
    inference.log("semantic_selection", &format!("Selection response received in {}ms", started.elapsed().as_millis()));
    if response.done_reason.as_deref() == Some("length") {
        inference.log("semantic_selection", "Structured action: unavailable\nValidation: failed\nError: Candidate selection response was malformed.");
        return Err("Candidate selection response was malformed.".into());
    }
    let action = serde_json::from_str::<CandidateSelection>(response.content.trim()).ok();
    let action_name = action.as_ref().map_or("malformed", |reply| match reply.result {
        CandidateSelectionResult::Selected => "selected", CandidateSelectionResult::Ambiguous => "ambiguous", CandidateSelectionResult::NoMatch => "no_match",
    });
    let id = match action.as_ref() {
        Some(CandidateSelection { result: CandidateSelectionResult::Selected, candidate_id: Some(candidate_id) }) if registry.candidate(candidate_id).is_some() => candidate_id.as_str(),
        Some(CandidateSelection { result: CandidateSelectionResult::Selected, .. }) => "unverified",
        _ => "none",
    };
    inference.log("semantic_selection", &format!("Structured action: {action_name}\nCandidate ID: {id}"));
    if let Some(CandidateSelection { result: CandidateSelectionResult::Selected, candidate_id: Some(candidate_id) }) = action.as_ref() {
        if !views.iter().any(|view| view.id == *candidate_id) {
            inference.log("semantic_selection", "Validation: failed\nError: Unknown candidate ID.");
            return Err("Unknown candidate ID.".into());
        }
    }
    let outcome = resolve_selection(&response.content, registry, root).map_err(|error| {
        inference.log("semantic_selection", &format!("Validation: failed\nError: {error}"));
        error
    })?;
    inference.log("semantic_selection", &match &outcome {
        SelectionOutcome::Selected(view) => format!("Validation: passed\nSelected ID: {} at {}:{}", view.id, view.path, view.start_line),
        SelectionOutcome::Ambiguous => "Validation: passed\nAmbiguous candidates; clarification required".into(),
        SelectionOutcome::NoMatch => "Validation: passed\nNo matching candidate".into(),
    });
    Ok(outcome)
}

pub(crate) async fn generate_replacement(inference: &impl SemanticInference, model: &str, prompt: &str, root: &std::path::Path, registry: &CandidateRegistry, candidate_id: &str) -> Result<String, String> {
    let candidate = registry.verify_current(root, candidate_id)?;
    let schema = replacement_schema();
    let messages = [
        ModelMessage { role: "system".into(), content: "Generate only the complete replacement value for the selected existing text or attribute candidate. Return the value without surrounding quote delimiters. Do not return HTML, markdown, a path, old text, a source range, a summary, or a proposal. Do not recreate surrounding markup. Return exactly one JSON object with the single field replacement.".into() },
        ModelMessage { role: "user".into(), content: format!("Original request: {prompt}\n\nSelected candidate ({}), current value:\n{}", candidate.description(), candidate.original()) },
    ];
    inference.log("semantic_generation", &format!("Candidate replacement call started\nSchema supplied: {schema}"));
    let response = inference.infer_structured(model, &messages, schema, "CANDIDATE REPLACEMENT").await?;
    if response.done_reason.as_deref() == Some("length") { return Err("Candidate replacement response was malformed.".into()); }
    let reply: ReplacementGeneration = serde_json::from_str(response.content.trim()).map_err(|_| "Candidate replacement response was malformed.".to_owned())?;
    if reply.replacement.trim().is_empty() { return Err("Candidate replacement must not be empty.".into()); }
    inference.log("semantic_generation", "Candidate replacement response validated");
    Ok(reply.replacement)
}

pub(crate) async fn generate_insertion(inference: &impl SemanticInference, model: &str, prompt: &str, root: &std::path::Path, registry: &CandidateRegistry, candidate_id: &str, element: repository::InsertedElement) -> Result<InsertionGeneration, String> {
    let candidate = registry.verify_current(root, candidate_id)?;
    let schema = insertion_schema(element);
    let element_name = match element {
        repository::InsertedElement::Paragraph => "paragraph",
        repository::InsertedElement::Heading => "heading",
        repository::InsertedElement::Link => "link",
    };
    let messages = [
        ModelMessage { role: "system".into(), content: format!("Generate content for one new {element_name} element. Return structured data only: element_type must be {element_name}; text is plain text, never HTML. For a link, provide its destination in href without quote delimiters. Do not return markup, paths, source text, offsets, or insertion positions. Return exactly one JSON object matching the supplied schema.") },
        ModelMessage { role: "user".into(), content: format!("Original request: {prompt}\n\nSelected insertion anchor: {}", candidate.description()) },
    ];
    inference.log("semantic_generation", &format!("Candidate insertion content call started\nSchema supplied: {schema}"));
    let response = inference.infer_structured(model, &messages, schema, "CANDIDATE INSERTION").await?;
    if response.done_reason.as_deref() == Some("length") { return Err("Candidate insertion response was malformed.".into()); }
    serde_json::from_str(response.content.trim()).map_err(|_| "Candidate insertion response was malformed.".to_owned())
}

pub(crate) async fn run_text_replacement(inference: &impl SemanticInference, model: &str, prompt: &str, role: CandidateRole, root: &std::path::Path, pending: Option<&repository::PendingChanges>, app: Option<&tauri::AppHandle>) -> Result<SemanticEditResponse, String> {
    if let Some(app) = app { let _ = app.emit("repository-inspection-start", ()); }
    let mut registry = CandidateRegistry::new();
    let count = repository::discover_html_candidates(root, &mut registry)?;
    let activity_item = repository::Activity { label: format!("Discovered {count} verified HTML targets") };
    if let Some(app) = app { let _ = app.emit("repository-activity", &activity_item); }
    let activity = vec![activity_item];
    let selected = match select_candidate(inference, model, prompt, root, &registry, Some(role)).await? {
        SelectionOutcome::Selected(view) => view,
        SelectionOutcome::Ambiguous => return Ok(SemanticEditResponse { content: "I found multiple plausible text targets. Please clarify which one you mean.".into(), activity, proposal: None }),
        SelectionOutcome::NoMatch => return Ok(SemanticEditResponse { content: "I found no verified existing text matching that request.".into(), activity, proposal: None }),
    };
    if selected.role != role { return Err("Selected candidate does not match the requested text target.".into()); }
    let replacement = generate_replacement(inference, model, prompt, root, &registry, &selected.id).await?;
    if let Some(app) = app { let _ = app.emit("proposal-validation-start", ()); }
    let summary = format!("Update the {} in {}.", selected.description.to_ascii_lowercase(), selected.path);
    let edit = if selected.role.is_attribute() {
        repository::SemanticEdit::ReplaceAttribute { candidate_id: selected.id.clone(), replacement_value: replacement, summary }
    } else {
        repository::SemanticEdit::ReplaceTextCandidate { candidate_id: selected.id.clone(), replacement, summary }
    };
    let proposal = repository::validate_semantic_edit(root, &registry, edit)?;
    if let Some(pending) = pending { repository::stage_pending_proposal(pending, proposal.clone())?; }
    inference.log("semantic_proposal", "Validation: passed\nPending Changes creation: passed");
    Ok(SemanticEditResponse { content: "Done — have a wee look in Changes 👀".into(), activity, proposal: Some(proposal) })
}

pub(crate) async fn run_relative_insertion(inference: &impl SemanticInference, model: &str, prompt: &str, anchor_role: CandidateRole, position: repository::CandidatePosition, element: repository::InsertedElement, root: &std::path::Path, pending: Option<&repository::PendingChanges>, app: Option<&tauri::AppHandle>) -> Result<SemanticEditResponse, String> {
    if let Some(app) = app { let _ = app.emit("repository-inspection-start", ()); }
    let mut registry = CandidateRegistry::new();
    let count = repository::discover_html_candidates(root, &mut registry)?;
    let activity_item = repository::Activity { label: format!("Discovered {count} verified HTML targets") };
    if let Some(app) = app { let _ = app.emit("repository-activity", &activity_item); }
    let activity = vec![activity_item];
    let selected = match select_candidate(inference, model, prompt, root, &registry, Some(anchor_role)).await? {
        SelectionOutcome::Selected(view) => view,
        SelectionOutcome::Ambiguous => return Ok(SemanticEditResponse { content: "I found multiple plausible insertion anchors. Please clarify which one you mean.".into(), activity, proposal: None }),
        SelectionOutcome::NoMatch => return Ok(SemanticEditResponse { content: "I found no verified existing element matching that insertion request.".into(), activity, proposal: None }),
    };
    if selected.role != anchor_role { return Err("Selected candidate does not match the requested insertion anchor.".into()); }
    let generated = generate_insertion(inference, model, prompt, root, &registry, &selected.id, element).await?;
    let expected_type = match element {
        repository::InsertedElement::Paragraph => "paragraph",
        repository::InsertedElement::Heading => "heading",
        repository::InsertedElement::Link => "link",
    };
    if generated.element_type != expected_type { return Err("Candidate insertion response used an unsupported element type.".into()); }
    if element != repository::InsertedElement::Link && generated.href.is_some() { return Err("Only an inserted link may include a destination.".into()); }
    if let Some(app) = app { let _ = app.emit("proposal-validation-start", ()); }
    let relation = match position { repository::CandidatePosition::Before => "before", repository::CandidatePosition::After => "after" };
    let proposal = repository::validate_semantic_edit(root, &registry, repository::SemanticEdit::InsertRelative {
        anchor_candidate_id: selected.id.clone(), position, element, text: generated.text, href: generated.href,
        summary: format!("Insert a {expected_type} {relation} {} in {}.", selected.description.to_ascii_lowercase(), selected.path),
    })?;
    if let Some(pending) = pending { repository::stage_pending_proposal(pending, proposal.clone())?; }
    inference.log("semantic_proposal", "Validation: passed\nPending Changes creation: passed");
    Ok(SemanticEditResponse { content: "Done — have a wee look in Changes 👀".into(), activity, proposal: Some(proposal) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_classifies_supported_legacy_and_unsupported_edits() {
        assert_eq!(dispatch_edit_request("Change the main page heading to Welcome", RequestIntent::Edit), EditDispatch::Semantic(SemanticEditRoute::ReplaceCandidate { role: CandidateRole::HeadingOne }));
        assert_eq!(dispatch_edit_request("Change the signup link to point to /register", RequestIntent::Edit), EditDispatch::Semantic(SemanticEditRoute::ReplaceCandidate { role: CandidateRole::AttributeHref }));
        assert_eq!(dispatch_edit_request("Add a paragraph below the main heading saying Hello", RequestIntent::Edit), EditDispatch::Semantic(SemanticEditRoute::InsertRelative { anchor: CandidateRole::HeadingOne, position: repository::CandidatePosition::After, element: repository::InsertedElement::Paragraph }));
        assert_eq!(dispatch_edit_request("In src/site.css, change body color to black", RequestIntent::Edit), EditDispatch::Legacy);
        assert_eq!(dispatch_edit_request("In src/app.ts, rename the function to start", RequestIntent::Edit), EditDispatch::Legacy);
        assert_eq!(dispatch_edit_request("Change the main heading to Welcome but preserve the span", RequestIntent::Edit), EditDispatch::Unsupported);
        assert_eq!(dispatch_edit_request("Change the main heading to Welcome", RequestIntent::Answer), EditDispatch::NotEdit);
    }

    #[test]
    fn candidate_selection_contract_represents_each_result() {
        assert_eq!(serde_json::from_str::<CandidateSelection>(r#"{"result":"selected","candidate_id":"c1"}"#).unwrap(), CandidateSelection { result: CandidateSelectionResult::Selected, candidate_id: Some("c1".into()) });
        assert_eq!(serde_json::from_str::<CandidateSelection>(r#"{"result":"ambiguous"}"#).unwrap(), CandidateSelection { result: CandidateSelectionResult::Ambiguous, candidate_id: None });
        assert_eq!(serde_json::from_str::<CandidateSelection>(r#"{"result":"no_match"}"#).unwrap(), CandidateSelection { result: CandidateSelectionResult::NoMatch, candidate_id: None });
        assert!(serde_json::from_str::<CandidateSelection>(r#"{"result":"ambiguous","candidate_id":"c1","extra":true}"#).is_err());
    }

    #[test]
    fn request_intent_classification_is_provider_neutral() {
        assert_eq!(classify_request_intent("Please change the main heading to Welcome."), RequestIntent::Edit);
        assert_eq!(classify_request_intent("What does the main heading say?"), RequestIntent::Answer);
    }
}
