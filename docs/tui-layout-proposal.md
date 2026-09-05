# TUI Layout Proposal

## Status

This is an **exploratory proposal**, not a final UI specification. It records a
possible information architecture for future DevScope TUI growth before the
`Evidence summary display` task is implemented.

The following remain intentionally undecided:

- Exact layout, panel sizes, and responsive behavior details.
- Exact key bindings, focus model, and navigation model.
- Current Work and Agent detail presentation.
- The architecture of a generic detail system.

No implementation decision is made by this document.

## Motivation

The current Overview presents Project Progress, Task Summary, Changed Files, and
Recent Commits. Build/Test lifecycle state has joined Project Progress as Evidence.
Future work may add Evidence summaries, Current Work, and Agent activity.

Adding a permanent panel for every new source would turn the Overview from a
quickly readable dashboard into a long vertical stack. That is especially costly in
Medium and Small terminals. The layout should preserve an immediate answer to
"what is progressing?" while leaving detailed inspection available when needed.

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

Information needed only while inspecting a selected item:

- Evidence command, duration, exit code, summary, and diagnostic tail.
- Current Work steps.
- Changed-file details.
- Future Agent details.

This keeps the established distinction intact:

```text
Task Summary  = persistent project plan
Current Work  = temporary breakdown of an active or selected task
Evidence      = observed verification results
```

Current Work must not become a second persistent Plan list, and Evidence must not
be treated as a claim made by Current Work.

## Evidence in the hierarchy

The Project Progress Evidence row remains the overview summary, for example:

```text
Evidence   Build Passed · Test Failed
```

It should remain short and glanceable. Evidence summary and diagnostics are
candidates for a detail region rather than content to append to this row.

## Candidate A: Overview expansion

Add more permanent panels to the existing vertical Overview.

```text
┌ Project Progress ───────────────┐
│ Plan       ...                  │
│ Activity   ...                  │
│ Evidence   ...                  │
│ Agent      ...                  │
├ Task Summary ───────────────────┤
│ ...                             │
├ Evidence Details ───────────────┤
│ ...                             │
├ Changed Files ──────────────────┤
│ ...                             │
├ Recent Commits ─────────────────┤
│ ...                             │
```

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

Retain the Overview and place focused information in one contextual Details region.

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

Possible detail mappings are:

```text
selected Task       -> Current Work
selected Evidence   -> Evidence details
selected file       -> file details
future Agent focus  -> Agent details
```

Advantages:

- Preserves Project Progress as a stable Overview.
- Gives Evidence summary information space without growing its overview row.
- Creates a candidate shared region for Current Work and future inspection.
- Avoids a permanent-panel-per-source pattern.
- Is less complex than a full multi-screen UI.

Disadvantages:

- Requires a future decision about what the Details region follows.
- Focus and selection rules need careful design.
- A compact detail region may still be insufficient for long diagnostics.

This is an information-architecture candidate only. It does not propose a generic
detail-pane framework for v0.3.

## Candidate C: Dedicated screens

Use screen switching, such as Overview, Evidence, Work, and Activity.

Advantages:

- Supports large information volumes.
- Lets each screen use substantial terminal space.
- Is a natural home for Evidence diagnostics and future scrolling.

Disadvantages:

- Adds navigation complexity.
- Hides information that is presently visible at a glance.
- Moves DevScope substantially away from its current simple Overview.
- Is likely excessive for v0.3.

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

This is a future possibility, not a v0.3 implementation commitment.

## Comparison

| Criterion | A: Overview expansion | B: Contextual detail pane | C: Dedicated screens | D: Hybrid later |
| --- | --- | --- | --- | --- |
| Overview clarity | Declines as panels grow | Strong | Strong within Overview | Strong |
| Evidence detail capacity | Limited | Moderate | High | High later |
| Current Work compatibility | Adds another panel | Strong shared context | Strong separate screen | Strong |
| Small terminal behavior | Weakens quickly | Requires priorities | Requires navigation | Deferred complexity |
| Navigation complexity | Low | Moderate | High | Moderate to high |
| Implementation cost | Low initially | Moderate | High | High over time |
| Future extensibility | Moderate | High | High | High |

## Preferred direction

**Candidate B: Contextual detail pane** is the current leading direction, not a
final decision. It keeps Project Progress as a concise Overview, allows Evidence
summary content to move out of the Evidence row, gives Current Work a possible
shared home, and avoids both panel proliferation and the immediate complexity of
multiple screens.

A useful conceptual role split is:

```text
Project Progress = project-wide summary
Task Summary     = persistent Plan navigation
Details          = focused contextual information
Changed Files    = Activity inspection
Recent Commits   = Activity history
```

## Illustrative detail content

The following mockups are visual proposals only. They do not specify exact
terminal dimensions, panel heights, focus behavior, or interaction details.

### Evidence detail example

```text
┌ Project Progress ─────────────────────────────┐
│ Plan       18 / 27 tasks complete             │
│ Activity   3 changed files                    │
│ Evidence   Build Passed · Test Failed         │
│ Agent      Not available                      │
├ Task Summary ─────────────────────────────────┤
│ > □ Evidence summary display                  │
│   □ Evidence becomes stale after changes      │
├ Details: Test ────────────────────────────────┤
│ cargo test                                    │
│ Failed · 3.4s · exit 101                      │
│                                               │
│ error[E0308]: ...                             │
├ Changed Files ────────────────────────────────┤
│ ...                                           │
└───────────────────────────────────────────────┘
```

### Current Work detail example

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

These are priorities rather than a proposed implementation.

- **Large:** Overview, Task Summary, Details, Changed Files, and Recent Commits.
- **Medium:** Overview, Task Summary, Details, and Changed Files; Recent Commits
  may be omitted.
- **Small:** Overview and Task Summary remain essential. Details is a candidate
  only if it can be shown without displacing the primary glanceable state.
- **Compact:** Keep the existing minimum message. Do not add detail content.

The final Large, Medium, Small, and Compact layout rules remain undecided.

## Evidence detail priority

For a future Evidence summary display, detail content can be prioritized as follows.

### Essential

- Build or Test kind.
- State or outcome.
- Command label.
- Duration.

### Useful

- Exit code.
- Summary.

### Expandable / lower priority

- Diagnostic tail.
- Source label.

A diagnostic tail can be up to 4096 characters. It must not be placed into the
Overview. A bounded Details region with clipping is an initial candidate; scrolling
or a dedicated Evidence screen can be considered later. This proposal does not
require a scrolling log viewer in v0.3.

## Open questions

### Focus versus selection

Task Summary already has a selected task. A future Details region may also need a
focused panel. These may be separate concepts: selection identifies an item, while
focus identifies where navigation keys act. No focus system is designed or
implemented by this proposal.

### Evidence selection

Build and Test are independent. The Detail region needs a future rule for which
Evidence it shows. Candidates include:

- Last completed Evidence.
- Last interacted Evidence.
- Explicit Build/Test selection.
- Running Evidence priority.
- Failed Evidence priority.

### Current Work association

Current Work may follow the selected Task Summary item, or it may belong to a
separately explicit active task. The relationship remains undecided.

## Interaction candidates

These are candidates only and do not change existing key bindings:

```text
Tab       focus next panel
Enter     inspect selected item
b / t     run Build/Test
j / k     navigate the currently focused list
Esc       return to overview focus
```

## Keep the Overview glanceable

On launch, DevScope should still make it clear what is planned, what is currently
being worked on, what changed, what was verified, and eventually what an Agent is
doing. Details should add inspection capacity without hiding the project-wide
summary. This supports DevScope as a progress tool for both humans and AI without
requiring agent-specific implementation.

## Non-goals

This proposal does not implement or decide:

- A generic widget or panel framework.
- A full focus manager, tab system, mouse support, or split-pane resizing.
- Scrolling diagnostics or a log viewer.
- Persistent UI-layout configuration.
- Current Work implementation or Agent integration.
- Evidence source abstraction or plugin UI.
- A Web frontend.

## Recommended next discussion

Before implementing a larger TUI change, decide:

1. Whether to adopt the contextual detail-pane direction.
2. Which operation reveals Evidence Details.
3. Which Build/Test result appears in Details.
4. Large, Medium, and Small priorities for a Details region.
5. The precise v0.3 scope of `Evidence summary display`.
6. Whether Current Work should eventually share the same Details region.
