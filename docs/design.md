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
