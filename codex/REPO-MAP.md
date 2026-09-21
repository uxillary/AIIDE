# AIIDE repository map

Use this map to pick a starting point, then inspect only the necessary call boundaries. [CONTEXT.md](CONTEXT.md) records durable architecture; [PROJECT.md](../PROJECT.md) owns capability status and roadmap.

| Feature | Start here | Cross boundary only if needed |
| --- | --- | --- |
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
