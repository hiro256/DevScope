# DevScope Skill Prototype

## Purpose

Use DevScope as the shared progress surface for humans and AI. This guidance is a
workflow aid, not project truth, an Evidence source, a Current Work store, or an
authority source.

## Start

1. Run `devscope context` first; do not begin by reading all repository documents.
2. Orient from Plan state, Current Work parent/progress/next item when present, Git
   Activity, Evidence availability, and the remaining tasks it shows.
3. Treat `context` as orientation, not a complete specification.

## Read details only when needed

- Run `devscope work list` only for all Current Work items, a current `work done`
  number, or the explicit error behind `Current Work: unavailable`.
- Run `devscope task list` only to find remaining Plan tasks not shown by `context`.
- Read source Markdown only for acceptance criteria, detailed specification, or design
  constraints.

## During work

```text
context
  -> needed details
  -> small implementation step
  -> appropriate verification
  -> logical boundary
  -> work list, then work done N when needed
```

Use only existing DevScope commands:

```text
devscope context
devscope task list
devscope work list
devscope work done <number>
```

## Current Work rules

```text
Plan          = canonical project intent
Current Work  = temporary recorded working state
```

Current Work completion does not complete its parent Plan task. The only Current Work
write in this workflow is `devscope work done <number>`. Immediately before it, run
`devscope work list` and confirm the latest number: it is a current display-order
position, not a persistent ID.

Do not invent or invoke `work add`, `work start`, `work clear`, `work reopen`, or
`work undo`. Do not normally edit `.devscope/work/current.md` directly; use an existing
narrow CLI write when one exists.

## Verification and Evidence

```text
Evidence          = verification information surfaced by DevScope
Observed Evidence = verification directly observed by DevScope
AI assessment     = interpretation
```

This is not a stable provenance taxonomy. A successful `work done` is a Recorded
Current Work update, not Evidence. An AI running tests or reporting success is not, by
itself, Observed Evidence. Report recorded work, DevScope-surfaced verification, and
AI interpretation separately.

Git Activity shows what changed; it does not prove that a requirement is complete.

## Authority

The Skill does not grant authority. For Plan mutations, commits, or pushes:

```text
explicitly authorized -> follow the existing user or project instruction
not authorized        -> do not mutate
ambiguous             -> report and seek direction
```

Existing authority can come from the current user request, `AGENTS.md`, repository
instructions, or an explicit workflow instruction.

## Config maintenance

```text
Config = project-specific observation policy
       != Plan, Current Work, Evidence, or AI memory/notes
```

Start with defaults and inspect actual DevScope behavior. Consider Config only for a
concrete mismatch; otherwise leave the project zero-config. Do not add exclusions
merely to make Plan output cleaner or shorter. A justified reason is a derived,
generated, duplicated, intentionally non-authoritative, or semantically irrelevant
Plan source.

When authorized to change the project file, make the smallest explicit rule that
resolves the mismatch. Re-run the relevant DevScope behavior, confirm the mismatch is
resolved and unrelated observations remain correct, then review the Config diff.
Remove or simplify a rule that becomes obsolete. A Config change may legitimately make
completed Build/Test Evidence stale because Config is a project-relevant input.

## Stop

Before stopping, inspect `devscope context`, relevant verification, Current Work when
active, and `git status`. Report facts without mixing Plan, Current Work, Evidence, and
AI interpretation. Do not commit, push, or create a Handoff without explicit
user/project authorization.
