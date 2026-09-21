# AIIDE Codex working rules

## Context discipline

Read this file first, then [codex/REPO-MAP.md](codex/REPO-MAP.md). Read [codex/CONTEXT.md](codex/CONTEXT.md) only when the task needs broader architectural context, and use targeted sections of [PROJECT.md](PROJECT.md) only for product status, roadmap, or wider decisions. Treat `codex/archive/` as cold history.

Start with files named by the user. Explore progressively: task → repository map → relevant files → direct dependencies → targeted search → wider subsystem only when justified. Initially aim for 3–8 relevant source files and expand only when necessary. Stop once there is enough evidence to act safely.

Avoid whole-repository scans, recursive exploration, large file dumps, repeated reads of unchanged files, historical documentation, and unrelated subsystems. Do not inspect generated output, dependencies, binaries, large asset collections, or secrets unless the task specifically requires them. For a large file, inspect headings, targeted matches, or relevant ranges before reading it in full.

Make the smallest reliable change. Reuse existing patterns; do not perform unrelated refactoring, cleanup, dependency upgrades, UI revisits, or documentation rewrites.

The current application has bounded inspection and an approved one-file proposal path; deterministic source selection, commands, and Git mutations are planned. Use current code as implementation truth and [codex/CONTEXT.md](codex/CONTEXT.md) for the architectural direction.
## Verification

Do **not** run AIIDE benchmarks by default or automatically after implementations, fixes, or refactors. The user runs benchmarks manually. Run the benchmark harness only when the user explicitly requests it.

Run only tests directly affected by modified code and add focused regression coverage when new behaviour requires it. Prefer scoped lint or type checks where supported. Run broader tests or builds only when a repository rule requires them or the change justifies them. Do not repeat a passing check without a relevant change, weaken mandatory safety checks or CI requirements, or fix unrelated warnings and pre-existing failures.

For documentation-only work, validate changed paths and references and run `git diff --check`; do not run application tests, benchmarks, lint, type checks, or builds unless mandatory. Report checks performed, checks intentionally skipped, and genuine remaining risks.

## Safety and Git

Preserve application-led editing, repository grounding, the trusted Rust/Tauri boundary, and human approval for consequential actions. Model output is untrusted; repository access must remain project-relative, protected, and bounded. See `src-tauri/src/repository.rs` for the enforced rules.

Never inspect or expose credentials unnecessarily. Preserve user changes. Do not commit, push, merge, reset, discard work, or change an established behavioural contract unless explicitly requested. Never weaken tests merely to make CI pass.
