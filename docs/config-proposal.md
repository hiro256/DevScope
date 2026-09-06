# Config File Proposal

## Status

The first supported slice provides an optional project Config loader and a small
`[plan].exclude` schema. It is complete as a roadmap experiment, but not a generic
configuration framework or global configuration system.

## Purpose

Configuration is machine-readable, project-specific observation policy: an explicit
rule for how DevScope should observe and interpret a project within its configurable
boundaries. It changes policy, not truth. It is not Plan, Activity, Evidence, Current
Work, Handoff, AI memory, general notes, Agent state, or project progress. Defaults
must continue to work without a configuration file.

## Configuration principles

- Config changes policy, not truth.
- Only real project-specific differences belong in project config.
- Semantic safety boundaries are not configurable.
- Implementation tuning is not user config by default.
- Future feature settings are not designed before the feature exists.
- Config remains optional and provider-neutral.
- Start with defaults, detect when possible, and configure only when necessary.
- Change minimally, keep reasons understandable, verify behavior after a change, and
  remove configuration that becomes obsolete.

## Candidate settings

| Candidate | Project-specific? | Current need? | Semantic risk | First slice? |
| --- | --- | --- | --- | --- |
| `plan.include` paths | Possibly | Root-recursive discovery already covers normal subdirectories | Low if it only extends Plan discovery | Defer |
| `plan.exclude` paths | Yes | Projects can exclude generated or non-Plan Markdown | Medium; mandatory exclusions stay enforced | Yes |
| Project name override | Sometimes | No demonstrated need beyond directory naming | Low | Defer |
| Project root behavior | Rarely | Current root detection works | Medium | Defer |
| Activity enablement or Git options | Possibly | No demonstrated project difference | Medium | Defer |
| Cargo Evidence enablement | Possibly | No demonstrated need | Medium | Defer |
| Cargo command/working-directory override | Possibly | Requires source-specific design | High | Defer |
| Current Work path or persistence mode | Possibly | Initial experiment is sufficient | High | Defer |
| Context task limit | Usually preference, not project policy | No demonstrated need | Low | Defer; consider a future CLI flag |
| Diagnostic length | Safety/resource limit or global preference | No project need | Medium | Keep fixed initially |
| Refresh/poll intervals | Implementation tuning | No project need | Medium | Keep fixed initially |
| Agent, Handoff, Artifact Evidence, IDE, or Web settings | Future features | No | High | Defer |

## What is not configurable

The following are semantic invariants rather than project preferences:

- Current Work is not counted as Plan.
- `.git/`, `target/`, and `.devscope/work/` remain mandatory Plan-discovery exclusions.
- Agent telemetry is not Evidence.
- Current Work completion does not auto-complete a Plan task.
- Failed verification is not treated as passed.
- Project configuration is relevant project input rather than an Evidence-freshness
  exclusion.

User `plan.exclude` values may add exclusions but must not re-include mandatory
semantic exclusions. Config must not become a way to rewrite DevScope trust boundaries.

## Categories

- **Project discovery:** Project-name override and root behavior are candidates, but
  neither has a demonstrated first-slice need.
- **Plan discovery:** Include and additive exclude paths are the strongest current
  candidates; file patterns and task markers remain speculative.
- **Activity:** Git behavior is observed by default; no current setting is justified.
- **Evidence:** Source enablement and source-specific options may matter later, but
  command override must not turn Cargo Evidence into a premature generic-command
  source.
- **Current Work:** Path, enablement, and tracked/local policy remain experiments, not
  first-slice config.
- **CLI / presentation:** Context limit and path presentation are better evaluated as
  future CLI or global preferences, not project policy.
- **Refresh / performance:** Polling and diagnostic limits remain fixed safety or
  implementation choices.
- **Future integrations:** No Agent, Handoff, Artifact Evidence, IDE, or Web settings
  are designed before those features exist.

## Configuration layers and location

The first scope is project config only. A future specific CLI flag may override a
project setting when that flag is actually introduced; otherwise the useful precedence
model is simply project config over defaults. Global config and environment-variable
layers are outside this proposal.

Candidate locations:

| Location | Assessment |
| --- | --- |
| `.devscope/config.toml` | Recommended: repository-local, namespaced, and adjacent to existing DevScope state. |
| `.devscope.toml` | Visible at root, but competes with other root-level tooling files. |
| `devscope.toml` | Discoverable, but has the greatest namespace-collision risk. |

`.devscope/config.toml` can be tracked as project policy while `.devscope/work/`
remains ignored temporary state. This coexistence is natural if `.gitignore` excludes
only the work subtree, not the entire directory.

## Format candidates

| Format | Assessment |
| --- | --- |
| TOML | Best candidate: familiar in the Rust ecosystem, human-editable, comment-friendly, strict enough for a small project file, and Windows-friendly. |
| YAML | Human-friendly but more flexible and edge-case-prone; it would add parsing complexity without a current benefit. |
| JSON | Strict and common, but lacks comments and is less pleasant for hand-maintained project policy. |

TOML is used by the initial implementation. It uses the `toml` crate to parse a TOML
value table and validates the small schema explicitly, without a direct Serde dependency.

## Error handling

Missing config is normal: DevScope uses existing defaults. Malformed config should
produce an explicit error rather than silently changing policy. Unknown keys should
initially be explicit errors to catch misspellings; a warning policy can be revisited
only if real compatibility needs appear.

Config-aware `context`, `task list`, and TUI startup report Config errors explicitly.
Unrelated Current Work commands remain independently usable without Config parsing.

A schema version field is deferred. The first deliberately small schema does not yet
need a compatibility mechanism, and adding one now could imply a stable contract too
early.

## Freshness and discovery interaction

A tracked `.devscope/config.toml` is relevant project input. Its change should make
completed Build/Test Evidence stale under the existing conservative freshness model.
The root `.devscope/` directory remains transparent: `.devscope/work/` is excluded as
temporary workflow state, but a future config file is not excluded.

Config creation, modification, and deletion are Plan-observation inputs and trigger Plan recollection. Malformed Config is surfaced explicitly rather than converted to Plan unavailable.

A TOML config file does not enter Markdown Plan discovery. User Plan discovery rules
can alter only configured policy; mandatory semantic exclusions continue to apply.

## Config growth and maintenance

Do not create comprehensive or empty Config up front. First ask whether DevScope can
determine the behavior correctly through defaults or automatic detection. Add a rule
only for a concrete observation mismatch, not merely to make output look cleaner.
A possible justified case is a derived translation that duplicates authoritative task
checkboxes: [translation-proposal.md](translation-proposal.md) treats translated
Markdown as a derived human-readable view, so excluding that non-authoritative
directory from Plan discovery can be appropriate.

Valid exclusion reasons are derived, generated, duplicated, intentionally
non-authoritative, or narrowly irrelevant to Plan semantics. Long-lived rationale
belongs in Markdown or decisions; an obvious rule needs no comment, and at most a
short TOML comment may clarify a non-obvious rule. Do not add `reason`, session,
memory, or AI-authorship metadata to the machine-readable schema.

After a Config change, verify before-versus-after behavior: confirm the mismatch is
resolved, authoritative Plan files remain visible, and Activity, Evidence semantics,
and Current Work did not change unexpectedly. A Config rule that no longer matters,
for example after improved automatic detection, is a removal candidate.

## Minimal first slice

The first implementation should contain one optional `.devscope/config.toml`, one
loader, one small model, and one setting only:

```toml
[plan]
exclude = ["some-derived-directory"]
```

Paths are project-root-relative and use `/` separators in Config. A directory path
excludes that directory subtree; a file path excludes that exact Markdown file. The
first slice has no glob syntax, negation, re-include, absolute paths, or paths outside
the project root. Effective exclusions are mandatory exclusions plus configured
`plan.exclude`; configuration never replaces mandatory exclusions.

`plan.exclude` affects Plan discovery only; it does not alter Current Work, Activity,
Evidence, or trust boundaries. `plan.include` is deferred because DevScope already
recursively discovers ordinary Markdown below the project root. No generic
configuration framework, global config, provider config, or Evidence source registry
is needed. Support may exist before this DevScope repository itself needs a Config;
do not invent a project-specific rule merely to dogfood Config.

## Deferred settings

Defer `plan.include`, project name/root overrides, task markers, context limits,
diagnostic limits, refresh tuning, Cargo command overrides, generic command Evidence,
Current Work persistence modes, Agent settings, Handoff settings, Artifact Evidence
settings, JSON, and a config-show command. These lack a demonstrated current need or
would preempt a separate design boundary. Do not add config init/generate/default-dump
or Config mutation CLI commands in the first slice.

## Implementation plan

1. Implement optional `.devscope/config.toml`.
2. Support only `[plan].exclude`.
3. Preserve current behavior exactly when Config is missing.
4. Add user exclusions to mandatory exclusions rather than replacing them.
5. Use the simple project-relative file/directory semantics above.
6. Reject malformed Config and unsupported keys explicitly.
7. Verify exclusions with focused temporary-project tests.
8. Verify `.devscope/work/` remains excluded and unrelated Plan files remain visible.
9. Verify a Config edit remains relevant to Build/Test Evidence freshness.

## Open questions
- `plan.include` semantics are deferred until a concrete discovery need exists.
- Glob support is a deferred future question; the initial slice accepts only literal paths.
- Does an initial config schema need a version only after a second setting family?

## Initial dogfood findings

The DevScope repository was inspected from zero-config state. Plan discovery reported
six remaining tasks: five explicit Post-MVP roadmap tasks and the existing Current
Work CLI dogfood item. No `translations/` directory or other derived Markdown copy
was present, so no duplicated or non-authoritative task checkbox was observed.

No project-specific Config rule was justified. The repository remains zero-config:
adding an exclusion would encode speculative policy rather than resolve an observed
mismatch. The check confirmed that `context`, `task list`, and Current Work work
normally without Config. This is one initial observation, not a universal rule; a
future real derived or non-authoritative Markdown source should be evaluated through
the same before-and-after workflow.

## First-slice conclusion

The first supported Config slice is complete: optional `.devscope/config.toml` and
`[plan].exclude` are implemented, while missing Config preserves default behavior.
Config creation, modification, and deletion trigger live Plan recollection; malformed
Config is explicit; and Config remains a Build/Test freshness-relevant input.

The zero-config-first dogfood succeeded. The DevScope repository itself currently has
no concrete mismatch that justifies Config, so it remains zero-config. Future rules
must grow only from a concrete project mismatch. `plan.include`, glob syntax, global
or environment layers, a generic Config framework, Config CLI or mutation commands,
schema versioning, and other setting families remain deferred.
