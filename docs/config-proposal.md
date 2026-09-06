# Config File Proposal

## Status

This is a docs-first proposal for the roadmap Config file item. It defines a small
project-configuration direction, not a parser, schema contract, dependency choice, or
global configuration system.

## Purpose

Configuration expresses project-specific policy and preferences where DevScope's
current defaults are not appropriate. It is not Plan, Activity, Evidence, Current
Work, Agent state, or project progress. Defaults must continue to work without a
configuration file.

## Configuration principles

- Config changes policy, not truth.
- Only real project-specific differences belong in project config.
- Semantic safety boundaries are not configurable.
- Implementation tuning is not user config by default.
- Future feature settings are not designed before the feature exists.
- Config remains optional and provider-neutral.

## Candidate settings

| Candidate | Project-specific? | Current need? | Semantic risk | First slice? |
| --- | --- | --- | --- | --- |
| `plan.include` paths | Yes | Projects can keep canonical Plan Markdown outside default discovery scope | Low if it only extends Plan discovery | Yes |
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
- `.devscope/work/` remains a mandatory Plan-discovery exclusion.
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

TOML is the recommended format candidate, but this proposal adds neither a TOML parser
nor Serde. Parser and dependency choices remain a separate implementation decision.

## Error handling

Missing config is normal: DevScope uses existing defaults. Malformed config should
produce an explicit error rather than silently changing policy. Unknown keys should
initially be explicit errors to catch misspellings; a warning policy can be revisited
only if real compatibility needs appear.

The initial implementation should avoid a broad fail-closed system. Commands that use
config-aware project collection should report the config error clearly, while an
unrelated Current Work command can remain independently usable when it does not need
config parsing. Exact command-level behavior belongs to the implementation slice.

A schema version field is deferred. The first deliberately small schema does not yet
need a compatibility mechanism, and adding one now could imply a stable contract too
early.

## Freshness and discovery interaction

A tracked `.devscope/config.toml` is relevant project input. Its change should make
completed Build/Test Evidence stale under the existing conservative freshness model.
The root `.devscope/` directory remains transparent: `.devscope/work/` is excluded as
temporary workflow state, but a future config file is not excluded.

A TOML config file does not enter Markdown Plan discovery. User Plan discovery rules
can alter only configured policy; mandatory semantic exclusions continue to apply.

## Minimal first slice

The first implementation should contain one optional `.devscope/config.toml`, one
loader, one small model, and two settings at most:

```toml
[plan]
include = ["planning/**/*.md"]
exclude = ["generated/**"]
```

The exact matching semantics should be specified and tested during implementation.
`include` and `exclude` affect Plan discovery only; they do not alter Current Work,
Activity, Evidence, or trust boundaries. No generic configuration framework, global
config, provider config, or Evidence source registry is needed.

## Deferred settings

Defer project name/root overrides, task markers, context limits, diagnostic limits,
refresh tuning, Cargo command overrides, generic command Evidence, Current Work
persistence modes, Agent settings, Handoff settings, Artifact Evidence settings, JSON,
and a config-show command. These lack a demonstrated current need or would preempt a
separate design boundary.

## Implementation plan

1. Confirm the include/exclude use case with one project fixture each.
2. Choose the smallest TOML parsing approach consistent with dependency policy.
3. Load optional `.devscope/config.toml` before Plan collection.
4. Apply user Plan rules while preserving mandatory exclusions.
5. Add focused tests for missing, malformed, unknown-key, include, exclude, and
   Current Work exclusion behavior.
6. Verify a config edit stales completed Build/Test Evidence.

## Open questions

- Are include paths additive or an explicit replacement of default discovery?
- Should glob syntax be supported in the first slice or only simple relative paths?
- Does an initial config schema need a version only after a second setting family?
- When a config error affects `context`, what concise error presentation is best?
