# Local AI Coding Companion

> Working title. Final product name and branding are still to be decided.

## 1. Project Overview

This project is a free, local-first AI coding companion for working directly with software projects and Git repositories.

The goal is to provide a practical alternative to token/credit-based coding agents for everyday development work.

A user should be able to open a local project folder, describe or select a change, review a verified target and proposed content, apply an assembled diff with approval, run authorised tests, and eventually prepare a commit or optional GitHub pull request. AIIDE manages that workflow; Elma assists with understanding and code generation. These are intended capabilities, not a claim that the full workflow exists today.

The application should feel closer to a lightweight combination of:

- a local AI coding assistant
- GitHub Desktop
- a focused code review/diff tool
- a small amount of VS Code-style project awareness

It is **not intended to become a full IDE**.

The application should remain focused on the workflow:

**Open → Ask → Inspect → Edit → Review → Test → Commit**

---

# 2. Core Problem

Cloud coding agents are extremely capable, but frequent use can become limited by subscriptions, token allowances, credits, rate limits, or internet access.

Local AI models have become capable enough to handle many everyday development tasks without requiring paid inference.

This project aims to make those models useful through an application-led engineering workflow rather than simply providing another AI chat window.

The application must supply verified repository context and control changes. A small model should not be responsible for finding an exact original source target on its own.

---

# 3. Primary Goals

The application should eventually allow a user to:

1. Open a local project folder.
2. Detect whether the folder is a Git repository.
3. Understand the project's file structure.
4. Describe a development task to Elma or choose a file and source range directly.
5. Discover relevant files and verify source references in the application.
6. Let Elma analyse supplied code and generate replacement content for a verified target.
7. Assemble and apply modifications safely after approval.
8. Display exactly what changed.
9. Accept or reject changes.
10. Run controlled, authorised builds or tests.
11. Let Elma explain failures and suggest fixes for user review.
12. Commit successful changes using Git.
13. Optionally push branches and create GitHub pull requests.
14. Maintain useful project context between sessions.
15. Operate without paid AI inference.

---

# 4. Product Philosophy

## Local First

The default experience should use a locally running AI model.

Code should not need to leave the user's computer for normal AI operations.

Internet access should be optional rather than a requirement.

## Free to Use

The core application should not depend on paid API credits.

Users should be able to install a supported local model and use the application without paying per request.

Optional cloud AI providers may be supported later through user-provided API credentials.

## Safe by Default

The model must not receive unrestricted control over the computer or repository. AIIDE owns bounded repository inspection, verified source selection, deterministic edits, diffs, snapshot and path validation, and controlled test commands. Model-generated content and requests are untrusted. The user approves changes before application and external publishing actions. Ambiguous source targets require explicit selection; Elma must not invent original text or silently choose one.

## Reviewable

Changes, whether AI-generated or user-authored, should be transparent.

The user should be able to see:

- files inspected
- files modified
- lines added
- lines removed
- commands executed
- command output
- build/test results

The user should remain in control of whether changes are kept.

## Model Agnostic

The application should not be built around one specific AI model.

Models and providers should be interchangeable through a common provider interface.

---

# 5. Initial Model Strategy

The current local inference provider is **Ollama**.

The initial coding model should be selected based on the best balance between:

- coding ability
- tool use
- reasoning
- repository understanding
- speed
- VRAM usage
- consumer hardware compatibility

Initial candidates include:

- Qwen coding models
- Gemma
- other capable coding-focused models available through Ollama

The initial development target includes hardware such as an RTX 3070 Ti with 8 GB VRAM, so the default model should remain practical on this class of consumer GPU.

Larger models may be offered as optional "power" models for machines capable of running them.

The model layer should eventually support:

```text
AI Provider
│
├── Local
│   └── Ollama
│       ├── Qwen coding models
│       ├── Gemma
│       └── other compatible models
│
└── Optional Cloud
    ├── OpenAI
    ├── Google
    ├── Anthropic
    └── other providers
```

Cloud support is not required for the first release.

---

# 6. Proposed Technology Stack

The implemented core stack and longer-term integration options are:

### Desktop Application

**Tauri**

Chosen to provide a native desktop application without requiring the overhead of a full Electron application.

### Frontend

- React
- TypeScript
- Tailwind CSS

### Local AI

- Ollama
- interchangeable local models

### Code Diff / Editing

Potentially Monaco Editor or another suitable diff component.

### Git

Use the user's installed Git executable rather than implementing version control from scratch.

### GitHub

Initial GitHub integration may use the official GitHub CLI (`gh`).

This could support operations such as:

- authentication
- pushing branches
- repository information
- creating pull requests

A deeper GitHub API integration can be considered later if necessary.

### Persistence

Likely SQLite or a similarly lightweight local persistence system.

### Repository Search

Use efficient filesystem searching/indexing rather than sending entire repositories to the model.

Potential tools include `ripgrep` and a lightweight project index.

---

# 7. Repository Intelligence

A major design goal is to avoid blindly sending an entire repository to the AI model.

Instead, the application should progressively retrieve context.

Example:

```text
User request or selected source range
    ↓
AIIDE repository metadata, search, and candidate discovery
    ↓
Verified file and source references
    ↓
User selection when candidates are ambiguous
    ↓
Relevant code supplied to Elma when generation is needed
    ↓
AIIDE assembles and validates a reviewable change
```

This should improve:

- inference speed
- context usage
- model accuracy
- compatibility with smaller local models

Generated folders and irrelevant files should normally be ignored.

Examples:

- `.git`
- `node_modules`
- `dist`
- `build`
- binary files
- generated assets
- model files

The application should respect `.gitignore` where appropriate.

---

# 8. Project Context

Projects should eventually be able to contain persistent instructions for the AI.

For example:

```text
.ai/
├── project.md
├── context.json
└── sessions/
```

A project context file could describe:

```md
# Project

Personal portfolio website.

## Stack

- JavaScript
- Tailwind
- Cloudflare Pages

## Design

- Dark interface
- Green accent
- Minimal visual language

## Rules

- Preserve responsive behaviour.
- Maintain accessibility.
- Avoid unnecessary dependencies.
- Do not edit generated files.
```

This would give AIIDE and Elma persistent project knowledge without repeatedly rediscovering fundamental information.

The exact format should be determined during development.

---

# 9. Application-Led Architecture

AIIDE manages the engineering workflow. It owns repository discovery and contextual retrieval, verified file and source references, deterministic file creation and editing, diff assembly, snapshot and path validation, explicit review, and controlled authorised test execution. Git branches, commits, and optional GitHub pull request preparation come later.

Elma interprets requests, analyses relevant supplied code, generates replacement content, helps choose among verified candidates, explains changes and test failures, and suggests fixes. Local models such as Qwen through Ollama should be interchangeable. Elma cannot invent original source text, silently choose ambiguous targets, directly write arbitrary files, or execute unrestricted commands.

Conceptually:

```text
USER REQUEST / SOURCE SELECTION
   ↓
AIIDE DISCOVERY + VERIFIED TARGET
   ↓
ELMA CONTENT GENERATION (OPTIONAL)
   ↓
AIIDE DETERMINISTIC ASSEMBLY + VALIDATION
   ↓
USER REVIEW + APPROVAL
   ↓
AIIDE APPLICATION / AUTHORISED VERIFICATION
```

Example interaction:

```text
User selects a file and source range.
AIIDE captures and verifies the original snapshot.
User supplies replacement content, or Elma generates it from the selected code.
AIIDE builds the pending change and displays its diff.
User approves Apply; AIIDE revalidates the snapshot before writing.
User authorises a configured build or test command when that stage exists.
AIIDE reports the result; Elma may explain a failure.
```

---

# 10. Change Safety

Every AI editing session should eventually have a reliable recovery mechanism.

Possible implementation options include:

- Git worktrees
- temporary branches
- Git stash
- internal patch history

The user should always have an obvious equivalent of:

**Revert AI Changes**

The application should avoid silently destroying existing uncommitted user work.

Handling repositories that already contain uncommitted changes will therefore be an important design consideration.

---

# 11. Interface Direction

The interface should be focused rather than attempting to recreate an IDE.

Potential desktop structure:

```text
┌──────────────────────────────────────────────────────────────┐
│ PROJECT                                      MODEL ● LOCAL   │
├────────────────┬─────────────────────────┬───────────────────┤
│                │                         │                   │
│ PROJECT        │ AI                      │ CHANGES           │
│                │                         │                   │
│ File tree      │ Conversation            │ Changed files     │
│                │                         │                   │
│ Git status     │ Agent activity          │ + additions       │
│                │                         │ - removals        │
│ Branch         │ Tool activity           │                   │
│                │                         │ Diff              │
│                │                         │                   │
├────────────────┴─────────────────────────┴───────────────────┤
│ Build ✓            Review Changes             Commit         │
└──────────────────────────────────────────────────────────────┘
```

The three major concepts are:

**Project / Agent / Changes**

The interface should clearly communicate what the AI is currently doing.

For example:

- Searching repository
- Reading `Projects.tsx`
- Editing `styles.css`
- Running build
- Build failed
- Diagnosing failure
- Changes ready for review

---

# 12. Operating Modes

## Local Mode

AI inference runs locally.

Internet connectivity is unnecessary.

Example:

```text
AI: Local
Provider: Ollama
Model: Qwen
Internet: Off
```

## Connected Mode

AI inference remains local, but internet-enabled development features are available.

For example:

- GitHub
- Git push
- pull requests

## Hybrid Mode — Future

Simple or private work could remain local while difficult tasks could optionally be sent to a user-configured cloud model.

This is not required for the initial version.

---

# 13. Permissions

Different operations carry different risk levels.

The application should eventually distinguish between operations such as:

### Low Risk

- list directory
- search repository
- read files
- inspect Git status

### Modification

- modify files
- create files
- delete files

### Execution

- run build
- run tests
- run an authorised, controlled project command

### Git

- create branch
- commit
- push

### Remote

- create pull request
- interact with GitHub

File changes require explicit review and approval. Command execution must be constrained to controlled, authorised commands. Remote publishing requires user approval. Any future permission settings must preserve these boundaries.

---

# 14. Checkpoints

Before substantial modifications, the application should provide a recoverable checkpoint. Snapshot validation must prevent overwriting work that changed after review.

After a change task:

```text
Pending change: 4 files

+84
-31

Build: Passed

[ Review Changes ]
[ Keep Changes ]
[ Revert Session ]
```

This should make review and recovery clear without implying autonomous application.

---

# 15. Previous Autonomous-Agent Experiments

The existing bounded Elma proposal path is an implemented experiment, not the intended basis for reliable editing. Experiments on `fix/reliable-editing` improved source validation, but real-world acceptance still failed when the small local model chose incorrect source targets. That branch is historical/experimental and is not approved for merging. Keep its findings; build the new application-led path on `main` in scoped stages. Automatic application, repair, commits, and unrestricted commands are not the active direction.

---

# 16. Revised Development Roadmap

These milestones are planned work. Current behavior is described in [codex/CONTEXT.md](codex/CONTEXT.md) and must be checked against current code.

## Milestone 1: Minimal regression protection

Protect the current passing repository-inspection, proposal-validation, and approval behavior with a small, focused regression baseline. Keep tests meaningful; do not weaken them to make CI pass.

## Milestone 2: Deterministic change engine

Let the user select a verified project file and source range for edits, or a validated project-relative path for creation, and provide content without AI. AIIDE owns original text capture, deterministic creation and editing, snapshot and path checks, pending change assembly, diff display, and explicit Apply/Reject. Resolve ambiguous ranges through user selection.

## Milestone 3: Elma-generated replacement content

Supply Elma with the verified selected target and relevant context. Elma generates replacement code while AIIDE retains the original source, assembles the change, validates it, and presents it for user approval.

## Milestone 4: Repository intelligence and candidate discovery

Add application-owned search and contextual retrieval that present verified files and source candidates. Elma may help rank or explain candidates; ambiguity remains visible to the user.

## Milestone 5: Controlled verification and multi-file editing

Support assembled multi-file changes, recoverable checkpoints, and tests/builds through controlled, authorised commands. Show command results and let Elma explain failures and suggest reviewed fixes.

## Milestone 6: Git and optional GitHub workflow

Prepare branches, commits, and optional GitHub pull requests under explicit user control. Publishing and other external actions require approval. A GitHub account is not required for the core local workflow.

---

# 17. Non-Goals

To prevent uncontrolled scope growth, the initial project is **not** intended to:

- replace VS Code
- implement a complete code editor
- host Git repositories
- train its own AI model
- implement its own version-control system
- provide paid cloud inference
- support every operating system immediately
- autonomously execute arbitrary commands without safeguards
- reproduce every feature of commercial coding agents

The focus is the application-managed, AI-assisted modification workflow.

---

# 18. Development Principles

During development:

1. Build the smallest working vertical slice first.
2. Prefer existing reliable tools over recreating infrastructure.
3. Keep AI providers interchangeable.
4. Keep Git operations independent from AI logic.
5. Treat repository safety as a core feature.
6. Make every AI modification reviewable.
7. Avoid unnecessary dependencies.
8. Maintain clear separation between UI, Elma, model provider, filesystem and Git functionality.
9. Do not add features solely because competing coding agents have them.
10. Keep the application useful on normal consumer hardware.

---

# 19. Current Baseline and Development Order

The application already has a Tauri shell, open-folder workflow, Git status and branch display, file tree, Ollama conversation, bounded repository inspection, and an approved one-file exact-replacement proposal path. This is the baseline to protect, not completion of the application-led editing architecture.

Follow the six milestones in Section 16. Start with regression protection, then a user-selected deterministic change engine before adding Elma-generated content. Repository intelligence, controlled verification, multi-file changes, and Git/GitHub workflow follow in that order. Packaging, onboarding, persistence, model management, and broader platform support remain later product decisions.

---

# 20. Open Decisions

These decisions have deliberately **not** been locked yet:

- Product name
- Branding
- Exact local coding model
- Exact Ollama model size
- Final persistence system
- Monaco vs alternative diff viewer
- Exact checkpoint implementation
- Elma interaction protocol for verified targets
- Candidate presentation and source-range selection
- GitHub CLI vs direct GitHub API
- Distribution/installer method
- Whether macOS/Linux support belongs in V1
- Whether the project context format should become a reusable/open specification

These should be decided through prototypes and testing rather than prematurely.

---

# 21. Long-Term Vision

The application should become a useful open-source coding companion rather than simply a demonstration of local AI.

A developer should eventually be able to install it, open an existing project and say:

> "Add a settings page matching the existing design. Don't introduce any new dependencies. Run the build when you're finished."

The application should:

1. discover relevant repository context and present verified source candidates,
2. obtain an explicit target selection when needed,
3. let Elma generate code for verified targets,
4. assemble and validate changes deterministically,
5. show the diff and obtain approval before applying changes,
6. run an authorised build and present its result,
7. let Elma explain failures and suggest reviewed fixes,
8. prepare a commit and optional pull request under user control.

The core promise is:

> **Your project. Your machine. Your model. You approve the changes.**

No AI credits should be required for the core workflow.
