# Design

DevScope observes project progress from information that can be inspected in the
project itself. It does not treat an AI agent's self-reported status as the source of
truth.

```text
Markdown   = Intent / Plan
Git        = Activity
Build/Test = Evidence
Agent      = Current activity
```

Markdown and Git are implemented as the Plan and Activity sources, with a TUI that
presents both. Build/Test Evidence is the next source under development. Agent
integrations remain optional future adapters.

## Target architecture

```text
                 TUI
                  │
            Progress Core
                  │
     ┌────────────┼────────────┐
     │            │            │
 Markdown       Git        Build/Test
     │
     └────────────── Agent adapters
                     ├─ Codex
                     ├─ Claude Code
                     └─ others
```

The TUI and Progress Core must remain separate. The core should own project analysis
and produce UI-independent data, allowing future consumers such as a TUI, VS Code
extension, Web UI, or JSON/API to use the same progress model.

Agent integrations are adapters. They may enrich the observed state, but must not
become a dependency of the core model.
## TUI interaction model

The TUI supports human-oriented project understanding, while the CLI remains the
precise interface for AI and automation. A focused panel identifies the visible
interaction surface that receives navigation; selection identifies an item within that
panel. Project Progress remains an overview rather than a focus target. A future Detail
View may provide optional human drill-down.

This follows lazygit's separation of focus and local selection without copying its UI:
DevScope remains observation- and understanding-centered, and avoids accumulating
context-dependent shortcuts.

Visual hierarchy should prefer spacing, borders, and restrained progress marks before decorative
color or additional widgets. Hidden panels must not retain interactive focus.

The initial panel-focus experiment validated Tasks, Evidence, and Changed Files as a small focus cycle.
Hidden panels are excluded from navigation, and panel-local selection persists independently.

Current Work is Recorded state and may appear in the human overview without becoming Plan or Evidence.
Current Work changes update independently from Plan and Activity observation.

The Current Work TUI experiment validated overview-only Work progress as sufficient for
the current workflow. A dedicated Current Work panel remains deferred until a concrete
drill-down need appears.

The primary TUI screen should remain stable and compact. New information should prefer
optional drill-down over additional permanent overview panels.
Detail View is optional drill-down from an existing panel. The first experiment uses
whole-screen replacement to validate the interaction before introducing a split-pane
layout; a later split-pane implementation should preserve the same target and
interaction model. Enter opens the selected detail, Esc returns when detail is open,
and q always quits.
Changed File Detail may surface observed change magnitude in addition to path and status.
Change magnitude remains Activity data, not Evidence.
Changed Files may also surface observed change magnitude for quick comparison across files.
Change counts are secondary to path and status and may be omitted in narrow layouts.
Changed File Detail uses optional on-demand Activity drill-down for file content changes.
The overview remains compact; diff content belongs in detail rather than the Changed Files list.
The primary overview keeps Project Progress full-width. Large and Medium layouts may expose a
passive global Preview surface beside the navigation panels. Preview content follows the focused
panel and its selection, and can be hidden without changing project state. Whole-screen Detail
remains the deeper inspection mode.
