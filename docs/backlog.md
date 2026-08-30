# Backlog

The backlog contains implementation candidates and exploratory ideas. Entries here
are not committed roadmap work. An item must be promoted to
[roadmap.md](roadmap.md) before implementation begins.

## Implementation candidates

- **Agent-neutral DevScope CLI.** Explore a small interface that is useful to
  people and any AI agent rather than one specific agent. Initial candidates are
  `status`, `lists`, `task list`, and `task done`. Evaluate dogfooding, safe
  narrow Markdown updates, agent independence, and potential context or token
  savings. See [cli-proposal.md](cli-proposal.md).
- **Pre-generated translated Markdown.** Explore English source documents with
  derived Japanese Markdown. Evaluate missing and stale detection, exclusion from
  Plan discovery, and AI/provider-independent synchronization. See
  [translation-proposal.md](translation-proposal.md).

## Promotion flow

```text
Idea / proposal
      ↓
Backlog
      ↓
Evaluation
      ↓
Roadmap
      ↓
Implementation
```

Being listed in the backlog does not authorize implementation.

## Document roles

- **Backlog:** Candidate index and implementation candidates.
- **Proposal:** Deeper exploration and design notes.
- **Roadmap:** Accepted implementation work.
- **Decisions:** Adopted design decisions.
