# AIIDE

AIIDE is an early-development, local-first desktop coding workspace. Its long-term vision is to help developers use local AI models to work with real repositories while keeping changes controlled and reviewable.

The planned workflow is **Open → Ask → Inspect → Edit → Review → Test → Commit**. Milestone 04 supports opening a folder, repository-aware local chat through Ollama, and reviewable modifications to existing text files. Elma can request bounded `list_files`, `search_files`, and `read_file` operations, then propose exact replacements. The app sends only project name and Git summary automatically; file contents are retrieved on demand.

The Rust backend validates every model-generated path against the currently opened project. Absolute paths, traversal, symlink escapes, generated directories, and common sensitive files such as `.env`, keys, and credentials are blocked. Binary and non-UTF-8 files are not sent to the model. A request is limited to eight tool calls and 48 KB of repository context; individual reads return at most 12 KB from files no larger than 256 KB. Listings return at most 120 entries; searches return at most 30 matches and scan at most 2,000 files or 8 MB. Large files and results report truncation or a limit.

Ollama's JSON-schema output format constrains tool, answer, and change-proposal responses. AIIDE treats proposals as untrusted: Rust validates paths and exact replacements, captures the original file snapshot, and generates the pending before/after content shown in the Changes panel. Only the explicit Apply button writes; Reject writes nothing. Apply rechecks the complete original snapshot and fails closed if the file changed after review began.

Milestone 04 proposals are limited to one existing UTF-8 text file, four exact unambiguous replacements, and 24 KB of proposal content. New files, deletion, rename, fuzzy matching, automatic merge, commands, and Git mutations are not supported.

## Local image generation

M08A adds an opt-in **Image** composer mode backed by a separately running local ComfyUI service. It uses a fixed SDXL 1.0 workflow at 1024 × 1024 with one GPU-heavy job at a time. Generated PNGs stay in temporary storage until **Save to project** is selected; **Reject** removes the temporary copy without modifying the project. Images never enter the text proposal or diff pipeline.

Windows setup:

1. Install the official [ComfyUI Desktop app](https://www.comfy.org/download), or the official NVIDIA Windows portable build from the [ComfyUI releases](https://github.com/Comfy-Org/ComfyUI/releases).
2. Download `sd_xl_base_1.0.safetensors` from the [official SDXL model page](https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0) and place it in `ComfyUI\models\checkpoints`. The checkpoint is approximately 6.94 GB; review its CreativeML Open RAIL++-M license before use.
3. Start ComfyUI yourself and leave it bound to the local machine. AIIDE uses `http://127.0.0.1:8188` by default.
4. If needed, set `AIIDE_COMFYUI_ENDPOINT` to another loopback-only HTTP URL and `AIIDE_COMFYUI_CHECKPOINT` to the exact ComfyUI-relative checkpoint filename before launching AIIDE.
5. On an 8 GB card, close other GPU workloads. If ComfyUI reports an out-of-memory failure, restart its portable server with `--lowvram`; this trades speed and system memory for lower VRAM use.

AIIDE does not install or start ComfyUI, download models, or use ComfyUI's global interrupt endpoint. Running-job cancellation requires a current ComfyUI version with prompt-specific cancellation; older versions can still cancel jobs that have not started.

## Agent Debug Mode

Use the secondary **Debug [OFF/ON]** toggle beside the model selector, reproduce one request, then choose **Agent diagnostics → Copy trace**. Debug Mode retains only the latest request in memory and prints the same chronological trace to the `npm run tauri dev` terminal. Traces include request metadata, supplied messages and schemas, exact raw model responses, parse results, repairs, tool summaries, final stages, and timings. They can contain prompts, local paths, and source context, so review them before sharing. Debug Mode changes observability only and is off by default.

See [PROJECT.md](PROJECT.md) for the detailed specification and roadmap.
