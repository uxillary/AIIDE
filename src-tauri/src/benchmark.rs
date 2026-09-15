use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ollama::{self, BenchmarkAgentOutput};

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
    else if lower.contains("proposal") && lower.contains("changes") { "proposal missing changes".into() }
    else { error.lines().next().unwrap_or("benchmark failed").trim().to_owned() }
}

fn trace_failure(trace: &str) -> Option<&'static str> {
    if trace.contains("Only project-relative paths are allowed") { Some("invalid project-relative path") }
    else if trace.contains("inspection limit") { Some("inspection limit reached") }
    else if trace.contains("Proposal validation") && trace.contains("changes") { Some("proposal missing changes") }
    else { None }
}

fn classify(model: &str, case: Case, elapsed: u128, outcome: Result<BenchmarkAgentOutput, String>, fixture_before: &str, fixture_after: &str) -> BenchmarkResult {
    let mut result = BenchmarkResult { model: model.into(), case: case.id().into(), passed: false, duration_ms: elapsed,
        tool_calls: 0, repair_count: 0, final_action: "error".into(), failure_reason: None };
    let output = match outcome {
        Ok(output) => output,
        Err(error) => { result.failure_reason = Some(concise_error(&error)); return result; }
    };
    result.tool_calls = output.activity.len();
    result.repair_count = output.trace.matches("[AIIDE][repair] Attempt").count();
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
            Some(proposal) if proposal.changes[0].before != fixture_before || proposal.changes[0].after != fixture_before.replacen(OLD_HEADING, NEW_HEADING, 1) => Some("proposal did not match the expected heading replacement"),
            Some(_) if fixture_after != fixture_before => Some("benchmark modified the fixture"),
            Some(_) => None,
        },
        _ => None,
    };
    result.passed = failure.is_none();
    result.failure_reason = failure.map(|reason| trace_failure(&output.trace).unwrap_or(reason).to_owned());
    result
}

fn fixture_root() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/agent-benchmark");
    std::fs::canonicalize(root).map_err(|_| "Benchmark fixture is missing.".to_owned())
}

fn parse_args() -> Result<(String, Option<Case>, bool), String> {
    let mut model = None;
    let mut case = None;
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => model = args.next(),
            "--case" => case = Some(match args.next().as_deref() {
                Some("answer") => Case::Answer, Some("lookup") => Case::Lookup, Some("edit") => Case::Edit,
                _ => return Err("--case must be answer, lookup, or edit".into()),
            }),
            "--json" => json = true,
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    Ok((model.filter(|value| !value.trim().is_empty()).ok_or("Usage: agent-benchmark --model <ollama-model> [--case answer|lookup|edit] [--json]")?, case, json))
}

pub fn run_cli() -> Result<(), String> {
    let (model, selected, json) = parse_args()?;
    let root = fixture_root()?;
    let cases = selected.map_or_else(|| vec![Case::Answer, Case::Lookup, Case::Edit], |value| vec![value]);
    let mut results = Vec::new();
    for case in cases {
        let target = root.join(EXPECTED_PATH);
        let before = std::fs::read_to_string(&target).map_err(|_| "Benchmark fixture target is missing.".to_owned())?;
        let started = Instant::now();
        let outcome = tauri::async_runtime::block_on(ollama::run_benchmark_agent(&model, root.clone(), case.prompt()));
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
}
