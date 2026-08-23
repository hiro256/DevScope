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

The MVP starts with Markdown as the Plan source, Git as the Activity source, and a
TUI that presents both. Build/Test evidence and agent integrations are deliberately
outside the initial implementation.

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
