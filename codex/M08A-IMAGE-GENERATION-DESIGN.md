# M08A local image generation and managed-engine direction

Status: Phase 2 baseline **IMPLEMENTED** on `image-generation`; managed-runtime security/ownership foundations are **IMPLEMENTED BUT DISABLED**. Acquisition, execution, repair/removal, release trust, legal approval, real GPU acceptance, and production-model selection remain pending.

## Implemented prototype baseline

This milestone introduced two material external choices: a multi-gigabyte model/runtime and a ComfyUI API compatibility floor. The implemented prototype baseline is:

- ComfyUI as an independently installed, user-managed local service.
- SDXL 1.0 base (`sd_xl_base_1.0.safetensors`) as the first supported checkpoint.
- Current ComfyUI with core nodes only; no custom nodes, model downloads, service management, or Python dependencies inside AIIDE.
- `1024 × 1024`, batch size 1, 30 steps, `dpmpp_2m`/`karras`, CFG 7, and a random seed as quality-focused defaults.
- One image job at a time. No refiner, upscaler, LoRA, prompt rewriting, image editing, or image analysis in M08A.

This baseline was approved for Phase 2 and remains the acceptance-test baseline until a replacement is demonstrated. It does not select SDXL 1.0 as AIIDE's production default. The alternatives below remain useful context because they materially change installation size, compatibility, licensing, workflow shape, and memory behaviour.

## Approved product direction

AIIDE's target is a cohesive Windows desktop experience: install AIIDE, optionally enable image generation, approve the required downloads, let AIIDE configure the engine and selected model, and generate through Elma without manually installing Python, configuring ComfyUI, starting an inference server, or using terminal commands. Runtime and model acquisition will remain optional, consent-based, and separate from the primary installer. Local generation will not require a paid cloud API.

Application-managed ComfyUI is the selected initial engine strategy. AIIDE will eventually install, configure, start, and stop an approved pinned ComfyUI runtime through a separately approved management mechanism. External user-managed ComfyUI remains supported for compatibility; AIIDE must never terminate, update, overwrite, repair, or uninstall a runtime it does not own.

This is a **PLANNED** product direction, not current functionality. Managed installation remains gated on version pinning, archive verification, compatibility testing, legal/licensing review, safe extraction, and reliable process ownership. Fully embedded inference is **DEFERRED** for investigation. ComfyUI remains behind the existing image-provider abstraction so another engine can replace it without replacing the composer, result cards, or Save/Reject approval flow.

## What exists today

### Chat and provider flow

- `src/components/LocalChat.tsx` owns session-only chat state and calls the frontend `AIProvider` interface.
- `src/services/ai/ollama.ts` invokes `ollama_status` and `ollama_chat` through Tauri.
- `src-tauri/src/ollama.rs` validates requests, routes repository-aware turns, emits activity events, retries malformed structured output, and creates text-only pending proposals.
- `src-tauri/src/model_provider.rs` abstracts text inference across Ollama and OpenRouter. Its request/response contract is not suitable for long-running binary image jobs.

Image generation therefore uses a separate frontend `ImageProvider` and Rust `ImageGenerationProvider` contract. It does not add image fields or lifecycle states to `ModelProvider`.

### Approval and repository boundary

- `src-tauri/src/repository.rs` validates project-relative text targets, snapshots original UTF-8 content, and writes only after `apply_pending_change`.
- `src/components/ChangesPanel.tsx` renders text diffs and Apply/Reject controls.
- Opening another project clears the pending text proposal.

Generated images use independent pending-image state and commands. Binary bytes never become `PendingChange`, pass through text decoding, or appear in the Changes diff. Saving remains a Rust-owned, project-relative operation after an explicit Save action.

### Errors, cancellation, tracing, and tests

- Chat networking has bounded request timeouts and provider-neutral user errors, but no request cancellation.
- Agent debug tracing is in-memory and chat-specific. Image activity currently uses bounded console events rather than a persistent diagnostic log.
- Tauri events currently carry activity labels to the chat UI.
- Rust unit tests cover the provider boundary, fixed workflow, lifecycle, cancellation behavior, temporary previews, rejection, safe saves, collision handling, and path protection. There is no frontend unit-test harness; frontend regression protection is lint, typecheck, and build.

### Phase 3 baseline audit

The implementation has the intended architecture and should be extended rather than rewritten:

- `src-tauri/src/image_generation.rs` owns the provider trait, loopback-only ComfyUI adapter, fixed core-node SDXL workflow, single-job state, polling, prompt-specific cancellation, bounded PNG retrieval, temporary storage, Reject, and protected collision-safe Save.
- `src/services/image/` and `src/types/image.ts` keep the frontend provider contract separate from chat inference.
- `LocalChat.tsx` retains the Chat/Image composer boundary, filters image turns out of coding chat history, and polls only the current image job.
- `ImageResultCard.tsx` renders actual lifecycle states and requires an explicit project-relative Save.
- Tauri command registration is narrow and image bytes remain outside the text proposal path.

The following gaps remain:

- `image_generation_status()` currently proves only that `GET /system_stats` succeeds. It does not verify a supported ComfyUI version, required core nodes, the checkpoint list, GPU/device suitability, or the prompt-specific cancellation endpoint.
- Configuration still relies on developer environment variables for non-default endpoints and checkpoint names.
- There is no capability/setup state model, compact guided setup UI, managed installation record, runtime acquisition, process ownership, update, repair, or uninstall path.
- There is no live ComfyUI/SDXL evidence. Mock-provider tests prove the application lifecycle, not GPU execution, output quality, performance, or VRAM behavior.
- A transient provider polling error is reported to the UI but does not by itself establish a terminal ComfyUI job state; live acceptance must exercise disconnect and recovery behavior.

## ComfyUI integration

ComfyUI has a local workflow API, asynchronous queueing, history, image retrieval, and progress events. Its official websocket example submits `POST /prompt`, follows `/ws`, reads `/history/{prompt_id}`, and retrieves outputs from `/view`. A second official example can stream final images over a websocket, but it relies on the bundled example `SaveImageWebsocket` custom node rather than a core node.

M08A favours core nodes and broad install reliability:

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

### Implemented prototype baseline: SDXL 1.0 base

SDXL base is a 3B-parameter model with an official 6.94 GB single checkpoint and can run without its refiner. ComfyUI's maintained model guidance places SDXL base at an 8 GB VRAM minimum and 12 GB recommended. It was the conservative prototype choice for the RTX 3070 Ti: mature native support, a compact core-node workflow, no gated text-encoder bundle, and no quantization/custom-node dependency.

Trade-offs: 8 GB is the floor, so model offloading and slower generation are expected; the refiner is excluded because the documented class is 16 GB+; generated text, anatomy, compositional precision, and photorealism remain imperfect. A 1024-square generation is the baseline. Wider/taller presets and batches wait for a later milestone.

### Option: Stable Diffusion 3.5 Medium

SD3.5 Medium is newer, 2.5B parameters, and generally offers stronger prompt adherence and text rendering. Its official model file is about 5.11 GB, but Stability reports 9.9 GB VRAM for full performance excluding text encoders. ComfyUI also requires separate CLIP-L, CLIP-G, and T5-XXL weights unless an all-in-one checkpoint is used; the lower-memory T5 option is FP8. On an 8 GB card this necessarily relies on offloading/quantization and more system RAM, so it is not the reliability-first baseline. The official weights are gated and use the Stability AI Community License.

### Rejected for the first baseline

- FLUX.1 schnell: the official model file is about 23.8 GB; reference precision is commonly a 24 GB-class workload and 8 GB use depends on quantization/offloading.
- Z-Image Turbo: the official model is 6B parameters and advertises comfortable operation at 16 GB VRAM. An 8 GB setup again depends on lower precision/offloading and newer workflow support.
- SDXL base plus refiner, high-resolution fix, upscaling, or multi-stage workflows: materially higher memory, runtime, and failure surface than this first vertical slice.

### Production model selection and registry

The production default model is undecided. A focused comparison and real RTX 3070 Ti acceptance are required before selection, covering verified engine compatibility, licence and redistribution terms, generation time, memory behaviour, workflow and dependency requirements, and representative image quality. Eight gigabytes of VRAM is a test constraint, not evidence that a model is accepted.

Stage B should introduce a small internal model registry, or equivalent typed configuration boundary, separate from runtime management. It should be capable of representing:

- a stable model identifier and display name;
- architecture and supported capabilities;
- compatible engine/runtime versions;
- the model-specific workflow and required nodes;
- required files, validated download sources, sizes, and expected SHA-256 hashes;
- licence and redistribution conditions;
- GPU/VRAM and storage guidance; and
- installed, unavailable, and incompatible states.

Different architectures may require different workflows and dependencies. A checkpoint must never be treated as a drop-in substitution for the fixed SDXL workflow unless its registry entry and verified workflow explicitly support that. M08A does not include a model marketplace or broad model-management interface.

## Phase 3 managed-runtime investigation and Stage C1 approval record

### Evaluated Windows options

| Option | Suitability | Decision |
| --- | --- | --- |
| Existing user-managed ComfyUI | Preserves current installations and is the fastest route to real GPU acceptance. Compatibility and lifecycle vary by installation. | Continue supporting loopback endpoints. Never stop or modify a process AIIDE did not launch. |
| Official Comfy Desktop | Officially recommended for normal ComfyUI users and manages isolated environments and updates. It is a separate Electron application, currently AGPL-3.0-or-later/commercial dual-licensed, and controls its own installs and release cadence. | Offer only as an external user-managed setup path. Do not silently embed, redistribute, automate, or take ownership without separate licensing and integration approval. |
| Official Windows portable NVIDIA archive | Self-contained embedded Python/PyTorch environment, no system Python requirement, official GitHub release assets, and explicit command-line control. Upstream describes portable as unsuitable for ordinary manual users, but that concern is the manual archive workflow that AIIDE could hide. | Recommended technical substrate for a future optional AIIDE-managed engine, subject to an immutable version pin, verified digest, licence review, extraction testing, and explicit user consent. |
| `comfy-cli` or a manual Python environment | Useful for developers but adds Python/package resolution and mutable dependency state. | Do not use for the normal supported AIIDE flow. |
| Fully embedded inference library | Removes the local HTTP service but makes AIIDE own model formats, CUDA/PyTorch packaging, scheduling, crash isolation, and hardware backends. | Deferred; the replaceable provider remains the architectural boundary. |

Managed ComfyUI is technically appropriate only as a separately installed optional capability. It must not be folded into the main AIIDE installer, the project repository, or the text-provider abstraction.

### Recommended distribution strategy

Use two supported ownership modes behind the existing provider:

1. **External** — detect and verify a user-started loopback ComfyUI service. AIIDE stores only the approved endpoint/checkpoint configuration and never starts, updates, stops, repairs, or removes that runtime.
2. **Managed** — after a later explicit install approval, acquire one pinned official Windows portable NVIDIA release into AIIDE's local application-data area, acquire the selected compatible model separately, and launch the engine as an AIIDE-owned child process with an argument array.

The managed layout should be resolved through Tauri's per-user application-data APIs rather than hard-coded absolute paths:

- `capabilities/image/comfyui/<pinned-version>/` — immutable runtime payload;
- `models/image/sdxl/` — separately retained model data;
- `downloads/staging/` — resumable partial artifacts;
- `runtime/image/<session>/` — temporary input/output/user directories and logs;
- a small installation record containing version, source URL, size, SHA-256, licence identifier, install path, ownership, and verification result.

No component belongs in the opened project. Updates install side by side and become active only after verification; they do not mutate a working runtime in place. Uninstall removes only records and files marked AIIDE-owned and asks separately whether to retain the model.

### Artifact and licence gates

- Pin an exact ComfyUI stable release and exact portable asset name. Never use a moving `/latest/` URL in a shipped manifest.
- Download only from the official `Comfy-Org/ComfyUI` GitHub release. Verify the asset against an expected SHA-256 stored in signed AIIDE release metadata before extraction or execution. GitHub exposes release-asset SHA-256 digests, but AIIDE must ship its expected value rather than trust mutable network metadata at install time.
- Reject archives with absolute paths, traversal, links/reparse points, duplicate destinations, or an unexpected top-level layout. Extract to a new staging directory and atomically promote it.
- For the existing SDXL acceptance baseline, pin `sd_xl_base_1.0.safetensors` from Stability AI's official Hugging Face repository: 6.94 GB, SHA-256 `31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b`, CreativeML Open RAIL++-M. Show the licence and use restrictions before any future download. A production model requires its own approved registry record.
- ComfyUI core is GPL-3.0. Comfy Desktop is AGPL-3.0-or-later or commercial. Release/legal review must confirm AIIDE's notices, source-offer obligations, separation model, and model-licence presentation before AIIDE distributes or automates either component. This document is not a legal determination.
- The runtime archive's compressed size and build inputs are recorded below. Installed size, the resolved Python dependency inventory, and complete third-party notices are not published with the release and remain blocking requirements. Do not substitute a moving estimate.

### Managed process contract

For an approved managed runtime, AIIDE should:

- select an unused loopback port, detect startup races/conflicts, and retry only with a new managed port;
- launch embedded Python directly with an argument array, never a batch file or shell, and never include prompt text in process arguments;
- pass explicit `--listen 127.0.0.1`, `--port`, `--disable-auto-launch`, `--disable-all-custom-nodes`, `--disable-api-nodes`, `--base-directory`, and managed temp/model directory arguments supported by the pinned version;
- keep the child handle for the current AIIDE session and stop only that child; a surviving process discovered after restart is external/unowned until the user resolves it;
- use bounded startup/shutdown timeouts, capture bounded redacted diagnostics, and never terminate a process merely because it owns the expected port;
- keep ComfyUI's targeted job cancellation and never fall back to the global interrupt route;
- work offline after the verified runtime and model are installed, without background update requirements.

Windows Job Object ownership is the preferred crash-cleanup mechanism, but it should be approved with the managed-process implementation because it may require a small Windows-specific dependency. Without reliable ownership, automatic shutdown must be deferred rather than approximated with stored process IDs.

### Stage C1 pinned runtime candidate

The C1 investigation was performed on 2026-09-21. The exact candidate is the official ComfyUI `v0.36.0` Windows NVIDIA portable release, published on 2026-09-15 and built from tag commit `ee71d5c4993f29086b27fde1629a945ae48425bf`.

| Field | Verified value |
| --- | --- |
| Release | `v0.36.0` |
| Official release page | `https://github.com/Comfy-Org/ComfyUI/releases/tag/v0.36.0` |
| Asset | `ComfyUI_windows_portable_nvidia.7z` |
| GitHub release asset ID | `566545265` |
| Official download URL | `https://github.com/Comfy-Org/ComfyUI/releases/download/v0.36.0/ComfyUI_windows_portable_nvidia.7z` |
| Compressed size | `1,917,442,353` bytes (about 1.917 GB / 1.786 GiB) |
| SHA-256 | `c3c60192840f8b68c9a47cf3e8161ecb108e5ffdf5ea236c1c72a402e442695d` |
| Upstream platform label | NVIDIA default, `cu130`, Python 3.13 |
| Embedded Python build | Python `3.13.14` Windows embeddable x64 |

GitHub marks this release as immutable and states that only its title and notes can be modified. The tag-specific asset URL is therefore the official immutable upstream source. AIIDE must still bind the release tag, source commit, asset name, GitHub asset ID, exact byte count, and SHA-256 together in signed AIIDE release metadata; the independently shipped digest, rather than live network metadata, is the install-time trust root. Any mismatched response must fail closed, and a removed or unavailable asset must not fall back to `/latest/` or another build. A mirror is unnecessary for identity but would require a separate availability and licence decision.

The archive's installed size is **not verified**. Neither the release metadata nor the pinned build workflow publishes an extracted-size manifest, and C1 was explicitly prohibited from downloading the archive. C2 must not begin until a separately approved inspection of this exact asset, or an upstream manifest, records the extracted byte count, file count, maximum extraction requirement, and an allowance for download staging, extraction staging, logs, and rollback.

### Bundled dependency evidence and limits

The pinned build workflows verify that the portable archive contains:

- ComfyUI source from the pinned release checkout;
- Python 3.13.14 embedded x64, `pip`, and the Python packages installed from ComfyUI's requirements;
- NVIDIA PyTorch, torchvision, and torchaudio wheels selected from the official PyTorch `cu130` wheel index;
- `pygit2` and upstream update scripts, even though an AIIDE-managed runtime must never invoke those update paths;
- TAESD approximate-VAE `.safetensors` files copied from a shallow clone of `comfyanonymous/taesd`; and
- the portable launcher files and `.comfy_environment` marker. The build removes `dnnl.lib`, `libprotoc.lib`, and `libprotobuf.lib` from the packaged PyTorch tree.

The release does **not** include an SDXL checkpoint. Model weights remain a separate acquisition, licence, version, hash, storage, and uninstall decision.

Only some direct ComfyUI dependencies are exactly pinned by the release requirements: `comfyui-frontend-package==1.52.7`, `comfyui-workflow-templates==0.11.62`, `comfyui-embedded-docs==0.5.11`, `comfy-kitchen==0.2.34`, and `comfy-aimdo==0.5.3`. PyTorch, torchvision, torchaudio, `pygit2`, TAESD's commit, and many direct/transitive Python packages are not pinned in the build source. The workflow installs the packages and clones TAESD at build time, so source inspection cannot reconstruct their exact resolved versions. A complete file/dependency inventory or upstream SBOM is therefore a blocking C2 input; the build is not reproducible from the tag alone.

### RTX 3070 Ti compatibility finding

Verified facts:

- NVIDIA lists the GeForce RTX 3070 Ti as compute capability 8.6 (Ampere).
- NVIDIA's compatibility matrix shows Ampere 8.0/8.6 supported from CUDA 11.0 and still supported by CUDA 13.x.
- ComfyUI's official portable guidance assigns the default CUDA 13.0/Python 3.13 archive to RTX 20-series and newer cards. Its CUDA 12.6 archive is for 10-series and older cards and explicitly is not the preferred build for 20-series and newer.
- NVIDIA's CUDA 13.x compatibility guidance requires the R580 driver branch or newer. CUDA 13 no longer bundles a Windows display driver, so acquisition must not attempt a driver installation.

The RTX 3070 Ti is therefore architecture-compatible with the selected archive. The exact supported Windows driver patch is not established by the reviewed upstream tables; managed readiness must require a detected R580-or-newer NVIDIA driver, successful CUDA initialization from ComfyUI's `/system_stats`, and a real inference smoke test. Eight GB VRAM is the current SDXL acceptance floor, not a guarantee of generation speed or freedom from out-of-memory failures. Shared-memory behavior, peak VRAM, chosen attention backend, and the need for a pinned low-memory option remain live-test findings rather than C1 facts.

### Licence and automated-acquisition gate

Verified licence facts:

- ComfyUI core is GPL-3.0 licensed.
- The embedded Python distribution is subject to the PSF licence and incorporated notices.
- PyTorch uses a BSD-style licence requiring preservation of its notices and disclaimer for binary redistribution.
- bundled CUDA runtime components are governed by the NVIDIA CUDA Toolkit EULA, which permits redistribution only for enumerated components and subject to its conditions;
- every bundled Python package and the TAESD assets retain their own notices and conditions; and
- the SDXL acceptance checkpoint is separately governed by CreativeML Open RAIL++-M and is not part of this runtime archive.

AIIDE downloading an official asset on the user's explicit request is operationally different from AIIDE repackaging or hosting it, but this document does not decide whether a particular release channel constitutes redistribution or what GPL source-delivery mechanism is sufficient. Before C2, release/legal approval must decide whether AIIDE may automate the official download, whether it may mirror the archive, which notices and source links/offers must be displayed or installed, and how NVIDIA redistributable and all transitive-package terms are satisfied. Approval requires the exact archive inventory; a core-repository licence review alone is insufficient.

### Secure acquisition, extraction, and recovery contract

An approved C2 must implement the following transaction and no broader package-manager behavior:

1. Load a signed, application-shipped manifest for the one approved asset. Show source, exact compressed and installed/storage requirements, licences, ownership, and offline behavior before obtaining consent. No download starts from merely entering Image mode.
2. Download over HTTPS into a fresh application-owned partial file. Follow only HTTPS redirects to an explicit GitHub release-host allowlist, send no credentials, cap bytes at the manifest size, and stream SHA-256 calculation.
3. Resume only when the saved source identity, ETag or equivalent validator, total length, and `Content-Range` agree. If the server ignores a range or any identity changes, discard that partial and restart. A completed file is usable only when both exact byte count and SHA-256 match the signed manifest.
4. Preflight free space for the retained archive, a complete extraction staging tree, the promoted installation, and rollback allowance. Extract with a reviewed in-process 7z implementation into a new random staging directory on the same volume as the final installation; do not run a downloaded extractor or a batch file.
5. Before writing entries, reject absolute, UNC, drive-qualified, traversal, alternate-data-stream, reserved-device, trailing-dot/space, overlong, case-insensitive duplicate, link, hardlink, reparse-point, sparse, excessive-count, or excessive-uncompressed-size entries. Require the expected `ComfyUI_windows_portable` layout and required executable/source files. Never execute from staging.
6. Record a bounded installed-file inventory, verify the runtime layout, atomically rename the verified tree into a versioned final directory, and write the installation record last. Change the active-version pointer only after a startup readiness check. Keep the prior verified version until rollback policy permits removal.
7. Persist transaction states such as downloading, archive-verified, extracting, installation-verified, and active. On restart, resume only a validated partial download; otherwise remove or quarantine only the exact recorded staging directory. A damaged active install is never repaired in place: reacquire to a new versioned directory and retain the prior known-good install.

The extractor/library, its licence, and the exact archive limits are unresolved C2 prerequisites. Shelling out to `7z.exe`, Windows Explorer, PowerShell archive commands, or an executable supplied by the archive is not approved.

### Windows process ownership and recovery contract

For managed mode, AIIDE must invoke the embedded `python.exe` directly with the pinned `ComfyUI/main.py`; it must never invoke `run_nvidia_gpu.bat`, an updater, a shell, or a command assembled as text. The pinned version supports the required `--listen`, `--port`, `--base-directory`, `--temp-directory`, `--user-directory`, `--models-directory`, `--disable-auto-launch`, `--disable-all-custom-nodes`, and `--disable-api-nodes` arguments. Managed mode must bind only to `127.0.0.1`, disable custom/API nodes, use explicit application-owned directories, and select a session port without touching a conflicting listener.

On Windows, process-tree ownership requires a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`:

1. create the Job Object and apply its limits;
2. create the ComfyUI process suspended, with non-inheritable handles and no breakaway permission;
3. assign the suspended process to the Job Object; and
4. resume it only after successful assignment.

If assignment fails, terminate only the just-created suspended process and report managed startup failure. Store the live Job Object and process handles as the ownership proof; a PID, port, executable path, or persisted session record is never sufficient authority to terminate a process. Monitor the process handle and bounded stdout/stderr logs, and declare readiness only after the child remains alive and the expected loopback API reports compatible engine/device data within a timeout.

Normal shutdown stops new submissions, performs the existing prompt-specific cancellation where applicable, waits for a bounded drain interval, and then terminates the owned Job Object if the process has not exited. No stable upstream graceful-shutdown API was verified, so C2 must not claim graceful engine shutdown or send a global interrupt. Closing the last Job Object handle provides process-tree cleanup if AIIDE crashes. After an AIIDE restart, ignore stale PIDs and never kill a discovered listener; start a new managed session or present it as an unowned conflict. Unexpected child exit records the exit code and bounded log tail, clears only session state, and offers a bounded explicit restart—never an infinite automatic restart loop.

### Managed/external separation and model registry

Persist engine configuration as an explicit tagged ownership mode, not as fields inferred from a path or endpoint:

- **External** retains the Stage B loopback endpoint and selected registry model. It carries no executable path, installation ID, process authority, repair/update state, or uninstall capability. AIIDE may probe it but must never launch, stop, mutate, or remove it.
- **Managed** stores an AIIDE installation ID, runtime version, manifest digest, selected registry model, and application-owned data roots. Its endpoint and live ownership handle are session data. Switching modes is explicit and preserves the external record; matching ports or executable paths never convert an external service into a managed one.

Runtime removal may delete only paths enumerated by a verified managed installation record beneath the resolved application-data root. Port conflicts cause managed mode to choose another port or fail; they never authorize terminating the occupant.

The model registry remains separate from the runtime manifest. Each model-package version needs a stable registry ID, architecture and workflow version, runtime compatibility range, required nodes/files, official source, exact sizes and SHA-256 values, licence/notice data, and hardware/storage guidance. Model files live in a versioned or content-addressed model root supplied through `--models-directory`, outside every runtime version. Model download, upgrade, rollback, selection, and removal are separate consented transactions; a runtime upgrade must neither overwrite nor delete a model, and the bundled updater must never manage that root. Readiness requires a compatible registry/workflow record plus verified files, not filename presence alone.

### Smallest safe Stage C2 proposal

C1 does **not** approve implementation yet. Once every blocking item below is approved, the smallest safe C2 is Windows x86-64 managed acquisition for this one runtime asset only:

- one signed static runtime manifest and its legal/notice bundle;
- an explicit-consent UI using factual byte counts and transaction states;
- resumable bounded download, SHA-256 verification, hardened 7z extraction, versioned installation, recovery, repair by reacquisition, rollback, and owned uninstall;
- the Windows Job Object supervisor and pinned loopback launch/readiness contract;
- the explicit external/managed configuration union, preserving the Stage B external setup; and
- the separate registry boundary needed to select an already installed compatible model.

C2 excludes model download, model marketplace UI, automatic runtime/model/driver updates, upstream updater execution, custom nodes, API nodes, CPU fallback, non-NVIDIA packages, non-Windows platforms, main-installer bundling, background acquisition, and changes to ordinary chat.

Acceptance requires deterministic tests for manifest tampering, byte/hash mismatch, interrupted and changed-source resume, disk exhaustion, all rejected archive-entry classes, crash at every transaction boundary, corrupt/partial installed state, port races, incompatible readiness, process exit/restart limits, and path-bounded uninstall. Windows integration tests must prove that no file is executed before verification/promotion, external services are untouched, Job Object close removes the entire managed process tree on normal and abnormal AIIDE termination, stale PIDs never grant ownership, side-by-side upgrade can roll back, and model data survives runtime upgrade/removal. A real RTX 3070 Ti acceptance must record OS and driver, ComfyUI/Python/PyTorch/CUDA/model versions, startup/shutdown behavior, generation time, peak/dedicated/shared memory behavior, output validity, cancellation, OOM reporting, crash cleanup, and offline restart.

### C2 approval checklist

- [x] Exact upstream release, tag commit, asset name/ID, compressed byte count, and SHA-256 recorded.
- [x] Official tag-specific source is marked immutable by GitHub; no moving URL or replacement fallback is permitted.
- [x] Correct RTX-family archive and architecture-level RTX 3070 Ti/CUDA compatibility established.
- [x] Required pinned command-line isolation flags verified in the selected source tag.
- [x] Managed/external ownership boundary and independent model-registry direction defined.
- [x] Inspect the exact archive under separate approval and record its verified digest, logical installed size, file count/path inventory, resolved Python/PyTorch/CUDA API versions, TAESD file hashes, package metadata, and installed notices.
- [ ] Resolve the remaining archive-evidence gaps: exact TAESD commit, native CUDA patch/component mapping, three Python packages without installed licence evidence, and final reviewed SBOM/notices.
- [ ] Complete release/legal approval for automated official acquisition, any future mirroring, GPL source/notices, Python/PyTorch notices, NVIDIA redistributables, TAESD, and every transitive package.
- [ ] Choose and approve the in-process 7z library, its licence, supported 7z features, bomb limits, and Windows path/reparse defenses.
- [ ] Approve the signed runtime-manifest trust, update/revocation, redirect allowlist, and upstream asset-unavailability policy.
- [ ] Approve supported Windows editions and the R580-or-newer driver readiness rule; do not add driver installation to C2.
- [ ] Approve disk-space/rollback retention values using the verified installed-size data.
- [ ] Approve forced owned-Job termination as the fallback because no upstream graceful-shutdown contract was verified, plus the bounded drain and restart limits.
- [x] Identify a separately acquired, exact hash-pinned SDXL fixture for real GPU acceptance. Its licence/product approval remains separate below; the runtime archive alone cannot prove inference readiness.

Until all unchecked items are resolved, Stage C2 remains blocked and the Stage B external/manual flow remains the only supported path.

## Stage C1.5 managed-runtime approval-blocker investigation

Stage C1.5 was performed on 2026-09-21 without downloading the portable archive or model, installing software, executing any archive content, changing dependencies, or changing application code. It refines the C1 gates but does not approve C2.

### Exact archive inspection and reproducible evidence record

The exact archive can be inspected without installing or executing ComfyUI: download it as inert data, verify the complete file, and parse or extract it into a quarantined data-only working directory that is never searched, imported, loaded, or executed. It cannot be inspected completely without obtaining the asset. GitHub's release record supplies publisher identity, byte count, and digest, while immutable-release attestations are additional provenance evidence; neither exposes the archive's entry table, installed size, resolved packages, native libraries, licence files, or TAESD bytes. A range-only header read would also be insufficient because dependency and notice evidence requires the entry contents and a full-file SHA-256 requires all bytes.

No archive bytes were obtained in C1.5. A later evidence-only download of `1,917,442,353` bytes requires explicit approval before it starts. That inspection is not an installation and must not launch the embedded Python, batch files, update scripts, DLLs, or any other archive content.

After approval, use this reproducible evidence procedure:

1. Download the exact immutable asset into a fresh access-controlled evidence directory. Record UTC time, final URL chain, response validators, exact byte count, GitHub release/asset IDs, release attestation if available, and SHA-256. Stop unless every C1 pin matches.
2. Open the verified archive with the selected in-process reader. First pass: enumerate metadata only and record entry index, raw name, normalized proposed path, entry kind, compressed and uncompressed sizes, CRC if present, attributes, and compression methods. Reject the archive under the C1 path/type/count/size rules before creating any entry.
3. Define reproducible installed size as the sum of decoded regular-file bytes, not NTFS allocation size. Record regular-file count and directory count separately. Also record observed allocated bytes after extraction on a documented reference NTFS volume; label that value environment-specific rather than manifest truth.
4. Second pass: stream every regular entry into a fresh access-controlled directory on the same volume, hashing each file while enforcing the declared and cumulative byte limits. Permit only regular files and directories. Use create-new semantics, reject reparse/link/anti-item metadata, verify every parent remains within the staging root, and never search, import, or execute from that directory.
5. Generate a deterministic file inventory sorted by normalized path with path, decoded size, SHA-256, and evidence category. Preserve the archive inventory, file inventory, and scanner version/configuration alongside the runtime manifest review record.
6. Parse package metadata as data. Read `*.dist-info/METADATA`, `RECORD`, `WHEEL`, `direct_url.json`, and installed licence/notice files; do not run `pip`, the embedded interpreter, or package code. Record distribution name/version, declared licence expression/classifiers, metadata and licence-file hashes, and owned paths. Separately inventory Python DLLs, PyTorch/CUDA/native DLLs, executables, launchers, and updater files because wheel metadata does not establish every native component's licence.
7. Identify TAESD by hashing every copied `models/vae_approx/*.safetensors` file and matching the complete set to a specific commit of the `comfyanonymous/taesd` fork. The upstream build cloned mutable `main` without recording its commit, so a date-based guess is unacceptable. If the set cannot be matched uniquely, the TAESD commit remains unknown and legal/provenance approval stays blocked.
8. Produce a machine-readable component register plus a human review table. A declared package licence is evidence, not legal clearance; missing or ambiguous metadata remains an exception requiring manual resolution.

At C1.5, the logical installed size, file counts, exact PyTorch/Python package versions, native CUDA payload, TAESD commit, and complete notices remained blocked on the separately approved artifact inspection. C1.6 below closes the size/file/package/installed-notice evidence items; TAESD commit provenance and complete native/legal resolution remain blocked.

### Rust-compatible 7z approach comparison

| Approach | Maintenance and licence | Security/control | Windows and packaging | C1.5 decision |
| --- | --- | --- | --- | --- |
| `sevenz-rust2` 0.21.5 | Actively maintained pure Rust fork; Apache-2.0; released 2026-08-16. Version 0.21.1 fixed a traversal advisory and 0.21.3/0.21.4 added malformed-input, allocation, loop, and integer bounds. The project has no published `SECURITY.md`; 0.21.x requires Rust 1.93. | Low-level reader permits per-entry streaming and caller-owned limits. The convenience `decompress_file` path is not sufficient for AIIDE. Recent security fixes make exact version pinning, adversarial fixtures, and dependency audit mandatory. | No external executable or C runtime. Supports the archive's LZMA2 method and Windows. Project currently has no pinned Rust version, so release-toolchain compatibility must be decided. | **Recommended, proposed:** exact audited version at implementation time, no version range, minimal features, and a custom two-pass validator/writer. Do not add it in C1.5. |
| `compress-tools` 0.16.x over libarchive | Maintained Apache-2.0 Rust wrapper around mature multi-format libarchive. Libarchive has permissive but file-varying notices. | Strong format coverage and metadata iteration, but a much larger native parser surface. High-level extraction does not replace AIIDE's own Windows path, type, count, and byte checks. | Requires a separately built/shipped libarchive through vcpkg or equivalent, its transitive native codecs/notices, ABI/update policy, and additional clean-machine tests. | **Fallback, proposed:** use only if the native reader cannot process the exact verified archive or fails security review. |
| Bundled/called `7z.exe` or `7z.dll` | Upstream 7-Zip is primarily LGPL with BSD and unRAR-qualified portions; shipping it adds a second executable/DLL and its notice/source obligations. | Mature decoder, but subprocess output parsing and destination enforcement are weaker ownership boundaries; resource and path policy would be split across processes. | Works on Windows but adds executable acquisition/signing/versioning and subprocess recovery. | **Rejected for C2:** do not execute a downloaded or bundled extractor. |
| Windows Explorer or PowerShell extraction | OS-version-dependent and not a stable Rust API contract. | No adequate preflight entry policy, deterministic resource caps, or machine-readable evidence contract. | Availability and 7z behavior vary by Windows servicing state. | **Rejected.** |

The recommended reader must first validate the exact known-good archive and also pass malicious fixtures covering absolute/UNC/drive paths, both slash styles, `..`, ADS colons, reserved devices, trailing dots/spaces, case-fold collisions, links/reparse data, anti-items, huge counts/sizes, integer overflow, truncated/CRC-invalid streams, solid archives, and cancellation. Extraction must run on a bounded blocking worker, count actual decoded bytes rather than trusting headers, and leave only a recorded staging directory on failure. The dependency and the project's release Rust toolchain remain approval items.

### Manifest trust, redirects, revocation, and unavailable assets

The smallest C2 trust model is a static runtime manifest compiled into the AIIDE executable, not downloaded configuration and not an editable application-data file. The release executable/installer signing chain authenticates the embedded manifest; changing a pin requires a new signed AIIDE release. A detached remotely updated catalog or a new application-level signing system is outside C2.

The manifest should contain a schema version, runtime/package ID, approved/revoked state, platform/architecture, release tag and commit, release and asset IDs, exact initial URL and asset name, exact compressed bytes and SHA-256, verified archive evidence version, logical installed bytes and counts, required layout/files, licence/register references, compatible OS/GPU/driver policy, launch arguments, and extraction limits. GitHub's immutable-release attestation and live API digest are corroborating evidence; the embedded hash and size remain the install-time trust decision.

Network policy:

- begin only at the exact `https://github.com/Comfy-Org/ComfyUI/releases/download/v0.36.0/ComfyUI_windows_portable_nvidia.7z` URL;
- handle redirects manually, permit at most five, require HTTPS throughout, never forward credentials/cookies, reject userinfo and IP-literal destinations, and permit only an explicit set of GitHub asset hosts established by an approved metadata/HEAD probe before implementation; the expected asset CDN host must not be inferred from arbitrary response data;
- reject any redirect back to a moving `/latest/` path, another repository/tag/asset, or a non-allowlisted host;
- require the expected total length, consistent ETag or equivalent validator and `Content-Range` for resume, exact final byte count, and a streamed SHA-256 match before parsing; and
- treat TLS, GitHub metadata, and the publisher digest as transport/provenance layers, never as substitutes for the embedded digest.

For C2, revocation should be application-release-driven: a later signed AIIDE build marks a manifest/runtime hash revoked and refuses new acquisition or managed launch while leaving external ComfyUI untouched. It should offer removal or a separately approved replacement and retain diagnostic evidence. This model cannot emergency-revoke an unupdated offline application. If product policy requires immediate remote revocation, a signed, expiring, rollback-resistant remote revocation channel must be designed and approved before C2; silently trusting an ordinary web response is not acceptable.

If the asset is unavailable, new managed setup fails with a retryable, source-specific error. It never substitutes `/latest/`, a mirror, another CUDA build, or an unpinned model. An already verified non-revoked installation continues to work offline; external/manual setup remains available. A future mirror requires separate immutable identity, hosting, and legal approval.

### Minimum reliable Windows process ownership

| Option | Failure behavior | Ownership quality | Decision |
| --- | --- | --- | --- |
| Ordinary Rust/Tokio child handle, optionally killed on drop | Can stop the direct Python child during orderly AIIDE shutdown. Windows does not terminate child processes when a parent exits, and descendants can survive both direct-child termination and an AIIDE crash. | Insufficient for a managed runtime; a PID/port/path after restart is not ownership proof. | Reject as the sole mechanism. It remains useful only for waiting and exit diagnostics inside the stronger job boundary. |
| Start normally, then assign to a Job Object | Captures the process tree after assignment, but the child can run and create descendants before assignment. | Race prevents a reliable no-orphan claim. | Reject for managed startup. |
| Create suspended, assign to an unnamed Job Object, then resume | No archive code runs before job assignment. With `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, no breakaway flags, and a non-inheritable job handle, normal close or AIIDE crash terminates the associated process tree. | Strong current-session authority based on live kernel handles. Assignment failure can terminate the still-suspended process without affecting anything else. | **Recommended minimum.** |

The minimum implementation is one Windows-only supervisor around `CreateJobObjectW`, `SetInformationJobObject`, `CreateProcessW(CREATE_SUSPENDED)`, `AssignProcessToJobObject`, `ResumeThread`, wait/exit inspection, `TerminateJobObject`, and `CloseHandle`. Use an unnamed job, set kill-on-close before process creation, set neither breakaway limit, pass absolute executable/script paths and a fixed argument array, and use `STARTUPINFOEX` handle-list inheritance when stdout/stderr pipes are required so only those handles cross the boundary. If nested-job policy or assignment fails, terminate and close only the newly created suspended process, report managed startup unavailable, and keep external mode usable.

The supervisor owns only handles it created for the current managed session. It never opens a persisted PID to regain authority, never adopts a matching path or port, and never places an existing external ComfyUI in its job. The direct child handle supplies readiness/exit monitoring; the Job Object supplies tree lifetime. Bounded prompt cancellation and drain happen first, but forced termination of the owned job remains the fallback because no stable ComfyUI shutdown API was verified.

### Conservative supported platform and storage policy

Upstream evidence supports CUDA 13 on Windows x86-64, R580-or-newer drivers, and Ampere compute capability 8.6. NVIDIA's R580 Windows notes include Windows 11 25H2 and CUDA 13.x, while Microsoft has ended general Windows 10 22H2 support. The smallest C2 release claim should therefore be:

| Layer | Initial managed support | Other detected systems |
| --- | --- | --- |
| OS/architecture | Windows 11 25H2 x86-64, fully updated | Windows 11 24H2 may be a test/transition target while Microsoft supports the installed edition. Windows 10, Server, ARM64, older Windows, and newer untested feature releases remain external-only until separately accepted. |
| GPU | Desktop GeForce RTX 3070 Ti, compute capability 8.6, 8 GB dedicated VRAM—the actual acceptance machine | Other RTX 20-series-and-newer GPUs are upstream-compatible candidates, not release-verified AIIDE configurations. Below 8 GB is unsupported for the SDXL fixture. |
| Driver | NVIDIA Windows driver `580.88` or later; prefer a current WHQL driver. Require live driver/GPU/VRAM and ComfyUI CUDA readiness checks. | Older than R580 fails managed readiness. AIIDE links to NVIDIA guidance but never installs or updates a driver. |
| Runtime/model | Exact ComfyUI `v0.36.0` asset plus the separately verified SDXL fixture below | No substitute runtime, checkpoint, or CPU fallback. |

This is a proposed product support boundary, not live acceptance. C2 cannot advertise the combination until it passes the recorded clean-machine and RTX 3070 Ti tests.

Let `A` be the compressed archive bytes, `R` the verified logical installed runtime bytes, and `M` the selected model bytes. Runtime installation on a single volume should require free space for `A + R + reserve`; an upgrade uses the same free-space formula because the already installed prior runtime is not free space, while the total-storage disclosure also includes that retained prior version. Model acquisition separately requires `M + reserve`. When runtime and model share a volume, calculate one combined peak and one reserve. Use `reserve = max(4 GiB, 10% of the new payload)` and require all staging/final paths on the same volume so promotion is a rename rather than a second copy. C1.6 establishes `R = 4,166,922,797` bytes and the resulting proposed display thresholds in the approval matrix below.

Retain the downloaded runtime archive only through successful extraction, promotion, and first readiness check, then delete it by default. Keep the active runtime and one last-known-good runtime for rollback; removal of older managed versions is explicit and path-manifest-bounded. Keep models outside runtime directories and retain them by default across runtime upgrade, repair, or removal. Removing an AIIDE-acquired model requires a separate size-labelled confirmation; external or user-owned models are never removed. Partial downloads, failed staging trees, logs, and generated temporary images use separately bounded cleanup rules and never consume the model-retention choice.

### Separately acquired SDXL acceptance fixture

The existing fixed workflow's suitable acceptance fixture is Stability AI's official `sd_xl_base_1.0.safetensors` at Hugging Face commit `f298da3c058bd8f1f1c62f3ecfa775244a243897`:

- immutable source: `https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/resolve/f298da3c058bd8f1f1c62f3ecfa775244a243897/sd_xl_base_1.0.safetensors`;
- exact size: `6,938,078,334` bytes (about 6.938 GB / 6.462 GiB);
- SHA-256: `31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b`;
- format: single safetensors checkpoint, loaded by the existing core-node SDXL workflow; and
- licence: CreativeML Open RAIL++-M dated 2023-07-26, with use-based restrictions, licence/notice-on-distribution, and downstream-user obligations.

The repository's Git LFS pointer at that commit independently records the exact byte count and SHA-256. This makes the file technically suitable and hash-pinned, but not legally cleared. C2 excludes model acquisition; the fixture may be acquired only through a separate consent/licence decision or installed manually for acceptance.

### Component licensing checklist

| Component | Verified evidence | Technical notice/source record | Independent legal approval still required |
| --- | --- | --- | --- |
| ComfyUI core `v0.36.0` | GPL-3.0 source tag/commit and immutable release asset. | Preserve licence/copyright; record exact corresponding source commit and source URL; inventory any generated/bundled core files. | Whether automated user acquisition, bundling, or mirroring is conveyance; corresponding-source delivery/offer method; interaction with AIIDE distribution and user-facing notices. |
| CPython 3.13.14 embedded | Pinned workflow downloads the official Windows embeddable package; Python is under the PSF licence with incorporated historical notices. | Preserve the Python licence/notice set and identify standard-library/native components actually present. | Approval of the installed notice bundle and any separately licensed standard-library/native components found in the archive. |
| PyTorch, torchvision, torchaudio | C1.6 verifies PyTorch `2.13.0+cu130`, torchvision `0.28.0+cu130`, torchaudio `2.11.0+cu130`, their wheel hashes where recorded, and 109 PyTorch licence files. | Preserve exact `dist-info`, notices, native DLL inventory/hashes, and upstream corresponding release/source links. | Binary redistribution/notice sufficiency and every bundled third-party library; archive metadata is not legal clearance. |
| CUDA runtime libraries | CUDA 13 channel and R580 compatibility are verified; NVIDIA EULA controls redistribution of enumerated components. C1.6 records 22 CUDA-family DLL paths/sizes, but filenames do not establish exact patch builds or redistributable status. | Map each shipped NVIDIA DLL/version/hash to the EULA redistributables list and retain required notices. | Whether AIIDE's acquisition/distribution path is permitted, whether every actual DLL is redistributable, end-user terms/notice presentation, and any geographic/export requirements. |
| TAESD weights | Build copies safetensors from mutable `comfyanonymous/taesd`, a fork of the MIT-licensed `madebyollin/taesd`; build does not record the commit. C1.6 records six exact file hashes but their headers contain no commit metadata. | Resolve one exact fork commit if authoritative history can match the complete set, and preserve that commit's MIT notice and provenance. | Approval of the weights and notices remains blocked at commit level; exact file-hash approval may be considered independently if legal review accepts that provenance basis. |
| ComfyUI frontend/templates/docs and all Python/transitive packages | C1.6 inventories 86 top-level and 12 vendored metadata instances, 76 exact wheel hashes, and 272 installed licence/notice files. Three top-level packages have no archive licence evidence. | Preserve the deterministic register and manually resolve missing/ambiguous declarations and native subcomponents. | Component-by-component redistribution, attribution, source, patent, trademark, and other obligations. Automated licence classification is not legal review. |
| SDXL acceptance fixture | Exact official commit, LFS size/hash, model card, and CreativeML Open RAIL++-M text are recorded. | Present source, size, hash, licence, use restrictions, retention and ownership before any separate acquisition; preserve licence/notices. | Whether AIIDE may automate acquisition or distribute/mirror it, how enforceable use restrictions and downstream notice/acceptance are presented, and any product-policy restrictions. |

No row above is legal clearance. Final release approval must be performed independently against the exact artifact/component register and the intended acquisition/distribution channel.

### Stage C1.5 approval matrix

| Requirement | Evidence | Proposed decision | Status | Remaining action |
| --- | --- | --- | --- | --- |
| Official immutable runtime identity | GitHub marks `v0.36.0` immutable; tag, commit, asset ID/name, byte count and SHA-256 are recorded. | Keep the exact browser-download URL and embedded pin; never use `/latest/` or substitute builds. | **Verified** | None for identity; preserve evidence in the manifest review record. |
| Exact installed size, files, dependencies and notices | C1.6 verified the exact archive and recorded 57,031 regular files, 5,868 directories, `4,166,922,797` decoded file bytes, 98 Python metadata instances and 272 licence/notice files. | Use the generated inventories as the immutable evidence baseline. | **Verified** | Preserve the report and inventory hashes; repeat only if the runtime artifact changes. |
| TAESD provenance | C1.6 recorded exact hashes for six weights. Safetensors have no source metadata and the packaged workflow cloned mutable `main` without recording a commit. | Treat file identities as verified but commit provenance as unresolved; do not infer a commit by build date. | **Blocked** | Establish the matching official source commit independently or obtain legal approval using exact file hashes and other sufficient provenance. |
| 7z reader | Pure-Rust `sevenz-rust2` 0.21.5 supports LZMA2 and contains recent security fixes; libarchive is a viable native fallback. | Audit and pin one exact `sevenz-rust2` version, custom validation only, minimal features; require Rust 1.93-compatible release toolchain or revisit. | **Proposed** | Approve dependency/toolchain/security review after proving it reads the exact archive and fixtures. |
| Manifest authenticity and immutable pins | The runtime can be represented entirely by static facts; AIIDE release signing is the existing distribution trust direction. | Compile manifest into the signed executable; no remote catalog in C2. | **Proposed** | Approve embedded-manifest schema and confirm installer/executable signing plan. |
| Redirect and verification policy | GitHub documents 200/302 asset delivery and immutable assets; exact hash/size are known. | Manual HTTPS redirects to observed allowlisted GitHub hosts; exact size plus streamed SHA-256; strict resume validators. | **Proposed** | Approve a metadata/HEAD-only redirect-host probe and final allowlist during C2 design. |
| Revocation/unavailable asset | Immutable assets cannot be replaced or deleted while the release exists; an offline app cannot receive immediate revocation. | Signed-AIIDE-release denylist; no silent fallback; existing verified non-revoked runtime may work offline. | **Proposed** | Decide whether release-driven revocation is sufficient or remote signed revocation is mandatory. |
| No orphaned managed processes | Microsoft states parent termination does not terminate children; Job Objects group descendants and kill-on-close terminates them. | Direct `CreateProcessW` suspended, assign to unnamed kill-on-close Job Object, then resume; fail closed on assignment error. | **Proposed** | Approve Windows-specific API/dependency boundary and forced owned-job fallback; integration-test normal/crash paths. |
| External ComfyUI isolation | Stage B stores external loopback configuration; live Job/process handles are the only proposed managed authority. | Never adopt, assign, signal or terminate an existing PID/port/path; external mode remains usable on managed failure. | **Verified** | Preserve the design boundary in implementation and tests. |
| Windows/GPU/driver support | Windows 11 25H2 is current and present in R580 notes; CUDA 13 requires R580+; RTX 3070 Ti is CC 8.6 with 8 GB. | Initial claim only for Windows 11 25H2 x64 + desktop RTX 3070 Ti + driver 580.88 or later. | **Proposed** | Approve matrix and pass clean-machine/live GPU acceptance before advertising support. |
| Disk and retention | `A = 1,917,442,353`, `R = 4,166,922,797`, and the SDXL fixture `M = 6,938,078,334` bytes are now exact. | Peak-space formula and 4 GiB/10% reserve; retain one prior runtime; retain models by default and remove separately. | **Proposed** | Approve the resulting minimum free-space displays: runtime `10,379,332,446` bytes, model `11,233,045,630`, or combined same-volume acquisition `17,317,410,780`. |
| SDXL acceptance fixture identity | Official immutable commit and exact LFS size/SHA-256 are recorded; it fits the existing fixed SDXL workflow. | Use this file as the acceptance fixture; do not include model acquisition in C2. | **Verified** | None for technical identity; preserve the pinned registry record. |
| SDXL licence/product use | CreativeML Open RAIL++-M text and restrictions are identified, but no legal/product approval is claimed. | Keep acquisition separate and manual unless later automation receives explicit approval and consent UX. | **Blocked** | Independent model-licence/product review and approval. |
| Component licence register | C1.6 supplies the complete archive path inventory, 98 Python metadata instances, recorded wheel hashes, installed notices and CUDA-family DLL paths/sizes; native patch/subcomponent mapping and three package licence records remain unresolved. | Resolve evidence exceptions and obtain independent legal review for the actual delivery channel. | **Blocked** | TAESD/native/licence exception resolution and signed legal/release approval. |

### Smallest remaining pre-C2 decisions

C1.6 closes the artifact-download and base-inventory package. The remaining approvals are:

1. **Dependency and supervision:** approve the exact audited 7z crate plus Rust toolchain, and the Windows suspended-process/Job Object API boundary with forced owned-job termination.
2. **Release policy:** approve the embedded signed-manifest schema, observed redirect allowlist, release-driven versus remote revocation choice, Windows 11 25H2/driver 580.88+ support matrix, and now-calculated disk/retention values.
3. **Legal and provenance:** resolve or explicitly accept the TAESD/native/licence-evidence gaps, independently approve automated runtime acquisition against the exact component inventory, and separately approve SDXL licence presentation/use. No model download belongs in C2.

Until all three are approved, C2 remains blocked and no managed-runtime implementation should begin.

## Stage C1.6 evidence-only archive inspection

Stage C1.6 was performed on 2026-09-21 after explicit authorization for the one-time 1.9 GB download. Only the exact pinned ComfyUI `v0.36.0` Windows NVIDIA asset was downloaded. Its complete length and SHA-256 matched before any archive reader was used. No contained executable, DLL, Python interpreter, script, or ComfyUI code was executed; no package was installed; no model was downloaded; and no full extraction was performed.

The complete reproducible report is [M08A ComfyUI v0.36.0 archive evidence](evidence/M08A-COMFYUI-V0.36.0-ARCHIVE-EVIDENCE.md). Its generated inventories are [all archive entries](evidence/M08A-COMFYUI-V0.36.0-ARCHIVE-INVENTORY.tsv) and [Python components and available wheel/licence evidence](evidence/M08A-COMFYUI-V0.36.0-COMPONENT-INVENTORY.tsv).

### Verified results

| Requirement | Direct evidence | Result |
| --- | --- | --- |
| Artifact identity | Downloaded `1,917,442,353` bytes; observed SHA-256 `c3c60192840f8b68c9a47cf3e8161ecb108e5ffdf5ea236c1c72a402e442695d`. | Exact match to the C1 pin. |
| Logical installed size | Sum of every regular archive entry. | `4,166,922,797` bytes. Actual NTFS allocation was not measured. |
| Entry inventory | libarchive 3.8.8 listed 62,899 entries. | 57,031 regular files, 5,868 directories, no other entry types. |
| Windows path audit | All listed names checked for absolute/drive/UNC paths, backslashes, traversal, ADS, trailing dots/spaces, reserved devices, links/reparse types, control characters and case-fold collisions. | No suspicious entry found under these checks. C2 must still enforce them itself. |
| ComfyUI | Detached Git HEAD and reflog inside the archive. | Commit `ee71d5c4993f29086b27fde1629a945ae48425bf`, tag checkout `v0.36.0`. |
| Python | Static `python313.dll` version resource. | CPython `3.13.14`. |
| PyTorch stack | Installed `dist-info` and `torch/version.py`. | PyTorch `2.13.0+cu130`, torchvision `0.28.0+cu130`, torchaudio `2.11.0+cu130`, CUDA API `13.0`; PyTorch Git revision `cf30153c4c131c8164ee7798e5022d810682e2cb`. |
| CUDA-family payload | Archive path/size inventory. | 22 CUDA-family DLLs totaling `1,957,279,288` bytes; filenames evidence CUDA 13, cuDNN 9 and associated library major versions, not exact patch builds. |
| Python dependencies | Installed metadata, records and build-origin records. | 86 top-level distributions plus 12 setuptools-vendored metadata instances; 76 exact wheel filenames and hashes; all 98 have `RECORD` and `INSTALLER`. |
| Notices | Archive-wide licence/notice basename inventory. | 272 files totaling `1,266,929` bytes. Frontend 1.52.7, PyOpenGL 3.1.10 and tokenizers 0.22.2 have no installed licence metadata/file and require upstream review. |
| TAESD | Six bundled files hashed; safetensors headers and pinned build workflow inspected. | File identities are exact; no source metadata is embedded and the workflow used an unpinned shallow clone, so the commit remains unresolved. |

### Remaining blockers after C1.6

- legal review must map the exact Python/native inventory and every actual NVIDIA DLL to its distribution and notice obligations;
- TAESD commit-level provenance remains unavailable from the archive, although all six file hashes are known;
- exact native CUDA/cuDNN patch versions and a complete native subcomponent SBOM are not published as an installed manifest;
- the three Python packages without archive licence evidence need authoritative upstream licence/notice records;
- the 7z dependency, Job Object supervisor, manifest/revocation/network policy, Windows support matrix, and disk/retention UX remain proposed rather than approved; and
- no runtime execution, GPU readiness, inference, shutdown or crash-recovery acceptance was performed.

Stage C1.6 supplies evidence only. It does not authorize C2.

## Current manual GPU-acceptance setup

1. Install the official ComfyUI Desktop app, or the official Windows portable NVIDIA build for RTX 20-series and newer cards.
2. Download `sd_xl_base_1.0.safetensors` manually from Stability AI and place it in `ComfyUI\models\checkpoints`. The file is 6.94 GB, has SHA-256 `31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b`, and uses the CreativeML Open RAIL++-M licence.
3. Start ComfyUI yourself and keep it bound to loopback. The normal endpoint is `http://127.0.0.1:8188`.
4. If necessary, set `AIIDE_COMFYUI_ENDPOINT` to another loopback URL and `AIIDE_COMFYUI_CHECKPOINT` to the exact installed filename before launching AIIDE.
5. Start with ComfyUI's current default dynamic VRAM behavior. Use a pinned-version-specific low-memory recovery option only if live testing shows it is needed; current ComfyUI documents `--lowvram` as having no effect when dynamic VRAM is active.

The current implementation will not install ComfyUI, download weights, alter the service, or start/stop it. Those capabilities belong to the later approved managed-runtime direction and are not part of current GPU acceptance.

### First-image acceptance checklist

On the RTX 3070 Ti 8 GB target, record Windows/NVIDIA driver, ComfyUI/Python/PyTorch/CUDA versions and the exact checkpoint hash before testing. Then:

1. Open Image mode and confirm engine, required-node, checkpoint and hardware readiness separately.
2. Generate one 1024×1024 image with the fixed SDXL workflow; record wall time and peak dedicated/shared GPU memory, and confirm the preview is a valid PNG.
3. Reject one result and verify no project file remains; generate again, save it to a project-relative path, and verify collision-safe naming.
4. Use **Regenerate**, then test prompt-specific cancellation once while queued and once while running. Confirm an unsupported running cancellation never uses ComfyUI's global interrupt.
5. Put another job in ComfyUI's queue and confirm AIIDE reports busy without inventing progress.
6. Disconnect the engine during polling, restore it, and confirm the job can be checked again without a false terminal success.
7. Reproduce or safely simulate a ComfyUI CUDA out-of-memory response and confirm AIIDE reports it without automatic retry; use default dynamic VRAM behaviour for the baseline.
8. After Reject, Save, project change and application exit, verify AIIDE-owned temporary PNGs are removed. Record any remaining ComfyUI-owned `PreviewImage` temporary file separately.

Mocks do not satisfy this checklist. Managed acquisition/execution must remain disabled throughout the external-engine acceptance run.

## Phase 2 implementation files

- `src-tauri/src/image_generation.rs`: provider trait, ComfyUI adapter, fixed workflow, job state, commands, safe save/cleanup, and deterministic tests.
- `src-tauri/src/lib.rs`: state and command registration only.
- `src/types/image.ts`: frontend job/status contracts.
- `src/services/image/provider.ts` and `src/services/image/comfyui.ts`: separate image-provider boundary.
- `src/components/ImageResultCard.tsx`: reusable result card.
- `src/components/LocalChat.tsx`: explicit composer route and message-card integration without changing ordinary chat requests.
- `src/styles.css`: scoped card and mode-control styles using current tokens.
- `README.md`: opt-in Windows/ComfyUI/model setup and limitations.

No new frontend dependency was required. The Rust implementation uses the existing `reqwest`, `serde`, and `serde_json` stack with HTTP polling. Websocket progress remains a later, separately reviewed dependency change.

## Verification and bounded Stage B plan

The mockable Rust provider tests currently cover:

- unavailable engine and workflow/checkpoint submission errors
- submission and execution failure, including OOM mapping
- queued cancellation and supported prompt-specific cancellation
- cancellation-not-supported behavior without global interrupt
- successful PNG retrieval and preview state
- reject without project write
- approved save
- traversal, absolute, protected, symlink, and non-PNG paths
- exclusive collision naming
- cleanup after reject, save, project change, and state drop
- the single-job concurrency guard

Stage B should establish the smallest architecture that supports future model changes. It is a detection-and-guidance slice, not an installer:

1. Replace the binary connected/offline probe with structured readiness states: checking, external-ready, managed-ready, unavailable, incompatible version/API, missing core nodes, missing checkpoint, and actionable hardware warning.
2. Separate engine readiness from model readiness. Probe `/system_stats` for version/device data and `/object_info/<node>` for the selected model workflow. Read available model choices and require the configured registry model and files. Treat unknown schemas as incompatible, not ready.
3. Add the small typed model registry described above, initially containing the SDXL acceptance baseline without making it the permanent production default.
4. Add a compact setup card shown only in Image mode. For the selected model, explain download and storage size, practical GPU/VRAM guidance, local/offline behavior, source, ownership, and licence. Provide `Set up`, `Use existing ComfyUI`, `Retry`, and `Skip` states without invented progress.
5. Keep external `127.0.0.1:8188` compatibility and add app-owned persisted external-engine configuration rather than requiring environment variables for normal use. Continue rejecting non-loopback endpoints.
6. Provide official upstream setup/model-page links after explicit user action. Do not download, install, launch, update, or uninstall anything in this slice.
7. Add focused deterministic tests for engine-readiness failures, model-readiness failures, schema variation, missing files or nodes, unsafe endpoints, registry incompatibility, and ownership labels; verify the existing chat route remains unchanged.
8. Use the resulting readiness report to run the documented real GPU acceptance on the RTX 3070 Ti once ComfyUI and SDXL are manually available. Record engine/core/model versions, driver, generation time, memory behaviour, output quality, and observed failure/recovery behavior. Do not mark managed setup, a production model, or GPU acceptance complete from mocks.

The bounded detection-and-guidance slice is expected to change:

- `src-tauri/src/image_generation.rs` for typed engine/model readiness probes, the small model registry boundary, and focused tests;
- `src/types/image.ts` and `src/services/image/provider.ts` for the separated readiness contract;
- `src/components/LocalChat.tsx` for Image-mode setup-state routing;
- a small `src/components/ImageSetupCard.tsx` component, if extracting the setup state keeps `LocalChat` readable;
- `src/styles.css` for scoped setup-card states;
- existing image setup documentation if the user-visible instructions change.

No runtime dependency or Tauri process/filesystem capability should be added in this slice. Persisted external endpoint/checkpoint configuration should use the smallest existing Tauri-safe application-data pattern available when implementation begins; if no suitable persistence boundary exists, that item requires a narrow design decision rather than writing configuration into the project.

Only after that slice and release/legal approval should a separate managed-acquisition slice implement the pinned portable manifest, resumable verified downloads, safe extraction, managed-process ownership, repair, update, and uninstall behavior.

The managed ComfyUI strategy and this Stage B boundary are approved. Production-model selection and managed acquisition remain separate approval gates.

## Sources

- ComfyUI repository and Windows installation: <https://github.com/Comfy-Org/ComfyUI>
- ComfyUI `v0.36.0` release and official asset metadata: <https://github.com/Comfy-Org/ComfyUI/releases/tag/v0.36.0> and <https://api.github.com/repos/Comfy-Org/ComfyUI/releases/tags/v0.36.0>
- Pinned portable release matrix and build workflow: <https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/.github/workflows/release-stable-all.yml> and <https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/.github/workflows/stable-release.yml>
- Pinned ComfyUI dependency requirements: <https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/requirements.txt>
- Pinned ComfyUI command-line contract: <https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/comfy/cli_args.py>
- ComfyUI GPL-3.0 licence: <https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/LICENSE>
- Comfy Desktop architecture and dual licence: <https://github.com/Comfy-Org/Comfy-Desktop>
- ComfyUI command-line contract: <https://github.com/Comfy-Org/ComfyUI/blob/master/comfy/cli_args.py>
- ComfyUI official websocket API example: <https://github.com/Comfy-Org/ComfyUI/blob/master/script_examples/websockets_api_example.py>
- ComfyUI direct websocket image example: <https://github.com/Comfy-Org/ComfyUI/blob/master/script_examples/websockets_api_example_ws_images.py>
- ComfyUI server routes and job cancellation: <https://github.com/Comfy-Org/ComfyUI/blob/master/server.py>
- NVIDIA GPU compute capability table: <https://developer.nvidia.com/cuda/gpus>
- NVIDIA CUDA driver/toolkit/architecture matrix: <https://docs.nvidia.com/datacenter/tesla/drivers/cuda-toolkit-driver-and-architecture-matrix.html>
- NVIDIA CUDA 13.0 release notes: <https://docs.nvidia.com/cuda/archive/13.0.3/cuda-toolkit-release-notes/index.html>
- NVIDIA CUDA Toolkit EULA: <https://docs.nvidia.com/cuda/eula/>
- Python 3.13 licence: <https://docs.python.org/3.13/license.html>
- PyTorch licence: <https://github.com/pytorch/pytorch/blob/main/LICENSE>
- Microsoft Windows Job Objects and kill-on-close limit: <https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects> and <https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information>
- Microsoft Windows suspended-process creation and process flags: <https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw> and <https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags>
- SDXL 1.0 model card and weights: <https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0>
- SDXL 1.0 licence: <https://github.com/Stability-AI/generative-models/blob/main/model_licenses/LICENSE-SDXL1.0>
- ComfyUI maintained model guidance: <https://github.com/Comfy-Org/workflow_templates/blob/main/site/knowledge/models/sdxl.md>
- GitHub release-asset digests: <https://docs.github.com/en/rest/releases/releases>
- GitHub immutable-release guarantees and attestations: <https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases>
- GitHub release-asset 200/302 download behavior: <https://docs.github.com/en/rest/releases/assets>
- `sevenz-rust2` repository, changelog, licence, and traversal advisory: <https://github.com/hasenbanck/sevenz-rust2>, <https://github.com/hasenbanck/sevenz-rust2/blob/main/CHANGELOG.md>, <https://github.com/hasenbanck/sevenz-rust2/blob/main/LICENSE>, and <https://github.com/advisories/GHSA-qh76-45cr-8xrc>
- `compress-tools`/libarchive alternative and libarchive licence summary: <https://github.com/OSSystems/compress-tools-rs>, <https://github.com/libarchive/libarchive>, and <https://github.com/libarchive/libarchive/blob/master/COPYING>
- Official 7-Zip licence and integration guidance: <https://www.7-zip.org/faq.html> and <https://www.7-zip.org/>
- Microsoft parent/child termination behavior and suspended Job Object assignment: <https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process>, <https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject>, and <https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes>
- NVIDIA CUDA 13 Windows system matrix: <https://docs.nvidia.com/cuda/archive/13.0.0/cuda-installation-guide-microsoft-windows/index.html>
- NVIDIA R580 Windows/Windows 11 25H2 evidence and driver branch history: <https://docs.nvidia.com/datacenter/tesla/tesla-release-notes-580-105-08/index.html> and <https://www.nvidia.com/en-us/drivers/rtx-enterprise-and-quadro-driver-branch-history/>
- Microsoft Windows 10 end of support and current Windows 11 servicing matrix: <https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro> and <https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information>
- SDXL acceptance fixture immutable commit/LFS pointer and licence: <https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/commit/f298da3c058bd8f1f1c62f3ecfa775244a243897>, <https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/raw/f298da3c058bd8f1f1c62f3ecfa775244a243897/sd_xl_base_1.0.safetensors>, and <https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/blob/f298da3c058bd8f1f1c62f3ecfa775244a243897/LICENSE.md>
- TAESD fork/provenance and MIT licence: <https://github.com/comfyanonymous/taesd>, <https://github.com/madebyollin/taesd>, and <https://github.com/madebyollin/taesd/blob/main/LICENSE>
- SD3.5 ComfyUI example and dependencies: <https://github.com/comfyanonymous/ComfyUI_examples/tree/master/sd3>
- Stability AI SD3.5 hardware statement: <https://stability.ai/news/introducing-stable-diffusion-3-5>
- FLUX.1 schnell official weights: <https://huggingface.co/black-forest-labs/FLUX.1-schnell>
- Z-Image Turbo model card: <https://huggingface.co/Tongyi-MAI/Z-Image-Turbo>
