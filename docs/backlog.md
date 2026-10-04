# Backlog

The backlog contains implementation candidates and exploratory ideas. Entries here
are not committed roadmap work. An item must be promoted to
[roadmap.md](roadmap.md) before implementation begins.

## Implementation candidates

- **Detail inspection refinement.** Revisit Task Detail and Artifact Evidence Detail
  within the compact Preview / explicit deeper-inspection model. Evaluate useful
  source-grounded fields and context, without a generic Detail framework merely for
  symmetry. Build/Test process-output inspection remains the separate candidate below.
- **Evidence output follow-up.** If retained output still makes failures hard to locate,
  explore failure extraction, noise reduction, or output navigation using concrete
  dogfood cases. Current compact Preview and snapshot Full Detail are implemented;
  this candidate does not authorize implementation or Evidence history.
- **File Browser rich Preview.** Evaluate improved Markdown readability and minimal
  source-code syntax/color emphasis separately, so neither forces the other. Preserve
  read-only, bounded cached content; this is not an editor or full IDE and does not
  commit to a large syntax-highlighting framework.
- **Live update visual feedback.** Explore feedback that helps users recognize what
  changed: refine transient emphasis, consider Project Progress reacting to relevant
  underlying updates, and reconsider time-phase progression and coherent accent-color
  use. Preserve the distinct semantics of Tasks, Evidence, Changed Files, Recent Commits,
  and Project Progress; do not prescribe an animation implementation yet.
- **Theme color customization.** Evaluate one configurable accent/theme color with
  existing semantic styles derived consistently where appropriate. Consider it together
  with transient update feedback and Preview syntax emphasis, not full theme packs,
  arbitrary per-widget colors, or extensive styling configuration.
- **Backlog inspection.** Explore read-only inspection/navigation of uncommitted
  candidates through a TUI view or CLI listing (for example, `devscope backlog list`;
  syntax and shortcuts remain undecided). Backlog is distinct from Plan, Tasks, Current
  Work, and Evidence: viewing a candidate never makes it accepted Plan or Roadmap work.
  Defer adding, editing, deleting, promotion, automatic Plan creation, and automatic
  Current Work startup. A CLI surface could complement accepted-work orientation through
  `devscope context` with future-candidate inspection, without requiring every Skill
  request to read Backlog. Evaluate presentation later, including an on-demand view;
  do not commit to a permanent Overview panel. Keep this read-only candidate separate
  from the writable CLI workflows below.
- **Agent-neutral DevScope CLI.** The minimal read-only `context` and `task list`
  experiment completed successfully. Explore narrowly scoped writes such as `task add`,
  Current Work creation, and structural updates where commands remove direct-file-edit
  workarounds. Preserve Plan / Current Work separation and explicit authority: command
  availability alone never authorizes a write. Further read-oriented commands, filters,
  JSON, and Evidence state commands remain exploratory candidates. See
  [cli-proposal.md](cli-proposal.md) and [ai-workflow-proposal.md](ai-workflow-proposal.md).
- **Skill-assisted DevScope setup and maintenance.** Explore detecting usable existing
  repository setup, helping initialize supported repo-local state, and organizing
  `.devscope/config.toml` to reduce manual setup friction. This concerns setup workflow,
  not CLI command design. Preserve Skill authority: no silent introduction into unrelated
  repositories, project-policy changes, exclusion broadening, or unauthorized Config
  rewrites. Policy-changing edits remain explicitly approval-gated.
- **Project-root discovery on startup.** Define semantics before exploring discovery
  when launched from a subdirectory: current directory versus Git repository root,
  Config/state location, non-Git projects, nested repositories/worktrees, and how an
  explicitly selected root overrides discovery. Precedence remains undecided.
- **Task history.** Explore read-only Task/Plan history, distinct from Current Work
  history, Evidence history, and Git Recent Commits. First define useful events,
  completion/uncompletion, text edits and path movement, and whether history is observed,
  derived, or explicitly recorded; do not define storage yet.
- **External tool launching.** Explore a configured external command receiving the
  selected file or directory as context. Execution introduces a write/action boundary
  unlike current observation-focused surfaces; define authority and safety before
  implementation. Do not start with tool-specific integrations, plugins, or an arbitrary
  shell-automation framework.
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
- **Current Work dedicated panel.** Reconsider only when overview-only Work progress
  is insufficient for a concrete drill-down need.

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
