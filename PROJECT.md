# AIIDE product vision and roadmap

## Product identity

AIIDE is a Windows-first desktop AI development companion. Elma is the user-facing assistant. AIIDE owns project access, validation, review, approval, Git operations, and other consequential actions; Elma helps the user understand a repository and generate content within those boundaries.

The intended workflow remains:

**Open → Ask → Inspect → Edit → Review → Test → Commit**

The product should combine local and optional cloud AI assistance, repository-aware coding, reviewable file modification, Git and GitHub workflows, image generation and future visual capabilities, and a cohesive desktop interface. It is not intended to become a full IDE or a full graphics editor.

## Product principles

1. Users control project modifications and external actions.
2. Consequential actions require explicit approval.
3. AI suggestions are distinct from verified application actions.
4. AIIDE owns deterministic validation; model output is untrusted.
5. Operation is local-first where practical.
6. Cloud services are optional and clearly identified.
7. Text, image, and future vision providers remain replaceable.
8. The interface stays chat-first and avoids unnecessary permanent complexity.
9. Dependencies, downloads, licences, storage, and hardware requirements are transparent.
10. Reliable behaviour comes before feature expansion.
11. Interaction and visual design are accessible and consistent.
12. Privacy-sensitive data stays bounded; credentials and source context are not exposed unnecessarily.

The application should hide incidental technical complexity without hiding decisions that affect files, privacy, cost, downloads, hardware, Git history, or remote services. Elma never claims that an operation succeeded until AIIDE confirms it.

## Status vocabulary

- **IMPLEMENTED** — functionality exists in the named branch.
- **VERIFIED** — the stated test or acceptance evidence passed; this does not imply release readiness beyond that evidence.
- **EXPERIMENTAL** — implemented, but not sufficiently validated for reliable release use.
- **PLANNED** — approved direction that has not been implemented.
- **PROPOSED** — an idea that still requires product or architectural decisions.
- **DEFERRED** — deliberately outside the current scope.

Statuses may be combined. Branch-specific functionality must not be presented as part of `main`.

## Capability inventory

| Capability | Branch and status | Evidence and limits |
| --- | --- | --- |
| Local project selection, bounded file tree, read-only file viewer, Git summary | `main` — **IMPLEMENTED**, **VERIFIED** | Rust path/read tests pass; frontend build checks the UI. Generated, sensitive, binary, non-UTF-8, oversized, traversal, and symlink-escape cases are bounded or rejected. |
| Ollama text inference and model selection | `main` — **IMPLEMENTED**, **VERIFIED** deterministically, **EXPERIMENTAL** for model quality | The desktop UI connects to local Ollama and lists installed models. Provider/protocol tests pass; six opt-in live Ollama tests were not run in the 2026-09-19 audit. |
| OpenRouter text inference | `main` — **IMPLEMENTED** backend, **VERIFIED** deterministically, **EXPERIMENTAL** | The Rust provider, API-key validation, response normalization, safe errors, and benchmark routing are tested. It is not selectable in the desktop UI, and no live OpenRouter acceptance was run. |
| Repository-aware answers using list/search/read | `main` — **IMPLEMENTED**, **VERIFIED** deterministically, **EXPERIMENTAL** for broad model reliability | Bounded tools, evidence gating, retries, and grounding tests pass. Real model performance varies by model and prompt. |
| Older exact-replacement proposal path | `main` — **IMPLEMENTED**, **VERIFIED** deterministically, **EXPERIMENTAL** | One existing UTF-8 file, up to four exact replacements, explicit Apply/Reject, and stale-snapshot protection. Small models have selected incorrect source targets in live experiments. |
| Application-led editing | `main` — **IMPLEMENTED** narrow slice, **VERIFIED** deterministically, **EXPERIMENTAL** | AIIDE discovers bounded HTML title, `<h1>`, and paragraph candidates. The active edit slice supports a complete visible `<h1>` plain-text replacement; nested-markup preservation, arbitrary ranges, creation, and broad language support are not available. |
| Pending change history | `main` — **IMPLEMENTED**, **VERIFIED** by build/unit boundary checks | Session-only applied/rejected summaries; not a durable recovery system. |
| Local Git worktree and commits | `main` — **IMPLEMENTED**, **VERIFIED** deterministically, **EXPERIMENTAL** for release | Status, branch/detached state, staged and unstaged diffs, stage/unstage, approved commit with snapshot token, local history, and bounded commit details. It never pushes or amends. Hooks are disabled for AIIDE-created commits. |
| GitHub integration | `main` — **PLANNED** | No authentication, remote metadata, issues, pull requests, push, or remote mutation exists. CLI versus direct API remains open. |
| Agent Debug Mode | `main` — **IMPLEMENTED**, **VERIFIED** | Off by default; retains the latest trace in memory and mirrors it to the development terminal. Traces may contain private project context. |
| Agent benchmark and model profiles | `main` — **IMPLEMENTED**, **VERIFIED** as a harness | Three controlled answer/lookup/edit cases protect grounding and no-write proposal behaviour. Live results are model- and runtime-specific and were not rerun during this documentation audit. |
| Elma animation states | `main` — **IMPLEMENTED**, **VERIFIED** by frontend build | Sprite-sheet states: idle, thinking, working, inspecting, success, and error. CSS disables sprite animation for reduced-motion preference. |
| General settings and onboarding | `main` — **PLANNED** | The chosen Ollama model is stored locally, but there is no settings page, capability manager, first-run flow, or secure credential UI. |
| M08A image generation | `image-generation` only — **IMPLEMENTED**, **EXPERIMENTAL** | ComfyUI/SDXL prototype provider, single job, state tracking, PNG preview, Save/Reject, safe save, temporary cleanup, and Chat/Image toggle are committed. Disabled managed-runtime contracts now cover ownership migration, pinned identity/integrity, safe staging/extraction, and Windows Job Object process ownership; no managed download or execution is exposed. SDXL is the acceptance baseline, not the production default. Real GPU acceptance remains outstanding. |
| M08B image editing | **PLANNED** | Requires original preservation, edit-capable provider/model selection, preview, comparison, and approval. SDXL text-to-image alone does not provide this workflow. |
| M08C image analysis | **PLANNED** | Requires a vision-capable provider/model and bounded project-image access. The current coding model must not be assumed to understand images. |
| Sprite sheets, background removal, upscaling, variants, metadata, animation workflows | **PROPOSED** | Evaluate only after the core image workflow is reliable; AIIDE should not become a general graphics editor. |
| Standalone Windows distribution and guided setup | **PLANNED** | Architecture direction is recorded in [codex/STANDALONE-DISTRIBUTION-DESIGN.md](codex/STANDALONE-DISTRIBUTION-DESIGN.md); no end-user installer or runtime manager exists yet. |

Current `main` verification on 2026-09-19: `cargo test --manifest-path src-tauri/Cargo.toml` passed 82 tests with 6 live Ollama tests ignored; frontend lint, typecheck, and production build also passed. Deterministic success is not a substitute for live acceptance.

## Architecture

### Trust boundary

The React/TypeScript frontend presents state and requests Tauri commands. Rust owns the opened-project identity, canonical path validation, bounded repository access, proposal assembly, stale-snapshot checks, file application, and local Git commands. Model responses, repository tool arguments, candidate choices, generated source, diffs supplied to a model, and provider errors are treated as untrusted data.

Current repository limits live in `src-tauri/src/repository.rs`. Important properties include project-relative paths, protected-path checks, generated-directory exclusions, bounded file sizes and result counts, and rejection of binary/non-UTF-8 content. Apply rechecks the original snapshot and fails closed when it is stale.

### Text providers

`src-tauri/src/model_provider.rs` defines the replaceable text-inference boundary. Ollama is the current desktop provider. OpenRouter implements the same internal request/response contract and is reachable from the benchmark, but desktop provider selection and credential management are unfinished.

Provider support is capability-specific. A provider or model that can chat is not automatically suitable for structured edits, image generation, image editing, or vision. Model profiles describe compatibility with AIIDE's protocol rather than general model quality.

### Repository inspection and editing

Repository-aware chat uses a bounded loop of `list_files`, `search_files`, and `read_file`. The app supplies only project metadata automatically; source is retrieved on demand. Evidence gates prevent a repository-specific answer from completing without relevant inspection.

The current editing system has two paths:

1. A narrow application-led HTML heading path discovers candidates in Rust, exposes opaque verified IDs to the model, rechecks the selected candidate, asks the model only for replacement text, escapes it, and assembles a pending change.
2. The older model-led fallback can propose exact replacements after inspection. Rust still validates path, uniqueness, size, snapshot, and no-op rules before creating a pending change.

Only Apply writes. Reject writes nothing. The deterministic candidate path is the architectural direction, but its present HTML `<h1>` slice is not a general editing engine. Future stages should expand application-owned target discovery, user disambiguation, generated content, multi-file assembly, controlled verification, and recoverable checkpoints without returning source-location authority to the model.

### Git and GitHub

Local Git is independent from AI chat. AIIDE invokes the installed Git executable with explicit argument arrays and exact repository-relative paths. The current UI reads status/history, displays bounded diffs, stages or unstages selected files, previews the exact staged snapshot, requires confirmation, rechecks a snapshot token, and creates a local commit. It rejects detached-HEAD commits and conflicts. Elma may suggest a commit subject from a bounded staged diff through Ollama; failure preserves the user's message.

No remote operation is implemented. Future branch creation, push, authentication, repository metadata, issues, and pull requests require separate commands, narrowly scoped credentials, explicit user intent, and confirmation at the remote mutation boundary. Secrets must use an operating-system-appropriate credential store and must never enter prompts, logs, diagnostics, or project files.

### Image capability family

Image generation is separate from text inference because it is asynchronous, GPU-heavy, binary, previewable, and approval-driven. The `image-generation` branch therefore defines independent frontend and Rust image-provider contracts rather than adding binary lifecycle state to the text `ModelProvider`.

M08A uses a fixed, core-node SDXL 1.0 ComfyUI workflow at 1024 × 1024, batch size 1, 30 steps, DPM++ 2M/Karras, CFG 7, and a random seed. It permits one active or reviewable job. AIIDE polls real states without invented percentages, downloads and validates a bounded PNG into AIIDE-owned temporary storage, and saves only after approval to a protected project-relative `.png` path. Collision naming is exclusive and non-destructive. Older ComfyUI versions may not safely cancel a running job; AIIDE does not use the global interrupt endpoint.

SDXL 1.0 remains the implemented prototype and acceptance baseline until a replacement is demonstrated; it is not permanently selected as the production default. Production-model selection requires focused comparison and real testing on the RTX 3070 Ti's 8 GB VRAM, covering compatibility, licensing, performance, memory behaviour, workflow requirements, and image quality. A small typed model registry should separate model configuration and model-specific workflows from runtime management. ComfyUI remains behind the replaceable image-provider boundary so another engine can be introduced without replacing the composer, result cards, or Save/Reject approval flow.

The branch design is `codex/M08A-IMAGE-GENERATION-DESIGN.md` on `image-generation`. It is intentionally not copied to `main` before the feature merges.

M08B should preserve the original image, show a before/after comparison, and require approval before replacement or saving. M08C should use a vision-capable provider to describe and reason about project images. Neither capability is supplied by the current SDXL text-to-image path.

### Elma and interface

[codex/PERSONALITY.md](codex/PERSONALITY.md) is canonical. Elma is friendly, concise, calm, trustworthy, technically direct, and honest about uncertainty. Dry humour and slight sarcasm are welcome when they do not obscure an error or dismiss the user. Personality never overrides application state, safety, or verification.

UI principles:

- chat-first interaction and minimal permanent navigation;
- progressive disclosure rather than a wall of settings;
- reusable proposal, Git, image, diagnostic, and future command-result cards;
- consistent dark-theme styling and clear focus/disabled/error states;
- accessible controls and reduced-motion support;
- image generation inside the conversation rather than a separate graphics workspace;
- onboarding consistent with the same restrained interaction model.

Future image work may add a distinct working/painting state for Elma, but it remains **PROPOSED** until assets, motion behaviour, and reduced-motion fallback are designed.

## Distribution direction

The current supported environment is a developer checkout. Normal end users should not need to prepare Python environments, model folders, inference servers, environment variables, or command-line arguments.

The approved image direction is a relatively small signed application installer plus optional, consent-based runtime and model acquisition. A normal user should eventually be able to enable image generation and generate through Elma without manually installing Python, configuring ComfyUI, starting a server, or using terminal commands. Application-managed ComfyUI is the selected initial engine strategy. Its security and ownership foundations exist behind a disabled experimental gate, but production acquisition, execution, repair, removal, installer integration, and release trust are **PLANNED**, not enabled; compatibility testing and licensing review remain gates. Existing compatible external ComfyUI installations remain usable and must never be modified or terminated as though AIIDE owns them. No paid cloud API is required for local generation. Fully embedded inference is **DEFERRED** for investigation.

See [codex/STANDALONE-DISTRIBUTION-DESIGN.md](codex/STANDALONE-DISTRIBUTION-DESIGN.md) for the onboarding sequence, runtime responsibilities, security and licensing requirements, phases, and open questions.

## Roadmap

Milestone identifiers before M08A are historical repository context; do not renumber completed work merely to make the roadmap look tidy. New milestone numbers should be assigned only when scope is approved.

### Immediate: M08A acceptance and stabilisation

1. Run the documented live ComfyUI/SDXL flow on supported Windows hardware.
2. Verify queue transitions, preview retrieval, Reject/no-write, Save, collision naming, cleanup, error recovery, and supported/unsupported cancellation.
3. Record generation time, peak VRAM/system-memory behaviour, low-VRAM behaviour, and representative output quality.
4. Fix confirmed defects with regression tests, rerun the full deterministic checks, and only then decide whether M08A is merge-ready.

This is the recommended next implementation milestone because M08B, M08C, managed image-runtime setup, and public image claims all depend on a trustworthy M08A baseline.

### Approved M08A Stage B architecture

After preserving the SDXL acceptance baseline, Stage B should add richer runtime readiness, persisted external-engine configuration, a compact Image-mode setup card, separate engine and model readiness, a small typed model registry, and focused deterministic failure tests. It must not download runtimes/models, manage processes, change the installer, or add a model marketplace. The production model and managed acquisition mechanism remain separate approval decisions after comparison and real GPU evidence.

### Next approved capability areas

- **M08B image editing** depends on M08A approval semantics and a researched edit-capable model/provider.
- **M08C image analysis** depends on a bounded image-read contract and a researched vision provider.
- **Editing reliability** expands application-owned candidate discovery, user disambiguation, supported languages/operations, and live acceptance before multi-file work.
- **GitHub completion** adds authentication and read-only metadata before any approved remote mutation; local Git must remain usable without GitHub.
- **Standalone distribution and guided onboarding** begin with detection and diagnostics, then optional managed installation. They depend on supported runtime/version matrices and licensing decisions.
- **Packaging and release validation** follow installer selection, update policy, signing, clean-machine tests, accessibility review, privacy review, and recovery/uninstall acceptance.

Controlled commands, multi-file edits, durable session recovery, and project-context persistence remain **PLANNED** but should not outrank current reliability and distribution foundations. A general IDE, unrestricted shell, autonomous publishing, hosted repositories, model training, and a full graphics editor are **DEFERRED**.

## Open decisions

- supported Windows versions, installer/update technology, code signing, and release channels;
- managed-installation details for image generation and whether other capabilities remain detection-only;
- supported Ollama, ComfyUI, model, driver, and GPU compatibility matrices;
- model download source, integrity/signature policy, licence presentation, and redistribution eligibility;
- secure OpenRouter and future GitHub credential storage and revocation;
- GitHub CLI versus direct API, permission scopes, and remote-action confirmation design;
- next application-led edit targets and how ambiguous candidates are presented;
- checkpoint/recovery design for multi-file changes and dirty worktrees;
- M08B edit model/provider and M08C vision provider;
- persistence format for projects, sessions, settings, and capability installations;
- macOS/Linux scope after the Windows release path is proven.

The long-term promise remains:

> **Your project. Your machine. Your model. You approve the changes.**
