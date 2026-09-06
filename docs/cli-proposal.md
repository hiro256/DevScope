# DevScope CLI Proposal

## Status

This is a proposal for a possible future CLI contract. The roadmap accepts only a
minimal read-only CLI experiment; its exact commands and all later capabilities remain
exploratory rather than adopted design decisions. DevScope must continue to work
without the CLI.

## Motivation

DevScope currently observes project state primarily from Markdown and Git. Projects
may use different Markdown structures, task checkboxes can lag behind work, and AI
agents may repeatedly read large planning files. Direct integration with
agent-specific session formats is also exposed to upstream format changes.

A small CLI could provide a stable interface between AI agents and ordinary project
Markdown, while reducing context and token overhead where that proves practical.

## Core principles

- DevScope continues to work without the CLI.
- Markdown remains the human-readable source of Plan information.
- No mandatory DevScope-specific Markdown format is introduced.
- Normal Markdown headings and task lists are preferred.
- Read broadly and write narrowly.
- Writes modify only the necessary task line or target section.
- Agent integrations remain optional adapters.
- Ambiguous or stale operations fail safely rather than guessing.

Conceptually:

```text
DevScope Core = observation
DevScope CLI  = safe query and narrow mutation interface
Skill         = teaches an AI how to use the CLI
Markdown      = remains ordinary human-readable Markdown
```

## Discovery and introspection

An AI may not know which task lists are present in a project. Candidate commands
are:

```text
devscope status
devscope context
devscope lists
```

`devscope lists` would discover manageable task-list sections, such as Markdown
sections containing task checkboxes. Identifiers should be derived from the file
and heading structure rather than requiring embedded metadata.

```text
roadmap:v020-quality   docs/roadmap.md   v0.2.0 > Quality
roadmap:post-mvp       docs/roadmap.md   Post-MVP
```

`devscope context` could provide a compact summary of plan progress, available
lists, Git state, and Evidence status, avoiding repeated reads of multiple files.

## Task queries

Candidate commands are:

```text
devscope task list
devscope task list --list <list>
```

Output should be compact and deterministic for humans and AI agents. Task numbers
can be transient identifiers derived from the current parsed snapshot; they need
not be persisted in Markdown.

## Task completion

A candidate command is:

```text
devscope task done <id>
```

It would update only the corresponding Markdown checkbox. Before writing, the CLI
must confirm that the list still matches the snapshot from which the identifier was
obtained. If it does not, it should fail safely, for example:

```text
Task list changed. Run `devscope task list` again.
```

It must not silently select a different task.

## Task addition

The AI decides what work belongs in which section; DevScope is responsible for a
safe, narrow Markdown edit. Possible commands are:

```text
devscope task add "..." --before <id>
devscope task add "..." --after <id>
devscope task add "..." --section <section>
```

The initial design does not require persistent DevScope identifiers or special HTML
comments in Markdown.

## Markdown handling

Reading should discover Markdown task checkboxes and retain their file path,
heading or section path, location, and completion state.

Writing should change one checkbox for completion, or insert one task at the
requested location. It should avoid rewriting unrelated Markdown. Ordinary manual
editing by humans and agents remains supported.

## Evidence extension

A future CLI might expose a command such as:

```text
devscope evidence run test
```

Evidence should be observed from actual build or test execution when possible, not
from an agent's textual claim. For example, an agent could request a configured
test run, DevScope could execute and observe the result, and the TUI could display
that Evidence. This is future work, outside the initial CLI experiment.

Build/Test results are currently runtime state owned by one running TUI process. A
separate one-shot CLI cannot truthfully expose a previous TUI session's Passed, Failed,
Stale, or Running state without a sharing or persistence mechanism. Therefore
`devscope evidence status` is deferred until `context` and `task list` have been
dogfooded and truthful state-sharing semantics are understood.

## Skill integration

A DevScope-oriented AI skill could teach agents to use `devscope context` or
`devscope lists` when state is unclear, `devscope task list` instead of rereading
large Markdown files, and narrow task completion or addition commands when needed.

A Skill can improve reliability but is not required for DevScope. The interface
must not couple the core to Codex, so it can also serve Claude Code or other agents.

## Token-efficiency hypothesis

The CLI may reduce AI context use by replacing repeated Markdown discovery and
parsing with compact structured queries. For example:

```text
Plan: 31/43
Lists: roadmap:v020-quality 0/4, roadmap:post-mvp 0/8
Git: clean
Evidence: unavailable
```

This is a hypothesis to measure, not an assumed benefit.

## Initial dogfood finding

In an initial DevScope repository comparison, `context` output was about 428 characters
and `task list` about 426 characters: roughly 82% smaller than reading the current
`docs/roadmap.md` directly. Ten native `context` invocations averaged about 262 ms.
These are exploratory observations for the narrow project-orientation use case, not a
benchmark contract or proof of overall AI token reduction.

## Follow-up workflow dogfood

Four representative AI workflows confirmed distinct command roles:

1. Identifying the highest-priority remaining task needed `context` only, with no
   additional documents or raw Markdown reread.
2. Understanding that task's specification used `context` to identify it, then one
   source document (`docs/cli-proposal.md`) for detail, without raw Markdown reread.
3. Choosing from all remaining tasks, including those beyond the context limit, needed
   `task list` after `context`, with no additional documents or raw Markdown reread.
4. Checking Evidence execution results used `context` to learn that run state is not
   exposed by the one-shot CLI; this truthfully communicated the observation boundary.

In this Windows DevScope observation, `context` was 428 characters across 12 lines and
averaged 259.3 ms. `task list` was 426 characters across 10 lines for 9 remaining tasks
and averaged 28.8 ms, about 89% faster than `context`. The standard orientation flow was
`context` only (428 characters); full task discovery used `context` plus `task list`
(854 characters). These measurements are experimental observations, not performance
guarantees or a claim of overall AI token reduction.

The observed operating rule is:

```text
Run devscope context once.
If its shown tasks select the work target, do not run task list.
If selection needs tasks behind "... N more", run devscope task list.
Read source Markdown only for specification, acceptance intent, or other detail.
```

The CLI is therefore an orientation and discovery surface, not a replacement for
Markdown as the authoritative Plan source. Nine remaining tasks did not demonstrate a
need for `--limit`, filtering, `lists`, or JSON. Those remain future candidates; no
syntax or implementation is selected. Evidence run-state sharing, persistence, IPC, and
a daemon likewise remain deferred.

## Experiment conclusion

The minimal read-only CLI experiment is successful enough to conclude. `context` was
sufficient for common orientation, `task list` was needed only for broader discovery,
and raw Markdown reads became selective rather than default. Output stayed compact, and
Codex used the CLI directly without requiring an Agent adapter. The findings do not
adopt a stable CLI contract or decide that an Agent adapter will never be useful.

## Initial experiment

Keep a first implementation deliberately small and primarily read-only:

```text
devscope context
devscope task list
```

`devscope status` and `devscope lists` remain possible supporting read commands, but
are not required for the first experiment. `devscope evidence status` is a later
candidate after truthful session-state sharing semantics are understood. Dogfood the
CLI while developing DevScope, then evaluate whether it is reliably used, reduces
repeated reads, reduces context usage, remains convenient for humans, and stays
independent of a specific AI agent.

After read-only dogfooding, a separate Current Work experiment may test narrow
operations conceptually such as `devscope work list`, `devscope work add`, and
`devscope work done`; exact syntax remains undecided. Only after that evidence should
a small DevScope Skill be dogfooded. Direct Agent integration is not required for the
first AI workflow experiment and can be reassessed after the CLI/Skill workflow.

`devscope task done`, task addition, and other writes are later experiments, not part
of the first CLI experiment. They must preserve the narrow-write boundary: explicit
targets, minimal Markdown edits, and safe failure for stale identifiers.

## Non-goals

The CLI must not become:

- A general Markdown editor.
- A Jira-like project management system.
- A mandatory DevScope database format.
- A dependency on one AI vendor or agent.
- A replacement for normal Git and Markdown workflows.
## Current Work read experiment

The Current Work experiment adds the read-only `devscope work list` command for local
working state. Further write syntax and persistence policy remain unspecified.

## Narrow Current Work write experiment

`devscope work done <number>` is the first concrete example of reading broadly and
writing narrowly: it completes only the selected Current Work checkbox. Its
one-based display-order number is intentionally not a persistent identifier.
