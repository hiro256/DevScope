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

When the selected Task matches the parent Task recorded in Current Work, the Task Detail Pane may include the recorded Current Work breakdown. Current Work remains subordinate to Plan and is shown as working context, not as Evidence or proof of completion.

The Tasks list may mark the Task referenced by Current Work with a lightweight `[Work]` indicator. The indicator denotes recorded Current Work association only; it does not imply active execution, completion, priority, or Evidence.

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
passive right-side Detail Pane beside the navigation panels. It follows the focused panel and
selection, can be hidden without changing project state, and carries richer context while left
panels favor selection and concise state. Full Detail remains the explicitly opened deeper
inspection mode.
Detail Pane availability is based on minimum usable navigation and detail widths rather than a
single wide-screen cutoff.
Task Detail surfaces source-grounded task context rather than generated interpretation. It may show
the selected task text, source path, section, and nearby Markdown context. It intentionally exposes
source quality rather than generating interpretation; improve ambiguous task text at the source or
workflow level when appropriate. Markdown checkbox syntax is broader than accepted Plan semantics,
and source classification remains an open design question.
Evidence Detail Pane surfaces the selected observed Build/Test state and its available execution
details. Freshness is shown separately from the underlying outcome.

## Detail View experiment closure

The Detail View experiment validated a passive right-side Detail Pane that follows the focused
panel and selection. Left panels favor navigation and concise state; the Detail Pane carries richer
context, can be toggled with `p`, and may be hidden responsively. This remained usable beside Codex
at half-screen width, with the toggle serving as an effective escape hatch.

Full Detail is an explicitly opened deeper inspection mode. It is opened with Enter only where a
concrete need exists, supports deeper inspection and scrolling, and does not need to exist for every
panel.

The current panel roles are:

```text
Tasks          Detail Pane; no Full Detail
Evidence       Detail Pane; no Full Detail
Changed Files  Detail Pane and Full Detail
Recent Commits overview-only
```

The Evidence left panel demonstrates the validated pattern: it is a Build/Test selector with concise
status, while the Detail Pane presents the selected state. Detail Pane availability follows usable
width; narrow layouts hide it, and resizing preserves the user's `p` toggle state.

The interaction model is considered validated, but the exact contents remain provisional. Task
Detail currently shows task text, source path, section, and Markdown context. Evidence Detail
currently shows Status, Freshness, Command, Duration, and Result or Error. Changed File Detail
currently shows status, change counts, and diff content, with Full Detail for scrolling. These are
current useful contents, not final contracts: future observation sources and workflows may change
them. Recent Commits remains overview-only until a concrete selection or drill-down need appears.

Future Detail Pane work, if justified, may refine content density, examine a Recent Commits detail,
or consider syntax highlighting and colors. It does not imply Detail Pane focus, scrolling, or Full
Detail for Tasks or Evidence.
## Instrumented verification experiment

Build/Test verification can be invoked through DevScope itself by the TUI or CLI. Verification
becomes agent-neutral Observed Evidence when DevScope runs the command and observes its result;
DevScope does not need to identify the human or agent that initiated it. Prefer explicit,
instrumented verification over passive process inference.

The initial CLI experiment persists its minimal Build/Test state locally so CLI-triggered results
can be restored in a later TUI session and compared against the existing freshness inputs. This is
not a generic Evidence persistence format or a history feature.

Build/Test freshness is observational, not proof that no transient input change occurred during a
verification. Both CLI and TUI capture relevant inputs at verification start and compare them with
the inputs observed at completion. A difference makes the result Stale; baseline capture or
comparison failures are also conservative and do not claim Freshness.

The TUI may additionally mark a run Stale when it directly observes a relevant input change while
the command is running, even if the inputs later return to their start state. The CLI intentionally
uses only the shared start/end comparison and does not add a watcher or polling loop. Therefore a
Fresh result means DevScope observed no contradiction between that verification path and the
relevant input state, not that no transient change is mathematically ruled out. Passed or Failed
remains independent from Freshness. Fresh results retain their start baseline for later comparison;
Stale results remain Stale through persistence and reload.

### Verification integration closure

The verification integration experiment is validated. TUI `b` / `t` and CLI `devscope verify build` / `devscope verify test` invoke the same Build/Test runner. When DevScope runs a verification command and directly observes its process result, the result is Observed Evidence. This is agent-neutral: a human and an AI use the same path, and initiator identity is not part of the contract.

The latest Build/Test result is persisted as local-only current state and can be restored in a later TUI session. This is intentionally not Evidence history, a generic Evidence store, or a CI result database. Passed or Failed is independent from Fresh or Stale: every outcome/freshness combination is possible. Fresh means the observed start and end relevant inputs do not contradict the result; Stale means they differ, the TUI observed a relevant live change during the run, or baseline capture/comparison was unavailable. The CLI uses only start/end comparison, so Fresh does not prove that no transient change occurred. Failures to establish a comparison conservatively remain Stale.

AI workflow guidance now prefers `devscope verify build` and `devscope verify test`; people may use the same commands. If DevScope verification is unavailable or cannot start, project-native verification remains available. A normally completed failed test is itself Observed Evidence and is not a reason to automatically rerun Cargo outside DevScope.

The experiment intentionally excludes automatic verification after file changes, OS process monitoring, terminal interception, Codex process detection, initiator tracking, generic Evidence APIs or persisted schemas, Evidence history, and CI integration. Before this integration, Build/Test Evidence depended on manually pressing `b` or `t` in a running TUI. It can now be generated through an explicit CLI or TUI path and remain visible as persisted Observed Evidence in later TUI sessions.

## Artifact Evidence experiment

Artifact Evidence directly observes one project-relative filesystem path. Its first slice distinguishes `Exists`, `Missing`, and observation failure with optional descriptive metadata, without persistence, freshness, validation rules, configuration, TUI integration, or generic Evidence abstractions. It is a second concrete Observed Evidence source to compare before stabilizing shared APIs.

Build/Test Evidence observes process execution, reports Passed or Failed, and has meaningful Fresh/Stale semantics. Artifact Evidence observes filesystem state and reports Exists or Missing; freshness is not yet defined. This first slice keeps target selection explicit at the CLI and does not imply that mtime or size proves validity, verification, or recency.
Artifact paths are project-relative both lexically and physically. DevScope rejects paths that resolve outside the project root through symlinks, junctions, or similar filesystem indirection. This is an observation boundary, not a general filesystem sandbox or a TOCTOU-proof security mechanism.