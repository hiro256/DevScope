# DevScope Skill Proposal

## Status

This is an exploratory, docs-first proposal for the DevScope Skill experiment. It
specifies reusable agent behavior, not a stable Skill package, file format, or
provider integration. No Skill implementation is created by this proposal.

## Purpose

The Skill should teach an AI agent how to use the existing DevScope CLI workflow to
orient, resume temporary work, perform narrow Current Work updates, and distinguish
recorded work from observed verification. It should reduce repeated human workflow
reminders without becoming a source of project truth.

## Position in the architecture

```text
Plan / Activity / Evidence / Current Work
                    ↓
                 DevScope
                    ↑
                   CLI
                    ↑
                  Skill
                    ↑
                AI Agent
```

The Skill is a behavior layer. It does not store Plan, Activity, Evidence, Current
Work, private session state, or hidden progress. DevScope and its CLI remain the
visible project surfaces.

## Non-goals

The Skill must not become:

- A project or task database.
- An Evidence source or a Current Work store.
- A Codex-only, Claude-only, or other provider-specific integration.
- A direct Markdown editor for Current Work in the normal workflow.
- A Handoff or Notes feature.
- An authority source for Plan changes, commits, or pushes.

## Core workflow

1. Run `devscope context` first.
2. Use it to inspect Plan state, Current Work parent/progress/next item when present,
   Git Activity, and Evidence availability.
3. Run `devscope work list` only when detailed Current Work items or a current
   display-order number are needed.
4. Run `devscope task list` only when the desired remaining Plan task is not shown by
   `context` or broader task discovery is necessary.
5. Read source Markdown only for acceptance intent, detailed specification, or design
   constraints not available from the orientation output.
6. Implement the requested work and perform appropriate verification.
7. At a logical work boundary, when a Current Work item is ready to record, run
   `work list`, confirm its current number, then run `devscope work done <number>`.
8. Re-run `devscope context` to reorient before reporting or continuing.

This is the practical form of **read broadly, write narrowly**: inspect the available
CLI and source documents only as needed, and use only explicit narrow DevScope writes.

## CLI usage rules

The initial Skill refers only to commands that exist today:

```text
devscope context
devscope task list
devscope work list
devscope work done <number>
```

It must not invent `work add`, `work start`, `work clear`, reopen/undo, or any other
unimplemented command. When no Current Work summary is present, treat Current Work as
inactive; do not try to create it through imagined CLI syntax. When `context` reports
`Current Work: unavailable`, run `work list` to obtain the explicit Current-Work error
rather than treating the state as healthy.

Before `work done N`, the Skill must run `work list` and confirm the latest `N`.
Numbers are one-based display-order positions, not persistent identities. Repeating a
completion for an already completed item is a safe retry because it exits successfully,
but the Skill should not perform redundant retries without a reason.

## Trust boundaries

```text
Plan              = canonical project intent
Current Work      = temporary recorded work state
Evidence          = verification information surfaced by DevScope
Observed Evidence = verification directly observed by DevScope
AI assessment     = interpretation, not observed truth
```

Evidence remains broader than the current Cargo source; this does not establish a
stable provenance taxonomy. A completed Current Work item does not complete its parent
Plan task, and a successful `work done` command is not Evidence. An AI running a test
or reporting that tests passed is not, by itself, Observed Evidence. The Skill should
request or inspect appropriate verification, then report the difference between what
DevScope surfaced, what it directly observed where applicable, and its own assessment.

Git Activity is an observation of what changed, not proof that a requirement is
complete. `context` is an orientation surface, not a replacement for source Markdown
when details are necessary.

## Stop behavior

At a stopping boundary, the Skill should inspect `devscope context`, relevant
verification, Current Work state when active, and `git status`, then report those
facts clearly while preserving the Plan/Evidence distinction. For a Plan-level
mutation, it follows explicit authority already supplied by the current user request,
repository instructions such as `AGENTS.md`, or an explicit workflow instruction. With
no authority, it does not mutate the Plan; with ambiguous authority, it reports the
situation and seeks direction. The Skill is not itself an authority source.

The same rule applies to commit and push: do not perform either autonomously, but
follow explicit user or project authorization when it exists. The Skill does not create
a Handoff.

## Agent neutrality and packaging candidates

The core guidance is agent-neutral: it assumes only that an agent can run the
DevScope CLI. Provider-specific packaging must remain separable from this guidance.

Candidate locations remain undecided:

- `skills/devscope/SKILL.md`: repository-local and compatible with conventions used
  by some agents, but its packaging semantics are provider-specific.
- `.devscope/skill.md`: clearly project-local, but not necessarily recognized by
  existing agent skill loaders.
- `docs/examples/devscope-skill.md`: agent-neutral and easy to review, but requires
  an external packaging step before an agent can use it automatically.

DevScope must not depend on any one of these conventions. The experiment should first
validate the concise core behavior; any provider wrapper can be added later.

## Provider-neutral core prototype

The concise runtime-guidance prototype is now available at
[examples/devscope-skill.md](examples/devscope-skill.md). It intentionally keeps only
the rules an agent needs while working; this proposal retains rationale, packaging
comparison, dogfood scenarios, and open questions. Provider-specific packaging has not
been selected.

## Config maintenance workflow

When Config support exists, treat Config as machine-readable project-specific
observation policy, not a Skill store or AI memory. Before changing it: inspect current
DevScope state and Config when present, identify a concrete mismatch, prefer automatic
detection and an existing rule, confirm semantic invariants remain intact, and make the
smallest necessary change. Do not create comprehensive, empty, or default-valued Config
without a specific rule to express.

After changing Config: re-run the relevant DevScope behavior, confirm the original
mismatch is resolved, check unrelated observations, review the Config diff, and decide
whether the rule is genuinely project-persistent. Remove or simplify an obsolete rule.
A stale Evidence transition after tracked Config changes is expected under the
conservative freshness model, not itself a Config-maintenance failure. Durable rationale
belongs in Markdown or decisions rather than in a comment-heavy Config schema.

## Dogfood scenarios

### Scenario A: fresh-session recovery

A fresh AI session receives only the repository and the Skill. It begins with
`devscope context`, recovers active work, and reads source Markdown only if the task
needs specification detail.

### Scenario B: active Current Work mutation

With active Current Work, the agent implements a bounded change, runs `work list` to
confirm the current number, and uses `work done N` at a sensible work boundary.

### Scenario C: Current Work absent

With no Current Work summary, the agent continues normal orientation without inventing
`work start` or `work add`.

### Scenario D: malformed Current Work

When context reports Current Work as unavailable, the agent uses `work list` to inspect
the explicit error and does not silently accept the state.

### Scenario E: verification boundary

After tests run, the agent explains separately what Current Work was recorded, what
verification information DevScope surfaced (including directly observed results when
applicable), and what it interprets from those facts.

## Measurements

Dogfooding should record, without requiring a formal benchmark:

- DevScope command count.
- Source Markdown reads.
- Human reminders required.
- Workflow mistakes.
- Invalid CLI commands attempted.
- Current Work and Evidence confusion.

## Success criteria

The experiment is successful when a fresh agent can begin with `context` without extra
human workflow instruction, recover active work, use `work list` only when detail or
numbers are needed, confirm a number before `work done`, and avoid nonexistent write
commands. It must keep Current Work distinct from Plan and Evidence, avoid promoting
self-reported verification to Observed Evidence, respect existing explicit authority
for Plan mutations without requesting redundant confirmation, and function without a
direct Agent adapter. The guidance should also remain short enough for agents to follow
consistently.

## Failure signals

The Skill needs revision if agents routinely read all documents before `context`, skip
context, guess `work done` numbers, treat Current Work completion as Plan completion,
treat any test command they ran as DevScope-observed Evidence, invoke nonexistent
`work add` or `work start`, mutate a Plan without authority, request permission despite
clear existing authority, or ignore essential rules because the Skill is too long.

## Open questions

- Which candidate packaging is most portable and usable in practice?
- How short can the guidance become without losing critical safety rules?
- Does a provider-specific wrapper add value beyond the agent-neutral core?
- Should a future Skill mention Handoff or Notes after those workflows are tested?
- When, if ever, should Current Work creation or archival become a supported write?

## Prototype dogfood results

The provider-neutral prototype at [examples/devscope-skill.md](examples/devscope-skill.md)
guided a fresh AI session through the tested context-first workflow. The session
recovered the parent Plan task, Current Work progress, and next item from `context`,
used `work list` only to obtain current mutation numbers, completed Current Work items
3 and 4 with `work done`, and then ran `cargo test`.

In this tested workflow, fresh-session recovery, context-first orientation, conditional
`work list` use, and confirming the number before `work done` all succeeded. No
nonexistent DevScope commands were attempted, and no Current Work/Plan or Current
Work/Evidence confusion was observed. The prototype was short enough to be followed
without additional detailed human workflow instruction. A direct Agent adapter was not
required for this tested workflow.

The direct `cargo test` execution reported 166 passing tests. It is an AI/session
verification fact, not automatically DevScope Observed Evidence: Evidence remains
verification information surfaced by DevScope, Observed Evidence is verification
directly observed by DevScope, and AI assessment remains interpretation. `work done`
remains a Recorded Current Work update, not Evidence. This wording does not define a
stable provenance taxonomy.

An `apply_patch` sandbox-initialization error occurred three times before a
documentation update could be applied. This was an environment/editing-tool failure,
not a DevScope or Skill workflow failure. The reported workflow did not switch editing
methods without appropriate authority, so the event did not change the authority model.

Scenario A (fresh-session recovery), Scenario B (active Current Work mutation), and
Scenario E (verification boundary) succeeded in this dogfood round. Scenario C
(Current Work absent) and Scenario D (malformed Current Work) were not exercised.

## Experiment conclusion

The dogfood demonstrated that the concise provider-neutral prototype can guide the
tested workflow without a direct Agent adapter and can reduce repeated human workflow
instructions. It does not establish a universal Skill format, a stable packaging
standard, or that Agent adapters are unnecessary. Provider-specific packaging remains
open, including whether a Codex-native wrapper, another provider wrapper, or a
repository-local convention improves reliability.
