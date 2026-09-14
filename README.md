# AIIDE

AIIDE is an early-development, local-first desktop coding workspace. Its long-term vision is to help developers use local AI models to work with real repositories while keeping changes controlled and reviewable.

The planned workflow is **Open → Ask → Inspect → Edit → Review → Test → Commit**. Milestone 03 supports opening a folder, inspecting its file tree and Git state, and repository-aware local chat through Ollama. Elma can request bounded, read-only `list_files`, `search_files`, and `read_file` operations. The app sends only project name and Git summary automatically; file contents are retrieved on demand. A completed answer shows a compact trail of inspected items.

The Rust backend validates every model-generated path against the currently opened project. Absolute paths, traversal, symlink escapes, generated directories, and common sensitive files such as `.env`, keys, and credentials are blocked. Binary and non-UTF-8 files are not sent to the model. A request is limited to eight tool calls and 48 KB of repository context; individual reads return at most 12 KB from files no larger than 256 KB. Listings return at most 120 entries; searches return at most 30 matches and scan at most 2,000 files or 8 MB. Large files and results report truncation or a limit.

Ollama's JSON-schema output format constrains tool and answer responses. AIIDE validates the resulting action and arguments in Rust and allows up to two corrective retries for malformed or ambiguous replies. If a project-specific answer would lack file evidence, Elma is prompted to inspect it; general questions can be answered directly. Search is a simple bounded literal-text filesystem scan; there is no index or persistent repository memory yet. Small local models can still make weak judgments about which files to inspect or how to interpret them. Repository access is read-only: editing, command execution, Git mutations, and change review are future work.

See [PROJECT.md](PROJECT.md) for the detailed specification and roadmap.
