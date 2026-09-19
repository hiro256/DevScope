# DevScope Setup Guide

Use this guide to make DevScope available on a machine and to confirm that it observes a
specific project correctly. Setup is not the project's implementation workflow. After
setup, return to the [DevScope Skill prototype](../examples/devscope-skill.md) for
normal daily work.

Setup does not define Plan authority, Current Work, Evidence, or AI memory. It must not
rewrite Plan files, complete checkboxes, create or complete Current Work, write Git,
commit, push, change a project's build system, or add agent instructions without
separate authority.

## 1. Make the CLI available

First check whether the command is available:

```powershell
devscope --help
```

If it is not, place a built DevScope executable in a stable user-level directory and
add that directory to the user PATH. On Windows, `C:\Tools\DevScope\` is one possible
example, not a required DevScope location. Validate the PATH change in a new shell with
`devscope --help`.

Do not use a development repository's `target/debug/devscope.exe` as a permanent PATH
target. It couples the tool used for observation to an in-progress build; rebuilds
change the executable and self-dogfooding can encounter Windows binary locks. Installer,
package-manager, PATH-editing, and deployment support are outside DevScope today.

## 2. Observe the project before configuring it

Move to the target repository root and start with defaults:

```powershell
cd <target-project>
devscope context
```

Use the TUI when a broader human view helps. Check the available surfaces:

- Plan and Git Activity;
- Current Work, when the project uses it;
- Cargo Build/Test availability; and
- an Artifact target, only when one is configured.

Missing Current Work, an empty history, or an unconfigured Artifact are normal unused
states, not setup failures.

**Observe first. Configure only a concrete mismatch.** Do not create an empty or
default-valued `.devscope/config.toml` merely because setup is occurring.

## 3. Classify a concrete mismatch

The current project Config is optional observation policy, not Plan, Current Work,
Evidence, or AI memory. It can address implemented project-specific cases:

- `[plan].exclude` for a literal project-relative file or directory that should not be
  observed as Plan, such as a derived or duplicated Markdown source;
- `[artifact].path` for one optional project-relative Artifact observation target; and
- `[verify]`, `[verify.build]`, and `[verify.test]` for Build/Test command resolution
  and freshness-only exclusions.

For example:

```toml
[plan]
exclude = ["generated"]

[artifact]
path = "target/release/example.exe"
```

`plan.exclude` uses `/` separators, accepts no globs, negation, absolute paths, or
parent-directory traversal, and cannot re-include mandatory exclusions. An Artifact
configuration is optional. A missing configured target is an observed `Missing` result,
not necessarily a setup failure.

### Build/Test verification

A Config-free project root with a regular `Cargo.toml` uses the Cargo defaults:

```powershell
devscope verify build  # cargo check
devscope verify test   # cargo test
```

A configured kind overrides that Cargo default. A non-Cargo project has an available
Build or Test slot only when that kind is configured; partial configuration is valid. For
example, configuring only Test leaves Build on its Cargo fallback when applicable, or
Unavailable for a non-Cargo project.

The following `.NET` project Config enables both kinds and excludes generated outputs
from freshness observation:

```toml
[verify]
exclude = [
  "src/DogfoodApp/bin",
  "src/DogfoodApp/obj",
  "tests/DogfoodApp.Tests/bin",
  "tests/DogfoodApp.Tests/obj",
]

[verify.build]
program = "dotnet"
args = ["build"]

[verify.test]
program = "dotnet"
args = ["test"]
```

`program` is the executable name or path, and `args` is an argv array: each value is one
argument. DevScope does not interpret shell command strings, `&&`, pipes, redirects, or
other shell syntax. Commands run with the project root as their working directory.

`verify.exclude` affects only Freshness observation, not the command's execution, Plan,
Git Activity, or Artifact observation. Every value is a literal project-relative path; a
directory excludes its subtree. Globs, negation, absolute paths, and `..` are rejected.

A result is Fresh when DevScope has not observed a relevant project-input change after
verification. It is Stale when a relevant input changed or Freshness could not be
confirmed. Generated outputs such as .NET `bin` and `obj` can be excluded, while Config
changes themselves remain relevant and make completed Evidence stale.

`.devscope/evidence/` stores local observed Evidence state. It is normally not committed.

## 4. Change minimally and validate

When a mismatch is concrete and the current Config can express it:

1. Make the smallest explicit Config change.
2. Re-run the relevant observation.
3. Confirm the mismatch is resolved and unrelated Plan, Activity, Evidence, and
   Current Work behavior remains correct.
4. Review the Config diff and remove a rule that no longer has a purpose.

Malformed or unsupported Config is an explicit error, not an unconfigured state. A
Config change may make completed Build/Test Evidence stale because Config is a relevant
project input; re-run the appropriate DevScope verification when a fresh observed
result is required.

## 5. Finish setup

Setup is complete when:

- `devscope` is available from the shell;
- the target repository root can be observed;
- zero-config behavior has been checked;
- any concrete mismatch has a minimal validated Config rule;
- Build/Test verification availability and its command source are understood; and
- optional Artifact behavior is understood when configured.

Return to the [DevScope Skill prototype](../examples/devscope-skill.md) for the normal
workflow: orient, set or update explicit Current Work when applicable, implement,
verify, and record a logical work boundary.

## External-project dogfood

For a new project, record observed mismatches rather than expanding DevScope during
setup:

1. Confirm CLI availability.
2. Run `devscope context` at the project root.
3. Inspect zero-config observation.
4. Record any mismatch.
5. Apply and validate only a supported minimal Config rule when justified.
6. Check Build/Test verification availability.
7. Return to the normal Skill workflow.

Current project-specific limits worth testing are per-kind configured Build/Test commands,
one optional Artifact target, literal Plan and Freshness exclusions only, root-recursive
Markdown discovery, and Windows as the primary verified environment.
