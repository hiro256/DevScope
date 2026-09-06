# Current Work Proposal

## Status

This is an exploratory proposal. The exact UI layout, persistence mechanism, CLI
shape, and lifecycle are intentionally undecided.

The initial Current Work CLI experiment has completed. Its findings inform the next
Skill experiment, but do not make the file format, persistence policy, or CLI syntax
a stable contract.

Its purpose is to define the conceptual distinction between persistent project tasks
and the smaller temporary steps used while performing one of those tasks.

## Motivation

Tasks in DevScope's Task Summary may represent relatively large units of planned
work. For example, a plan task such as "Git changes are reflected automatically"
can involve detecting Git metadata changes, refreshing repository status, updating
changed-file data, and adding regression tests. Showing only the parent task makes
fine-grained active work difficult to observe.

DevScope may therefore benefit from a separate representation of the current work
breakdown.

## Core distinction

Persistent project Plan tasks remain separate from temporary implementation steps.

```text
Task Summary  = persistent project plan
Current Work  = temporary breakdown of the selected or active Plan task
Evidence      = observed verification results
```

For example:

```text
Plan task
  ☐ Git changes are reflected automatically

Current Work
  ☑ Detect Git metadata changes
  ☑ Refresh repository status
  ☐ Refresh changed-files list
  ☐ Add regression test
```

The parent Plan task remains authoritative. Current Work is subordinate working
context, not a replacement for the Plan.

## Relationship to Plan

A Current Work item normally belongs to a parent Plan task.

```text
Plan
  -> Git changes are reflected automatically
       -> Detect Git metadata changes
       -> Refresh repository status
       -> Refresh changed-files list
       -> Add regression test
```

Fine-grained steps make implementation progress observable without forcing long-term
planning Markdown to contain every implementation detail. This keeps planning
documents readable.

## Temporary working state

Current Work is expected to be more temporary than normal Plan data. A work
breakdown may be created when beginning a Plan task, refined during implementation,
completed during the task, and discarded or archived after the parent task is
finished. The exact persistence lifecycle remains undecided.

Current Work does not need to become permanent project documentation.

## Possible storage

One possible representation is ordinary Markdown:

```text
.devscope/
  work/
    current.md
```

Conceptually, the file could record a parent reference and checkbox-style work
steps. Ordinary Markdown would keep the data readable by humans and AI agents
without requiring a custom database. The exact path and format remain undecided.

## Plan discovery safety

If Current Work is stored as Markdown with task-checkbox syntax, DevScope must not
mistake those checkboxes for additional canonical Plan tasks. Current Work data must
be distinguishable from normal Plan sources and excluded from ordinary Plan discovery
where necessary.

```text
Canonical Markdown     -> Plan
Current Work Markdown  -> temporary working context
```

They must not be counted together as duplicate project tasks.

## Git tracking

Whether Current Work is committed to Git remains undecided.

- **Local-only:** Useful as temporary AI or human working state without repository
  churn.
- **Git-tracked:** Useful when work context must survive across machines,
  collaborators, or long-running branches.

A first experiment may prefer local-only state, but this proposal does not commit to
that choice. Both models remain separate from authoritative Plan data.

## AI workflow

A DevScope-oriented Skill could encourage an AI agent to create a small work
breakdown before or during implementation:

1. Select or identify the parent Plan task.
2. Create a small implementation breakdown.
3. Update Current Work while implementation proceeds.
4. Use Current Work to resume context if the AI session changes.
5. Complete the parent Plan task separately when appropriate.

This must not require DevScope to inspect private agent internals. The AI records
working context explicitly through ordinary DevScope-visible state.

## Session continuity

The completed experiment showed that Current Work can help a new AI session recover
what was being done. `devscope context` now provides a compact orientation summary
when Current Work exists; `work list` remains available for the detailed state:

```text
Plan: 31/43

Current task:
Git changes are reflected automatically

Current work:
2/4 complete

Next:
Refresh changed-files list

Git:
3 files modified
```

The exact `devscope context` output is outside this proposal.

## Possible CLI direction

Future CLI operations might conceptually include `devscope task start <task>`,
`devscope work list`, `devscope work add <text>`, and `devscope work done <item>`.
These names are illustrative only and do not commit to syntax. The important idea is
a narrow interface for safely querying and updating Current Work.

## Parent completion

Completing all Current Work steps must not automatically complete the parent Plan
task.

```text
Current Work complete
  -> parent may be ready for completion or review

Current Work complete
  -/-> automatically mark the parent task complete
```

Additional verification, review, manual checks, or Evidence may still be required.
A human or AI must complete the parent task explicitly.

## Relationship to Evidence

Current Work and Evidence are distinct.

```text
Current Work  -> What steps are being performed?
Evidence      -> What observed results demonstrate correct behavior?
```

A completed work step remains a work-state claim. It is not independent Evidence;
for example, a recorded regression-test step does not replace an observed passing
test result.

## TUI presentation

The exact TUI presentation is deliberately open. Future approaches could include a
compact overview section, a detail pane linked to the selected Task Summary item, an
expandable Plan-task view, a dedicated Current Work screen, or only the next item on
the overview.

The information model and workflow should be validated before selecting a layout.
The existing overview should not be crowded merely to expose every fine-grained
item.

## Suggested terminology

- **Current Work:** User-facing representation of fine-grained work being performed.
- **Work Breakdown:** Underlying concept for decomposing a parent Plan task.
- **Task Summary:** Persistent Plan-level tasks.

This terminology is provisional.

## Initial experiment

The completed small experiment validated one parent Plan task, a flat Current Work
breakdown, checkbox progress, explicit parent-child association, exclusion from normal
Plan counting, fresh-session recovery, and explicit parent completion. Its findings are
recorded below; the remaining questions concern persistence, richer writes, TUI
presentation, stable identity, and Handoff or Notes.

## Non-goals

The initial concept should not become:

- A full project-management hierarchy.
- Arbitrary nested task trees.
- A Jira-style issue system.
- A replacement for normal Plan Markdown.
- Automatic proof that a parent task is complete.
- A dependency on agent-private state.
- A reason to overload the main TUI with implementation details.

Keep the proposal small and compatible with DevScope's observation-first
architecture.

## Round 1 Current Work CLI experiment

The first experiment uses `.devscope/work/current.md` as local-only working state.
It associates one parent through a project-relative Plan Markdown path and task text,
and contains one flat list of checkbox work items. The first read-only command is
`devscope work list`.

```text
# Current Work

Parent: docs/roadmap.md
Task: Current Work CLI experiment

- [x] Define storage
- [ ] Dogfood the workflow
```

This does not yet define a stable Current Work file format or permanent persistence
policy. Current Work completion does not complete its Plan task automatically, and a
Current Work checkbox is not Evidence.

## Round 2 narrow write experiment

After fresh-session recovery demonstrated that read-only Current Work supports
checkpoint recovery, direct Markdown mutation proved more cumbersome than reading the
state. The next dogfood slice adds `devscope work done <number>` as a narrow write.
The number is the current display-order position, not a persistent identity; users
should run `work list` before selecting an item.

## Round 3 context summary experiment

Fresh-session recovery and the narrow `work done` write succeeded, but orientation
previously required both `context` and `work list`. The experiment adds a compact
Current Work summary to `context` when local Current Work exists. Its roles remain
separate: `context` provides progress, parent, and next item; `work list` provides
full items and display-order numbers; `work done` performs the narrow mutation.

## Experiment conclusion

The tested workflow showed that Current Work can represent temporary implementation
progress while remaining separate from canonical Plan tasks and observed Evidence. A
fresh AI session recovered the parent Plan task, Current Work progress, and next
incomplete item through `devscope context` and `devscope work list`, without an
additional source-Markdown read. No need for Agent-private state was observed in this
experiment.

The first experiment stored `.devscope/work/current.md` as local-only state, and the
ignored file did not appear in `git status --short`. This was useful for the tested
workflow, but it is not a permanent policy: a future Git-tracked mode remains an open
question.

## Dogfood findings

`devscope work done <number>` was a sufficient minimum write surface for the tested
workflow and was more natural and safer for an AI than direct Markdown editing. The
tested workflow did not require `work add`, `work start`, `work clear`, reopen or
undo, or persistent item IDs. These are not decisions to exclude them permanently.

The one-based numbers printed by `work list` are display-order positions, not stable
identities. The safe workflow is `work list`, confirm the current number, then run
`work done N`. Repeating `work done N` for an already completed item returns
`already complete` with exit code 0; this idempotency was useful for AI retries.

The compact `context` summary reduced orientation to one command when Current Work
exists. It reports completed/total progress, parent, and next incomplete item. It does
not replace `work list`: detailed items and current display-order numbers are still
needed before a mutation. Projects without Current Work receive no additional context
line. When only Current Work is malformed, `context` remains a best-effort orientation
view and reports it as unavailable, while `work list` returns a Current-Work-specific
error.

The command roles remain deliberately small:

```text
context              -> parent / progress / next item
work list            -> detailed Current Work / item numbers
work done <number>   -> selected item mutation
```

Current Work checkbox changes do not change Plan task counts or automatically
complete the parent Plan task. Completing a Current Work item is also not Evidence,
and Current Work changes do not make Build/Test Evidence stale.

## Remaining open questions

- Should Current Work ever be Git-tracked?
- Should `work add` become useful?
- Should `work clear` or archive support exist?
- Should the TUI expose Current Work?
- Should stable item identity ever be introduced?
- How should Current Work relate to future Handoff or Notes?
