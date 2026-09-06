# DevScope Skill Proposal

## Status

This is an exploratory, docs-first proposal for the DevScope Skill experiment. It
specifies reusable agent behavior, not a stable Skill package, file format, or
provider integration. No Skill implementation is created by this proposal.

## Purpose

The Skill should teach an AI agent how to use the existing DevScope CLI workflow to
orient, resume temporary work, perform narrow Current Work updates, and distinguish
recorded work from observed verification. It should reduce repeated human workflow
reminders without becoming a source of project truth.

## Position in the architecture

```text
Plan / Activity / Evidence / Current Work
                    ↓
                 DevScope
                    ↑
                   CLI
                    ↑
                  Skill
                    ↑
                AI Agent
```

The Skill is a behavior layer. It does not store Plan, Activity, Evidence, Current
Work, private session state, or hidden progress. DevScope and its CLI remain the
visible project surfaces.

## Non-goals

The Skill must not become:

- A project or task database.
- An Evidence source or a Current Work store.
- A Codex-only, Claude-only, or other provider-specific integration.
- A direct Markdown editor for Current Work in the normal workflow.
- A Handoff or Notes feature.
- An authority to complete Plan tasks, rewrite a roadmap, start unrelated work,
  commit, or push without user direction.

## Core workflow

1. Run `devscope context` first.
2. Use it to inspect Plan state, Current Work parent/progress/next item when present,
   Git Activity, and Evidence availability.
3. Run `devscope work list` only when detailed Current Work items or a current
   display-order number are needed.
4. Run `devscope task list` only when the desired remaining Plan task is not shown by
   `context` or broader task discovery is necessary.
5. Read source Markdown only for acceptance intent, detailed specification, or design
   constraints not available from the orientation output.
6. Implement the requested work and perform appropriate verification.
7. At a logical work boundary, when a Current Work item is ready to record, run
   `work list`, confirm its current number, then run `devscope work done <number>`.
8. Re-run `devscope context` to reorient before reporting or continuing.

This is the practical form of **read broadly, write narrowly**: inspect the available
CLI and source documents only as needed, and use only explicit narrow DevScope writes.

## CLI usage rules

The initial Skill refers only to commands that exist today:

```text
devscope context
devscope task list
devscope work list
devscope work done <number>
```

It must not invent `work add`, `work start`, `work clear`, reopen/undo, or any other
unimplemented command. When no Current Work summary is present, treat Current Work as
inactive; do not try to create it through imagined CLI syntax. When `context` reports
`Current Work: unavailable`, run `work list` to obtain the explicit Current-Work error
rather than treating the state as healthy.

Before `work done N`, the Skill must run `work list` and confirm the latest `N`.
Numbers are one-based display-order positions, not persistent identities. Repeating a
completion for an already completed item is a safe retry because it exits successfully,
but the Skill should not perform redundant retries without a reason.

## Trust boundaries

```text
Plan          = canonical project intent
Current Work  = temporary recorded work state
Evidence      = DevScope-observed verification result
AI assessment = interpretation, not observed truth
```

A completed Current Work item does not complete its parent Plan task. A successful
`work done` command is not Evidence, and an AI statement that tests passed is not
Observed Evidence. The Skill should request or inspect appropriate verification, then
report the difference between observed results and its own assessment.

Git Activity is an observation of what changed, not proof that a requirement is
complete. `context` is an orientation surface, not a replacement for source Markdown
when details are necessary.

## Stop behavior

At a stopping boundary, the Skill should inspect `devscope context`, relevant
verification, Current Work state when active, and `git status`. It should report those
facts clearly, preserve the Plan/Evidence distinction, and request direction for any
Plan-level mutation. It must not automatically commit, push, or create a Handoff.

## Agent neutrality and packaging candidates

The core guidance is agent-neutral: it assumes only that an agent can run the
DevScope CLI. Provider-specific packaging must remain separable from this guidance.

Candidate locations remain undecided:

- `skills/devscope/SKILL.md`: repository-local and compatible with conventions used
  by some agents, but its packaging semantics are provider-specific.
- `.devscope/skill.md`: clearly project-local, but not necessarily recognized by
  existing agent skill loaders.
- `docs/examples/devscope-skill.md`: agent-neutral and easy to review, but requires
  an external packaging step before an agent can use it automatically.

DevScope must not depend on any one of these conventions. The experiment should first
validate the concise core behavior; any provider wrapper can be added later.

## Dogfood scenarios

### Scenario A: fresh-session recovery

A fresh AI session receives only the repository and the Skill. It begins with
`devscope context`, recovers active work, and reads source Markdown only if the task
needs specification detail.

### Scenario B: active Current Work mutation

With active Current Work, the agent implements a bounded change, runs `work list` to
confirm the current number, and uses `work done N` at a sensible work boundary.

### Scenario C: Current Work absent

With no Current Work summary, the agent continues normal orientation without inventing
`work start` or `work add`.

### Scenario D: malformed Current Work

When context reports Current Work as unavailable, the agent uses `work list` to inspect
the explicit error and does not silently accept the state.

### Scenario E: verification boundary

After tests run, the agent explains separately what Current Work was recorded, what
DevScope observed as Evidence, and what it interprets from those facts.

## Measurements

Dogfooding should record, without requiring a formal benchmark:

- DevScope command count.
- Source Markdown reads.
- Human reminders required.
- Workflow mistakes.
- Invalid CLI commands attempted.
- Current Work and Evidence confusion.

## Success criteria

The experiment is successful when a fresh agent can begin with `context` without extra
human workflow instruction, recover active work, use `work list` only when detail or
numbers are needed, confirm a number before `work done`, and avoid nonexistent write
commands. It must keep Current Work distinct from Plan and Evidence, avoid inventing
verification results, and function without a direct Agent adapter. The guidance should
also remain short enough for agents to follow consistently.

## Failure signals

The Skill needs revision if agents routinely read all documents before `context`, skip
context, guess `work done` numbers, treat Current Work completion as Plan completion,
treat self-reported tests as Observed Evidence, invoke nonexistent `work add` or
`work start`, or ignore essential rules because the Skill is too long.

## Open questions

- Which candidate packaging is most portable and usable in practice?
- How short can the guidance become without losing critical safety rules?
- Does a provider-specific wrapper add value beyond the agent-neutral core?
- Should a future Skill mention Handoff or Notes after those workflows are tested?
- When, if ever, should Current Work creation or archival become a supported write?
