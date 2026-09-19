# Durable AIIDE context

AIIDE is the current repository/application name. **Elma** is its local AI coding assistant and candidate product identity. AIIDE is a Windows-first, local-first coding application, not a full IDE. The intended workflow is **Open → Ask → Inspect → Edit → Review → Test → Commit**. See [PROJECT.md](../PROJECT.md) for the development roadmap; planned stages are not proof of implemented features.

## Current implementation

The frontend uses React, TypeScript, Vite, and Tailwind CSS; the desktop backend uses Tauri 2 and Rust. Local inference runs through Ollama, primarily tested with `qwen2.5-coder:7b`. The design should allow interchangeable local models. Users can open a project, inspect its file tree and Git summary, and ask Elma questions. Elma can request bounded `list_files`, `search_files`, and `read_file` operations. The current experimental editing path lets Elma propose up to four exact replacements in one existing UTF-8 text file. Rust validates the path and unique source text, captures the original snapshot, and generates a reviewable pending change. Only the user's Apply action writes, after a stale-snapshot check. File creation, multi-file edits, command execution, and Git mutations are not implemented. [REPO-MAP.md](REPO-MAP.md) identifies current owners; `src-tauri/src/repository.rs` defines current limits.

Selecting a file in the sidebar currently opens a separate read-only viewer. The selected path and viewer contents are not supplied to Elma; repository questions still require the agent's bounded discovery and read tools.

## Application-led direction

Experiments on `fix/reliable-editing` improved source validation but did not achieve reliable real-world editing acceptance: the small model still selected incorrect source targets. That branch is historical/experimental and is not approved for merging. Do not treat its implementation as the active design.

AIIDE should own repository discovery and contextual retrieval, verified file and source references, deterministic file creation and editing, change assembly and diffs, snapshot and path validation, explicit review and approval, and test execution through controlled, authorised commands. Later it should prepare Git branches, commits, and optional GitHub pull requests. Elma should interpret requests, analyse supplied code, generate replacement content, help choose among verified candidates, explain changes and test failures, and suggest fixes. The model must not invent original source text, silently resolve ambiguous targets, write arbitrary files, or execute unrestricted commands. In the first deterministic editing stage, the user selects the file and source range; Elma is not required.

The Rust/Tauri backend remains the trusted project and filesystem boundary. Model output and tool arguments are untrusted; access must be project-relative, protected, and bounded. The user approves changes before application and controls external publishing actions. Keep the current bounded inspection and approved one-file proposal behavior accurate while building the new workflow in small stages. Preserve existing passing behavior unless an established contract change is explicitly approved.

Elma's personality instructions remain separate from core protocol and safety rules. Agent Debug Mode is an observability-only, in-memory trace of the latest request, available in the UI and development terminal when enabled; it must not alter prompts, schemas, model settings, permissions, or behavior.
