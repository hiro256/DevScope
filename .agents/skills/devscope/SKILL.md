---
name: devscope
description: "Use DevScope for progress, Current Work, and Build/Test verification in repositories that already use the DevScope workflow. Do not use to introduce DevScope into unrelated repositories."
---

# DevScope

Use this skill only in a repository that already uses DevScope or when the user explicitly
asks to use its workflow. It guides DevScope operation; repository-specific authority and
quality rules remain in `AGENTS.md`. Do not introduce DevScope, create Config, or change
project policy merely because this skill is available.

## Start

Run `devscope context` before reading repository documents broadly. Use its Plan, Current
Work, Activity, and Evidence summary to decide which detail is needed. Treat it as
orientation, not a complete specification.

Read the repository-root `docs/guides/devscope-setup.md` only when `devscope` is
unavailable, Config is invalid, Build/Test is unexpectedly unavailable, the observation
does not match the project, or project structure changed.

## Canonical workflow

```text
Observe: devscope context
  ↓
Classify request
  ├─ Question / review / investigation / trivial read-only work
  │    └─ inspect only what is needed → stop
  └─ Implementation / modification (multi-step)
       ↓
     Plan: reuse a suitable incomplete parent;
           otherwise record one minimal request-scoped checkbox
       ↓
     Current Work: create/update a small meaningful checklist
       ↓
     Active: devscope work list → devscope work active N
       ↓
     Implement one meaningful Active item
       ↓
     devscope work list → devscope work done N
       ↓
     More work?
       ├─ yes → devscope work list → devscope work active N → Implement
       └─ no → Verify: relevant verification
                 ↓
               Close: check parent Plan acceptance intent;
                      complete its checkbox only if satisfied
                 ↓
               devscope context → git status --short
```

Questions, reviews, investigation-only requests, and trivial read-only work normally
stop after Observe/Classify and the needed inspection. Do not create or mutate Plan,
Current Work, or Active merely to satisfy this workflow. The implementation branch
applies to explicit multi-step code, documentation, or settings changes, not as a rigid
requirement to create Work for trivial edits.

Use `devscope task list` only to find Plan tasks not shown by `context`. Do not read the
whole repository solely to satisfy this workflow. The rules below define the Plan and
Current Work authority used by the canonical path.

Before implementation, reuse a suitable incomplete Plan task for the same purpose.
If none exists, the explicit implementation request itself authorizes recording one
minimal Markdown checkbox task in the appropriate Roadmap section. Read the relevant
section and avoid duplicates. This authority covers only the requested work: do not
reorganize unrelated Roadmap content, promote unrelated Backlog candidates, add future
work or unrequested design changes, or split one request into many Plan tasks. Plan edits
not naturally entailed by the request still require explicit user authorization.

Under that parent, create or update a small Current Work checklist of the actual steps
before implementation, then read `devscope work list` and explicitly activate the first
meaningful item. Good stages include Inspect Evidence behavior, Implement Evidence Full
Detail, and Verify failure flow. Open ui.rs, Edit app.rs, and Run cargo fmt are individual
files/commands, not meaningful Work stages. Avoid a single broad improve-project item.
This is a temporary implementation breakdown, not a permanent Plan task hierarchy.

There is currently no `devscope task add`, `devscope work add`, or `devscope work create`.
Record the Plan task by a targeted edit to its canonical Markdown. For Current Work
creation or structural updates that the CLI cannot express, inspect the existing
`.devscope/work/current.md` (or the repository's documented storage example if absent)
and minimally edit that file, preserving its format, parent/task association, and
unrelated state. In this repository the existing structure is:

```markdown
# Current Work

Parent: docs/roadmap.md
Task: <exact parent Plan task text>

- [ ] Inspect current behavior
- [ ] Implement requested change
- [ ] Verify outcome
```

Use the actual parent source path and exact Plan task text. Do not overwrite unfinished
unrelated Work to start a new request; resolve its disposition first. Direct file edits
are limited to creation and structural updates; prefer the available `work active` and
`work done` CLI for ordinary mutations, and confirm structural edits with `work list`.

Active is the live execution cursor driving the TUI current-work state: the meaningful
Current Work item actually being executed. Switching work means moving to another
meaningful checklist item, even within the same user request. Set the new Active before beginning that item;
do not switch for each command, file edit, or brief check. Completing the Active item
with `work done` clears Active and does not activate a successor. If continuing another
item, read `work list` again and explicitly activate it before implementation resumes.
Read `work list` immediately before each numbered `work active` or `work done` operation.

Active may remain unset when the next step is undecided, work has ended or is paused,
or a user decision is needed. At completion, verify the parent's acceptance intent and
update its canonical checkbox explicitly only when satisfied.
Verification passed does not automatically mean parent Plan complete: Observed Evidence
is verification, not acceptance authority for the requested/accepted outcome.

## Current Work and Evidence

- Plan is canonical intent; Current Work is temporary Recorded state. Completing Work
  does not complete its parent Plan task.
- Active is explicit. Never infer it from `Next` or checklist order. Before `work active`
  or `work done`, run `devscope work list`: displayed numbers are not persistent IDs.
- Prefer `devscope verify build` and `devscope verify test` over directly invoking a
  project-native verification command when DevScope verification is available. DevScope
  resolves configured commands or the Cargo fallback.
- Evidence is not Current Work. Git Activity and reported AI success do not prove
  completion; only DevScope-observed command results are Observed Evidence.

## Activity exclusion maintenance

When worktree scan cost needs investigation, run `devscope activity suggest-excludes`. It reports
on-demand, Git-safe candidates only; it never edits Config automatically.

1. If it reports no proposals, make no Config change and report that no change is needed.
2. For each candidate under consideration, describe its exact project-relative path, diagnostic
   source, entries/duration hint, and Git safety reasons. A safe proposal is not permission to edit.
3. Show the exact `[activity].exclude` change and obtain explicit user approval before editing.
4. After approval, re-read `.devscope/config.toml`. If it is invalid, report the error and do not
   repair it without a separate request. Add only the explicitly approved proposal path, using
   forward slashes; never broaden it to an ancestor, glob, sibling, or another candidate.
5. Preserve unrelated sections, comments, formatting, and existing excludes with a targeted edit.
   If Config or `[activity]` is absent, add only the minimum approved `[activity].exclude` content.
   Do not add a duplicate or a path already covered by an existing ancestor exclusion.
6. Re-run `devscope activity suggest-excludes`, inspect `git diff -- .devscope/config.toml`, and
   report remaining candidates without applying them automatically.

Report the command outputs and Config diff as **Observed**, the approved exclusion as
**Configured**, and any expected scan-cost effect as **Interpretation**. A disappeared proposal or
changed duration is an approximate diagnostic hint, not a benchmark proof.

## Authority and stop

Beyond the request-scoped Plan recording above, this skill grants no authority for Plan edits,
commits, pushes, or Config changes. Follow
the user request and repository instructions. Before stopping, inspect `devscope context`,
relevant verification, Current Work when active, and `git status`; report Recorded Work,
Observed Evidence, and interpretation separately.
