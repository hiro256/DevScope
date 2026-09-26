# DevScope

DevScope is a project-centric terminal user interface (TUI) for observing progress
in AI-assisted software development. It derives progress from observable project
state rather than an agent's self-reported state.

```text
Markdown     = Plan
Current Work = Recorded working context; explicit Active drives NOW
Git          = Activity
Build/Test   = Evidence: outcome and separate Fresh/Stale status
Artifact     = Evidence: Exists / Missing / observation error
```

Activity is not Evidence, Current Work is not Plan, and agent self-report is not
verification. Fresh means observed relevant inputs do not contradict a result, not a
quality guarantee. Artifact existence does not prove validity and has no Fresh/Stale
interpretation. Agent integrations remain optional future adapters, not core dependencies.

This README describes the current `main` implementation. Published binaries may lag
behind it; consult the package's release notes for its features and controls.

## Current capabilities

- Current Work with explicit Active state, NOW presentation, and compact history
- Focused-panel Preview and Task Detail support for source-grounded context
- Configurable Build/Test commands and project-specific freshness exclusions
- Artifact Evidence alongside Build/Test Evidence
- Background Git worktree observation with safe Activity exclusion proposals and an approval-gated workflow
- Changed File Full View with Git diff or explicitly labeled safe current text
- Read-only File Browser with passive Preview and cached-file Full View
- Responsive layouts, wrapped inspection content, and contextual action hints

## Quick start: Windows binary

1. Open [GitHub Releases](https://github.com/hiro256/DevScope/releases) and download a
   Windows x64 ZIP from a release that provides one.
2. Extract it to a stable directory, for example `C:\Tools\DevScope`.
3. Optionally add that directory to your user PATH, then open a new shell.
4. From the project root you want to observe, run:

   ```powershell
   cd C:\path\to\your-project
   devscope
   ```

5. For AI or automation orientation, run `devscope context` from the same root.

Without PATH setup, use `& 'C:\Tools\DevScope\devscope.exe'` and append `context` for
CLI orientation. No Config file is required to start. If no suitable binary is
published, use [Build from source](#build-from-source).
See the [Setup Guide](docs/guides/devscope-setup.md) for project-specific details.

## AI-assisted workflow

Use the existing [DevScope Skill](.agents/skills/devscope/SKILL.md):

1. Run `devscope context`.
2. Identify the relevant Plan and Current Work; use `devscope task list` or
   `devscope work list` only when needed.
3. Read only needed detail; set or update explicit Current Work when applicable and authorized.
4. Implement a small step.
5. When relevant, run `devscope verify build` and `devscope verify test`.
6. Before stopping, inspect `devscope context` and `git status --short`.

Current Work completion does not complete its parent Plan task. Read `work list`
immediately before numbered `work active` or `work done` operations; Active is explicit,
never inferred from Next. The CLI works without an agent or Skill.

## Controls and inspection

### Overview

- Left/Right moves panel focus; Tab/Shift+Tab remains compatible. Up/Down or j/k
  moves local selection. `▌` marks focus; `>` marks selection.
- Enter or p toggles passive Preview; Ctrl+Up/Down scrolls it when visible.
- Space runs/re-runs the selected runnable Build/Test only with Evidence focused
  and Preview actually visible. Hidden/narrow-layout Preview, Artifact, Unavailable,
  or any active verification prevents execution. Global b/t execution shortcuts
  are removed; CLI verification is unchanged.
- Ctrl+Enter opens Full View for a selected Changed File. Ctrl+F opens File Browser.
- r reloads project state; q or Esc quits from Overview.

Preview shows source-grounded Task context, separate Evidence outcome/freshness, or
Changed File inspection. Git diff is preferred; unavailable diff may fall back to
safe current UTF-8 text explicitly labeled **File content**, never presented as a diff.
Contextual action hints stay fixed near Preview, separate from the global footer.

### File Browser and Full View

File Browser starts at project root and retains the last directory within the session.
Up/Down or j/k selects, Right/Enter enters a directory, Left goes to the parent, and
Enter on a file does nothing. r refreshes Browser listing and selected content only;
Esc returns to Overview; q quits.

Selection shows passive text Preview with Ctrl+Up/Down scrolling. Ctrl+Enter opens
Full View only for a readable cached file. Large/Medium uses a 40/60 Files/Preview
split; Small shows only the list. Listing is bounded and on demand; symlink/reparse
entries are not followed. No file editing or external editor launch is provided.

In either Full View, Up/Down or j/k scrolls, plain Enter/Esc returns to its originating
view (Overview or Browser) preserving selection and Preview state, and q quits.
Ctrl+Enter does not close Full View. Long inspection lines wrap; safe text reads are
bounded to 64 KiB with explicit truncation and unsupported-content reasons.

## CLI reference (main)

The read-only CLI, Current Work, and Skill workflow experiments are complete. Commands
are available for daily use; plain-text output is not a versioned JSON/API contract.
Use `devscope --help` for the current command list.

```powershell
devscope context
devscope task list
devscope work list
devscope activity suggest-excludes
devscope verify build
devscope verify test
devscope artifact inspect
```

`context`, `task list`, and `work list` print compact plain text without entering the
TUI. `context` includes Build/Test availability and latest saved outcome with current
Fresh/Stale status or Not run. Verification runs the resolved command and persists the
latest observed result locally for later CLI/TUI use; this is not Evidence history.
Cargo defaults are `cargo check` for Build and `cargo test` for Test. Other toolchains
can use configured commands; see [verification setup](docs/guides/devscope-setup.md#buildtest-verification).

`activity suggest-excludes` runs a read-only diagnostic and reports up to three Git-safe
Activity exclusion proposals; it never edits Config. The Skill permits an Activity
exclusion edit only after explicit human approval of an exact reported path.

## Plan sources

By default, Plan and Tasks discover checkboxes across project Markdown files.
Projects that need a precise set of accepted Plan sources can opt in with literal,
project-relative paths in `.devscope/config.toml`:

```toml
[plan]
include = ["docs/roadmap.md", "docs/plans"]
```

A file selects that Markdown file; a directory selects its subtree. Existing
`[plan].exclude` and built-in exclusions still apply. Omitting `include` keeps
broad discovery; `include = []` explicitly selects no Plan sources. Invalid or
missing include paths are Config errors, not a fallback to broad discovery.

## Artifact target

One optional target may be declared in `.devscope/config.toml`:

```toml
[artifact]
path = "target/debug/devscope.exe"
```

`devscope artifact inspect` observes the configured target; `devscope artifact inspect <path>` overrides it. Configuration defines what DevScope should observe; the observation result is Evidence.

When configured, Artifact is selectable in the TUI Evidence panel; it is re-observed
at startup/refresh and has no persisted history or Fresh/Stale state.

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

## Requirements

- Git must be available on `PATH` for Git Activity collection.
- A Rust toolchain is required to build from source.
- Build/Test Evidence needs either project-configured executable commands or, for a Cargo root, Cargo on `PATH`.
- Windows is the primary verified platform for v0.4.0.

## Build from source

From the DevScope repository in PowerShell:

```powershell
cargo build
cargo run
```

For development CLI checks use `cargo run -- context`. For repeated AI use, prefer
an installed or built executable so Cargo output does not mix with CLI output.

To build an optimized binary:

```powershell
cargo build --release
.\target\release\devscope.exe
```

## Windows x64 binary

The v0.4.0 package target is Windows x64; consult [GitHub Releases](https://github.com/hiro256/DevScope/releases)
for actual asset availability and release-specific features. When provided, download the
`devscope-v0.4.0-windows-x64.zip` archive, extract it to a directory of your choice, then run
`devscope.exe` from a project root. Add that directory to `PATH` for commands such as `devscope
context`, or invoke the executable by its absolute path. Run `devscope` with no arguments to start
the TUI.

## Not yet implemented

- Build Debug/Release profiles; dedicated Full View for Evidence and Evidence history
- Agent adapters, including a Codex adapter
- IDE or Web/API frontends
- Package-manager distribution

Task editing and Git/file write operations are outside the current observation-focused
scope, not promised onboarding features. Evidence Preview already scrolls; it is not a
dedicated full-screen diagnostics or history view.

## Further reading

- [Setup Guide](docs/guides/devscope-setup.md): installation and observation policy
- [DevScope Skill](.agents/skills/devscope/SKILL.md): daily AI/human workflow
- [Design](docs/design.md): state meanings and interaction boundaries
- [Roadmap](docs/roadmap.md): accepted work
- [Backlog](docs/backlog.md): candidates, not implementation commitments
