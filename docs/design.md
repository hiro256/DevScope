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

Git worktree change detection is a heuristic for deciding whether Git Activity may need recollection, not a generic filesystem watcher. It remains separate from Markdown, Config, Current Work, Build/Test Freshness, Artifact, and Git metadata observation. Its future candidate set should use Git ignore semantics rather than language-specific output-directory names.

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
Current Work changes update independently from Plan and Activity observation. Active is explicit Recorded state within Current Work: it is never inferred from checklist order. `first_incomplete` remains a Next candidate only. Completing the active item clears Active without selecting another item. NOW is the TUI representation of explicit Current Work Active state, not an inference from Next or checklist order. Thick borders denote the focused panel; `>` denotes only a selected item; `[Work]` denotes Current Work association; `● NOW` denotes explicit Active state; and `│` in Task context denotes the source task line, not selection.

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

Navigation uses Tab / Shift+Tab for panel focus and j/k for selection within the focused panel.
The Preview is a passive common detail area that follows that focus and selection; it is not a
focus target. `p` toggles Preview visibility only when the responsive layout can show it. Enter
opens deeper detail only for an item that provides it. Esc returns from Full Detail; on the overview
it retains the existing quit behavior. `q` quits in either state.

Status markers are source-state cues, separate from focus, selection, Current Work association, and
NOW: `✓` success, `!` attention or observation error, `✕` failed verification, `▶` running, `·`
neutral state, and `?` unavailable. They retain source-specific semantics: Stale keeps its Build/Test
outcome, and Artifact Missing remains a successful observation of an absent target rather than failure.
`|` separates multiple source summaries and carries no status meaning.

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

Broken filesystem indirection is treated as observation failure rather than Missing.

The first registration experiment uses one optional project-configured Artifact target. `devscope artifact inspect` observes it, while an explicit path overrides configuration. Configuration defines the observation target and is not Evidence itself. Multiple targets, names, labels, and generic Evidence configuration remain deferred until a concrete need appears.
When an Artifact target is configured, the Evidence panel includes it as a selectable third observed source. Its Detail Pane shows the recorded path and current observation status, with kind and size for an existing target or an error message when observation fails. Artifact Evidence has no Fresh/Stale interpretation, persistence, or dedicated panel.

The TUI observes the configured target on startup and when project state is refreshed. It remains a passive filesystem observation: no watch service, automatic verification, or inference about artifact validity is introduced.

An invalid project configuration is not treated as an unconfigured Artifact target. Configuration failures use the existing TUI startup or refresh error path, while Artifact `Error` remains reserved for failures during filesystem observation.

### Artifact Evidence experiment closure

The Artifact Evidence experiment is complete. Artifact is a second concrete Observed
Evidence source whose final shape is deliberately small:

```text
source              filesystem observation
target              one optional project-configured path
status              Exists / Missing / Observation error
detail              path / kind / size / error
freshness           none
persistence         none
observation timing  CLI explicit inspect / TUI startup / TUI refresh
TUI                 Evidence panel third selectable source / Detail Pane
```

Configuration answers what DevScope should observe; an Artifact observation is the
Evidence. `Missing` means that observation succeeded and the target is absent. Broken
filesystem indirection, an unsafe resolved path, and filesystem observation failures
are observation errors rather than Missing. A Config error is distinct from an
Artifact Error: it fails before observation begins.

Artifact paths are project-relative both lexically and physically. DevScope rejects a
symlink, junction, or other resolved path that escapes the project root. This is an
observation boundary, not a general filesystem sandbox or a TOCTOU-proof security
guarantee.

Build/Test and Artifact are both Observed Evidence and both have a label, summary
status, observation source, and Detail Pane view. Their important source-specific
differences remain explicit:

```text
Build/Test  source: process execution; status: Passed / Failed
            detail: command / duration / result / error
            freshness: Fresh / Stale; persistence: latest result persisted

Artifact    source: filesystem state; status: Exists / Missing / Error
            detail: path / kind / size / error
            freshness: not defined; persistence: none
```

Fresh/Stale, command, duration, size, kind, persistence, and error semantics are not
common concepts. Two concrete sources are still not enough to justify a generic
Evidence API: their shared surface is thin, their differences are substantial, and a
shared abstraction could force unrelated meanings into one model. Accordingly, this
experiment does not stabilize `trait EvidenceSource`, `enum Evidence`,
`GenericEvidenceStatus`, or `GenericEvidenceStore`. Reassess a generic API after the
Progress history experiment or when a third concrete Evidence source creates a clear
need.

The Evidence panel plus Detail Pane was sufficient; no dedicated Artifact panel was
needed. One optional configured target was also sufficient. Multiple targets, names,
labels, IDs, and globs remain deferred. Artifact freshness is not defined: mtime and
size do not prove validity or recency. Observation is not persisted; DevScope
re-observes current filesystem state at startup or refresh without a filesystem
watcher. Artifact history remains outside this experiment and separate from the
Progress history experiment.

Future validation may explore a hash, content validation, expected shape, or a
generated-by relation when a concrete workflow requires it. None is a current
roadmap commitment.

### Background worktree observation

Potentially expensive Git worktree change detection runs in a dedicated background worker rather than the TUI input and render path. Its result remains only a hint to request Git Activity recollection; it is not Evidence or an authoritative filesystem state. Git metadata detection remains separate and local to the event loop.
