# AIIDE repository map

Use this map to choose a starting point, then read only the files relevant to the task. [CONTEXT.md](CONTEXT.md) holds durable product context; [PROJECT.md](../PROJECT.md) holds the fuller vision and roadmap.
Use this map to pick a starting point, then inspect only the necessary call boundaries. [CONTEXT.md](CONTEXT.md) records durable architecture; [PROJECT.md](../PROJECT.md) owns capability status and roadmap.

| Feature | Start here | Cross boundary only if needed |
| --- | --- | --- |
| Chat layout, composer, activity | `src/components/LocalChat.tsx`, `src/styles.css` | `src/services/ai/`, `src/types/ai.ts` for data contracts |
| Ollama connection, model output, agent protocol/retries | `src-tauri/src/ollama.rs` (focused tests are in this module) | `src-tauri/src/repository.rs` for tool results or limits; AI service/types for UI contract |
| Elma conversational behaviour, personality tuning, model integration or personality benchmarks | `codex/PERSONALITY.md` (canonical personality specification) | `src-tauri/src/ollama.rs` for the current runtime prompt and rewrite boundary |
| Repository listing, search, reads, path safety | `src-tauri/src/repository.rs` (focused tests are in this module) | `ollama.rs` only for orchestration |
| Open folder, Git branch/status, project metadata | `src-tauri/src/project.rs` | `src/services/project.ts`, `src/types/project.ts`, `src/components/ProjectSidebar.tsx`, `src/components/FileTree.tsx` for presentation |
| Tauri command wiring/permissions | `src-tauri/src/lib.rs`, relevant command module | `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json` when permissions/config matter |
| General UI/layout | `src/App.tsx`, relevant component, `src/styles.css` | Rust only if backend data or behavior changes |
| Build/dependencies | `package.json` or `src-tauri/Cargo.toml` | Relevant Vite, TypeScript, ESLint, or Tauri config; lockfile only for dependency work |

`src/` is the React/TypeScript/Vite frontend: `components/` renders UI, `services/` wraps frontend calls, and `types/` defines contracts. `src-tauri/src/` is the Rust/Tauri backend: `project.rs` owns opened-project/Git inspection, `repository.rs` owns bounded repository inspection and validation/application of approved one-file proposals, `ollama.rs` owns local model networking and the current bounded agent loop, and `lib.rs` registers commands. `main.rs` launches Tauri. `src-tauri/capabilities/` and `tauri.conf.json` configure desktop behavior.

Rust unit and opt-in local Ollama acceptance tests currently live in `ollama.rs` and `repository.rs`; there is no separate frontend test tree. Check `package.json` for current npm scripts and `Cargo.toml` for Rust dependencies. `README.md` describes current features; `PROJECT.md` is active product vision, not routine startup context.

Generated/local areas currently present: `node_modules/`, `dist/`, `.npm-cache/`, `src-tauri/target/`, `src-tauri/gen/`, and TypeScript `*.tsbuildinfo`. Do not inspect them unless the task concerns their output. Treat any future `codex/archive/` as cold context.
| React UI, application state, chat and provider controls | `src/App.tsx`, `src/components/LocalChat.tsx` | `src/services/`, `src/types/`, then the owning Tauri command |
| Shared UI styling and accessibility | `src/styles.css` | relevant component in `src/components/` |
| Elma sprites, animation and state | `src/components/Elma.tsx`, `src/assets/elma/runtime/` | `LocalChat.tsx` and `App.tsx` for state selection; `codex/PERSONALITY.md` for voice |
| Text providers and agent orchestration | `src-tauri/src/model_provider.rs`, `src-tauri/src/ollama.rs` | `src/services/ai/`, `src/types/ai.ts`, `model_profiles.rs` |
| Repository inspection and reviewable editing | `src-tauri/src/repository.rs`, `src-tauri/src/repository/candidates.rs` | `ollama.rs`; `src/services/project.ts`, `src/types/project.ts`, `ChangesPanel.tsx` |
| Project opening, tree and file viewer | `src-tauri/src/project.rs` | `ProjectSidebar.tsx`, `FileTree.tsx`, `FileViewer.tsx` |
| Local Git status, diffs, staging and commits | `src-tauri/src/git.rs` | `GitPanel.tsx`, `src/services/git.ts`, `src/types/git.ts` |
| Tauri command wiring and permissions | `src-tauri/src/lib.rs` | owning Rust module, `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json` |
| Tests and agent benchmark | inline Rust test modules, `src-tauri/src/benchmark.rs` | `src-tauri/src/bin/agent-benchmark.rs`, `src-tauri/fixtures/agent-benchmark/`, `codex/MODEL-PROFILES.md` |
| Build and configuration | `package.json`, `src-tauri/Cargo.toml` | Vite, TypeScript, ESLint and Tauri configuration; lockfiles only for dependency work |
| Product decisions and distribution | `PROJECT.md`, `codex/STANDALONE-DISTRIBUTION-DESIGN.md` | `README.md` for public setup and limitations |

## Directory boundaries

- `src/` is the React/TypeScript/Vite frontend. Components render state, services invoke Tauri commands, types define frontend contracts, and `App.tsx` coordinates the workspace.
- `src-tauri/src/` is the trusted Rust backend. It owns project identity, bounded repository access, deterministic proposal assembly and application, provider orchestration, and approved local Git mutations.
- Repository-aware model calls use bounded list/search/read tools. Application-led HTML candidate discovery lives in `repository/candidates.rs`; only validated pending proposals can reach Apply/Reject in the UI.
- `src/assets/elma/` contains source frames, sheets, and runtime sprites. Start with the small `runtime/` set; inspect source art only for sprite work.

## Branch-only tools and capabilities

- Image generation is on `image-generation`, not the current mainline implementation. There, `src-tauri/src/image_generation.rs` owns the image-provider boundary, ComfyUI adapter, lifecycle, temporary output and protected save. UI owners are `src/components/ImageResultCard.tsx`, `src/services/image/`, `src/types/image.ts`, and image mode in `LocalChat.tsx`. The branch design is `codex/M08A-IMAGE-GENERATION-DESIGN.md`.
- Sprite Workshop is on `tools-workshop`. `tools/sprite-workshop/src/` owns its standalone React/Vite editor; `src/project.ts`, `detection.ts`, `alignment.ts`, `render.ts`, and `export.ts` own the main workflows. Its tests are in `tools/sprite-workshop/tests/`. It is a local browser utility, not part of the Tauri application.

Use `git show <branch>:<path>` for narrow read-only inspection of branch-only work. Do not treat a local ignored build directory as merged source.

## Default-ignore areas

Unless directly relevant, skip `node_modules/`, `.npm-cache/`, `dist/`, `build/`, `coverage/`, `src-tauri/target/`, generated Tauri schemas, `*.tsbuildinfo`, lockfiles, and local IDE files. Skip `files/` installer archives and other binaries. Treat `elma-images/` and non-runtime `src/assets/elma/` as large asset collections; inspect filenames or selected assets rather than dumping them. Never inspect `.env*`, `*.local`, credential stores, or diagnostic content merely for orientation. Treat any future `codex/archive/` as historical and opt-in.

These are behavioural defaults, not absolute bans: inspect an excluded area when the task specifically concerns it.
