# Backlog

The backlog contains implementation candidates and exploratory ideas. Entries here
are not committed roadmap work. An item must be promoted to
[roadmap.md](roadmap.md) before implementation begins.

## Implementation candidates

- **Agent-neutral DevScope CLI.** The minimal read-only `context` and `task list`
  experiment completed successfully. Further read-oriented commands, task writes,
  filters, JSON, and Evidence state commands remain exploratory candidates. See
  [cli-proposal.md](cli-proposal.md) and [ai-workflow-proposal.md](ai-workflow-proposal.md).
- **Human/AI workflow experiments.** The Current Work CLI, provider-neutral Skill,
  and Agent integration reassessment completed. Handoff / Notes remains a separate
  later candidate. An adapter is deferred until multi-agent ownership or lifecycle
  ambiguity exposes a concrete unmet question. See
  [agent-integration-reassessment.md](agent-integration-reassessment.md).
- **Pre-generated translated Markdown.** Explore English source documents with
  derived Japanese Markdown. Evaluate missing and stale detection, exclusion from
  Plan discovery, and AI/provider-independent synchronization. See
  [translation-proposal.md](translation-proposal.md).
- **Task weighting.** Reconsider only if equal-weight Markdown task counting creates
  a concrete progress-reporting problem. The current Plan + Current Work split
  reduces the need for weighting.
- **TUI visual polish.** Explore spacing, footer density, responsive balance, subtle
  focus refinement, and symbols or typography without adding permanent overview
  panels, a theme system, or decorative dashboard widgets.
- **Current Work dedicated panel.** Reconsider only when overview-only Work progress
  is insufficient for a concrete drill-down need.
- **Plan source / task discovery experiment.** Revisit how DevScope distinguishes
  accepted Plan work from document-local checklists when broad checkbox discovery
  creates concrete workflow ambiguity. See
  [task-discovery-reassessment.md](task-discovery-reassessment.md).

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
