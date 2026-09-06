# Backlog

The backlog contains implementation candidates and exploratory ideas. Entries here
are not committed roadmap work. An item must be promoted to
[roadmap.md](roadmap.md) before implementation begins.

## Implementation candidates

- **Agent-neutral DevScope CLI.** The minimal read-only `context` and `task list`
  experiment completed successfully. Further read-oriented commands, task writes,
  filters, JSON, and Evidence state commands remain exploratory candidates. See
  [cli-proposal.md](cli-proposal.md) and [ai-workflow-proposal.md](ai-workflow-proposal.md).
- **Human/AI workflow experiments.** The Current Work CLI experiment completed with
  a narrow `work done` write and compact context summary. Evaluate a DevScope Skill
  experiment and a possible Handoff / Notes experiment next. Reassess whether an
  Agent adapter is useful only after the CLI/Skill workflow. See
  [ai-workflow-proposal.md](ai-workflow-proposal.md).
- **Pre-generated translated Markdown.** Explore English source documents with
  derived Japanese Markdown. Evaluate missing and stale detection, exclusion from
  Plan discovery, and AI/provider-independent synchronization. See
  [translation-proposal.md](translation-proposal.md).
- **Artifact Evidence experiment.** After Cargo Build/Test Evidence is working,
  explore a small filesystem-based Evidence source for expected artifacts such as
  `reports/final-report.pdf`. Use it to compare process observation with filesystem
  observation before defining a stable generic Evidence Source extension contract.

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
