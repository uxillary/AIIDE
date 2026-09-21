# Durable AIIDE context

# Durable AIIDE context

AIIDE is the current repository/application name. Elma is its local AI coding assistant and candidate product identity. AIIDE is a Windows-first, local-first coding application, not a full IDE or graphics editor. The intended workflow is Open → Ask → Inspect → Edit → Review → Test → Commit. See [PROJECT.md](../PROJECT.md) for the current capability status and roadmap; planned stages are not proof of implemented features, and [REPO-MAP.md](REPO-MAP.md) identifies current owners.

## Current implementation

The frontend uses React, TypeScript, Vite, and Tailwind CSS; the desktop backend uses Tauri 2 and Rust as the trusted boundary. Local inference runs through Ollama, primarily tested with `qwen2.5-coder:7b`, with the design allowing interchangeable local models. Users can open a project, inspect its file tree and Git summary, and ask Elma questions. Elma can request bounded `list_files`, `search_files`, and `read_file` operations. The current experimental editing path lets Elma propose up to four exact replacements in one existing UTF-8 text file. Rust validates the path and unique source text, captures the original snapshot, and generates a reviewable pending change. Only the user's Apply action writes, after a stale-snapshot check. File creation, multi-file edits, command execution, and Git mutations are not implemented. [REPO-MAP.md](REPO-MAP.md) identifies current owners; `src-tauri/src/repository.rs` defines current limits.

## Technology and boundaries

The desktop uses React, TypeScript, Vite and Tailwind on the frontend, with Tauri 2 and Rust as the trusted backend. Frontend components present application state and request commands. Rust owns opened-project identity, canonical path validation, bounded repository access, snapshots, proposal assembly, file writes and local Git mutations.

Model and provider output is untrusted. Models may interpret intent, rank application-discovered candidates, and generate content, but they do not own paths, source locations, validation, writes, or claims of success. Project access stays relative to the opened root and rejects protected, escaping, binary, non-UTF-8, oversized, ambiguous, or stale operations as appropriate. Text inference, image generation, image editing and vision are separate capability contracts. Support for one never implies support for another. Optional local or cloud providers must not disable the core local workflow, and private source or credentials must not be exposed unnecessarily.

## Application-led direction

Experiments on `fix/reliable-editing` improved source validation but did not achieve reliable real-world editing acceptance: the small model still selected incorrect source targets. That branch is historical/experimental and is not approved for merging. Do not treat its implementation as the active design.

AIIDE should own repository discovery and contextual retrieval, verified file and source references, deterministic file creation and editing, change assembly and diffs, snapshot and path validation, explicit review and approval, and test execution through controlled, authorised commands. Later it should prepare Git branches, commits, and optional GitHub pull requests. Elma should interpret requests, analyse supplied code, generate replacement content, help choose among verified candidates, explain changes and test failures, and suggest fixes. The model must not invent original source text, silently resolve ambiguous targets, write arbitrary files, or execute unrestricted commands. In the first deterministic editing stage, the user selects the file and source range; Elma is not required.

The Rust/Tauri backend remains the trusted project and filesystem boundary. Model output and tool arguments are untrusted; access must be project-relative, protected, and bounded. The user approves changes before application and controls external publishing actions. Keep the current bounded inspection and approved one-file proposal behaviour accurate while building the new workflow in small stages. Preserve existing passing behaviour unless an established contract change is explicitly approved.

Broader editing should extend application-owned target discovery, explicit disambiguation, validation and recoverable review rather than returning source-location authority to the model. Only an explicit approved action may apply a pending file change or perform a Git mutation. Rejecting a proposal writes nothing. Remote operations, command execution, generated binary assets and other consequential actions require their own narrow contracts and approval boundaries. Elma reports success only after application confirmation.

## Interface conventions

Keep the interface chat-first with restrained permanent navigation and progressive disclosure for diagnostics or advanced controls. Use consistent proposal, Git and future result cards; preserve clear focus, disabled, busy, success and error states. Controls require accessible names and keyboard behaviour. Animation must respect reduced-motion preferences, and essential state must not depend on motion, colour, or decorative Elma sprites alone.

Elma's voice is defined in [PERSONALITY.md](PERSONALITY.md). Personality never overrides application state, uncertainty, safety or verification. Elma's personality instructions remain separate from core protocol and safety rules. Agent Debug Mode is an observability-only, in-memory trace of the latest request, available in the UI and development terminal when enabled; it must not alter prompts, schemas, model settings, permissions, or behaviour.

## Development conventions

Prefer existing service/type boundaries and keep filesystem, provider and mutation policy in Rust. Keep project data bounded and diagnostics reviewable because traces may contain prompts, paths or source context. Avoid introducing dependencies where the existing stack suffices. Preserve current behaviour unless the task explicitly changes it, and add focused regression coverage for new behaviour or confirmed fixes.

See [REPO-MAP.md](REPO-MAP.md) for code ownership, [MODEL-PROFILES.md](MODEL-PROFILES.md) for model compatibility and benchmark usage, and [STANDALONE-DISTRIBUTION-DESIGN.md](STANDALONE-DISTRIBUTION-DESIGN.md) for the planned packaging and runtime boundary.

