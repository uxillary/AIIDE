# AIIDE Codex working rules

Use minimum sufficient context. After this file, read [codex/REPO-MAP.md](codex/REPO-MAP.md). Read [codex/CONTEXT.md](codex/CONTEXT.md) for durable architecture and current-versus-planned capabilities; use targeted sections of [PROJECT.md](PROJECT.md) for the roadmap or broader product decisions. Treat any future `codex/archive/` as cold history.

Start with user-named files or the mapped subsystem. Inspect relevant files and only necessary call boundaries; use targeted searches when ownership is unclear. Avoid broad repository audits, generated/dependency output, and unnecessary context loading. Do not inspect secrets or local credentials by default.

Prefer small, scoped implementation tasks and the smallest reliable change. Do not refactor or reformat unrelated code, upgrade dependencies, or revisit adjacent UI. Preserve the trusted Rust/Tauri boundary: model output is untrusted and repository access must be project-relative, protected, and bounded. See `src-tauri/src/repository.rs` for current rules.

Preserve existing passing behavior and tests. Add a regression test for each confirmed bug fix. Never weaken tests to make CI pass. Get explicit approval before changing an established behavioral contract.

Validate proportionally: focused tests/checks first, then affected type/lint/compiler checks; use a broader build only when warranted. Use a focused live Ollama pass for model behavior when needed, and stop repeating a runtime-dependent check after about two similar failures. Report what passed, timed out, and whether direct connectivity worked. Do not fix unrelated warnings.

The current application has bounded inspection and an approved one-file proposal path; deterministic source selection, commands, and Git mutations are planned. Use current code as implementation truth and [codex/CONTEXT.md](codex/CONTEXT.md) for the architectural direction.
