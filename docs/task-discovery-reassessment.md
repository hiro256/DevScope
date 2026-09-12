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

DevScope currently discovers Markdown checkboxes broadly, subject to its existing
mandatory and configured exclusions. This is intentionally simple and useful, but
checkbox syntax is an observation format, not proof that an item is accepted Plan
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

These are alternatives to evaluate against concrete project examples. A roadmap-only
mode is not adopted: projects can have accepted plans outside `docs/roadmap.md`, and
the initial Config work intentionally deferred `plan.include`.

## Current recommendation

Keep broad Markdown checkbox discovery in the short term. Improve new task wording
through the provider-neutral Skill guidance, and do not hide ambiguous historical
checklists merely to make the TUI look cleaner. Treat this dogfood finding as evidence
for a future Plan-source semantics experiment, not justification for immediate
filtering or a final source-classification design.

Config exclusions remain for sources that should genuinely be outside the project's
observation policy, not for cosmetic Preview cleanup.

## Dogfood examples

| Source kind | Preview usefulness | Main issue |
| --- | --- | --- |
| `docs/roadmap.md` accepted work | Usually sufficient with source and section | Task wording if the action or target is vague |
| proposal checklist | Useful as document context, but not necessarily accepted Plan | Discovery semantics |
| historical dogfood checklist | Useful as history, but can be misleading in current Plan totals | Discovery semantics; sometimes vague wording |

## Non-goals

- Changing discovery, include/exclude rules, Config, ranking, or filtering.
- Changing Task Preview rendering or adding generated summaries.
- Rewriting historical checklists.
- Promoting a discovery experiment to the roadmap before concrete examples justify it.