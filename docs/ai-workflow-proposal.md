# Human/AI Workflow Proposal

## Status

This is an exploratory proposal. Exact CLI syntax, Skill format, persistence model,
Agent adapters, Handoff format, and TUI presentation remain intentionally undecided.
It defines an interaction model before additional AI-oriented features are implemented.

## Vision

DevScope should help humans and AI agents share the same understanding of project
progress. It should not merely display what an AI says it is doing. Instead, it should
combine directly observed project state with explicitly recorded working context.

```text
Plan
  -> What is intended?

Current Work
  -> What fine-grained work is being performed now?

Activity
  -> What actually changed?

Evidence
  -> What verification has been observed?

Handoff / Notes
  -> What does a participant want to communicate to the next participant?

Agent
  -> Who or what is currently working, when observable?
```

DevScope must remain useful when no AI agent is connected.

## Shared surface

The long-term direction is:

```text
Human
  <-> TUI / CLI
        |
        v
     DevScope
        ^
        |
AI Agent
  <-> CLI / Skill
```

Humans and agents inspect the same underlying state. The TUI is primarily the
human-readable view; the CLI is the machine-readable and automation-friendly
interface. A Skill defines how an AI agent should use the CLI during work.

## Information trust boundaries

Different DevScope information has different provenance and must not be treated as
equally authoritative.

```text
Observed
  -> Directly derived or observed by DevScope.
  -> Git Activity, process-based Evidence, filesystem observations.

Recorded
  -> Explicit working state written by a human or AI.
  -> Current Work.

Reported
  -> Communication or interpretation supplied by a human or AI.
  -> Handoff, Notes, explanations.
```

These categories are conceptual vocabulary only; this proposal does not introduce a
committed generic provenance data model. Reported information must not silently become
Observed Evidence. For example, an agent report that tests passed is not equivalent to
DevScope observing `cargo test` complete with exit code 0.

## CLI role and write boundary

A future CLI should provide a narrow, stable interface for humans, agents, and
automation to inspect DevScope state. It should prefer structured facts over
interpretation. Possible read-oriented commands include:

```text
devscope context
devscope task list
devscope work list
devscope evidence status
devscope evidence tests
devscope translate status
```

Exact command names, syntax, JSON, and other output formats remain provisional.

AI agents may eventually write limited working state through operations conceptually
like `devscope work add`, `devscope work done`, `devscope task done`, and `devscope
handoff`. Writes must be narrow and explicit. DevScope must not become an unrestricted
project-management database or agent scratchpad. Preserve the existing principle:
**read broadly, write narrowly**.

## Skill role

A DevScope Skill should define workflow behavior, not project truth. It may instruct
an agent to read context, identify a parent Plan task, create or update Current Work,
inspect Activity, run or inspect Evidence, review relevant tests, update Plan state,
record a Handoff, or check translation synchronization.

The Skill must not replace Evidence observation. It may request verification, but a
statement in a Skill or agent response is not itself Evidence.

## Suggested AI work cycle

This is illustrative, not a committed Skill specification.

### Work start

1. Read `devscope context`.
2. Inspect the relevant Plan task and existing Current Work.
3. Inspect current Git Activity and Evidence.
4. Resume existing work or create a small Work Breakdown.

### During implementation

1. Perform one small work step.
2. Update Current Work at logical boundaries.
3. Observe Activity and run appropriate development-time tests.
4. Avoid continually rewriting high-level Plan Markdown for temporary detail.

### Verification

1. Inspect the parent task's acceptance intent and relevant test scenarios.
2. Identify important missing verification.
3. Request or run verification through a DevScope-observable path where possible.
4. Inspect Evidence outcome and freshness.
5. Do not treat a recorded Current Work checkbox as independent Evidence.

### Work completion

1. Confirm Current Work is complete or intentionally deferred.
2. Confirm relevant Evidence is available and current where required.
3. Update the parent Plan task and source Markdown explicitly where appropriate.
4. Check translation synchronization when translations are enabled.
5. Record a concise Handoff when continuation is likely.
6. Review the Git diff before commit.

## Test and verification awareness

A future CLI may expose observed verification facts beyond Passed or Failed, including
test counts, passed/failed/ignored counts, test names, failing cases, prior-run
differences, and verification scenario summaries. The Skill or an AI may interpret
whether those tests appear adequate for a Plan task, but Core must not claim adequate
testing merely because discovered tests passed.

```text
Observed:
  46 tests passed

Interpretation:
  These tests appear sufficient for the current requirement
```

The second statement is reasoning, not raw Evidence.

## Handoff and session continuity

A Handoff is a possible future Reported communication mechanism between AI sessions,
an AI and a human, or two humans. It should be concise and subordinate to observable
project state. A conceptual form is:

```text
Current task:
Evidence freshness model

Done:
- filesystem fingerprint implemented
- target exclusion implemented

Next:
- wire stale transition into application loop

Attention:
- Windows symlink behavior not verified

Evidence:
- Tests 48/48 passed
- Build passed
```

Its format and storage are undecided. A Handoff is not authoritative Evidence.

One aim is to reduce dependence on conversational memory. A new AI session should be
able to reconstruct useful context from DevScope rather than the previous transcript.
A future `devscope context` could compactly report Project, Plan progress, parent task,
Current Work, recent Activity, Evidence, translation state, and a recent Handoff.

## Agent independence and human workflow

DevScope must remain independent of Codex, Claude, ChatGPT, or any specific agent.
Optional Agent adapters may report agent identity, active/idle state, or last observed
activity, but the workflow must function without them. A CLI plus Skill should be
enough for an initial experiment.

Humans should use the same model without AI: inspect the TUI, run CLI queries, update
Current Work if desired, trigger or inspect Evidence, leave a Handoff, and resume from
the shared state. AI participation must never be required.

## Relationships to Current Work, Evidence, and Activity

Current Work is temporary fine-grained implementation context, not a permanent task
hierarchy. The parent Plan task remains authoritative long-term planning state, and
completing all Current Work steps must not automatically complete it.

Evidence remains independently observed verification information. An AI may request,
inspect, or interpret Evidence but must not manufacture Observed Evidence by reporting
success text. Who triggered a verification and how its result was observed are separate
concerns; possible future metadata is not a committed taxonomy.

Git Activity remains directly observed project-change information. An agent report
such as "Updated freshness logic" complements, but does not replace, observed changed
files.

## Relationship to translation workflow

At a logical work boundary, a Skill may check translation pending state, delegate a
focused translation worker, verify synchronization, and commit source and translation
together. Translation remains derived human-readable content, not authoritative Plan
state.

## First Human/AI workflow implementation sequence

Do not implement the entire workflow at once. This sequence describes the Human/AI
workflow track; it is not the only implementation sequence for DevScope:

1. Implement a minimal read-oriented CLI.
2. Validate `devscope context` and related queries with an AI agent.
3. Experiment with Current Work through a narrow CLI.
4. Write and dogfood a small DevScope Skill.
5. Only then experiment with Handoff or Note recording.
6. Reassess Agent integration after the CLI/Skill workflow is proven.

The first implementation slice of the AI-oriented CLI is read-only: `devscope context`
and `devscope task list`. `devscope evidence status` is a later candidate after
truthful session-state sharing semantics are understood; `devscope work list` and
`devscope evidence tests` may follow. Evaluate whether an AI can reconstruct state
reliably, whether output reduces context use, whether it distinguishes Plan, Activity,
Current Work, and Evidence, and which writes are actually necessary.

After that, a small Skill can read context at work start, identify the parent task,
inspect Evidence before completion, check Current Work when available, record only
necessary state, avoid treating claims as Evidence, and leave a Handoff only when
useful. Dogfood it on DevScope before generalizing.

This workflow track does not replace the separate Evidence track in
[evidence-design.md](evidence-design.md). Cargo Evidence may continue to be dogfooded
and Artifact Evidence may be experimented with before a stable generic Evidence Source
contract is defined. The relative timing of those tracks remains open.

Follow-up dogfooding validated a context-first Human/AI workflow: use `devscope context`
for orientation, add `devscope task list` only when broader task discovery is needed,
and read source Markdown only for detail. This is an orientation and discovery surface,
not a replacement for authoritative Plan Markdown. The initial result also shows that a
direct Agent adapter is not required for this workflow today; reassessment remains later.

## Non-goals

The initial Human/AI workflow must not become:

- A chat system, issue tracker, general AI memory database, or automatic project manager.
- A replacement for Git or Markdown Plan documents.
- An unrestricted AI write interface.
- A requirement for a specific AI vendor.
- A system that treats AI reports as observed truth.
- A reason to delay completion of the current Evidence milestone.

## Open questions

- Exact CLI command names and output formats, including whether JSON is immediately needed.
- Current Work persistence details.
- Handoff storage format, lifecycle, and whether it is Git-tracked or local.
- How much test-case detail the CLI should expose.
- How verification scenarios should be summarized for humans.
- Whether Agent adapters add sufficient value after CLI/Skill dogfooding.
- Which concepts, if any, require a formal provenance model.
