# Durable AIIDE context

AIIDE is a Windows-first desktop AI development companion with Elma as its user-facing assistant. It supports a chat-first **Open → Ask → Inspect → Edit → Review → Test → Commit** workflow without trying to become a full IDE or graphics editor. [PROJECT.md](../PROJECT.md) is the authority for current capability status and roadmap; implementation remains the source of truth.

## Technology and boundaries

The desktop uses React, TypeScript, Vite and Tailwind on the frontend, with Tauri 2 and Rust as the trusted backend. Frontend components present application state and request commands. Rust owns opened-project identity, canonical path validation, bounded repository access, snapshots, proposal assembly, file writes and local Git mutations.

Model and provider output is untrusted. Models may interpret intent, rank application-discovered candidates, and generate content, but they do not own paths, source locations, validation, writes, or claims of success. Project access stays relative to the opened root and rejects protected, escaping, binary, non-UTF-8, oversized, ambiguous, or stale operations as appropriate.

Text inference, image generation, image editing and vision are separate capability contracts. Support for one never implies support for another. Optional local or cloud providers must not disable the core local workflow, and private source or credentials must not be exposed unnecessarily.

## Application-led editing

AIIDE's direction is deterministic, application-led editing. The application discovers and validates possible targets, the model selects only from opaque verified choices and generates bounded content, and Rust revalidates before assembling a reviewable proposal. Broader editing should extend application-owned target discovery, explicit disambiguation, validation and recoverable review rather than returning source-location authority to the model.

Only an explicit approved action may apply a pending file change or perform a Git mutation. Rejecting a proposal writes nothing. Remote operations, command execution, generated binary assets and other consequential actions require their own narrow contracts and approval boundaries. Elma reports success only after application confirmation.

## Interface conventions

Keep the interface chat-first with restrained permanent navigation and progressive disclosure for diagnostics or advanced controls. Use consistent proposal, Git and future result cards; preserve clear focus, disabled, busy, success and error states. Controls require accessible names and keyboard behaviour. Animation must respect reduced-motion preferences, and essential state must not depend on motion, colour, or decorative Elma sprites alone.

Elma's voice is defined in [PERSONALITY.md](PERSONALITY.md). Personality never overrides application state, uncertainty, safety or verification.

## Development conventions

Prefer existing service/type boundaries and keep filesystem, provider and mutation policy in Rust. Keep project data bounded and diagnostics reviewable because traces may contain prompts, paths or source context. Avoid introducing dependencies where the existing stack suffices. Preserve current behaviour unless the task explicitly changes it, and add focused regression coverage for new behaviour or confirmed fixes.

See [REPO-MAP.md](REPO-MAP.md) for code ownership, [MODEL-PROFILES.md](MODEL-PROFILES.md) for model compatibility and benchmark usage, and [STANDALONE-DISTRIBUTION-DESIGN.md](STANDALONE-DISTRIBUTION-DESIGN.md) for the planned packaging and runtime boundary.
