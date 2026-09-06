# Backlog

The backlog contains implementation candidates and exploratory ideas. Entries here
are not committed roadmap work. An item must be promoted to
[roadmap.md](roadmap.md) before implementation begins.

## Implementation candidates

- **Agent-neutral DevScope CLI.** Explore a small interface that is useful to
  people and any AI agent rather than one specific agent. Start with read-oriented
  candidates: `context`, `task list`, and `evidence status`; `status` and `lists`
  may be supporting queries. Evaluate dogfooding, agent independence, and potential
  context or token savings. See [cli-proposal.md](cli-proposal.md) and
  [ai-workflow-proposal.md](ai-workflow-proposal.md).
- **Human/AI workflow experiments.** After read-only CLI dogfooding, evaluate a
  narrow Current Work CLI experiment, a DevScope Skill experiment, and a possible
  Handoff / Notes experiment. Reassess whether an Agent adapter is useful only after
  the CLI/Skill workflow. See [ai-workflow-proposal.md](ai-workflow-proposal.md).
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
