# DevScope CLI Proposal

## Status

This is a proposal for a possible future CLI. It is not a committed roadmap item or
an adopted design decision, and DevScope must continue to work without it.

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

## Initial experiment

Keep a first implementation deliberately small:

```text
devscope status
devscope lists
devscope task list
devscope task done
```

Dogfood the CLI while developing DevScope, then evaluate whether it is reliably
used, reduces missed Markdown updates and repeated reads, reduces context usage,
remains convenient for humans, and stays independent of a specific AI agent. Only
then should task addition, Evidence commands, or broader workflow features be
considered for the roadmap.

## Non-goals

The CLI must not become:

- A general Markdown editor.
- A Jira-like project management system.
- A mandatory DevScope database format.
- A dependency on one AI vendor or agent.
- A replacement for normal Git and Markdown workflows.