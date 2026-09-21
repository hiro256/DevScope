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
b           Run Build evidence
t           Run Test evidence
r           Manual full reload
q / Esc     Quit
```

## Requirements

- Git must be available on `PATH` for Git Activity collection.
- A Rust toolchain is required to build from source.
- Build/Test Evidence needs either project-configured executable commands or, for a Cargo root, Cargo on `PATH`.
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

## Experimental CLI (main)

The initial read-only and Current Work CLI experiments completed successfully. The
commands remain experimental while the later Skill and workflow work continues.

For project onboarding, use the [Setup Guide](docs/guides/devscope-setup.md). The
repo-local [DevScope Skill](.agents/skills/devscope/SKILL.md) supports the daily
AI/human workflow; the [Skill prototype](docs/examples/devscope-skill.md) remains its
design reference.
For development checks, use Cargo:

```powershell
cargo run
cargo run -- context
cargo run -- task list
cargo run -- work list
cargo run -- work done 3
cargo run -- activity suggest-excludes
cargo run -- --help
```

For AI dogfooding or repeated use, invoke the built executable directly so Cargo build
output is not mixed with compact CLI output:

```powershell
.\target\debug\devscope.exe context
.\target\debug\devscope.exe task list
.\target\debug\devscope.exe work list
.\target\debug\devscope.exe work done 3
.\target\debug\devscope.exe activity suggest-excludes
.\target\debug\devscope.exe verify build
.\target\debug\devscope.exe verify test
.\target\debug\devscope.exe artifact inspect target\debug\devscope.exe
```

If DevScope is already available on `PATH`, `devscope context` and `devscope task list`
are equivalent. `context`, `task list`, and experimental `work list` print compact plain text without entering the
TUI. The CLI reports Build/Test availability resolved from project Config or the Cargo
fallback in `context`. `verify build` and `verify test` run the resolved command, then
persist the observed current result locally for restoration in a later TUI session.
Freshness compares relevant project inputs around the verification run. `activity suggest-excludes` runs an on-demand, read-only worktree diagnostic and reports up to three Git-safe Activity exclusion proposals; it never edits Config. The repo-local Skill permits a separate `[activity].exclude` edit only after explicit human approval of an exact reported path.

## Not yet implemented

- Scrollable, full-screen Evidence diagnostics and history
- Agent adapters, including a Codex adapter
- Task editing and Git write operations
- Task weighting, progress history, and IDE or Web/API frontends

DevScope v0.3.0 is a source-only release; binary packaging is not provided.

See [docs/roadmap.md](docs/roadmap.md) for the planned work.

## Artifact target

One optional target may be declared in `.devscope/config.toml`:

```toml
[artifact]
path = "target/debug/devscope.exe"
```

`devscope artifact inspect` observes the configured target; `devscope artifact inspect <path>` overrides it. Configuration defines what DevScope should observe; the observation result is Evidence.
