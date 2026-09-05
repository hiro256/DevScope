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

Cargo Build/Test Evidence is the first concrete source. Its v0.3 source is
structurally applicable when the observed project root directly contains a regular
`Cargo.toml` file. It does not search nested manifests, parse `Cargo.toml`, or detect
the Cargo executable.

Build and Test are separate v0.3 verification runs:

```text
Build -> cargo check
Test  -> cargo test
```

They have independent commands, outcomes, durations, diagnostics, freshness, and
lifecycle state. The Cargo source creates a `BuildTestCommandSpec` for each run; exact
process execution behavior remains for later implementation rounds. Cargo is a v0.3.0
scope decision, not a requirement that every future Evidence source resemble a
process.

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

A completed Cargo Build/Test result is not a permanent claim about the current
project. v0.3 records a `BuildTestFreshnessBaseline` after a verification process
completes, then compares later project state against that baseline.

```text
verification completes
        ↓
capture Build/Test filesystem baseline
        ↓
result is Fresh
        ↓
later relevant filesystem state differs
        ↓
result becomes Stale
```

The v0.3 Cargo Build/Test freshness baseline is a project filesystem fingerprint. It
includes everything under the project root by default, represented by relative path,
entry kind, file-content fingerprint, and symlink target where applicable. It excludes
`.git/` and every `target/` directory subtree.

This policy is intentionally conservative: any non-excluded project filesystem change
may make Build/Test Evidence stale. Documentation changes, such as `README.md` or
`docs/design.md`, therefore make Evidence stale in v0.3, as do source files,
`Cargo.toml`, `Cargo.lock`, `build.rs`, additions, and deletions. This accepts some
false-positive stale results to avoid presenting potentially outdated verification as
Fresh.

The baseline is captured after completion rather than before process start, so any
project state created by the completed verification is part of that completion state.
`target/` is excluded regardless, preventing Cargo's own build artifacts from causing
self-stale results.

A Git metadata-only change, such as committing unchanged project contents, does not
itself stale Build/Test Evidence because `.git/` is excluded from the fingerprint.

Comparisons do not update the baseline. Once a change is detected, later checks keep
comparing to the completion state until a new verification completes and explicitly
captures a new `BuildTestFreshnessBaseline`. Filesystem scan failures are returned as
errors rather than being treated as project changes.

Staleness semantics can differ by source shape. Cargo Evidence may become stale after
relevant project changes, while Artifact Evidence may become stale when an expected
file changes, disappears, or is replaced. This round does not define a generic stale
API.

## Execution behavior

The MVP starts verification only after an explicit user request. It does not run
`cargo test` or another command automatically whenever a project changes. Automatic
run policy belongs to a future configuration feature.

The initial runner is non-blocking for its caller:

```text
BuildTestCommandSpec
        ↓
BuildTestExecution::start
        ↓
worker thread
        ↓
process execution
        ↓
completion channel
        ↓
try_complete()
```

The TUI/event-loop thread never waits for the Build/Test child process. It only polls
execution completion non-blockingly through `try_complete()`. The worker executes the
process from the specification's program, argument vector, and working directory;
it does not parse the display command label or invoke a shell.

v0.3 initially uses worker-thread `Command::output()` to capture stdout and stderr.
It stores only a bounded diagnostic tail through `BuildTestDiagnostic`; stdout is
placed before stderr so a retained tail preferentially keeps likely error output.
Streaming output and a full log viewer remain outside the MVP and can be reconsidered
through dogfooding.

Manual controls are `b` for a Build run (`cargo check`) and `t` for a Test run
(`cargo test`). v0.3 permits one active Build/Test execution at a time; additional
Build/Test start requests while one is running are ignored rather than queued.

The runner produces a Fresh completed process result. After a completed process result,
application integration captures the Build/Test freshness baseline for that kind.
Baseline capture failure does not erase the observed process result. Freshness-baseline
association and later stale transitions are wired by application integration, not by
the runner itself.

Cancellation, parallel runs, queues, interactive test processes, terminal emulation,
and a full log viewer are outside the MVP.

## Application integration direction

`ProjectSnapshot` remains limited to Plan, Activity, and Tasks in this round. Build/Test
verification is runtime state rather than snapshot collection, so snapshot, Markdown,
and Git refreshes preserve it.

The App retains independent Build and Test lifecycle states. Manual execution state
remains separate from the existing Markdown/Git `RefreshStatus`; it does not add an
Evidence refresh source.

After a completed Build or Test run, application integration captures the corresponding
freshness baseline independently. Starting a replacement run or receiving an execution
error clears only that kind's prior baseline. This round does not yet poll baselines or
mark results stale after project changes.

Evidence execution remains agent-independent. An agent may request a verification run
in the future, but DevScope's observed process result remains the source of truth.

## TUI state integration

Project Progress keeps a single Evidence row. The row presents Build and Test
lifecycle status only, using the display status derived by `BuildTestState::status()`.
For example:

```text
Evidence   Build Not run · Test Not run
Evidence   Build Running · Test Not run
Evidence   Build Passed · Test Failed
Evidence   Build Stale · Test Passed
```

When both Build and Test are unavailable, the row displays `Not available` for
compatibility with the earlier reserved Evidence row. A mixed state remains explicit,
such as `Build Passed · Test Unavailable`.

Detailed command, result, duration, exit-code, summary, and diagnostic presentation
belongs to the later Evidence summary display task. The state row does not present
those details.
## TUI Evidence presentation

Project Progress remains a concise Build/Test lifecycle summary. A contextual Details
region presents command label, outcome, duration, optional exit code, summary, and
bounded diagnostic or execution-error message. The most recently accepted `b` or `t`
request selects Build or Test Details; completion preserves that selection.

Stale Details preserve the completed outcome (`Passed · Stale` or `Failed · Stale`).
Diagnostics use only the remaining visible Details height and are clipped without
scrolling. Source labels are not displayed while Cargo is the only source.
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

- Identify which concepts are genuinely shared between Cargo and Artifact Evidence.
