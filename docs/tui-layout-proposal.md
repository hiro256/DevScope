# TUI Layout Proposal

## Status

This document remains an **exploratory proposal** for DevScope's long-term UI. It
is not a permanent final UI specification.

For v0.3, however, **Candidate B: Contextual detail pane** is the selected
implementation direction for the Evidence summary experiment. The result should be
dogfooded before it is treated as a long-term UI decision.

### Selected for the v0.3 experiment

- Keep Project Progress as the project-wide Overview.
- Add one Details region for focused contextual information.
- Use Evidence as the first Details content provider.
- Keep the Details region reusable for future Current Work, changed-file, and Agent
  information rather than defining it as an Evidence-only permanent panel.

### Still undecided

- The permanent final UI architecture.
- A generic detail or panel framework.
- A full focus manager or exact navigation model.
- A full-screen Evidence view.
- Exact Current Work and Agent interactions.
- Mouse support, pane resizing, and diagnostic scrolling.
- Exact panel dimensions and final responsive rules.

No Rust implementation is made by this document.

## Motivation

The current Overview presents Project Progress, Task Summary, Changed Files, and
Recent Commits. Build/Test lifecycle state has joined Project Progress as Evidence.
Future work may add Evidence summaries, Current Work, and Agent activity.

Adding a permanent panel for every source would turn the Overview from a quickly
readable dashboard into a long vertical stack. That is especially costly in Medium
and Small terminals. The layout should preserve an immediate answer to "what is
progressing?" while leaving detailed inspection available when needed.

## Information hierarchy

A useful three-level model is:

```text
Overview
    ↓
Selection / Focus
    ↓
Details
```

### Overview

Always-visible, project-wide state:

- Plan progress.
- Git Activity summary.
- Build/Test Evidence state.
- Agent availability or activity summary.

### Work / selection

The item currently being considered by a human or AI:

- Selected Plan task.
- Current Work for an active or selected task.
- Selected Build or Test Evidence.
- Selected changed file.

### Details

Information needed while inspecting the currently relevant item:

- Evidence command, duration, exit code, summary, diagnostic tail, or error message.
- Current Work steps.
- Changed-file details.
- Future Agent details.

This preserves the established distinction:

```text
Task Summary  = persistent project plan
Current Work  = temporary breakdown of an active or selected task
Evidence      = observed verification results
```

Current Work must not become a second persistent Plan list, and Evidence must not
be treated as a claim made by Current Work.

## Core v0.3 layout direction

The v0.3 direction is conceptually:

```text
Project Progress
Task Summary
Details
Changed Files
Recent Commits
Footer
```

Project Progress remains the Overview and continues to show Plan, Activity,
Evidence, and Agent. The Evidence row remains concise, for example:

```text
Evidence   Build Passed · Test Failed
```

It does not gain a command label, duration, summary, diagnostic, or error message.

Task Summary remains persistent Plan navigation. Evidence Details must not mix
Evidence information into Task Summary.

Details means **focused contextual information**, not an Evidence-specific permanent
panel. v0.3 uses Evidence as its first content provider. Later it may present:

```text
Evidence      -> Evidence details
Current Work  -> active or selected task work breakdown
Changed file  -> file details
Agent         -> Agent detail
```

## Candidate A: Overview expansion

Add more permanent panels to the existing vertical Overview.

Advantages:

- Straightforward and close to the current architecture.
- Comparatively simple to implement.
- Makes an Evidence summary visible immediately.

Disadvantages:

- Consumes vertical space quickly.
- Adds another panel whenever Current Work or Agent detail arrives.
- Makes Medium and Small layouts more constrained.
- Risks changing the Overview from a dashboard into a long stack.

## Candidate B: Contextual detail pane

Retain the Overview and place focused information in one Details region.

```text
┌ Project Progress ───────────────┐
│ Plan       ...                  │
│ Activity   ...                  │
│ Evidence   ...                  │
│ Agent      ...                  │
├ Task Summary ───────────────────┤
│ > □ selected task               │
│   □ another task                │
├ Details ────────────────────────┤
│ context-sensitive content       │
├ Changed Files / Recent Commits ─┤
│ ...                             │
```

For v0.3, this is the **selected direction for the Evidence summary experiment**.
It is not yet the permanent final UI architecture. Dogfooding the result should
inform whether to retain, revise, or replace it later.

Advantages:

- Preserves Project Progress as a stable Overview.
- Gives Evidence detail information space without growing its Overview row.
- Creates a shared location for Current Work and future inspection.
- Avoids a permanent-panel-per-source pattern.
- Is less complex than a full multi-screen UI.

Disadvantages:

- Requires a future rule for what the Details region follows.
- Focus and selection rules need careful design later.
- A compact detail region may not suit long diagnostics.

This remains an information-architecture direction; v0.3 does not introduce a
generic detail-pane framework.

## Candidate C: Dedicated screens

Use screen switching, such as Overview, Evidence, Work, and Activity.

Advantages:

- Supports large information volumes.
- Lets each screen use substantial terminal space.
- Is a future candidate for long diagnostics, scrolling, history, or multiple
  Evidence sources.

Disadvantages:

- Adds navigation complexity.
- Hides information that is currently visible at a glance.
- Moves DevScope substantially away from its current simple Overview.
- Is likely excessive for the v0.3 Evidence summary experiment.

Candidate C remains available for later reconsideration.

## Candidate D: Hybrid future direction

A later hybrid could combine an Overview with a lightweight Details region and an
optional full detail screen:

```text
Overview
  ↓
compact Details
  ↓ Enter
full detail screen
```

This remains a future possibility, not a v0.3 implementation commitment.

## Comparison

| Criterion | A: Overview expansion | B: Contextual detail pane | C: Dedicated screens | D: Hybrid later |
| --- | --- | --- | --- | --- |
| Overview clarity | Declines as panels grow | **Selected for v0.3** | Strong within Overview | Strong |
| Evidence detail capacity | Limited | Moderate | High | High later |
| Current Work compatibility | Adds another panel | Strong shared context | Strong separate screen | Strong |
| Small terminal behavior | Weakens quickly | Experiment required | Requires navigation | Deferred complexity |
| Navigation complexity | Low | Low in v0.3; higher later | High | Moderate to high |
| Implementation cost | Low initially | Moderate | High | High over time |
| Future extensibility | Moderate | High | High | High |

## v0.3 Evidence summary experiment

`Evidence summary display` is now implemented as the v0.3 contextual Details experiment. It establishes:

- One contextual Details region.
- Evidence as the first Details content provider.
- The most recently interacted Build/Test Evidence as the proposed initial selection:
  `b` leads to Build Details and `t` leads to Test Details.
- Command label.
- Lifecycle state or completed outcome.
- Duration when available.
- Exit code when available.
- Summary.
- A bounded visible diagnostic tail.
- The ExecutionError message.

This selection rule is a proposed v0.3 rule, not a generic focus system. The initial
empty state for a Cargo project may be similar to:

```text
Details: Evidence

Build and Test have not been run yet.
Press b to run Build or t to run Test.
```

Exact wording remains an implementation detail.

### Details content and priority

The intended Evidence Details priority, from most to least important when height is
limited, is:

1. Kind or title.
2. Command label.
3. Status or outcome, duration, and exit code.
4. Summary.
5. Diagnostic.

`source_label` is not displayed in v0.3 because Cargo is the only current source.
It can be reconsidered when multiple Evidence sources exist.

Examples of intended states are:

```text
Details: Build

cargo check
Running
```

Running does not require real-time duration updates; it shows only information
already held by the existing state.

```text
Details: Build

cargo check
Passed · 1.8s · exit 0
cargo check passed
```

```text
Details: Test

cargo test
Failed · 3.4s · exit 101
cargo test failed

<diagnostic tail>
```

When an exit code is unavailable, the implementation need not show a placeholder.
Exact formatting may be adjusted during implementation.

For ExecutionError, Project Progress remains concise (`Test Error`) while Details
shows the actual message:

```text
Details: Test

cargo test
Error

failed to spawn cargo: ...
```

v0.3 uses outcome-first stale formatting: `Passed · Stale` and `Failed · Stale`. Details preserves both the original observed outcome and the fact that it is no longer fresh.

### Diagnostic boundary

Diagnostic data remains bounded to the existing 4096-character model limit. The UI
shows only lines that fit in the Details area. Diagnostics are clipped first when
height is scarce. v0.3 does not add vertical diagnostic scrolling or a full log
viewer.

### v0.3 non-goals

The Evidence summary experiment does not add:

- A generic focus manager or Tab navigation.
- Mouse support.
- Diagnostic scrolling or a full Evidence screen.
- Evidence history.
- Multiple simultaneous Details panes.
- Current Work implementation or Agent detail implementation.
- A generic panel framework.

## Illustrative layout

This is a visual proposal, not an exact dimension or Ratatui specification.

```text
┌──────────────────────────────────────────────────────────────┐
│ DevScope                                                     │
│ Watching · Last refresh: Git +00:18                          │
├─ Project Progress ────────────────────────────────────────────┤
│ Plan       18 / 27 tasks complete                            │
│ Activity   3 changed files                                   │
│ Evidence   Build Passed · Test Failed                        │
│ Agent      Not available                                     │
├─ Task Summary ────────────────────────────────────────────────┤
│ > □ Evidence summary display                                 │
│   □ Evidence becomes stale after relevant project changes    │
│   □ Evidence model/source tests                              │
├─ Details: Test ───────────────────────────────────────────────┤
│ cargo test                                                   │
│ Failed · 3.4s · exit 101                                     │
│ cargo test failed                                            │
│                                                              │
│ error[E0308]: mismatched types                               │
├─ Changed Files ───────────────────────────────────────────────┤
│ ...                                                          │
├─ Recent Commits ──────────────────────────────────────────────┤
│ ...                                                          │
└──────────────────────────────────────────────────────────────┘
```

A future Current Work view remains an example of the same shared Details region:

```text
┌ Task Summary ─────────────────────────────────┐
│ > □ Evidence becomes stale after changes      │
├ Details: Current Work ────────────────────────┤
│ [x] Detect relevant project changes           │
│ [x] Preserve previous result                  │
│ [ ] Mark completed Evidence stale             │
│ [ ] Add regression tests                      │
└───────────────────────────────────────────────┘
```

## Responsive priorities

The v0.3 direction is a vertical priority order, not an absolute statement of
semantic importance:

```text
Project Progress
Task Summary
Details
Changed Files
Recent Commits
```

- **Large:** Project Progress, Task Summary, Details, Changed Files, Recent Commits,
  and Footer. Details receives persistent space.
- **Medium:** Project Progress, Task Summary, Details, Changed Files, and Footer.
  Recent Commits may be omitted as it is today.
- **Small:** Project Progress, Task Summary, Details, and Footer is the first
  candidate. This is an implementation experiment: dogfooding may show that Details
  must be omitted to preserve usable Task Summary space.
- **Compact:** Keep the current minimum message. Evidence Details is not shown.

## Open questions

The following are resolved for v0.3: the contextual detail-pane direction is the
experiment direction, and the initial Evidence selection proposal is the most
recently interacted Build/Test run.

The following remain open:

- The long-term Small-layout policy after dogfooding.
- The Task Summary versus Details vertical balance, including adaptive Details sizing or diagnostic-heavy temporary growth.
- The future distinction between focused panel and selected item.
- Whether Current Work follows selected Task Summary or an explicit active task.
- When long diagnostics, history, or multiple sources justify a dedicated Evidence
  screen.

## Future interaction candidates

These are not v0.3 Evidence summary controls, but remain possible later directions:

```text
Tab       focus next panel
Enter     inspect selected item
j / k     navigate the currently focused list
Esc       return to overview focus
```

They do not change existing key bindings.

## Keep the Overview glanceable

On launch, DevScope should still make it clear what is planned, what is currently
being worked on, what changed, what was verified, and eventually what an Agent is
doing. Details should increase inspection capacity without hiding project-wide
summary. This supports DevScope as a progress tool for humans and AI without
requiring agent-specific implementation.

## Recommended next implementation

The next roadmap implementation is `Evidence becomes stale after relevant project changes`. The v0.3 Details implementation uses 6 rows including borders (4 inner rows). Command, status/outcome, and summary take priority; diagnostic uses every remaining inner row and is clipped first. The Details region intentionally consumes vertical space that previously belonged to Task Summary, an intentional v0.3 tradeoff to evaluate during dogfooding.

Future discussion can revisit the Task Summary versus Details balance, adaptive Details sizing, diagnostic-heavy temporary growth, and the long-term Small-layout policy.