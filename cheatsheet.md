# ── RUN ELMA / AIIDE ─────────────────────────────

npm run tauri dev

# ── FRONTEND CHECKS ──────────────────────────────

npm run dev
npm run build
npm run typecheck
npm run lint

# ── RUST / TAURI CHECKS ──────────────────────────

cd src-tauri

cargo check
cargo test

cd ..

# ── OLLAMA ───────────────────────────────────────

# Check Ollama version

ollama --version

# Installed models

ollama list

# Models currently loaded/running

ollama ps

# Test Qwen directly

ollama run qwen2.5-coder:7b

# Download model

ollama pull qwen2.5-coder:7b

# Stop loaded model

ollama stop qwen2.5-coder:7b

# ── GPU ──────────────────────────────────────────

# Check GPU + VRAM usage

nvidia-smi

# Continuously refresh GPU stats

nvidia-smi -l 1

# ── GIT ──────────────────────────────────────────

git status
git diff
git diff --check
git log --oneline -10

# ── GOOD FULL CHECK BEFORE COMMIT ────────────────

npm run typecheck
npm run lint
npm run build
cd src-tauri
cargo test
cargo check
cd ..
git diff --check
git status

# ── RUN ELMA / AIIDE ─────────────────────────────

npm run tauri dev
change the main heading to "Welcome to the AIIDE Sandbox". make only that change and prepare it for review