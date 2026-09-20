# AIIDE

AIIDE is an early-development, Windows-first desktop AI development companion. Elma is the user-facing assistant. The product combines local-first AI assistance, repository-aware chat, controlled file changes, local Git workflows, and—on a separate feature branch—local image generation.

AIIDE is not a full IDE and is not yet a standalone end-user install. The current build is a developer preview whose supported path requires the development toolchain, Git, Ollama, and a locally installed model.

## What is available on `main`

- Open a local folder and inspect its bounded file tree, project files, Git branch, and worktree status.
- Chat with an Ollama model. Elma can list, search, and read bounded repository context instead of receiving the entire project.
- Review a pending one-file text change before Apply; Reject writes nothing, and Apply fails if the source changed after review began.
- Use a narrow application-led HTML heading workflow: AIIDE discovers verified `<h1>` candidates, the model selects a candidate ID and generates replacement text, and AIIDE assembles the change.
- Inspect local Git diffs, stage and unstage files, review the exact staged snapshot, create an approved local commit, and inspect recent local commit summaries.
- Run in-memory Agent Debug traces and a three-case agent benchmark for grounding and proposal regression checks.
- Use reduced-motion-aware Elma states for idle, thinking, working, inspecting, success, and error.

The Rust/Tauri backend is the trusted boundary. Model output, paths, source selection, and generated content are untrusted until AIIDE validates them. Current editing remains deliberately limited and is not evidence of broad real-world reliability.

OpenRouter has a tested Rust provider and can be exercised by the benchmark CLI, but the desktop interface currently exposes Ollama only. GitHub authentication, issues, pull requests, push, remote mutations, controlled project commands, multi-file changes, and a general settings/onboarding flow are not implemented.

## Image-generation branch

The separate `image-generation` branch contains the M08A ComfyUI/SDXL implementation: a dedicated image-provider boundary, one active or reviewable job, state-based queue tracking, PNG preview, Save/Reject approval, project-relative collision-safe saving, temporary-file cleanup, and a minimal Chat/Image composer toggle.

That branch passed its reported mocked/deterministic checks, but a local ComfyUI probe failed because no service was running. Real GPU generation, output quality, memory behaviour, and the complete live acceptance flow therefore remain unverified. M08A is not available on `main` and should not be described as released.

## Developer setup

Prerequisites:

- Node.js and npm
- Rust and the Tauri 2 Windows prerequisites
- Git on `PATH`
- Ollama running at `http://127.0.0.1:11434`
- an installed model; `qwen2.5-coder:7b` is the primary current development profile, with limited structured-edit reliability

From the repository root:

```powershell
npm install
ollama pull qwen2.5-coder:7b
npm run tauri dev
```

Useful checks:

```powershell
npm run lint
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run benchmark -- --model qwen2.5-coder:7b
```

The benchmark requires a running provider and installed model. Rust unit tests do not prove live model quality.

## Agent Debug Mode

Use **Debug on**, reproduce one request, then open **Agent diagnostics** to copy the latest trace. Debug Mode is off by default, keeps only the latest request in memory, and also writes the trace to the `npm run tauri dev` terminal. Traces can contain prompts, local paths, source context, and raw model responses; review them before sharing.

## Project documentation

- [PROJECT.md](PROJECT.md) — product principles, capability inventory, architecture, and roadmap
- [codex/CONTEXT.md](codex/CONTEXT.md) — concise current implementation and limitations
- [codex/REPO-MAP.md](codex/REPO-MAP.md) — code and documentation navigation
- [codex/STANDALONE-DISTRIBUTION-DESIGN.md](codex/STANDALONE-DISTRIBUTION-DESIGN.md) — planned installer, onboarding, and runtime-management direction
- [codex/PERSONALITY.md](codex/PERSONALITY.md) — canonical Elma personality
- [codex/MODEL-PROFILES.md](codex/MODEL-PROFILES.md) — model compatibility and benchmark usage

The current distribution gap and release plan are documented separately; normal users should eventually receive guided setup rather than PowerShell environment-variable and runtime instructions.
