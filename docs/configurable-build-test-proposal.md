# Configurable Build/Test Verification Proposal

## Status

Implemented and dogfooded. This proposal records the adopted command-resolution and Freshness-exclusion boundary; it does not add generic Evidence APIs, additional verification slots, or automatic execution.

## Motivation and scope

External .NET dogfood confirmed that Plan, Git, Tasks, Preview, Current Work, and explicit Artifact inspection are project-neutral. `dotnet build` and `dotnet test` succeeded, but current Build/Test Evidence remained unavailable because the only command source is a root `Cargo.toml` check.

The proposed change generalizes only process-command resolution for the existing Build and Test slots. It does not add arbitrary Evidence slots, a generic Evidence API, language detection, command chaining, or automatic execution. DevScope must execute and observe a requested process itself; agent reports and externally run commands are not Evidence.

## Existing boundary assessment

`BuildTestCommandSpec` already has the required process representation: `kind`, `source_label`, `command_label`, `program`, `arguments`, and `working_directory`. The runner invokes `Command::new(program)`, passes the argument vector directly, and never parses `command_label` or starts a shell.

The runner, Build/Test state/result model, non-blocking TUI lifecycle, and local persistence can be reused without format changes. Persisted state already stores source, command, outcome, freshness, exit code, duration, summary, and a baseline fingerprint. A resolver such as `resolve_build_test_command(root, config, kind)` should decide only configured command, Cargo default, or unavailable. It must not absorb runner, persistence, or freshness work. `cargo_build_test_command` remains the Cargo-default branch.

## Proposed Config shape

```toml
[verify.build]
program = "dotnet"
args = ["build"]

[verify.test]
program = "dotnet"
args = ["test"]
```

`program` is an executable name or executable path; PATH resolution follows OS process execution. `args` is an array of strings, with each element one argv argument. DevScope does not shell-tokenize, expand, interpret pipes, redirects, `&&`, or PowerShell/cmd/bash syntax.

`program` must not be empty or whitespace-only. `args` should be optional and default to an empty array, so a no-argument program is valid. A malformed slot table, missing/invalid program, non-string args, unknown key, or invalid exclusion is a Config error. A program absent from PATH is an `ExecutionError`; a non-zero exit is `Failed`.

The first version fixes working directory to project root, matching current Cargo behavior and the .NET dogfood. It does not add Configurable cwd, environment injection, or timeouts. A project may explicitly invoke a script through a shell executable, for example `powershell -File scripts/verify.ps1`, but DevScope itself has no shell-command parser.

## Resolution and compatibility

Resolution is independent per kind:

1. A Config definition for that kind wins.
2. Otherwise a root regular `Cargo.toml` selects the existing Cargo default: `cargo check` for Build and `cargo test` for Test.
3. Otherwise that kind is `Unavailable`.

Partial configuration is valid. A non-Cargo project with only `[verify.test]` has Build unavailable and Test available/Not run. A Cargo project can override only Test:

```toml
[verify.test]
program = "cargo"
args = ["test", "--all-features"]
```

Therefore Rust is zero-config compatible. The configured `source_label` should use the program basename, such as `dotnet`, `python`, `npm`, `go`, or `cargo`; no arbitrary label field is needed. `command_label` is a human-readable join of program and arguments, never a shell replay contract.

## Language-neutral examples

```toml
# .NET
[verify.build]
program = "dotnet"
args = ["build"]
[verify.test]
program = "dotnet"
args = ["test"]

# Python / pytest
[verify.test]
program = "python"
args = ["-m", "pytest"]

# Python / uv
[verify.test]
program = "uv"
args = ["run", "pytest"]

# Node
[verify.build]
program = "npm"
args = ["run", "build"]
[verify.test]
program = "npm"
args = ["test"]

# Go
[verify.test]
program = "go"
args = ["test", "./..."]

# Java / Gradle wrapper
[verify.test]
program = "gradlew"
args = ["test"]
```

Rust/Cargo works naturally with no Config. .NET, Python/pytest, Python/uv, Node/npm, and Go work naturally with small configuration. Java/Gradle or Maven works with minor project policy, although wrapper naming and monorepo layout may need later cwd support.

## Freshness and `verify.exclude`

The current scanner excludes `.git`, every `target` directory, and `.devscope/work` and `.devscope/evidence`. It does not exclude .NET `bin` or `obj`. The .NET fixture produces both under application and test projects. An isolated scanner reproduction captured a baseline, invoked a .NET build, and observed `Changed` after the generated directory entries appeared; the minimal isolated project did not complete successfully because its standalone framework setup was incomplete. Together with the successful external fixture `dotnet build`/`dotnet test` run and the scanner rules, this established that .NET verification output changes the current freshness input set unless excluded. Focused Build/Test Freshness tests now cover generated-output exclusions and relevant input changes.

The implemented `verify.exclude` is part of the same command-resolution boundary:

```toml
[verify]
exclude = [
  "src/DogfoodApp/bin",
  "src/DogfoodApp/obj",
  "tests/DogfoodApp.Tests/bin",
  "tests/DogfoodApp.Tests/obj",
]
```

It excludes paths only from Build/Test freshness input observation, not process execution, Plan discovery, Git Activity, or Artifact observation. Its first-version semantics should be literal project-root-relative paths using `/`, no glob, negation, absolute path, or `..`; a directory excludes its subtree recursively. Reuse existing Config validation where applicable, while keeping `plan.exclude` and `verify.exclude` separate.

Keep current built-in Cargo-compatible exclusions. Do not add language-specific directories such as `bin`, `obj`, `node_modules`, `__pycache__`, `dist`, or `coverage` to Core. Do not adopt `.gitignore`: ignored does not mean verification-irrelevant. `.devscope/config.toml`, including command and exclusion changes, remains freshness-relevant and makes completed Evidence stale.

## Execution, UI, and errors

Configured commands run only after `devscope verify build`, `devscope verify test`, or the existing manual TUI action. DevScope captures a baseline, launches and observes the process, normalizes exit/output, evaluates freshness, and persists the latest result. Config reads, startup, polling, and file changes never run a command.

The existing two-slot UI model is reusable. A configured source starts Not run and can become Running, Passed/Failed Fresh or Stale, or ExecutionError. Details can show `Source: dotnet` and `Command: dotnet test`. Cargo-specific strings such as `Cargo Build/Test unavailable` should become generic, for example `Build/Test unavailable`.

No command resolved is Unavailable. A missing executable is ExecutionError. A non-zero command exit is Failed. A freshness scan/comparison error remains conservatively Stale.

## Non-goals and limitations

This proposal adds neither lint/format/typecheck/integration slots nor automatic language detection. It defers subdirectory cwd for monorepos, command chains, environment variables and secrets, timeouts, multiple suites, generated inputs that should remain relevant, and large-repository scan cost. It does not create a `VerificationSource` trait, Evidence registry, or shared Artifact/process abstraction.

## Completed acceptance criteria

- A Config-free Cargo root still resolves to `cargo check` and `cargo test`, with unchanged Evidence/freshness behavior.
- The .NET fixture with configured `dotnet build` and `dotnet test` reaches available, Not run, Passed/Fresh, stale after a source edit, Fresh after rerun, and restored persisted state after restart.
- Test-only Config leaves Build unavailable and Test available.
- Excluded generated output changes alone do not stale Evidence; a source edit, Config edit, or exclusion addition/removal does. Unsafe paths and glob-like input are rejected. Current Work and persisted Evidence remain irrelevant inputs.
- Focused tests cover resolver precedence, argv preservation, invalid Config, execution error versus failure, persistence restoration, generic CLI/TUI wording, and .NET `bin`/`obj` freshness behavior.

## Migration and documentation impact

Existing `[plan]` and `[artifact]` Config remains valid. The parser accepts top-level `verify` while preserving unknown-key rejection, and Config absence remains unchanged. The README, Setup Guide, Skill guidance, Decision Log, and roadmap now reflect the adopted boundary.

## Roadmap recommendation

The narrow **Configurable Build/Test command resolution and freshness exclusions** roadmap item is complete. This is not broad multi-language support: it retains the two existing Build/Test slots and their explicit execution model.
