# AIIDE

AIIDE is an early-development, local-first desktop coding workspace. Its long-term vision is to help developers use local AI models to work with real repositories while keeping changes controlled and reviewable.

The planned workflow is **Open → Ask → Inspect → Edit → Review → Test → Commit**. Milestone 04 supports opening a folder, repository-aware local chat through Ollama, and reviewable modifications to existing text files. Elma can request bounded `list_files`, `search_files`, and `read_file` operations, then propose exact replacements. The app sends only project name and Git summary automatically; file contents are retrieved on demand.

The Rust backend validates every model-generated path against the currently opened project. Absolute paths, traversal, symlink escapes, generated directories, and common sensitive files such as `.env`, keys, and credentials are blocked. Binary and non-UTF-8 files are not sent to the model. A request is limited to eight tool calls and 48 KB of repository context; individual reads return at most 12 KB from files no larger than 256 KB. Listings return at most 120 entries; searches return at most 30 matches and scan at most 2,000 files or 8 MB. Large files and results report truncation or a limit.

Ollama's JSON-schema output format constrains tool, answer, and change-proposal responses. AIIDE treats proposals as untrusted: Rust validates paths and exact replacements, captures the original file snapshot, and generates the pending before/after content shown in the Changes panel. Only the explicit Apply button writes; Reject writes nothing. Apply rechecks the complete original snapshot and fails closed if the file changed after review began.

Milestone 04 proposals are limited to one existing UTF-8 text file, four exact unambiguous replacements, and 24 KB of proposal content. New files, deletion, rename, fuzzy matching, automatic merge, commands, and Git mutations are not supported.

See [PROJECT.md](PROJECT.md) for the detailed specification and roadmap.
