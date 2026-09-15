# AIIDE repository map

Use this map to choose a starting point, then read only the files relevant to the task. [CONTEXT.md](CONTEXT.md) holds durable product context; [PROJECT.md](../PROJECT.md) holds the fuller vision and roadmap.

| Task area | Start here | Cross boundary only if needed |
| --- | --- | --- |
| Chat layout, composer, activity | `src/components/LocalChat.tsx`, `src/styles.css` | `src/services/ai/`, `src/types/ai.ts` for data contracts |
| Ollama connection, model output, agent protocol/retries | `src-tauri/src/ollama.rs` (focused tests are in this module) | `src-tauri/src/repository.rs` for tool results or limits; AI service/types for UI contract |
| Elma conversational behaviour, personality tuning, model integration or personality benchmarks | `codex/PERSONALITY.md` (canonical personality specification) | `src-tauri/src/ollama.rs` for the current runtime prompt and rewrite boundary |
| Repository listing, search, reads, path safety | `src-tauri/src/repository.rs` (focused tests are in this module) | `ollama.rs` only for orchestration |
| Open folder, Git branch/status, project metadata | `src-tauri/src/project.rs` | `src/services/project.ts`, `src/types/project.ts`, `src/components/ProjectSidebar.tsx`, `src/components/FileTree.tsx` for presentation |
| Tauri command wiring/permissions | `src-tauri/src/lib.rs`, relevant command module | `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json` when permissions/config matter |
| General UI/layout | `src/App.tsx`, relevant component, `src/styles.css` | Rust only if backend data or behavior changes |
| Build/dependencies | `package.json` or `src-tauri/Cargo.toml` | Relevant Vite, TypeScript, ESLint, or Tauri config; lockfile only for dependency work |

`src/` is the React/TypeScript/Vite frontend: `components/` renders UI, `services/` wraps frontend calls, and `types/` defines contracts. `src-tauri/src/` is the Rust/Tauri backend: `project.rs` owns opened-project/Git inspection, `repository.rs` is the trusted read-only filesystem boundary, `ollama.rs` owns local model networking and the bounded agent loop, and `lib.rs` registers commands. `main.rs` launches Tauri. `src-tauri/capabilities/` and `tauri.conf.json` configure desktop behavior.

Rust unit and opt-in local Ollama acceptance tests currently live in `ollama.rs` and `repository.rs`; there is no separate frontend test tree. Check `package.json` for current npm scripts and `Cargo.toml` for Rust dependencies. `README.md` describes current features; `PROJECT.md` is active product vision, not routine startup context.

Generated/local areas currently present: `node_modules/`, `dist/`, `.npm-cache/`, `src-tauri/target/`, `src-tauri/gen/`, and TypeScript `*.tsbuildinfo`. Do not inspect them unless the task concerns their output. Treat any future `codex/archive/` as cold context.
