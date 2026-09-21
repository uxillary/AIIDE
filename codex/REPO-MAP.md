# AIIDE repository map

Use this map to choose a starting point, then inspect only necessary call boundaries. [CONTEXT.md](CONTEXT.md) is the concise current source of truth; [PROJECT.md](../PROJECT.md) owns product principles, capability status, and roadmap.

| Task area | Start here | Cross boundary only if needed |
| --- | --- | --- |
| Chat, composer, activity, provider/model controls | `src/components/LocalChat.tsx`, `src/styles.css` | `src/services/ai/`, `src/types/ai.ts`, then `src-tauri/src/ollama.rs` for behaviour |
| Elma personality or model-specific prompting | `codex/PERSONALITY.md`, `codex/MODEL-PROFILES.md` | `src-tauri/src/model_profiles.rs`, then `ollama.rs` for runtime prompts |
| Elma animation states | `src/components/Elma.tsx`, `src/assets/elma/runtime/`, `src/styles.css` | `LocalChat.tsx` and `App.tsx` for state selection |
| Text-provider abstraction, Ollama, OpenRouter | `src-tauri/src/model_provider.rs` | `ollama.rs`; `src/services/ai/ollama.ts` for the Ollama-only desktop route |
| Agent protocol, grounding, retries, debug trace | `src-tauri/src/ollama.rs` | `repository.rs` for tool execution and proposal rules |
| Repository listing, search, reads, path safety, pending changes | `src-tauri/src/repository.rs` | `ollama.rs` for orchestration; `src/services/project.ts` and `src/types/project.ts` for UI contracts |
| Application-led HTML candidate discovery | `src-tauri/src/repository/candidates.rs` | `ollama.rs` for candidate selection/replacement and `repository.rs` for proposal assembly |
| Open folder, project tree, Git summary | `src-tauri/src/project.rs` | `src/components/ProjectSidebar.tsx`, `FileTree.tsx`, and `FileViewer.tsx` |
| Local Git status, diffs, staging, commits, history | `src-tauri/src/git.rs` | `src/components/GitPanel.tsx`, `src/services/git.ts`, `src/types/git.ts` |
| Pending proposal UI and session history | `src/components/ChangesPanel.tsx`, `src/App.tsx` | `repository.rs` for Apply/Reject safety |
| Agent benchmark and model compatibility | `src-tauri/src/benchmark.rs`, `src-tauri/src/model_profiles.rs` | `src-tauri/src/bin/agent-benchmark.rs`, `src-tauri/fixtures/agent-benchmark/` |
| Tauri command wiring and permissions | `src-tauri/src/lib.rs` | relevant command module, `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json` |
| Build and dependencies | `package.json`, `src-tauri/Cargo.toml` | Vite, TypeScript, ESLint, Tauri config; lockfiles only for dependency work |
| Product status and roadmap | `PROJECT.md` | `codex/CONTEXT.md` for a compact implementation snapshot |
| Standalone distribution and onboarding | `codex/STANDALONE-DISTRIBUTION-DESIGN.md` | `PROJECT.md` for priorities; no implementation exists yet |

## Architecture at a glance

`src/` is the React/TypeScript/Vite frontend. Components render the chat-first workspace; services wrap Tauri invocations; types define frontend contracts. There is no frontend unit-test harness, so frontend regression checks are lint, typecheck, and build.

`src-tauri/src/` is the trusted Rust/Tauri backend:

- `project.rs` owns the opened project and bounded tree metadata.
- `repository.rs` owns bounded inspection, protected/project-relative paths, pending proposals, read-only viewing, Apply/Reject, and stale-write protection.
- `repository/candidates.rs` extracts bounded verified HTML title, heading, and paragraph candidates.
- `ollama.rs` owns request classification, the bounded agent loop, provider-neutral model calls, grounding, candidate-edit orchestration, retries, and debug traces. The command names remain Ollama-specific because the desktop UI currently exposes only Ollama.
- `model_provider.rs` implements the internal Ollama/OpenRouter text-inference boundary. OpenRouter is available to the benchmark but not the desktop UI.
- `git.rs` owns local Git inspection and explicit mutations. No GitHub or remote commands exist.
- `benchmark.rs` owns the controlled three-case agent harness.
- `lib.rs` registers state and Tauri commands; `main.rs` launches the application.

## Image-generation branch

M08A is committed on `image-generation`, not `main`. Inspect it read-only with Git unless that branch is the active work target:

```powershell
git show image-generation:codex/M08A-IMAGE-GENERATION-DESIGN.md
git show image-generation:src-tauri/src/image_generation.rs
git diff --stat main...image-generation
```

On that branch, `src-tauri/src/image_generation.rs` owns the image-provider abstraction, ComfyUI adapter, fixed SDXL workflow, lifecycle, temporary storage, cancellation, and protected save. Frontend owners are `src/components/ImageResultCard.tsx`, `src/services/image/`, `src/types/image.ts`, and the image mode in `LocalChat.tsx`. Image bytes remain outside the text proposal/diff path.

## Documentation ownership

- `AGENTS.md` — short repository working rules only.
- `README.md` — public introduction, current developer setup, supported features, and release limitations.
- `PROJECT.md` — canonical vision, principles, capability inventory, architecture, roadmap, and open decisions.
- `codex/CONTEXT.md` — token-efficient current state and durable decisions.
- `codex/REPO-MAP.md` — navigation only.
- `codex/PERSONALITY.md` — canonical Elma personality.
- `codex/MODEL-PROFILES.md` — model compatibility and benchmark usage.
- `codex/STANDALONE-DISTRIBUTION-DESIGN.md` — planned distribution/onboarding/runtime architecture.
- milestone design documents — detailed milestone-specific decisions; branch-specific documents stay branch-specific until their feature merges.

Generated/local areas include `node_modules/`, `dist/`, `.npm-cache/`, `src-tauri/target/`, `src-tauri/gen/`, and TypeScript `*.tsbuildinfo`. Do not inspect them unless the task concerns generated output. Treat any future `codex/archive/` as cold history.
