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

## Workflow

```text
context
  -> needed details
  -> work list only when a Current Work number is needed
  -> work active N only when starting or switching work
  -> small implementation step
  -> devscope verify build/test when relevant
  -> work done N at an appropriate logical boundary
  -> context and git status before stopping
```

Use `devscope task list` only to find Plan tasks not shown by `context`. Do not read the
whole repository or create Current Work solely to satisfy this workflow.

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

## Authority and stop

The skill grants no authority for Plan edits, commits, pushes, or Config changes. Follow
the user request and repository instructions. Before stopping, inspect `devscope context`,
relevant verification, Current Work when active, and `git status`; report Recorded Work,
Observed Evidence, and interpretation separately.
