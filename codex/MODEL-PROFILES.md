# Model profiles and agent benchmarks

Model profiles describe observed compatibility with AIIDE's current agent protocol, not a model's absolute quality. Known local profiles live in `src-tauri/src/model_profiles.rs`. Every Ollama model still appears in the desktop selector; unrecognised identifiers receive a neutral `Unknown` profile and remain usable. OpenRouter uses the same Rust agent protocol in the benchmark, but is not currently exposed in the desktop provider selector.

`model_profiles::adaptation` is the explicit boundary for small model-specific prompt or generation settings. These adaptations describe how a model is instructed, while [PERSONALITY.md](PERSONALITY.md) remains the model-independent definition of who Elma is. Adapters never rewrite tool arguments, repair paths, bypass repository validation, or change Apply/Reject safety.

## Run the benchmark

For Ollama, the service must be running and the selected model must already be installed. From the repository root:

```powershell
npm run benchmark -- --model qwen2.5-coder:7b
```

The backend also accepts OpenRouter when `OPENROUTER_API_KEY` is present:

```powershell
npm run benchmark -- --provider openrouter --model vendor/model-id
```

The environment-variable credential is a developer/benchmark interface, not the planned end-user credential experience. Do not place keys in project files or shared traces.

Use `--case answer`, `--case lookup`, or `--case edit` to run one case, and add `--json` for machine-readable output. The default run executes all three cases:

- `answer`: requires a repository-grounded answer backed by a successful read.
- `lookup`: requires discovery and reading of `src/index.html`.
- `edit`: requires exactly one validated heading replacement and no write.

The controlled fixture is `src-tauri/fixtures/agent-benchmark`; the harness checks it was not modified. Results report model, case, pass/fail, duration, tool calls, repair count, final action, and a concise failure reason. Full raw responses are not printed; use AIIDE Debug Mode for interactive Ollama traces. A passing three-case run is useful regression evidence, not proof of broad editing reliability or release readiness.
