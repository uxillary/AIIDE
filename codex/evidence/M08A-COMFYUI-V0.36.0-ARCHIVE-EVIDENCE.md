# M08A ComfyUI v0.36.0 archive evidence

- Inspection date: 2026-09-21 UTC
- Scope: Stage C1.6 evidence-only inspection
- Result: archive identity verified; no contained program or script executed; no full extraction performed; Stage C2 remains unapproved

This report records direct observations from the exact official Windows NVIDIA portable archive. “Verified” means reproduced from the downloaded bytes or official immutable release metadata. “Inference” and “unresolved” are labelled explicitly. Legal metadata is evidence only and is not legal clearance.

## Artifact identity and transfer evidence

| Field | Verified value |
| --- | --- |
| Release | ComfyUI `v0.36.0` |
| Release source commit | `ee71d5c4993f29086b27fde1629a945ae48425bf` |
| GitHub asset ID | `566545265` |
| Asset | `ComfyUI_windows_portable_nvidia.7z` |
| Requested URL | `https://github.com/Comfy-Org/ComfyUI/releases/download/v0.36.0/ComfyUI_windows_portable_nvidia.7z` |
| Final host and path | `release-assets.githubusercontent.com/github-production-release-asset/589831718/a5d0d07a-4f2c-4691-8e37-92e962d02dee` (temporary query credentials omitted) |
| HTTP chain | `302` then `200` over HTTPS |
| Response content length | `1,917,442,353` bytes |
| Downloaded file length | `1,917,442,353` bytes |
| Expected SHA-256 | `c3c60192840f8b68c9a47cf3e8161ecb108e5ffdf5ea236c1c72a402e442695d` |
| Observed SHA-256 | `c3c60192840f8b68c9a47cf3e8161ecb108e5ffdf5ea236c1c72a402e442695d` |
| Verification decision | Exact length and digest match; metadata inspection permitted |
| Transfer time | File created `2026-09-21T02:09:11Z`, completed `2026-09-21T02:10:58Z` |
| Response validators | `ETag: "0x8DF13702823449E"`; `Last-Modified: Tue, 15 Sep 2026 21:27:34 GMT`; byte ranges advertised |

The source identity is supported by the [official release](https://github.com/Comfy-Org/ComfyUI/releases/tag/v0.36.0), [GitHub release-asset API contract](https://docs.github.com/en/rest/releases/assets), and [GitHub immutable-release guarantees](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases). The independently recorded expected hash, not the live response, was the verification authority.

## Inspection method

The host had no standalone 7-Zip installation. Metadata was listed with the Windows-provided `bsdtar 3.8.8` / `libarchive 3.8.8`. PowerShell `Get-FileHash` performed the full SHA-256 verification before any archive read.

The reproducible sequence was:

1. Download the exact URL with `curl 8.21.0`, HTTPS-only redirects, a five-redirect cap, and response-header capture.
2. Confirm the file length is exactly `1,917,442,353` bytes and calculate SHA-256 over the complete file.
3. Run `tar -tvf <archive>` and parse every listing row as mode/type, uncompressed size, and path.
4. Reject non-file/non-directory entries and check every path for absolute/drive/UNC forms, backslashes, `..` segments, colons/ADS, trailing dots or spaces, reserved Windows device names, normalized case-fold collisions, and control characters.
5. Sum regular-file sizes for the reproducible logical installed size.
6. Selectively extract only 765 allowlisted evidence files (package `METADATA`, `WHEEL`, `RECORD`, `INSTALLER`, `direct_url.json`, licence/notice files, six TAESD weights, `python313.dll`, `torch/version.py`, ComfyUI Git identity and relevant build workflows). No runnable tree was created and no archive content was loaded or executed.
7. Read text metadata as data, read static Windows version information from the copied Python DLL, hash the six TAESD files, and parse only the bounded JSON header of each safetensors file.

The complete generated inventories are:

- [archive entry inventory](M08A-COMFYUI-V0.36.0-ARCHIVE-INVENTORY.tsv): UTF-8/LF, 62,900 lines including the header, 8,222,494 bytes, SHA-256 `32c1e55671d7f801ad3aec7e865aed351af687d9e0e1848e067bec64acb49c9a`;
- [Python component inventory](M08A-COMFYUI-V0.36.0-COMPONENT-INVENTORY.tsv): UTF-8/LF, 99 lines including the header, 34,198 bytes, SHA-256 `9436e7bddcf31dbf098fd033bc0037135c9aba4d0eeadb3ef6bcff1c8b3a60a9`.

## Archive inventory

| Observation | Verified value |
| --- | --- |
| Archive bytes | `1,917,442,353` |
| Logical decoded regular-file bytes | `4,166,922,797` (about 4.167 GB / 3.881 GiB) |
| Total entries | `62,899` |
| Regular files | `57,031` |
| Directories | `5,868` |
| Other entry types | `0` |
| Root paths | One root: `ComfyUI_windows_portable/` |
| Maximum listed path length | `231` characters |
| Maximum path depth | `19` segments |
| Zero-byte regular files | `556` |
| Executables / DLLs / batch files / PowerShell files | `41` / `97` / `17` / `0` |
| Python source files | `16,041` |

No suspicious entry was found under the stated checks: no absolute, drive-relative or UNC path; no backslash or `..` segment; no ADS colon; no trailing dot/space; no reserved device name; no link, reparse, device or anti-item entry; and no normalized case-fold collision. This is verified for the decoded listing returned by libarchive. It is not a claim that archive parsing is generally safe or that the C2 extractor may omit its own checks.

`4,166,922,797` bytes is the reproducible installed-size input `R` for planning. It is not observed NTFS allocated size because a full extraction was intentionally not performed.

The largest files are PyTorch/CUDA binaries, led by `cublasLt64_13.dll` (`477,896,816` bytes), `torch_cuda.dll` (`409,584,640`), `torch_cpu.dll` (`305,204,224`), and `cufft64_12.dll` (`284,331,040`). The archive also contains a `97,792,000`-byte HIP backend at `comfy_kitchen/backends/hip/_C.abi3.pyd`; its presence in the NVIDIA package requires licensing and product review even if it is unused.

## Runtime and dependency evidence

| Component | Verified archive evidence | Limits |
| --- | --- | --- |
| ComfyUI | `.git/HEAD` is detached at `ee71d5c4993f29086b27fde1629a945ae48425bf`; reflog records checkout of `refs/tags/v0.36.0`; origin is `https://github.com/Comfy-Org/ComfyUI`. | No execution/readiness test was performed. |
| CPython | `python313.dll` file and product version are both `3.13.14`, company `Python Software Foundation`; embedded `LICENSE.txt` is present. | The standard-library/native transitive licence review remains separate. |
| PyTorch | `torch 2.13.0+cu130`; `torch/version.py` states CUDA `13.0` and PyTorch Git revision `cf30153c4c131c8164ee7798e5022d810682e2cb`. | Package metadata declares a compound licence expression and 109 licence files; legal review is unresolved. |
| torchvision | `0.28.0+cu130` | Archive metadata has BSD licence evidence. |
| torchaudio | `2.11.0+cu130` | A packaged licence file is present. |
| ComfyUI frontend | `comfyui_frontend_package 1.52.7` | Its installed `dist-info` contains no `License`, `License-Expression`, or licence/notice file. Upstream evidence must be obtained separately. |
| Workflow templates | Aggregate `comfyui_workflow_templates 0.11.62`, plus core `0.3.350`, JSON `0.1.85`, media API `0.3.84`, and media assets `0.1.46`. | Component-by-component licence review remains required. |
| Embedded docs | `comfyui-embedded-docs 0.5.11` | Declares GPL-3.0 and includes a licence file. |

There are 86 top-level installed Python distributions and 12 vendored `dist-info` distributions under setuptools, for 98 metadata instances. All 98 have `METADATA`, `RECORD`, and `INSTALLER`; 86 report installer `pip` and 12 report `uv`. Seventy-six `direct_url.json` records preserve an exact wheel filename and SHA-256 from the build cache. Ten top-level packages lack `direct_url.json`: the ComfyUI frontend, aggregate/templates packages, embedded docs, `comfy-aimdo`, `comfy-kitchen`, and `pip`.

One repeated project name is expected vendoring rather than a top-level collision: top-level `packaging 26.3` and setuptools-vendored `packaging 26.0`.

The archive contains 272 files named as licences, notices, copying terms, or copyright records, totaling `1,266,929` bytes. Three top-level distributions have neither licence metadata nor an installed licence/notice file: `comfyui_frontend_package 1.52.7`, `PyOpenGL 3.1.10`, and `tokenizers 0.22.2`. This records an archive-evidence gap, not a claim that those upstream projects are unlicensed.

### CUDA-family payload

`torch/version.py` directly verifies CUDA `13.0`. Twenty-two CUDA-family DLLs total `1,957,279,288` bytes. Their names evidence CUDA runtime/BLAS major 13, NVRTC/NVJitLink 13.0, cuDNN 9, cuFFT 12, cuSOLVER 12, cuSPARSE 12, cuRAND 10, and NVTX 1. Exact patch/build versions and NVIDIA redistributable mapping are not established by filenames or Python package metadata and remain unresolved.

## TAESD evidence

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `taef1_decoder.safetensors` | 2,464,414 | `bb41500b646d5b8b592b7f3ca5d20d888c3075e209d178d99af609cd7e02a1d4` |
| `taef1_encoder.safetensors` | 2,464,424 | `19f5a854ce575fd0a1c0a52938410cf5fb4dc4b8ae71c8887533646d8fe47030` |
| `taesd_decoder.safetensors` | 2,450,590 | `d903e7cc3ae58f21d985a3498a841fa8b230edccff7ac5d27607b0c557945c6f` |
| `taesd_encoder.safetensors` | 2,450,568 | `a6b8c0fda4aad50784e543389e0239c72c5e31e9775c9e612ea8333bf3a77dcb` |
| `taesdxl_decoder.safetensors` | 2,450,590 | `ae5256b046b01d577279ed93a55bbb1fb2689e55aa14cfc0f7f841e0160202a5` |
| `taesdxl_encoder.safetensors` | 2,450,568 | `3d78013286676a1a51074831230f90972e573698fe4b11416d6f28e0cd5bbe7e` |

Each safetensors header parsed successfully, contains 67 tensors, and has no `__metadata__` source/commit field. The packaged `windows_release_package.yml` proves the build used `git clone --depth 1 https://github.com/comfyanonymous/taesd` and copied `taesd/*.safetensors`, but it did not pin or record a commit. Therefore the file identities are verified while the exact TAESD commit is **unresolved**. A date guess or match against an unofficial copy is not sufficient provenance.

## Evidence versus inference and unresolved items

| Item | Classification | Result |
| --- | --- | --- |
| Archive identity, length and SHA-256 | Verified | Exact match. |
| Entry paths, types, counts and decoded file-byte sum | Verified | Complete libarchive listing captured in the archive inventory. |
| Logical installed size | Verified | `4,166,922,797` bytes by summing regular entries. |
| Actual NTFS allocated size | Unresolved | Full extraction was not authorised or performed; allocation also depends on volume settings. |
| Python/PyTorch/torchvision/torchaudio/CUDA API versions | Verified | Static DLL and package metadata listed above. |
| Native CUDA/cuDNN patch versions and complete native SBOM | Unresolved | The archive lacks a unified native-component manifest; filenames expose only major/API versions. |
| Python distribution inventory | Verified | 86 top-level plus 12 vendored metadata instances; exact rows and available wheel hashes are in the component inventory. |
| Completeness of legal notices | Unresolved | Three top-level distributions lack archive licence evidence; native/transitive obligations need manual review. |
| TAESD file identities | Verified | Six exact sizes and hashes recorded. |
| TAESD commit | Unresolved | Mutable shallow clone and no commit metadata. |
| Runtime readiness, GPU compatibility in practice, startup and inference | Not tested | Executing ComfyUI was outside C1.6. |
| Redistribution/legal clearance | Blocked | This report does not provide legal approval. |

## Official source references

- [ComfyUI v0.36.0 release](https://github.com/Comfy-Org/ComfyUI/releases/tag/v0.36.0)
- [ComfyUI v0.36.0 source and GPL-3.0 licence](https://github.com/Comfy-Org/ComfyUI/tree/v0.36.0)
- [Pinned portable build workflow](https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/.github/workflows/windows_release_package.yml)
- [ComfyUI official portable guidance](https://github.com/Comfy-Org/ComfyUI/blob/v0.36.0/README.md)
- [CPython 3.13 licence](https://docs.python.org/3.13/license.html)
- [PyTorch licence](https://github.com/pytorch/pytorch/blob/main/LICENSE)
- [NVIDIA CUDA Toolkit EULA](https://docs.nvidia.com/cuda/eula/)
- [TAESD build source fork](https://github.com/comfyanonymous/taesd) and [original project](https://github.com/madebyollin/taesd)
