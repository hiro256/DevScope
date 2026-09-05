# Build/Test Evidence Design

## Purpose

Evidence is information that supports verification of observed work or state.
v0.3.0 begins with DevScope-observed Cargo Build/Test process results, but process
execution is the first concrete Evidence shape, not the permanent definition of all
Evidence.

Future Evidence may include Build/Test process results, artifact or file
observations, HTTP or other machine checks, imported verification results from
external systems, or human review and approval attestations. These possibilities do
not expand the v0.3.0 implementation scope.

## Initial Cargo Build/Test execution direction


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

Evidence Core remains tool-neutral in concept. An Evidence Source is a conceptual
boundary between a concrete observation mechanism and the information it presents to
Evidence Core. It is not yet a stable trait, generic result type, extension contract,
plugin API, or provider registry.

Cargo Build/Test Evidence is the first concrete source. DevScope can dogfood fixed,
safe commands such as `cargo check` for Build and `cargo test` for Test. Exact
command behavior remains for later implementation rounds. Cargo is a v0.3.0 scope
decision, not a requirement that every future Evidence source resemble a process.

For the initial v0.3 process-source boundary, a concrete Build/Test source produces a
`BuildTestCommandSpec` for a process runner:

```text
Concrete Build/Test source
        ↓
BuildTestCommandSpec
        ↓
process runner
        ↓
BuildTestResult / BuildTestExecutionError
```

`BuildTestCommandSpec` holds the Build/Test kind, source label, command label,
program, argument vector, and working directory. It is a provisional v0.3
process-source boundary, not the stable generic Evidence extension contract.

The command label is human-readable presentation only. The program plus argument
vector are the executable process representation, and DevScope must not parse a
command label to reconstruct them.

For Cargo, structured process invocation such as `Command::new(...).args(...)` is
the intended direction. The Cargo execution path must not use arbitrary shell strings
through `cmd /C`, `powershell -Command`, or `sh -c`. This keeps shell injection and
platform-specific quoting out of the implementation.

Process observation makes command labels, exit codes, duration, stdout/stderr,
spawn errors, and Running state important for Cargo Build/Test Evidence. Those are
source-specific concerns, not fields that every future Evidence source must provide.
Artifact Evidence, for example, would care more about a path, existence, size, and
modified time.

After Cargo Evidence works, DevScope should dogfood it and then run a small, materially
different Artifact Evidence experiment. The intended sequence is:

```text
Cargo Build/Test Evidence
        ↓
dogfood
        ↓
Artifact Evidence experiment
        ↓
compare the two source shapes
        ↓
extract genuinely shared concepts
        ↓
consider a stable Evidence Source extension contract
```

Artifact Evidence is not part of v0.3.0 implementation. It is a future experiment
for expected files such as `reports/final-report.pdf`, where filesystem observation
can report existence, size, and modified time. This comparison avoids both Build/Test
overfitting and speculative generic abstraction.

Observed, Imported, and Attested are possible future provenance vocabulary. They are
not a committed data model in this round.

## v0.3 Build/Test Evidence model

The initial Rust model is intentionally limited to process-based Build/Test
verification. It is not a generic Evidence contract for future source shapes.

```text
BuildTestState
├─ Unavailable
├─ NotRun
├─ Running(BuildTestRun)
├─ Completed(BuildTestResult)
│    ├─ Outcome: Passed / Failed
│    └─ Freshness: Fresh / Stale
└─ ExecutionError(BuildTestExecutionError)
```

`BuildTestKind` distinguishes Build and Test. `BuildTestOutcome` records Passed or
Failed after a verification process completes. `BuildTestFreshness` is separate from
outcome, so a stale result retains whether the most recent completed process Passed
or Failed.

`BuildTestState::status()` derives a display-oriented status:

- Unavailable maps to Unavailable.
- NotRun maps to NotRun.
- Running maps to Running.
- Completed Fresh Passed maps to Passed.
- Completed Fresh Failed maps to Failed.
- Completed Stale maps to Stale while preserving its outcome.
- ExecutionError maps to ExecutionError.

`BuildTestRun` stores the kind, source label, and command label for an in-progress
process. It does not store a child handle, PID, thread, channel, or output reader.

A completed `BuildTestResult` stores kind, outcome, freshness, source label, command
label, optional exit code, `std::time::Duration`, summary, and an optional bounded
diagnostic tail. These fields are specific to the v0.3 process model; Artifact
Evidence will not be required to expose a command, exit code, duration, or diagnostic
output.

An exit code remains optional because a process outcome cannot always provide one.
`BuildTestExecutionError` instead records kind, source label, command label, and a
message when a verification process cannot be started or observed. A non-zero test
process is Failed; a missing executable or spawn failure is an ExecutionError.

`BuildTestDiagnostic` retains at most 4096 Unicode scalar values. When input exceeds
that limit, it preserves the tail because final errors and summaries are commonly the
most useful diagnostic details. Truncation is character-based, not byte-index-based,
so UTF-8 text such as Japanese remains valid.

## Build/Test freshness and project changes

A passed Cargo Build/Test result is not a permanent claim about the current project.
After relevant project state changes, the prior result becomes **Stale**, including
when the Git HEAD is unchanged but a dirty working tree has changed.

The initial Cargo freshness direction is conservative:

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

Staleness semantics can differ by source shape. Cargo Evidence may become stale after
relevant project changes, while Artifact Evidence may become stale when an expected
file changes, disappears, or is replaced. This round does not define a generic stale
API.

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
- A final generic Evidence extension API.
- Artifact Evidence implementation.
- A committed Evidence provenance taxonomy.
- Premature abstraction across hypothetical Evidence sources.

## Open questions

- Decide whether Build and Test are separate runs.
- Define the exact Build/Test stale fingerprint and relevant-path policy.
- Choose manual key bindings.
- Identify which concepts are genuinely shared between Cargo and Artifact Evidence.
