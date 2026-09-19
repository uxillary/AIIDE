# M08A local image generation — Phase 1 design

Status: approved baseline implemented on `image-generation`.

## Approved baseline

This milestone introduces two material external choices: a multi-gigabyte model/runtime and a ComfyUI API compatibility floor. The recommended baseline is:

- ComfyUI as an independently installed, user-managed local service.
- SDXL 1.0 base (`sd_xl_base_1.0.safetensors`) as the first supported checkpoint.
- Current ComfyUI with core nodes only; no custom nodes, model downloads, service management, or Python dependencies inside AIIDE.
- `1024 × 1024`, batch size 1, 30 steps, `dpmpp_2m`/`karras`, CFG 7, and a random seed as quality-focused defaults.
- One image job at a time. No refiner, upscaler, LoRA, prompt rewriting, image editing, or image analysis in M08A.

This baseline was approved for Phase 2. The alternatives below remain useful context for later milestones because they materially change installation size, compatibility, licensing, workflow shape, and memory behaviour.

## What exists today

### Chat and provider flow

- `src/components/LocalChat.tsx` owns session-only chat state and calls the frontend `AIProvider` interface.
- `src/services/ai/ollama.ts` invokes `ollama_status` and `ollama_chat` through Tauri.
- `src-tauri/src/ollama.rs` validates requests, routes repository-aware turns, emits activity events, retries malformed structured output, and creates text-only pending proposals.
- `src-tauri/src/model_provider.rs` abstracts text inference across Ollama and OpenRouter. Its request/response contract is not suitable for long-running binary image jobs.

Image generation should therefore use a separate frontend `ImageProvider` and Rust `ImageGenerationProvider` contract. It must not add image fields or lifecycle states to `ModelProvider`.

### Approval and repository boundary

- `src-tauri/src/repository.rs` validates project-relative text targets, snapshots original UTF-8 content, and writes only after `apply_pending_change`.
- `src/components/ChangesPanel.tsx` renders text diffs and Apply/Reject controls.
- Opening another project clears the pending text proposal.

Generated images must use independent pending-image state and commands. Binary bytes must never become `PendingChange`, pass through text decoding, or appear in the Changes diff. Saving remains a Rust-owned, project-relative operation after an explicit Save action.

### Errors, cancellation, tracing, and tests

- Chat networking has bounded request timeouts and provider-neutral user errors, but no request cancellation.
- Agent debug tracing is in-memory and chat-specific. Image tracing should be a small separate in-memory event log with prompt text omitted by default.
- Tauri events currently carry activity labels to the chat UI.
- Rust unit tests cover providers, routing, repository path safety, proposal validation, rejection, approval, and stale writes. There is no frontend unit-test harness; frontend regression protection is lint, typecheck, and build.

## ComfyUI integration

ComfyUI has a local workflow API, asynchronous queueing, history, image retrieval, and progress events. Its official websocket example submits `POST /prompt`, follows `/ws`, reads `/history/{prompt_id}`, and retrieves outputs from `/view`. A second official example can stream final images over a websocket, but it relies on the bundled example `SaveImageWebsocket` custom node rather than a core node.

M08A should favour core nodes and broad install reliability:

1. Build an embedded, fixed SDXL API workflow in Rust.
2. Substitute only validated prompt, checkpoint filename, dimensions, seed, steps, CFG, sampler, and scheduler values.
3. Use `PreviewImage` so output stays in ComfyUI's temporary area rather than its project/output directory.
4. Submit with a generated client/job ID.
5. Poll queue/history for `queued`, `running`, `complete`, `failed`, or `cancelled`; emit only real state transitions and node labels. Do not invent percentages.
6. On completion, retrieve the referenced PNG through `/view`, validate its content type, PNG signature, and a bounded byte size, then copy it into an AIIDE-owned per-session temporary directory.
7. Serve preview bytes through a Tauri command as a raw binary response; the React card creates and later revokes an object URL.
8. Delete AIIDE's temporary copy on reject, after an approved save, when it is superseded, when a project changes, and when application state is dropped. ComfyUI owns cleanup of its own `PreviewImage` temporary file.

The provider endpoint defaults to `http://127.0.0.1:8188`. `AIIDE_COMFYUI_ENDPOINT` may override the port or loopback host for M08A. Rust must reject non-HTTP schemes, credentials, query/fragment components, and non-loopback hosts so this setting cannot become an SSRF route. `AIIDE_COMFYUI_CHECKPOINT` selects the installed checkpoint filename; no model is downloaded automatically.

### Proposed command lifecycle

- `image_generation_status()` — engine reachability, configured checkpoint, and busy state.
- `start_image_generation(prompt)` — reserve the single GPU slot, validate the fixed workflow, submit it, and return the job ID immediately.
- `get_image_generation(job_id)` — return the observed state and, when complete, preview metadata.
- `get_image_preview(job_id)` — return bounded PNG bytes from AIIDE temporary storage.
- `cancel_image_generation(job_id)` — cancel only that prompt when the installed ComfyUI supports prompt-specific cancellation; otherwise safely remove it only if still queued.
- `reject_generated_image(job_id)` — remove pending metadata and temporary bytes without touching the project.
- `save_generated_image(job_id, relative_path)` — validate and collision-resolve the target, copy bytes into the open project, then clean temporary state.

Current ComfyUI master has `POST /api/jobs/{job_id}/cancel`, which atomically targets a running or queued job. Older versions expose `/interrupt`, which is global and can stop an unrelated workflow. AIIDE must never fall back to global interruption. On older versions, a running job becomes “cancellation unsupported by this ComfyUI version”; a queued prompt may still be deleted specifically through the queue API.

### Concurrency and state

A dedicated Rust state object owns at most one active or reviewable image job. It reserves the slot before network submission, so simultaneous UI calls cannot queue multiple GPU-heavy jobs. The state records the AiiDE job ID, ComfyUI prompt ID, actual provider state, prompt, seed, temporary path, output metadata, and cancellation intent. Locks are never held across network awaits.

`Regenerate` rejects/cleans the previous preview and submits the same unmodified prompt with a new random seed. It is disabled while a job is active.

## Save safety

The save command accepts only a project-relative filename ending in `.png`. It rejects empty paths, roots/prefixes, `..`, symlink traversal, protected paths, directories, and targets that resolve outside the canonical project root. Parent directory creation should be limited to the validated target path.

Collision handling is non-destructive: `assets/elma.png` becomes `assets/elma-2.png`, then `assets/elma-3.png`, using exclusive file creation so a race cannot overwrite an existing asset. The command returns the actual saved project-relative path. Rejection never writes. A failed copy retains the pending preview for retry.

## Minimal UI

Add a small `Chat` / `Image` composer mode control, defaulting to Chat. This is the explicit routing boundary: image prompts never pass through Ollama and ordinary coding prompts remain unchanged.

An `ImageResultCard` appears in the normal message stream and owns these states:

- checking/submitting
- queued
- generating, with the actual ComfyUI node label when supplied
- preview ready
- saving/saved
- failed
- cancelled or cancellation unsupported

The card shows Preview, Regenerate, Save to project, and Cancel only where applicable. Saving asks for a project-relative `.png` path inline. It does not reuse the Changes sidebar because that sidebar represents reviewable text diffs.

## Error mapping

Errors remain actionable and do not expose arbitrary response bodies:

- connection refused/timeout: “ComfyUI is unavailable at the configured local endpoint.”
- workflow validation names the checkpoint: “The configured SDXL checkpoint is not installed in ComfyUI.”
- node/class validation: “This ComfyUI installation does not support the required core workflow.”
- execution messages containing CUDA allocation/out-of-memory signals: explain that 8 GB is at the supported lower bound and suggest closing other GPU workloads or starting ComfyUI with its low-VRAM mode; do not retry automatically.
- malformed history/output, non-image content, oversized output, or missing preview: provider failure with retry guidance.
- concurrent start: “Another image generation is already running.”

## Model evaluation

### Recommended: SDXL 1.0 base

SDXL base is a 3B-parameter model with an official 6.94 GB single checkpoint and can run without its refiner. ComfyUI's maintained model guidance places SDXL base at an 8 GB VRAM minimum and 12 GB recommended. It is the most conservative quality/reliability choice for an RTX 3070 Ti: mature native support, a compact core-node workflow, no gated text-encoder bundle, and no quantization/custom-node dependency.

Trade-offs: 8 GB is the floor, so model offloading and slower generation are expected; the refiner is excluded because the documented class is 16 GB+; generated text, anatomy, compositional precision, and photorealism remain imperfect. A 1024-square generation is the baseline. Wider/taller presets and batches wait for a later milestone.

### Option: Stable Diffusion 3.5 Medium

SD3.5 Medium is newer, 2.5B parameters, and generally offers stronger prompt adherence and text rendering. Its official model file is about 5.11 GB, but Stability reports 9.9 GB VRAM for full performance excluding text encoders. ComfyUI also requires separate CLIP-L, CLIP-G, and T5-XXL weights unless an all-in-one checkpoint is used; the lower-memory T5 option is FP8. On an 8 GB card this necessarily relies on offloading/quantization and more system RAM, so it is not the reliability-first baseline. The official weights are gated and use the Stability AI Community License.

### Rejected for the first baseline

- FLUX.1 schnell: the official model file is about 23.8 GB; reference precision is commonly a 24 GB-class workload and 8 GB use depends on quantization/offloading.
- Z-Image Turbo: the official model is 6B parameters and advertises comfortable operation at 16 GB VRAM. An 8 GB setup again depends on lower precision/offloading and newer workflow support.
- SDXL base plus refiner, high-resolution fix, upscaling, or multi-stage workflows: materially higher memory, runtime, and failure surface than this first vertical slice.

## Windows setup if the recommendation is approved

1. Install the official ComfyUI Desktop app, or the official Windows portable NVIDIA build for RTX 20-series and newer cards.
2. Download `sd_xl_base_1.0.safetensors` manually from Stability AI and place it in `ComfyUI\models\checkpoints`. The file is approximately 6.94 GB and uses the CreativeML Open RAIL++-M license.
3. Start ComfyUI yourself and keep it bound to loopback. The normal endpoint is `http://127.0.0.1:8188`.
4. If necessary, set `AIIDE_COMFYUI_ENDPOINT` to another loopback URL and `AIIDE_COMFYUI_CHECKPOINT` to the exact installed filename before launching AIIDE.
5. If normal mode exhausts 8 GB VRAM, start the portable server with ComfyUI's `--lowvram` option. Expect slower generation and additional system-memory use.

AIIDE will not install ComfyUI, download weights, alter the service, or start/stop it.

## Planned files

- `src-tauri/src/image_generation.rs`: provider trait, ComfyUI adapter, fixed workflow, job state, commands, safe save/cleanup, and deterministic tests.
- `src-tauri/src/lib.rs`: state and command registration only.
- `src/types/image.ts`: frontend job/status contracts.
- `src/services/image/provider.ts` and `src/services/image/comfyui.ts`: separate image-provider boundary.
- `src/components/ImageResultCard.tsx`: reusable result card.
- `src/components/LocalChat.tsx`: explicit composer route and message-card integration without changing ordinary chat requests.
- `src/styles.css`: scoped card and mode-control styles using current tokens.
- `README.md`: opt-in Windows/ComfyUI/model setup and limitations.

No new frontend dependency is required. The initial Rust implementation can use the existing `reqwest`, `serde`, and `serde_json` stack with HTTP polling. If websocket progress or direct websocket image transport becomes mandatory, that should be a later, separately reviewed dependency change.

## Test plan

Provider behavior will be behind a mockable Rust trait. Deterministic tests will cover:

- explicit image routing and unchanged chat routing
- unavailable engine and missing checkpoint
- submission and execution failure, including OOM mapping
- queued cancellation and supported prompt-specific cancellation
- cancellation-not-supported behavior without global interrupt
- successful PNG retrieval and preview state
- reject without project write
- approved save
- traversal, absolute, protected, symlink, and non-PNG paths
- exclusive collision naming
- cleanup after reject, save, supersede, project change, and state drop
- the single-job concurrency guard

Verification after implementation: focused Rust tests, full Rust tests, `npm run lint`, `npm run typecheck`, and `npm run build`. A real GPU run is performed only when ComfyUI and the selected checkpoint are already installed; mocked tests are not evidence of image quality.

## Sources

- ComfyUI repository and Windows installation: <https://github.com/Comfy-Org/ComfyUI>
- ComfyUI official websocket API example: <https://github.com/Comfy-Org/ComfyUI/blob/master/script_examples/websockets_api_example.py>
- ComfyUI direct websocket image example: <https://github.com/Comfy-Org/ComfyUI/blob/master/script_examples/websockets_api_example_ws_images.py>
- ComfyUI server routes and job cancellation: <https://github.com/Comfy-Org/ComfyUI/blob/master/server.py>
- SDXL 1.0 model card and weights: <https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0>
- ComfyUI maintained model guidance: <https://github.com/Comfy-Org/workflow_templates/blob/main/site/knowledge/models/sdxl.md>
- SD3.5 ComfyUI example and dependencies: <https://github.com/comfyanonymous/ComfyUI_examples/tree/master/sd3>
- Stability AI SD3.5 hardware statement: <https://stability.ai/news/introducing-stable-diffusion-3-5>
- FLUX.1 schnell official weights: <https://huggingface.co/black-forest-labs/FLUX.1-schnell>
- Z-Image Turbo model card: <https://huggingface.co/Tongyi-MAI/Z-Image-Turbo>
