# Local AI Coding Companion

> Working title. Final product name and branding are still to be decided.

## 1. Project Overview

This project is a free, local-first AI coding companion for working directly with software projects and Git repositories.

The goal is to provide a practical alternative to token/credit-based coding agents for everyday development work.

A user should be able to open a local project folder, describe a change in natural language, allow an AI agent to inspect the repository, review its proposed changes, apply those changes safely, test them, and optionally commit or publish the result through Git/GitHub.

The application should feel closer to a lightweight combination of:

- an AI coding agent
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

This project aims to make those models useful through a polished coding-agent interface rather than simply providing another AI chat window.

The AI needs to understand and interact with an actual repository.

---

# 3. Primary Goals

The application should eventually allow a user to:

1. Open a local project folder.
2. Detect whether the folder is a Git repository.
3. Understand the project's file structure.
4. Give the AI a natural-language development task.
5. Allow the AI to search and inspect relevant files.
6. Allow the AI to propose modifications.
7. Apply modifications safely.
8. Display exactly what changed.
9. Accept or reject changes.
10. Run project commands, builds, or tests.
11. Allow the AI to diagnose and potentially repair failures.
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

The AI should not receive unrestricted control over the computer.

Instead, the agent should interact with the project through a controlled collection of tools.

Potential tools include:

- `list_files`
- `search_files`
- `read_file`
- `write_file`
- `apply_patch`
- `git_status`
- `git_diff`
- `run_command`
- `run_tests`
- `run_build`

Potentially destructive operations should require appropriate safeguards or user approval.

## Reviewable

AI-generated changes should be transparent.

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

The initial local inference provider will likely be **Ollama**.

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

The current preferred stack is:

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
User Prompt
    ↓
Repository metadata
    ↓
File tree
    ↓
Agent searches repository
    ↓
Agent identifies likely relevant files
    ↓
Relevant files/sections retrieved
    ↓
Agent creates plan
    ↓
Agent proposes patches
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

This gives the local agent persistent knowledge without repeatedly rediscovering fundamental project information.

The exact format should be determined during development.

---

# 9. Agent Architecture

The AI model should reason about tasks while the application controls what actions can actually occur.

Conceptually:

```text
USER
   ↓
AGENT
   ↓
TOOL REQUEST
   ↓
APPLICATION PERMISSION / VALIDATION
   ↓
TOOL EXECUTION
   ↓
RESULT
   ↓
AGENT
```

Example interaction:

```text
User:
"Make the project cards more compact."

Agent:
Needs repository context.

Tool:
search_files("project")

Agent:
Identifies relevant page and stylesheet.

Tool:
read_file(...)
read_file(...)

Agent:
Creates modification plan.

Tool:
apply_patch(...)

Application:
Displays diff.

User:
Accepts changes.

Tool:
run_command("npm run build")

Application:
Build succeeds.

User:
Commits changes.
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
- execute terminal command

### Git

- create branch
- commit
- push

### Remote

- create pull request
- interact with GitHub

Users may eventually be able to configure which actions require approval.

---

# 14. Checkpoints

Before substantial modifications, the application should create a recoverable checkpoint.

After an agent task:

```text
AI changed 4 files

+84
-31

Build: Passed

[ Review Changes ]
[ Keep Changes ]
[ Revert Session ]
```

This should make experimentation safe and encourage users to let the agent attempt more substantial work.

---

# 15. Autonomous Behaviour

The first version should remain strongly user-controlled.

Later versions may introduce configurable automation such as:

```text
[ ] Automatically apply patches
[ ] Automatically run tests
[ ] Attempt to repair failed tests
[ ] Automatically commit successful tasks
```

Full autonomy should never be necessary to use the application.

---

# 16. Initial MVP

The first milestone should remain deliberately small.

## V0.1

- Open a project folder.
- Detect Git repository.
- Display project file tree.
- Display current Git branch.
- Display Git status.
- Connect to local Ollama installation.
- Select supported local model.
- Send prompt to local model.
- Allow agent to search repository.
- Allow agent to read relevant files.
- Generate file modifications.
- Apply modifications safely.
- Display diff.
- Accept changes.
- Revert changes.
- Run a configured build/test command.
- Display command result.
- Create Git commit.

A GitHub account should **not** be required for V0.1.

A local Git repository should be enough.

---

# 17. V0.2

Potential additions:

- create Git branches
- GitHub authentication
- push branches
- create pull requests
- persistent project instructions
- better session history
- repository indexing improvements

---

# 18. V0.3

Potential additions:

- multi-step autonomous agent loop
- test → diagnose → repair workflow
- checkpoints
- session restoration
- better context retrieval
- command permission system
- model performance presets

---

# 19. V1.0

Potential goals:

- polished Windows installer
- model management
- multiple AI providers
- configurable permissions
- robust project indexing
- GitHub integration
- persistent project context
- session history
- safe autonomous workflows
- onboarding
- documentation
- GitHub release

---

# 20. Non-Goals

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

The focus is the AI-assisted modification workflow.

---

# 21. Development Principles

During development:

1. Build the smallest working vertical slice first.
2. Prefer existing reliable tools over recreating infrastructure.
3. Keep AI providers interchangeable.
4. Keep Git operations independent from AI logic.
5. Treat repository safety as a core feature.
6. Make every AI modification reviewable.
7. Avoid unnecessary dependencies.
8. Maintain clear separation between UI, agent, model provider, filesystem and Git functionality.
9. Do not add features solely because competing coding agents have them.
10. Keep the application useful on normal consumer hardware.

---

# 22. First Development Milestone

Do **not** begin by building the autonomous AI agent.

First prove that the desktop application can reliably interact with a real development project.

The first milestone is:

```text
Launch application
        ↓
Open Folder
        ↓
Select Git repository
        ↓
Display repository name
        ↓
Display current branch
        ↓
Display Git status
        ↓
Display file tree
```

Once this works reliably, integrate Ollama.

Then:

```text
Repository
    +
Ollama
    ↓
Ask AI about repository
```

Only after repository-aware conversation works should the application be allowed to modify files.

This reduces the number of difficult systems being developed simultaneously.

---

# 23. Current Development Order

The intended order is:

```text
01  Repository + project planning
02  Tauri application shell
03  Open-folder workflow
04  Git detection
05  File tree
06  Git status / branch
07  Ollama connection
08  Basic repository-aware AI conversation
09  Repository search tools
10  Controlled file editing
11  Diff review
12  Revert/checkpoint system
13  Build/test commands
14  Git commits
15  GitHub integration
16  Multi-step agent behaviour
17  Packaging + release
```

Do not skip directly to autonomous agent functionality.

---

# 24. Open Decisions

These decisions have deliberately **not** been locked yet:

- Product name
- Branding
- Exact local coding model
- Exact Ollama model size
- Final persistence system
- Monaco vs alternative diff viewer
- Exact checkpoint implementation
- Agent protocol
- Tool-call format
- GitHub CLI vs direct GitHub API
- Distribution/installer method
- Whether macOS/Linux support belongs in V1
- Whether the project context format should become a reusable/open specification

These should be decided through prototypes and testing rather than prematurely.

---

# 25. Long-Term Vision

The application should become a genuinely useful open-source coding companion rather than simply a demonstration of local AI.

A developer should eventually be able to install it, open an existing project and say:

> "Add a settings page matching the existing design. Don't introduce any new dependencies. Run the build when you're finished."

The application should:

1. understand the repository,
2. locate relevant code,
3. formulate a plan,
4. modify the appropriate files,
5. show what it changed,
6. run the build,
7. diagnose failures when appropriate,
8. allow the user to review everything,
9. and commit the finished work.

The core promise is:

> **Your project. Your machine. Your model. You approve the changes.**

No AI credits should be required for the core workflow.