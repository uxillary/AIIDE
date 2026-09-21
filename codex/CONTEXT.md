# Durable AIIDE context

AIIDE is a Windows-first desktop AI development companion; Elma is the user-facing assistant. It is not a full IDE and is not yet a standalone end-user install. The intended workflow is **Open → Ask → Inspect → Edit → Review → Test → Commit**. [PROJECT.md](../PROJECT.md) is the canonical product vision, capability inventory, and roadmap; planned stages are not proof of implementation.

## Current `main`

React/TypeScript/Vite/Tailwind form the frontend; Tauri 2/Rust is the trusted backend. Users can open a folder, browse a bounded tree, view permitted text files, inspect Git status, and chat through Ollama. The bounded agent can list, search, and read repository content, with project-specific answers gated on evidence. OpenRouter has a tested Rust provider and benchmark route but no desktop selector or credential UI.

Editing has two limited paths. The application-led path discovers verified HTML candidates and supports complete visible `<h1>` plain-text replacement: the model selects an opaque candidate ID and supplies replacement content; Rust revalidates and assembles the proposal. The older fallback permits up to four exact replacements in one existing UTF-8 file after inspection. Both remain experimental for real-world reliability. Only Apply writes, after path, proposal, and stale-snapshot validation; Reject writes nothing. File creation, arbitrary source-range editing, multi-file changes, commands/tests, durable recovery, and automatic repair are not implemented.

Local Git is separate from AI editing. AIIDE supports bounded status/diffs, stage/unstage, an approved snapshot-checked local commit, recent history, and an optional Ollama commit-subject suggestion. It does not create branches, amend, push, authenticate to GitHub, read issues, or create pull requests.

Agent Debug Mode is an observability-only in-memory trace of the latest request. The controlled benchmark covers grounded answer, lookup, and no-write edit cases for Ollama or OpenRouter. Current audit evidence: 82 Rust tests passed and 6 opt-in live Ollama tests were ignored on 2026-09-19. Deterministic tests do not establish live model reliability.

Elma's canonical personality is [PERSONALITY.md](PERSONALITY.md). Implemented sprite states are idle, thinking, working, inspecting, success, and error, with reduced-motion CSS. Elma never reports success without application confirmation.

## Feature-branch context

M08A exists only on `image-generation`. It implements a separate ComfyUI/SDXL image-provider path, one active or reviewable job, real queue/generation states, PNG preview, Save/Reject approval, protected project-relative saving, collision naming, temporary cleanup, prompt-specific cancellation where supported, and a minimal Chat/Image composer toggle. The feature commit reports 81 Rust tests plus lint, typecheck, build, and diff checks passing. ComfyUI was unavailable during its probe, so real GPU generation, output quality, memory behaviour, and the live acceptance flow are unverified. Its design file is `codex/M08A-IMAGE-GENERATION-DESIGN.md` on that branch.

M08B image editing and M08C vision-based image analysis are planned, not implemented. Additional visual tooling is proposed only.

## Durable decisions

- Rust/Tauri owns project identity, path safety, candidate discovery, snapshots, diff assembly, writes, Git mutations, and future controlled command execution.
- Model/provider output is untrusted. A model may interpret intent, rank verified candidates, and generate content; it must not invent source locations, bypass ambiguity, or claim an unconfirmed action.
- Text inference, image generation, image editing, and vision are separate capability contracts. Provider support for one does not imply support for another.
- File changes and remote mutations require explicit review/approval. Optional services must not disable the core local workflow.
- The release direction is a small Windows installer with guided, optional capability setup; existing Ollama and ComfyUI installations remain usable. Application-managed runtimes may follow. Fully embedded inference remains an open option.
- Reliability and live acceptance precede feature expansion. Immediate priority is M08A real-GPU acceptance and stabilisation.

See [REPO-MAP.md](REPO-MAP.md) for owners and [STANDALONE-DISTRIBUTION-DESIGN.md](STANDALONE-DISTRIBUTION-DESIGN.md) for the planned installer/onboarding architecture.
