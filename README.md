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

DevScope v0.1.0 implements Markdown-based Plan progress and Git-based Activity.
Evidence and Agent are shown in the overview as future sources and are not yet
implemented.

## v0.1.0 features

- Markdown task discovery across multiple Markdown files
- Markdown checkbox parsing and completed/total progress
- Git repository detection and changed-file count
- Recent Git commits
- Overview TUI and Task Summary
- Keyboard task navigation
- Responsive terminal resize handling

## Controls

```text
Up / k      Previous task
Down / j    Next task
q / Esc     Quit
```

## Requirements

- Git must be available on `PATH` for Git Activity collection.
- A Rust toolchain is required to build from source.
- Windows is the primary verified platform for v0.1.0.

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

- Build/Test Evidence sources
- Agent adapters
- Configuration files
- Automatic reload or file watching
- Task editing and Git write operations

See [docs/roadmap.md](docs/roadmap.md) for the planned work.
