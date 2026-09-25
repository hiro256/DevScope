# Task Discovery Reassessment

## Observation

Task Preview now exposes the selected Markdown task with its source, section, and
nearby lines. Dogfooding showed that this source-grounded behavior is useful: it
makes vague task text and mixed document roles visible instead of concealing them
behind generated interpretation.

For example, `Dogfood the workflow` lacks its target and expected outcome when
shown alone. A roadmap task and a historical proposal checklist can also appear
together under the current broad checkbox discovery.

## Current semantics

Without `[plan].include`, DevScope discovers Markdown checkboxes broadly, subject
to mandatory and configured exclusions. An explicit include selects only the named
Markdown file(s) or directory subtrees; an empty include selects no Plan sources.
Checkbox syntax is an observation format, not proof that an item is accepted Plan
work.

The following concepts should remain distinct:

```text
Accepted Plan = work the project has accepted for the future
Checklist     = document-local proposal, verification, or experiment steps
Current Work  = temporary breakdown of a selected Plan task
```

Current Work is already kept out of ordinary Plan discovery. Proposal and historical
checklists are not yet classified separately.

## Task-writing guidance

New Plan and Current Work items should be self-explanatory when displayed without
chat context. Prefer concise text that includes an action, target, and purpose or
expected outcome where practical.

```text
Avoid:     Verify behavior
Prefer:    Verify automatic Git refresh updates the open diff

Avoid:     Dogfood the workflow
Prefer:    Dogfood Current Work recovery in a fresh AI session
```

This is workflow guidance, not authority to alter task importance, Plan membership,
completion truth, or Evidence. Improve wording only when the relevant plan or work
item is already in scope; do not cosmetically rewrite unrelated historical tasks.

## Candidate future discovery models

- Explicit Plan sources selected per project.
- Source classification for accepted plans, checklists, and historical documents.
- An accepted-Plan marker or document convention.
- File or path policy through Config, when it represents a real observation policy.
- Roadmap-first ranking while retaining other accepted sources.
- Checklist classification that keeps document-local verification out of Plan totals.

These were alternatives pending concrete project examples. A roadmap-only
mode is not adopted: projects can have accepted plans outside `docs/roadmap.md`, and
the initial Config work intentionally deferred `plan.include`.

## Concrete dogfood finding

The earlier recommendation was to retain broad discovery until a concrete workflow
failure justified changing it. That failure is now observable: `devscope context`
counts the historical checklist item at `docs/current-work-proposal.md:248`
(`Dogfood Current Work CLI storage and recovery workflow`) as a remaining Plan task.
The accepted project work is in `docs/roadmap.md`; this proposal-local example is
not an accepted Plan task. The misleading total affects both short CLI orientation
and the TUI's Plan/Tasks view, rather than merely the appearance of Task Preview.
The proposal document and its checkbox should remain intact.

## Options and recommendation

| Option | Compatibility | User effort / Config fit | Ambiguity / omission risk | Complexity / portability | JSON/API fit |
| --- | --- | --- | --- | --- | --- |
| A. Required explicit include paths | Changes unset projects | Every project must configure sources; explicit but high effort | Clear selected files, but missing entries can hide work | Moderate; literal paths are portable | Clear source policy |
| B. Document marker/front matter | Unmarked plans need migration | Edit every accepted document; weak fit for central AI-maintained Config | Clear per-document intent, but easy to forget a marker | New parsing convention; portable with files | Clear source role if standardized |
| C. Name/path convention | Changes projects using other names | Low setup, but relocation follows convention | Names do not prove acceptance; high false omission risk | Simple but repo conventions vary | Implicit policy is hard to explain |
| D. Source classification | Needs migration or an ambiguous default | More labels to maintain | Explicit roles but more chances to mislabel or omit | Larger model/API change; portable if specified | Rich but premature contract |
| E. Broad default, optional explicit sources | Preserves unset projects | One Config policy only where needed; fits AI-maintained review | Opt-in source set is clear; misconfigured paths need errors | Moderate filtering; literal paths are portable | One stable selected-source rule |

Recommend **E as the first implementation experiment**, using A-style literal
`[plan].include` list of project-relative Markdown files and/or directory subtrees.
For example:

```toml
[plan]
include = ["docs/roadmap.md", "docs/plans"]
exclude = ["docs/plans/archive"]
```

Omission keeps today's broad discovery (including existing mandatory exclusions),
so old Config files and projects do not silently change. When present, only sources
under the listed paths are candidates; existing `[plan].exclude` and mandatory
exclusions still win. `include = []` explicitly selects no Plan sources, producing
an empty Plan rather than falling back to broad discovery. This distinction must be
tested and explained clearly so an empty policy is not mistaken for success.

Paths should be literal and project-relative, using `/` in Config on Windows too:
no absolute paths, parent traversal, glob syntax, or negation. A file path selects
that Markdown file; a directory path selects Markdown files beneath it. Reject
nonexistent entries and non-Markdown file entries with a clear Config error rather
than quietly hiding accepted work. Do not add a default `roadmap.md` assumption.
The implementation should resolve paths within the project root and preserve the
existing handling of excluded directories and symlinks. A deliberate root entry
(`.`) may explicitly retain broad source selection; it is not an implicit default.

This is **source selection**, not automatic classification of every checkbox in a
selected document. A mixed accepted Plan file may still need a later, separately
justified document-level convention. Preserve task text, completed state, source
path, line, heading, context, and source order for selected tasks. Apply the same
selection to Plan totals, task lists, CLI context, and TUI; Current Work remains
separate and cannot become Plan through an include path. Neither source selection
nor Current Work implies priority, task completion, or Evidence.

The first implementation slice adds parsing/validation and source filtering, with
regression tests for omitted vs empty include, file and subtree selection, overlap,
exclude precedence, invalid paths, and proposal-checklist exclusion. Plan totals
and Tasks share filtered MarkdownProgress through collect_markdown_state; CLI and TUI
do not apply separate filters. DevScope has not opted into its accepted roadmap
through local Config yet: that dogfood edit is a separate, reviewed step. A future
JSON/API surface should report the same selected-source Plan semantics.

Config exclusions remain useful for sources genuinely outside observation policy;
they are not a cosmetic Preview-cleanup mechanism.

## Dogfood examples

| Source kind | Preview usefulness | Main issue |
| --- | --- | --- |
| `docs/roadmap.md` accepted work | Usually sufficient with source and section | Task wording if the action or target is vague |
| proposal checklist | Useful as document context, but not necessarily accepted Plan | Discovery semantics |
| historical dogfood checklist | Useful as history, but can be misleading in current Plan totals | Discovery semantics; sometimes vague wording |

## Non-goals

- Classifying selected checkboxes as Plan, Checklist, or Historical.
- Changing Task Preview rendering or adding generated summaries.
- Rewriting historical checklists or automatically editing Plan.
- Applying a DevScope-local include policy before separate dogfood review.
