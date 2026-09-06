# DevScope

DevScope is a project-centric terminal user interface (TUI) for observing progress
in AI-assisted software development. It derives progress from observable project
state rather than an agent's self-reported state.

```text
Markdown   = Plan
Git        = Activity
Build/Test = Evidence
Agent      = Current activity
```

The latest release is v0.3.0, **Build/Test Evidence**. It extends v0.2.0 Live
Observation with DevScope-observed Cargo verification execution and outcome.

## v0.3.0 features

- Cargo project detection
- Manual `cargo check` Build and `cargo test` Test execution
- Non-blocking process observation and Build/Test lifecycle states
- Evidence Details for commands, outcomes, and bounded diagnostics
- Automatic Fresh/Stale tracking, including relevant input changes while a run is active
- Windows Build/Test Evidence verification

## v0.2.0 features

- Markdown task discovery across multiple Markdown files
- Markdown checkbox parsing and completed / total progress
- Task Summary with keyboard navigation
- Git repository detection and changed-file count
- Changed Files panel with Git status details
- Recent Git commits
- Automatic live refresh for Markdown, Git worktree, and Git metadata changes
- Manual full reload with `r`
- Refresh status and session-relative last-refresh timestamp
- Responsive terminal layout

## Live observation

DevScope polls project state approximately once per second. Markdown changes update
Plan and Task state. Git worktree or Git metadata changes update Activity state.

Change detection is lightweight: unchanged polling does not recollect Git Activity.
Git status and commit data are collected only after a relevant worktree or Git
metadata change is detected.

The status line reports the latest refresh source and timestamp. For example:

```text
Watching · Last refresh: Initial +00:00
Watching · Last refresh: Git +00:15
Retry pending · Last refresh: Markdown +00:20
```

The `+00:15` value is the timestamp relative to the start of the current DevScope
session, not wall-clock time or an "ago" value.

## Changed Files

The readonly Changed Files panel shows the current Git working-tree status:

```text
M  Modified
A  Added
D  Deleted
R  Renamed
```

DevScope observes Git state only; it does not edit files or perform Git write
operations.

## Controls

```text
Up / k      Previous task
Down / j    Next task
b           Run Build evidence (cargo check)
t           Run Test evidence (cargo test)
r           Manual full reload
q / Esc     Quit
```

## Requirements

- Git must be available on `PATH` for Git Activity collection.
- A Rust toolchain is required to build from source.
- Cargo must be available on `PATH` to execute Cargo Build/Test Evidence.
- Windows is the primary verified platform for v0.3.0.

## Build and run

In PowerShell:

```powershell
cargo build
cargo run
```

To build an optimized binary:

```powershell
cargo build --release
.\target\release\devscope.exe
```

## Not yet implemented

- Scrollable, full-screen Evidence diagnostics and history
- Minimal read-only CLI experiment
- Agent adapters, including a Codex adapter
- Configuration files
- Task editing and Git write operations
- Task weighting, progress history, and IDE or Web/API frontends

DevScope v0.3.0 is a source-only release; binary packaging is not provided.

See [docs/roadmap.md](docs/roadmap.md) for the planned work.
