# AIIDE --- Cloud Agent Pivot & Product Direction

**Status:** Confirmed strategic direction\
**Date:** 23 September 2026\
**Project:** AIIDE / Elma

## 1. Executive Summary

AIIDE will pivot from being primarily a local coding assistant that
depends on small local models behaving like a full autonomous coding
agent.

The new direction is:

> **AIIDE is a provider-agnostic AI coding-agent interface and
> orchestration layer, with Elma as the product identity and
> interchangeable local or cloud models as its intelligence.**

The current local-first application is not discarded. It becomes the
preserved **AIIDE Local** foundation: private, offline-capable,
Ollama-powered, and able to use local ComfyUI image generation.

Active development will move to a new cloud-agent branch. This version
will focus on powerful hosted models---initially through
OpenRouter---while retaining AIIDE's existing repository tools, review
workflow, Git integration, Elma personality/animations, and
local-provider support.

The key architectural principle is:

> **The model decides what the user means. AIIDE decides what actually
> changes.**

Models may inspect repositories, reason, choose targets, and generate
replacement content. AIIDE should own exact source ranges, diff
construction, validation, approval, application, and Git operations.

------------------------------------------------------------------------

## 2. Why the Direction Is Changing

The local-agent experiments proved that AIIDE's interface and workflow
are viable, but also exposed the limitations of making small local
models responsible for precise repository editing.

Observed problems included:

- Edit requests being misclassified as conversational answers.
- Models failing to reproduce exact `old_text`.
- Minor whitespace, entity, or source-text differences invalidating
    proposals.
- Excessive repair loops and token usage.
- Small reasoning models exhausting output limits before returning
    structured actions.
- Large amounts of orchestration complexity being added to compensate
    for model weaknesses.
- Long response times for operations that should be simple.
- Reliability varying significantly between models and prompt wording.

The routing and validation work remains valuable, but the project should
no longer depend on making a 4B--7B local model behave like a
state-of-the-art cloud coding agent.

Powerful hosted models can provide the reasoning capability, while AIIDE
provides the controlled environment in which that reasoning operates.

------------------------------------------------------------------------

## 3. Product Vision

AIIDE should become a **cheap or free alternative coding-agent
interface** with a distinctive user experience.

The intended workflow remains:

1. Open a project.
2. Ask Elma for a change.
3. Elma investigates the repository using tools.
4. The selected AI model reasons about the task.
5. AIIDE constructs validated proposed changes.
6. Elma presents them in **Changes**.
7. The user reviews and approves or rejects them.
8. Approved changes are applied.
9. Git status, staging, and commit workflows remain available.

The model itself is interchangeable.

AIIDE's value comes from:

- Elma and the interaction design.
- Repository awareness.
- Agent orchestration.
- Safe tools.
- Reviewable changes.
- Application-owned editing.
- Git integration.
- Provider flexibility.
- Cost visibility.
- Local/private options.
- A pleasant alternative interface to existing coding agents.

------------------------------------------------------------------------

## 4. Product Modes

### 4.1 AIIDE Cloud / Main Product

This becomes the primary development direction.

Cloud AIIDE can use hosted models capable of significantly stronger
coding and agentic reasoning than the small models currently used
locally.

Initial focus:

- OpenRouter.
- Free OpenRouter models where available.
- User-owned API keys.
- Native tool/function calling where supported.
- Capability-aware model selection.
- Optional paid models later.

A future provider architecture may include:

- OpenRouter
- Ollama
- Mistral
- Google/Gemini
- Other compatible providers
- AIIDE-managed providers, if commercially justified

The application should not be tightly coupled to any individual model.

### 4.2 AIIDE Local

The current local application should be preserved as a stable
local/private configuration.

It can retain:

- Ollama.
- Local coding models.
- Local repository tools.
- Reviewable changes.
- Git workflows.
- Local ComfyUI image generation.
- Managed local-runtime work.
- Experimental local agent capabilities.

Local mode should be presented honestly as hardware- and
model-dependent.

The project should stop spending disproportionate development time
trying to make weak local models perfectly imitate premium cloud coding
agents.

### 4.3 Long-Term Unification

AIIDE Local and AIIDE Cloud should not become permanently duplicated
applications.

The desired long-term architecture is one AIIDE codebase with
interchangeable providers:

``` text
AIIDE
│
├── Agent Core
│
├── Repository Tools
│
├── Editing Engine
│
├── Git
│
├── Image Tools
│
├── Providers
│   ├── OpenRouter
│   ├── Ollama
│   └── Future Providers
│
├── UI
│   ├── Chat
│   ├── Files
│   ├── Changes
│   ├── Git
│   └── Images
│
└── Elma
```

"Local" ultimately describes a provider configuration rather than an
entirely separate product.

------------------------------------------------------------------------

## 5. What We Keep

Most of the existing AIIDE application remains valuable.

### Core UI and identity

Keep:

- Elma.
- Elma's personality.
- Chat interface.
- Chat/Image mode foundation.
- File tree.
- Changes panel.
- Debug interface.
- Existing dark visual identity.
- Elma state animations.
- Portal transitions between Chat and Changes.

Elma remains the consistent assistant regardless of which model powers a
request.

### Repository functionality

Keep:

- Project opening.
- File listing.
- File reading.
- Repository search.
- Path protections.
- File-size and tool limits where appropriate.
- Project awareness.
- Git status.
- Staging/unstaging.
- Commit approval workflow.

These become tools available to the agent rather than behaviour tied to
a particular model.

### Changes workflow

Keep and strengthen:

- Pending changes.
- Diff preview.
- Apply.
- Reject.
- Stale-change protection.
- User approval.
- Git integration.

This is one of AIIDE's strongest differentiators and remains central to
the product.

### Provider foundation

Keep:

- Ollama provider.
- OpenRouter provider.
- Provider/model selection.
- Usage tracking.
- Token reporting.
- Debug traces.

The OpenRouter integration will become significantly more important.

### Reliability work

Keep the useful lessons and infrastructure from previous editing work:

- Intent routing.
- Validation.
- Read-before-edit protections.
- Stale-file protection.
- Candidate-selection work.
- Regression tests.
- Application-led editing foundation.

However, model-specific workarounds should not dictate the new
architecture.

------------------------------------------------------------------------

## 6. What We Stop or Replace

### Model-authored exact `old_text`

Remove this as a core dependency.

A model should not need to reproduce exact existing source text merely
to identify what should change.

This has been a major source of failures.

### Excessive prompt-based repair logic

Reduce reliance on:

- Repeated "return valid JSON" repairs.
- Model-specific formatting tricks.
- Prompt hacks for exact source reproduction.
- Large retry chains intended to compensate for weak models.

Fallbacks remain useful, but should not be the normal execution path.

### Qwen-specific architecture

Qwen and other local models remain supported, but AIIDE's agent protocol
must not be designed around the limitations or quirks of one model
family.

### Local model as the default intelligence

Local inference becomes an option rather than the requirement on which
AIIDE's success depends.

------------------------------------------------------------------------

## 7. Application-Led Editing

This is the most important architectural change.

The model should handle semantic reasoning.

AIIDE should handle deterministic editing.

### Model responsibilities

The model may:

- Understand the user's request.
- Decide whether repository inspection is necessary.
- Search for relevant files.
- Read relevant files.
- Reason about implementation.
- Select an identified source candidate.
- Generate replacement content.
- Explain a proposed change.
- Request additional inspection where necessary.

### AIIDE responsibilities

AIIDE should own:

- Current file contents.
- File revision/hash.
- Candidate extraction.
- Exact source ranges.
- Exact current source text.
- Diff construction.
- Replacement boundaries.
- Validation.
- Stale-file detection.
- Apply/reject.
- Filesystem mutation.
- Git staging.
- Commit approval.

A desirable edit interaction is therefore closer to:

``` text
Model:
candidate_id = "c00042"
replacement_text = "<h1>Elma Editing Works</h1>"
```

rather than:

``` text
Model:
old_text = "...copy the source perfectly..."
new_text = "..."
```

AIIDE already knows what `c00042` contains and where it exists.

This substantially reduces the precision burden placed on the model.

------------------------------------------------------------------------

## 8. Native Agent Tools

For providers that support native tool/function calling, AIIDE should
prefer that capability rather than requiring every model to emulate a
custom JSON action protocol.

Potential agent tools include:

``` text
list_files
search_files
read_file
inspect_candidates
select_candidate
propose_replacement
git_status
```

Later capabilities may include additional safe repository or development
tools.

The agent loop becomes:

``` text
User request
    ↓
Model
    ↓
Tool request
    ↓
AIIDE validates + executes
    ↓
Tool result
    ↓
Model
    ↓
Further inspection/reasoning
    ↓
Proposed semantic edit
    ↓
AIIDE constructs validated diff
    ↓
Changes
    ↓
User approval
```

The model does not receive unrestricted filesystem authority.

------------------------------------------------------------------------

## 9. Cloud Provider Strategy

### OpenRouter first

OpenRouter is the initial cloud-agent focus because AIIDE already has
provider support and OpenRouter exposes many models through one API.

The initial cloud-agent milestone should test a powerful model with
strong agent/tool capabilities.

Free models may be used while provider quotas and availability permit.

AIIDE should never imply that a third-party free endpoint is permanently
unlimited.

### Free model routing

A future **Free / Auto** mode may select from currently available free
models based on required capabilities.

Selection can consider:

- Tool calling.
- Coding capability.
- Context length.
- Structured-output support.
- Availability.
- Cost.
- Latency.
- User privacy preferences.

This prevents AIIDE from becoming dependent on a single free model.

### Bring Your Own Key (BYOK)

BYOK should be a core commercial architecture.

Users configure their own provider/API credentials locally.

Benefits:

- AIIDE does not fund every user's inference.
- Users can use free provider allowances.
- Users can buy provider credits directly.
- Advanced users can choose premium models.
- AIIDE operating costs remain low.
- Provider choice remains flexible.

Credentials must be stored securely and never embedded in distributed
application code.

------------------------------------------------------------------------

## 10. Free and Commercial Direction

The initial objective is not to build subscriptions immediately.

First prove that AIIDE can deliver a reliable cloud-agent coding
experience.

A possible future structure is:

### AIIDE Free

Potential features:

- BYOK.
- Free OpenRouter models when available.
- Ollama/local models.
- Core repository tools.
- Reviewable changes.
- Git workflow.
- Small, restrained sponsor/advertising placements.

### AIIDE Plus

Possible future features:

- No advertising.
- AIIDE-managed cloud allowance.
- Premium automatic routing.
- Convenience features.
- Higher hosted usage allowances.
- Additional integrations.

This is exploratory and should not be implemented until the core agent
experience works.

### Advertising principles

If advertising is introduced, it should not damage the coding
experience.

Avoid:

- Ads inside source code.
- Ads inside diffs.
- Ads in approval dialogs.
- Intrusive banners during active agent work.

Potential placements:

- Home/start screen.
- New-session screen.
- Restrained sponsor card.
- Post-task completion area.

The interface should remain credible as professional developer software.

------------------------------------------------------------------------

## 11. Image Generation Direction

Image generation remains part of AIIDE, but is no longer a blocker for
the coding-agent roadmap.

### Keep

Retain the existing work:

- Image mode.
- Independent image/chat drafts.
- Preview/history.
- Regenerate.
- Save/Reject.
- Save As.
- Project-relative saving.
- Full-size viewer.
- Checkpoint selection.
- Elma image-thinking/creating/complete animations.
- Existing ComfyUI adapter.
- Managed local runtime foundation.

### Local images

AIIDE Local can continue to use ComfyUI.

This remains useful for users who want private/local image generation
and have appropriate hardware.

### Cloud images

Cloud image providers may be added later through the same provider
philosophy.

The long-term image architecture should be:

``` text
Image Generation
│
├── Local
│   └── ComfyUI
│
└── Cloud
    ├── Provider A
    ├── Provider B
    └── Future APIs
```

### Future opportunity

Eventually Elma could perform workflows such as:

``` text
"Create a dark pixel-art background for this page and use it in the project."
```

AIIDE could then:

1. Generate the asset.
2. Save it into the project.
3. Inspect the relevant UI files.
4. Propose code changes using the asset.
5. Present everything for review.

This could become a distinctive AIIDE capability, but it is explicitly
**not part of the immediate cloud-agent milestone**.

------------------------------------------------------------------------

## 12. Immediate Branch Strategy

Preserve the current known state before the pivot.

Recommended approach:

1. Ensure current routing/reliability work is safely committed.
2. Preserve the current local-capable state.
3. Create a new development branch for the cloud-agent pivot.

Suggested branch:

``` text
feat/application-led-editing-v2
```

The branch should inherit the useful application foundation rather than
rebuilding AIIDE.

Do not remove Ollama or ComfyUI merely because they are not part of the
first cloud-agent milestone.

------------------------------------------------------------------------

## 13. Next Milestone --- M09: OpenRouter Native Agent

### Objective

Prove that a powerful OpenRouter-hosted model can inspect a repository
and produce a safe, reviewable file edit through AIIDE.

### Scope

Implement the smallest viable cloud-agent loop:

1. User submits an Edit request.
2. OpenRouter model receives the request.
3. Model can call AIIDE repository tools.
4. AIIDE validates and executes tool calls.
5. Model can inspect relevant repository content.
6. Model identifies the intended edit.
7. AIIDE owns exact source targeting and diff construction.
8. A pending change appears in Changes.
9. User can Apply or Reject it.
10. Existing stale-change protections remain intact.

### Initial tools

Prefer a deliberately small tool surface:

- `list_files`
- `search_files`
- `read_file`
- candidate/target selection
- replacement proposal

Git mutation does not need to become autonomous in M09.

### Out of scope

Do **not** include in M09:

- Advertising.
- Subscription infrastructure.
- AIIDE-managed billing.
- Cloud image providers.
- ComfyUI expansion.
- Major UI redesign.
- Large provider marketplace.
- Autonomous commits.
- Unrestricted shell access.
- Removal of Ollama.
- Removal of the existing image-generation work.

------------------------------------------------------------------------

## 14. M09 Acceptance Tests

The first benchmark should deliberately reuse tasks that exposed
weaknesses in the local architecture.

### Test A --- Simple explicit edit

Request a known heading change in `aiide-sandbox`.

Expected:

``` text
inspect
→ identify target
→ proposal
→ Changes
```

No model-authored exact `old_text` dependency.

### Test B --- Natural-language contextual edit

Example:

``` text
Make the main page heading say "Elma Editing Works".
```

Expected:

- Agent discovers the appropriate file/target.
- No exact original text is supplied by the user.
- Correct proposal is created.

### Test C --- Ambiguous target

Repository contains multiple plausible headings.

Expected:

- AIIDE/model identifies ambiguity.
- Candidate selection occurs safely.
- No arbitrary source mutation.

### Test D --- Missing target

Requested element does not exist.

Expected:

- No edit is fabricated.
- No file is changed.
- User receives a useful explanation.

### Test E --- Multi-file realistic task

A small UI change requiring two or more related files.

Expected:

- Agent investigates relevant files.
- Proposals are coherent.
- All mutations remain reviewable.
- No unrelated files are changed.

### Test F --- Stale proposal

Modify a target file after proposal creation.

Expected:

- AIIDE detects the stale state.
- Unsafe application is blocked.

------------------------------------------------------------------------

## 15. Success Criteria for the Pivot

The cloud-agent direction is validated when AIIDE can reliably:

- Understand natural coding requests.
- Investigate repositories without excessive prompting.
- Use tools correctly.
- Make useful multi-step decisions.
- Produce reviewable edits without exact-text copying failures.
- Keep filesystem mutation under application/user control.
- Switch models/providers without rewriting the core editing system.
- Provide a materially better experience than the current
    small-local-model loop.
- Keep inference costs controllable through free models, BYOK, local
    inference, and optional paid providers.

Performance should be judged by successful user tasks, not merely
whether a model returned valid JSON.

------------------------------------------------------------------------

## 16. Product Principle

AIIDE should not try to build its own foundation model.

It should build a better environment in which capable models can work.

The durable product is:

> **Elma + AIIDE's agent harness + safe repository tools +
> application-led editing + reviewable changes + provider choice.**

Models will change rapidly.

AIIDE should benefit from that change rather than requiring
architectural rewrites every time a better model appears.

### Core rule

> **Elma is the product. Models are interchangeable engines.**

And for editing:

> **The model decides what the user means. AIIDE decides what bytes
> change.**
