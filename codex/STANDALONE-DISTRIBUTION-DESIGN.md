# Standalone Windows distribution design

Status: **PLANNED architectural direction**. The image capability's managed-runtime investigation is complete, but no guided installer, first-run flow, runtime manager, or embedded inference engine is implemented.

## Goal

A normal supported user should be able to install AIIDE, understand optional capabilities and their costs, configure only what they want, verify the result, and open a project without manually preparing PowerShell environment variables, Python environments, inference servers, model directories, or runtime command lines.

The preferred shape is a relatively small signed AIIDE installer plus optional, separately acquired runtimes and models. Multi-gigabyte model files should not all be bundled into the primary installer. Existing compatible Ollama and ComfyUI installations remain valid choices, and provider boundaries must allow alternatives later.

This document defines release architecture, not a promise that every runtime can or will be installed by AIIDE.

## Confirmed direction

- Windows is the first distribution target.
- Capability setup is optional and progressive; skipping one must not make unrelated capabilities unusable.
- Local coding, image generation, cloud providers, and GitHub are distinct capability choices.
- Cloud/GitHub capabilities do not inherently require a local model.
- Downloads or installations require explicit consent after size, storage, licence, source, and hardware implications are shown.
- AIIDE should detect and reuse compatible existing engines before offering managed installation.
- Text, image, editing, and vision providers remain replaceable.
- Runtime/model failures must degrade the affected capability rather than prevent the application from opening.
- Packaging technology, supported version matrices, and redistribution rights remain open until investigated and tested.

## Distribution stages

| Stage | User experience | Benefits | Costs and risks |
| --- | --- | --- | --- |
| A. Current developer setup | User installs Node/Rust/Tauri prerequisites, Git, Ollama and optional ComfyUI/models, then starts a development build. | Fastest development loop; engines remain independent. | Not suitable for normal users; configuration and failures are manual. |
| B. Guided setup | Packaged AIIDE detects engines/hardware and guides the user through official downloads and verification, but engines may remain user-managed. | Smaller scope and lower redistribution burden; preserves existing installs. | Multiple installers and compatibility surfaces; lifecycle may still feel fragmented. |
| C. Application-managed runtimes | AIIDE downloads/installs approved optional runtimes/models, owns compatible configuration, and can start/stop managed processes. | Cohesive setup and diagnostics; reproducible versions. | Large security, update, process, licence, support, recovery, and storage responsibilities. |
| D. Fully embedded inference | Inference libraries run inside or beside the application without a separately visible service. | Potentially simpler surface and tighter lifecycle control. | Largest packaging footprint and engineering commitment; hardware backends, updates, model formats, licensing, and crash isolation become AIIDE responsibilities. |

The near-term target is Stage B. For image generation, a pinned official Windows portable ComfyUI archive is the recommended future Stage C substrate, while official Comfy Desktop remains an external user-managed option. Stage C still requires a supported matrix, licence approval, verified artifact manifest, and process-ownership implementation. Stage D is an investigation option, not the default destination.

## Proposed first-run flow

1. Welcome to AIIDE and explain that Elma works through selected local or cloud capabilities.
2. Detect the Windows version, CPU architecture, system memory, available disk space, GPU/vendor/VRAM where reliable, Git, and known compatible engine endpoints/installations.
3. Explain available capabilities and which are already usable.
4. Let the user choose any combination of local coding assistant, image generation, optional cloud provider, and GitHub integration.
5. For each choice, show required downloads, estimated installed size, expected hardware limits, network/offline behaviour, source, licence, and whether AIIDE or the user will own the runtime.
6. Obtain explicit consent before downloading, installing, modifying configuration, starting a process, or storing a credential.
7. Configure only the selected capability. Prefer reusing a compatible existing installation.
8. Verify the engine/API, selected model, permissions, and a small non-destructive capability-specific check.
9. Report success, partial availability, or actionable failure without blocking unrelated features.
10. Offer a bundled or generated test project, then let the user open their own project.

Every step must be skippable. The same capability manager should be reachable later without creating a sprawling permanent settings surface.

## Capability packages

### Local coding assistant

Current engine: Ollama. Guided setup should detect its loopback service and installed models, explain model size/resource class, and test provider connectivity separately from agent quality. A successful version/model probe does not prove repository editing reliability; compatibility profiles and a bounded acceptance prompt remain separate evidence.

### Image generation

Current feature-branch engine: ComfyUI with an SDXL 1.0 checkpoint. Guided setup should verify the endpoint, compatible core nodes/API, exact checkpoint, disk space, and practical GPU limits. Eight gigabytes of VRAM is the current lower-bound target rather than a guarantee of performance. A mocked lifecycle test or reachable engine does not prove generation quality or stable memory behaviour.

The Phase 3 investigation selects a two-mode direction:

- **External mode:** reuse a user-started loopback ComfyUI installation without taking lifecycle ownership. Comfy Desktop may be recommended as an official setup route, but AIIDE does not silently embed or automate it.
- **Managed mode:** later install an exact, approved official Windows portable NVIDIA release and SDXL checkpoint as separate optional components in AIIDE's per-user application-data area. AIIDE owns only components recorded in its installation manifest and only processes launched in the current managed session.

The first implementation slice remains guided detection, not acquisition. It should expose structured readiness for runtime version/API, required core nodes, checkpoint choice, GPU/device information, and busy state; show a compact setup card only in Image mode; and retain a skip path. Managed download and process control are a separate approval gate.

ComfyUI core is GPL-3.0. Comfy Desktop is AGPL-3.0-or-later or commercially licensed. SDXL 1.0 uses CreativeML Open RAIL++-M. Distribution, automated acquisition, notices, source obligations, model restrictions, and any commercial-licence requirement need release/legal approval before Stage C. Technical separation through a loopback API does not replace that review.

### Optional cloud AI

OpenRouter is the current backend experiment. It needs a desktop provider selector, consent explaining source-code transmission, secure credential storage, model/cost visibility, rate-limit handling, revocation, and offline degradation. AIIDE must never copy keys into project files, prompts, or diagnostics.

### GitHub

GitHub integration needs no local inference model. It should begin with authentication and read-only repository metadata, then add narrowly confirmed remote actions. The user must be able to use local Git without signing in. Credential storage, scopes, account/repository identity, push behaviour, pull-request content, and every remote mutation need explicit boundaries.

## Runtime and model management requirements

### Detection and compatibility

- distinguish external user-managed, AIIDE-managed, and embedded engines;
- detect executable/service location without searching private directories indiscriminately;
- verify API/runtime/model versions against a supported matrix;
- avoid taking ownership of unknown existing processes;
- identify port conflicts and explain which process owns a required endpoint where the OS permits;
- preserve a diagnostic-only mode when a component is unsupported.

### Downloads and installation

- use an approved source over authenticated transport;
- show exact download and installed-size estimates before consent;
- verify cryptographic hashes and, where available, publisher signatures;
- download to a resumable staging area and use atomic finalisation;
- survive interruption, restart, insufficient disk space, and partial extraction;
- never execute a downloaded binary before verification;
- record component source, version, hash, licence, install location, and ownership.

For GitHub-hosted runtime archives, pin a tag and asset name and ship the expected SHA-256 in signed AIIDE release metadata. Never ship a moving `latest` URL. GitHub's release API digest is useful release evidence but must not be the only network-time trust input. For SDXL 1.0, the currently selected checkpoint is 6.94 GB with SHA-256 `31e35c80fc4829d14f90153f4c74cd59c90b779f6afe05a74cd6120b893f7e5b` from Stability AI's official Hugging Face repository.

Archive extraction must reject absolute paths, traversal, links/reparse points, duplicate destinations, and unexpected layouts. Extract into a fresh staging directory, verify the expected runtime layout, then atomically promote it. Never update a working runtime in place.

### Hardware and storage

- check system memory, free disk space, GPU vendor and practical VRAM where detectable;
- communicate that detection is advisory and shared GPU memory/workloads affect results;
- present CPU fallback only when the selected engine/model genuinely supports it;
- reserve space for download staging as well as final installation;
- keep models outside the application binary and allow a user-approved data location.

### Process lifecycle

- distinguish a user-started service from an AIIDE-managed process;
- start/stop only processes AIIDE owns unless the user explicitly asks otherwise;
- bind managed local services to loopback by default;
- allocate or validate ports without silently reconfiguring unrelated services;
- avoid orphan processes on ordinary shutdown while preserving long-running external services;
- use bounded startup/health timeouts and actionable recovery;
- isolate engine crashes from the AIIDE UI process where practical.

For managed ComfyUI specifically, launch embedded Python directly with an argument array and fixed safe flags for the pinned version: explicit loopback listen address, dynamically selected port, no browser auto-launch, no custom nodes, no cloud/API nodes, and AIIDE-owned base/temp/model directories. Never launch the bundled batch file. Retain a live child handle and preferably place it in a Windows Job Object so crash cleanup does not depend on process-name or stored-PID matching. After AIIDE restarts, any surviving process is unowned and must not be terminated automatically.

### Updates and compatibility

- update AIIDE, runtimes, and models independently;
- show size, licence, compatibility, and restart impact before optional component updates;
- prevent an automatic engine update from silently breaking the supported API/model matrix;
- retain or restore the last compatible managed version when feasible;
- define release channels and rollback policy before enabling unattended updates.

### Offline behaviour

- launch and open projects when optional engines or network services are unavailable;
- keep local Git/file inspection usable without an AI provider;
- clearly mark which selected capability is offline and why;
- avoid repeated background network attempts without user-visible policy;
- document which cached models/runtime installers remain usable offline.

### Uninstall and retention

AIIDE, managed runtimes, models, cache/temp data, logs, settings, and credentials are separate data classes. Uninstall must explain them and let the user choose whether large model data is retained. External user-managed installations must never be removed as though AIIDE owns them. Temporary generated images and interrupted downloads need bounded cleanup rules.

## Security and privacy

- keep the Rust/Tauri boundary for filesystem and process operations;
- require project-relative validation and explicit approval for project writes;
- run managed engines with least privilege and loopback-only networking by default;
- never interpolate user/model text into a shell command;
- keep credentials in an OS-appropriate secret store, not environment-variable instructions for normal use;
- redact secrets and minimise prompts, paths, source, and image text in logs/telemetry;
- make telemetry opt-in if introduced;
- display when project content will leave the machine and which provider receives it;
- authenticate update metadata and downloaded artifacts;
- publish dependency, runtime, model, and licence notices appropriate to the distributed combination.

## Licensing and redistribution

Before AIIDE downloads, bundles, mirrors, or recommends a runtime/model, record:

- software and model licence versions;
- redistribution and commercial-use rights;
- attribution/notice obligations;
- gated-access or click-through requirements;
- acceptable-use restrictions;
- whether AIIDE may automate download or must direct the user to the publisher;
- whether derivative or generated-output terms need UI disclosure;
- export, privacy, or regional restrictions that affect distribution.

This requires release/legal review. Technical ability to download a component is not permission to redistribute it.

## Phased implementation

### Phase 0 — release inventory

Document the supported Windows/toolchain baseline, component ownership model, version matrix, licence register, storage layout, credential plan, and clean-machine acceptance matrix.

### Phase 1 — detection and diagnostics

Package AIIDE without managed inference. Add first-run capability selection, detection of existing Git/Ollama/ComfyUI, hardware/storage reporting, and non-destructive verification. Provide official setup links and actionable errors.

For the existing image branch, implement this first as an Image-mode setup card and richer readiness contract rather than a general first-run dashboard. Verify the ComfyUI version/API, every core node used by the fixed workflow, the exact checkpoint choice, and device data. Reachability alone is not readiness.

### Phase 2 — guided acquisition

Add explicit-consent download guidance or launch approved upstream installers. Track completion and resume onboarding. Do not claim ownership of external installs.

### Phase 3 — selected managed components

Manage only components whose redistribution, unattended installation, update, process, and recovery stories are approved. Introduce them one capability at a time with clean install/update/uninstall tests.

The candidate image package is a pinned official Windows portable NVIDIA ComfyUI release plus the separately pinned SDXL 1.0 checkpoint. Before implementation, record the exact runtime archive size/digest, extracted size, Python/PyTorch/CUDA and driver matrix, full notices, and licence approval. Install versions side by side, keep models separately retainable, and never remove external installations or user-owned models.

### Phase 4 — embedded-engine evaluation

Prototype only if it materially improves reliability or user experience over managed external engines. Compare installer size, hardware coverage, performance, maintenance, security, and licence obligations before committing.

## Release acceptance

A standalone release is not ready until clean supported Windows machines can:

- install, launch, update, and uninstall AIIDE without a development toolchain;
- skip all optional capabilities and still open/inspect a project;
- configure each advertised capability independently;
- understand and consent to downloads, licences, storage, credentials, and cloud data transfer;
- recover from offline engines, port conflicts, low disk space, interrupted download, incompatible versions, and failed verification;
- preserve external installations and user-selected model data;
- pass accessibility, reduced-motion, privacy, signing, and upgrade/rollback checks;
- distinguish deterministic tests, live provider acceptance, and release-supported behaviour in the UI and documentation.

## Open decisions

- exact Windows versions and CPU architectures;
- installer, updater, signing, and release-channel technology;
- per-capability external versus managed ownership;
- supported Ollama, ComfyUI, model, Python, driver, and GPU matrix;
- model storage layout, sharing, migration, and retention defaults;
- artifact hashing/signature and mirror policy;
- credentials/secret-store implementation;
- managed process supervision and port-allocation strategy;
- offline installer/cache support;
- licence-review ownership and user-facing notice format;
- whether any embedded engine beats the replaceable external-provider approach;
- macOS/Linux scope after Windows acceptance.

## Image strategy approval gate

Approval is required before Stage B application changes. The proposed approval is:

1. accept official Windows portable ComfyUI as the future optional managed image runtime, while retaining external loopback ComfyUI support;
2. implement readiness detection, app-owned configuration, and the compact Image-mode setup card first, with no downloads or process management;
3. run real GPU acceptance against a manually installed compatible engine/checkpoint;
4. defer managed acquisition until an exact portable release, artifact digest, supported driver matrix, notices/licences, safe extraction design, and Windows process-ownership mechanism have separate approval.

See [M08A-IMAGE-GENERATION-DESIGN.md](M08A-IMAGE-GENERATION-DESIGN.md) for the current implementation audit, source comparison, storage/process contract, and bounded Stage B file plan.
