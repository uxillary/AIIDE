# AIIDE developer command cheatsheet

Run commands from the repository root unless noted.

## Run AIIDE

```powershell
npm run tauri dev
```

## Frontend checks

```powershell
npm run lint
npm run typecheck
npm run build
```

## Rust checks

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Six live Ollama acceptance tests are opt-in/ignored by the ordinary Rust suite.

## Ollama

```powershell
ollama --version
ollama list
ollama ps
ollama pull qwen2.5-coder:7b
ollama run qwen2.5-coder:7b
ollama stop qwen2.5-coder:7b
```

## Agent benchmark

```powershell
npm run benchmark -- --model qwen2.5-coder:7b
npm run benchmark -- --model qwen2.5-coder:7b --case edit
```

OpenRouter is available to the benchmark backend, not the desktop provider selector. When intentionally testing it, supply `OPENROUTER_API_KEY` through the process environment and never save or share the key:

```powershell
npm run benchmark -- --provider openrouter --model vendor/model-id
```

## GPU diagnostics

```powershell
nvidia-smi
nvidia-smi -l 1
```

## Git inspection

```powershell
git status --short --branch
git diff
git diff --check
git log --oneline -10
```

## Full local check

```powershell
npm run lint
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
git status --short
```
