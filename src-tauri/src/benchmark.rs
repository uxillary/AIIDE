use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ollama::{self, BenchmarkAgentFailure, BenchmarkAgentOutput};
use crate::model_provider::{OLLAMA_PROVIDER_ID, OPENROUTER_PROVIDER_ID};

const EXPECTED_PATH: &str = "src/index.html";
const OLD_HEADING: &str = "<h1>OrbitNote</h1>";
const NEW_HEADING: &str = "<h1>Welcome to the AIIDE Sandbox</h1>";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case { Answer, Lookup, Edit }

impl Case {
    fn id(self) -> &'static str { match self { Self::Answer => "answer", Self::Lookup => "lookup", Self::Edit => "edit" } }
    fn prompt(self) -> &'static str { match self {
        Self::Answer => "Tell me what the main page of this project is.",
        Self::Lookup => "Find the file that contains the main heading.",
        Self::Edit => "change the main heading to \"Welcome to the AIIDE Sandbox\". make only that change and prepare it for review",
    } }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkResult {
    model: String,
    case: String,
    passed: bool,
    duration_ms: u128,
    tool_calls: usize,
    repair_count: usize,
    final_action: String,
    failure_reason: Option<String>,
}

fn read_target(activity: &[String]) -> bool {
    activity.iter().any(|label| label == &format!("Read: {EXPECTED_PATH}"))
}

fn concise_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("project-relative") { "invalid project-relative path".into() }
    else if lower.contains("inspection limit") { "inspection limit reached".into() }
    else if error.contains("For action='propose_change', include changes") { "proposal missing changes".into() }
    else { error.lines().next().unwrap_or("benchmark failed").trim().to_owned() }
}

fn repair_count(trace: &str) -> usize { trace.matches("[AIIDE][repair] ").count() }

fn edit_mismatch(after: &str) -> &'static str {
    if after.contains(OLD_HEADING) { "intended heading unchanged; other content changed" }
    else if !after.contains(NEW_HEADING) { "incorrect or missing heading replacement" }
    else { "unexpected additional change beyond the heading replacement" }
}

fn classify(model: &str, case: Case, elapsed: u128, outcome: Result<BenchmarkAgentOutput, BenchmarkAgentFailure>, fixture_before: &str, fixture_after: &str) -> BenchmarkResult {
    let mut result = BenchmarkResult { model: model.into(), case: case.id().into(), passed: false, duration_ms: elapsed,
        tool_calls: 0, repair_count: 0, final_action: "error".into(), failure_reason: None };
    let output = match outcome {
        Ok(output) => output,
        Err(failure) => {
            result.repair_count = repair_count(&failure.trace);
            result.failure_reason = Some(concise_error(&failure.error));
            return result;
        }
    };
    result.tool_calls = output.activity.len();
    result.repair_count = repair_count(&output.trace);
    result.final_action = if output.proposal.is_some() { "propose_change" } else { "answer" }.into();
    let failure = match case {
        Case::Answer if output.proposal.is_some() => Some("unexpected proposal"),
        Case::Answer if output.content.trim().is_empty() => Some("answer was empty"),
        Case::Answer if !read_target(&output.activity) => Some("answer was not grounded by reading src/index.html"),
        Case::Lookup if output.proposal.is_some() => Some("unexpected proposal"),
        Case::Lookup if !read_target(&output.activity) => Some("target file was not successfully read"),
        Case::Edit => match output.proposal.as_ref() {
            None => Some("no validated proposal"),
            Some(proposal) if proposal.changes.len() != 1 => Some("proposal did not contain exactly one file change"),
            Some(proposal) if proposal.changes[0].path != EXPECTED_PATH => Some("proposal targeted the wrong file"),
            Some(proposal) if proposal.changes[0].replacements != 1 => Some("proposal was not one focused replacement"),
            Some(proposal) if proposal.changes[0].before != fixture_before => Some("proposal was based on a different fixture snapshot"),
            Some(proposal) if proposal.changes[0].after != fixture_before.replacen(OLD_HEADING, NEW_HEADING, 1) => Some(edit_mismatch(&proposal.changes[0].after)),
            Some(_) if fixture_after != fixture_before => Some("benchmark modified the fixture"),
            Some(_) => None,
        },
        _ => None,
    };
    result.passed = failure.is_none();
    result.failure_reason = failure.map(str::to_owned);
    result
}

fn fixture_root() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/agent-benchmark");
    std::fs::canonicalize(root).map_err(|_| "Benchmark fixture is missing.".to_owned())
}

fn parse_args_from(args: impl IntoIterator<Item = String>) -> Result<(String, String, Option<Case>, bool), String> {
    let mut provider = OLLAMA_PROVIDER_ID.to_owned();
    let mut model = None;
    let mut case = None;
    let mut json = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--provider" => provider = args.next().ok_or("--provider requires ollama or openrouter")?,
            "--model" => model = args.next(),
            "--case" => case = Some(match args.next().as_deref() {
                Some("answer") => Case::Answer, Some("lookup") => Case::Lookup, Some("edit") => Case::Edit,
                _ => return Err("--case must be answer, lookup, or edit".into()),
            }),
            "--json" => json = true,
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    if !matches!(provider.as_str(), OLLAMA_PROVIDER_ID | OPENROUTER_PROVIDER_ID) {
        return Err("Unknown model provider. Use 'ollama' or 'openrouter'.".into());
    }
    let model = model.filter(|value| !value.trim().is_empty()).ok_or(
        "Usage: agent-benchmark [--provider ollama|openrouter] --model <model-id> [--case answer|lookup|edit] [--json]"
    )?;
    Ok((provider, model, case, json))
}

fn parse_args() -> Result<(String, String, Option<Case>, bool), String> {
    parse_args_from(std::env::args().skip(1))
}

pub fn run_cli() -> Result<(), String> {
    let (provider, model, selected, json) = parse_args()?;
    let root = fixture_root()?;
    let cases = selected.map_or_else(|| vec![Case::Answer, Case::Lookup, Case::Edit], |value| vec![value]);
    let mut results = Vec::new();
    for case in cases {
        let target = root.join(EXPECTED_PATH);
        let before = std::fs::read_to_string(&target).map_err(|_| "Benchmark fixture target is missing.".to_owned())?;
        let started = Instant::now();
        let outcome = tauri::async_runtime::block_on(ollama::run_benchmark_agent(&provider, &model, root.clone(), case.prompt()));
        let after = std::fs::read_to_string(&target).map_err(|_| "Benchmark fixture target is missing.".to_owned())?;
        results.push(classify(&model, case, started.elapsed().as_millis(), outcome, &before, &after));
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&results).map_err(|_| "Could not serialize benchmark results.")?);
    } else {
        for item in &results {
            let verdict = if item.passed { "PASS".to_owned() } else { format!("FAIL — {}", item.failure_reason.as_deref().unwrap_or("unknown failure")) };
            println!("{} / {}: {} ({}ms, tools={}, repairs={}, action={})", item.model, item.case, verdict, item.duration_ms, item.tool_calls, item.repair_count, item.final_action);
        }
        let passed = results.iter().filter(|item| item.passed).count();
        println!("Summary: {passed}/{} passed", results.len());
    }
    if results.iter().all(|item| item.passed) { Ok(()) } else { Err("One or more benchmark cases failed.".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{PendingChange, PendingProposal};

    fn output(activity: &[&str]) -> BenchmarkAgentOutput {
        BenchmarkAgentOutput { content: "grounded".into(), activity: activity.iter().map(|v| (*v).into()).collect(), proposal: None, trace: "[AIIDE][repair] Attempt 1/2".into() }
    }

    #[test]
    fn answer_and_lookup_require_a_successful_target_read() {
        assert!(!classify("m", Case::Answer, 1, Ok(output(&["Project structure"])), OLD_HEADING, OLD_HEADING).passed);
        let passed = classify("m", Case::Lookup, 1, Ok(output(&["Read: src/index.html"])), OLD_HEADING, OLD_HEADING);
        assert!(passed.passed);
        assert_eq!(passed.repair_count, 1);
    }

    #[test]
    fn failure_reasons_are_concise() {
        assert_eq!(concise_error("Only project-relative paths are allowed."), "invalid project-relative path");
        assert_eq!(concise_error("I reached the repository inspection limit"), "inspection limit reached");
        assert_eq!(concise_error("For action='propose_change', include changes; each change needs path, old_text, and new_text."), "proposal missing changes");
    }

    #[test]
    fn edit_requires_the_exact_validated_change_and_an_untouched_fixture() {
        let before = format!("before\n{OLD_HEADING}\nafter");
        let proposal = PendingProposal { summary: "Change the heading".into(), changes: vec![PendingChange {
            path: EXPECTED_PATH.into(), before: before.clone(), after: before.replacen(OLD_HEADING, NEW_HEADING, 1), replacements: 1,
        }] };
        let result = classify("m", Case::Edit, 1, Ok(BenchmarkAgentOutput {
            content: "ready".into(), activity: vec!["Read: src/index.html".into()], proposal: Some(proposal), trace: String::new(),
        }), &before, &before);
        assert!(result.passed);
        assert_eq!(result.final_action, "propose_change");
    }

    #[test]
    fn validated_edit_mismatch_is_not_misreported_as_missing_changes() {
        let before = format!("before\n{OLD_HEADING}\nafter");
        let proposal = PendingProposal { summary: "Wrong replacement".into(), changes: vec![PendingChange {
            path: EXPECTED_PATH.into(), before: before.clone(), after: before.replacen(OLD_HEADING, "<h1>Wrong</h1>", 1), replacements: 1,
        }] };
        let result = classify("m", Case::Edit, 1, Ok(BenchmarkAgentOutput {
            content: "ready".into(), activity: vec!["Read: src/index.html".into()], proposal: Some(proposal),
            trace: "[AIIDE][protocol] Schema/format: changes\n[AIIDE][tool] Proposal validation: passed\nPending change creation: passed".into(),
        }), &before, &before);
        assert!(!result.passed);
        assert_eq!(result.final_action, "propose_change");
        assert_eq!(result.failure_reason.as_deref(), Some("incorrect or missing heading replacement"));
    }

    #[test]
    fn edit_mismatch_categories_do_not_expose_proposal_content() {
        let before = format!("<title>OrbitNote</title>\n{OLD_HEADING}\n<footer>Keep</footer>");
        let cases = [
            (before.replacen("<title>OrbitNote</title>", "<title>Changed</title>", 1), "intended heading unchanged; other content changed"),
            (before.replacen(OLD_HEADING, "<h1>Different</h1>", 1), "incorrect or missing heading replacement"),
            (before.replacen(OLD_HEADING, NEW_HEADING, 1).replacen("<footer>Keep</footer>", "<footer>Changed</footer>", 1), "unexpected additional change beyond the heading replacement"),
        ];
        for (after, expected_reason) in cases {
            let proposal = PendingProposal { summary: "Edit".into(), changes: vec![PendingChange {
                path: EXPECTED_PATH.into(), before: before.clone(), after: after.clone(), replacements: 1,
            }] };
            let result = classify("m", Case::Edit, 1, Ok(BenchmarkAgentOutput {
                content: "ready".into(), activity: vec!["Read: src/index.html".into()], proposal: Some(proposal), trace: String::new(),
            }), &before, &before);
            assert_eq!(result.failure_reason.as_deref(), Some(expected_reason));
            assert!(!result.failure_reason.unwrap().contains(&after));
        }
    }

    #[test]
    fn repair_count_includes_structural_and_proposal_retries() {
        let trace = "[AIIDE][repair] Attempt 1/2\n[AIIDE][repair] Ambiguous proposal anchor rejected\nAttempt 1/2";
        let result = classify("m", Case::Answer, 1, Ok(BenchmarkAgentOutput {
            content: "grounded".into(), activity: vec![format!("Read: {EXPECTED_PATH}")], proposal: None, trace: trace.into(),
        }), OLD_HEADING, OLD_HEADING);
        assert!(result.passed);
        assert_eq!(result.repair_count, 2);
    }

    #[test]
    fn failed_agent_attempts_retain_their_repair_count() {
        let failure = BenchmarkAgentFailure {
            error: "The local model could not produce a valid structured response after two retries.".into(),
            trace: "[AIIDE][repair] Attempt 1/2\n[AIIDE][repair] Attempt 2/2".into(),
        };
        let result = classify("m", Case::Edit, 1, Err(failure), OLD_HEADING, OLD_HEADING);
        assert!(!result.passed);
        assert_eq!(result.final_action, "error");
        assert_eq!(result.repair_count, 2);
    }

    #[test]
    fn benchmark_defaults_to_ollama_and_accepts_explicit_openrouter() {
        let (provider, model, _, _) = parse_args_from(["--model", "local-model"].into_iter().map(str::to_owned)).unwrap();
        assert_eq!(provider, "ollama");
        assert_eq!(model, "local-model");

        let (provider, model, _, _) = parse_args_from(
            ["--provider", "openrouter", "--model", "vendor/model:free"].into_iter().map(str::to_owned)
        ).unwrap();
        assert_eq!(provider, "openrouter");
        assert_eq!(model, "vendor/model:free");
    }

    #[test]
    fn benchmark_rejects_unknown_providers() {
        let error = parse_args_from(["--provider", "cloud", "--model", "m"].into_iter().map(str::to_owned)).unwrap_err();
        assert_eq!(error, "Unknown model provider. Use 'ollama' or 'openrouter'.");
    }
}
