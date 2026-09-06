# Current Work Proposal

## Status

This is an exploratory proposal. The exact UI layout, persistence mechanism, CLI
shape, and lifecycle are intentionally undecided.

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

Current Work may help a new AI session recover what was being done. A future compact
context view could conceptually report:

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

A later small experiment could validate one parent Plan task, one Current Work
breakdown, several checkbox-style work steps, progress such as 2/4, explicit
parent-child association, safe exclusion from normal Plan counting, recovery through
a new AI session, and explicit parent completion.

Key questions include whether Current Work improves active-development clarity and
session continuity, avoids over-expanding roadmap Markdown, creates acceptable
overhead, should be local-only or Git-tracked, belongs on the overview, benefits
from CLI helpers, and remains clearly distinct from Plan and Evidence.

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
