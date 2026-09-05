# Build/Test Evidence Design

## Purpose

Evidence is an observed result from an actual verification process. It is not an
agent statement such as "tests passed" or "build succeeded." A result is evidence
only when DevScope can observe the process it started and normalize its outcome.

Examples include `cargo check` exiting with code 0, `cargo test` exiting with code
0, `pytest` exiting with code 1, or `npm test` exiting with code 1.

## Initial execution direction

The initial direction is DevScope-owned execution:

```text
DevScope
  -> launches a Build/Test process
  -> observes process completion
  -> normalizes the result
  -> updates Evidence Core
  -> presents the result in the TUI
```

This establishes that a command actually ran, provides its exit status, and permits
DevScope to record start time, completion time, and duration. It does not depend on
an AI agent being present or reporting a result.

External log scraping is not the MVP's primary mechanism. Parsing Codex output,
terminal scrollback, arbitrary success strings in logs, or agent session files
cannot reliably establish a current process outcome or exit status. CI and JUnit
results may become future Evidence sources, but are outside the initial source.

## Source boundary and tool neutrality

Evidence Core remains tool-neutral. An Evidence Source launches or observes one
verification mechanism and converts it into a normalized Evidence Result. The Core
must not embed Cargo, Rust, `Cargo.toml`, or a particular command-line tool.

Cargo is the initial concrete source candidate because DevScope is a Rust project.
A Cargo Evidence Source could safely provide fixed commands such as `cargo check`
for Build and `cargo test` for Test. This is a dogfooding choice, not a Core
requirement. Future sources can cover pytest, npm, dotnet test, Gradle, CI/JUnit,
or other tools through the same boundary.

Process execution should use structured invocation such as `Command::new(...).args(...)`.
The initial execution foundation must not use arbitrary shell strings through
`cmd /C`, `powershell -Command`, or `sh -c`. This keeps shell injection and
platform-specific quoting out of Evidence Core.

## Evidence model

Evidence distinguishes at least these kinds:

```text
Build
Test
```

The MVP state model is:

- **Unavailable:** No usable Evidence Source is available.
- **NotRun:** A source is available, but it has not yet been run.
- **Running:** A verification process is in progress.
- **Passed:** The latest completed verification exited successfully and is fresh.
- **Failed:** The verification process ran and reported a failing result.
- **Stale:** A prior completed result no longer corresponds to the observed project state.

A verification failure differs from an execution error. A test command that exits
with a failing status is **Failed**. A missing executable, process-spawn failure, or
inaccessible working directory is an execution error or unavailable source. DevScope
must not present failure to start as a failed test.

A completed result should be able to retain the following structured fields:

- Evidence kind and normalized status.
- Source identity and command label.
- Exit code when available.
- Duration.
- A short summary.
- Bounded diagnostic detail, such as a final output tail.

Raw stdout and stderr must not become unbounded Core data. The MVP keeps a structured
summary and bounded diagnostics rather than retaining full logs indefinitely.

## Freshness and project changes

A passed result is not a permanent claim about the current project. After relevant
project state changes, the prior result becomes **Stale**, including when the Git
HEAD is unchanged but a dirty working tree has changed.

The initial freshness direction is conservative:

```text
verification completes
  -> synchronize the relevant project-observation baseline
  -> mark the result fresh

relevant project change is observed
  -> mark the result stale
```

The exact fingerprint is deferred. It must not be based only on the HEAD commit,
because verification commonly runs in a dirty working tree.

Build and test commands can create `target/`, coverage, caches, or generated files.
Those self-generated artifacts must not make a just-completed result immediately
stale. Completion therefore needs detector-baseline synchronization before Evidence
is marked fresh. The exact relevant-path policy remains an implementation detail.

## Execution behavior

The MVP starts verification only after an explicit user request. It does not run
`cargo test` or another command automatically whenever a project changes. Automatic
run policy belongs to a future configuration feature.

Evidence execution must be non-blocking so the TUI can continue navigation, redraw,
quit handling, and project observation while a process is running. Exact key bindings
are not decided yet; a manual TUI trigger is required.

The initial scope can allow one Evidence run at a time. Cancellation, parallel runs,
queues, interactive test processes, terminal emulation, and a full log viewer are
outside the MVP.

## Application integration direction

`ProjectSnapshot` will eventually contain Plan, Activity, Evidence, and Tasks. This
round does not change `ProjectSnapshot`. Evidence should support independent partial
updates alongside future operations conceptually similar to applying Markdown state
or Activity state.

Project refresh status and Evidence execution status are distinct. Existing refresh
status describes Markdown, Git, and manual snapshot updates. Evidence Running,
Passed, Failed, and Stale describe verification state, so this round does not add an
Evidence refresh source.

Evidence execution remains agent-independent. An agent may request a verification
run in the future, but DevScope's observed process result remains the source of
truth.

## Configuration boundary

A general configuration system is scheduled after Evidence work and must not be
introduced first. If Cargo becomes the initial source, project detection plus fixed,
safe commands can be evaluated without arbitrary command configuration.

## MVP non-goals

- Arbitrary configurable commands.
- Automatic test execution on every project change.
- CI service integration.
- JUnit parsing.
- Agent-specific evidence ingestion.
- Parallel verification.
- Interactive subprocess UI.
- Full log viewer.

## Open questions

- Confirm whether Cargo is the first concrete source.
- Decide whether Build and Test are separate runs.
- Define exact Evidence summary fields.
- Define the exact stale fingerprint and relevant-path policy.
- Choose a bounded diagnostic-output limit.
- Choose manual key bindings.
