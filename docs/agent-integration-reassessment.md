# Agent Integration Reassessment

## Status

This is a docs-first reassessment after the CLI, Current Work, and provider-neutral
Skill experiments. It evaluates whether a direct Agent adapter has a concrete reason to
exist now. It does not implement an adapter or make provider packaging a Core concern.

## Why reassess now

The tested workflow already uses `devscope context` for orientation, `work list` for
Current Work detail and mutation numbers, `work done <number>` for narrow Recorded
updates, and a Skill for fresh-session behavior. The question is therefore not whether
agent data might be interesting, but what useful question remains unanswered by the
existing shared project surfaces.

## What CLI and Skill already solve

CLI plus Skill demonstrated fresh-session recovery, Current Work continuity,
context-first orientation, conditional detail reads, narrow Current Work completion,
and the Plan/Current Work/Evidence boundaries. It does not require agent-private state
or a direct adapter for the tested workflow. These are not justification for an
adapter.

## Candidate unmet user questions

| User question | Agent-originated signal | Existing alternatives | Assessment |
| --- | --- | --- | --- |
| Who is working now? | Optional agent identity and lifecycle state | Current Work, Git Activity, Handoff | Limited value for one agent; may become useful with concurrent agents. |
| Is an AI actively working or unexpectedly stopped? | Best-effort working/idle/stopped state and timestamps | Git Activity for effects; Handoff for intent | Potentially useful, but no dogfood case has required it. |
| Which agent produced concurrent activity? | Agent identity and operation association | Git history and separate work coordination | Strongest possible future case, but multi-agent work is not yet dogfooded. |
| What high-level operation is underway? | Reported agent operation | Current Work and Skill behavior | Often duplicates explicit Recorded state and may be stale. |
| Did an agent encounter a private-side error? | Reported adapter error | Git, Evidence, Handoff, human report | Potentially supplementary, not project truth or Evidence. |
| What token or resource use is occurring? | Provider telemetry | None in current project surfaces | No demonstrated DevScope decision benefit; privacy and provider coupling are high. |

## Trust boundaries

Agent state is supplementary reported telemetry. It is not Activity, Current Work, or
Evidence. An agent report such as "tests passed" does not become Observed Evidence;
Evidence remains verification information surfaced by DevScope, while directly observed
verification is a narrower case. Current Work remains explicit Recorded state and must
not be auto-completed from telemetry.

Observed, Recorded, and Reported remain conceptual vocabulary rather than a stable
provenance taxonomy. Agent identity or provider name, if ever shown, would be optional
metadata rather than a required Core concept.

## Alternatives

- **CLI + Skill:** Already handles orientation, recovery, safe Current Work updates,
  and workflow guidance.
- **Current Work:** Answers intentional temporary work and next steps when explicitly
  recorded; it should not be inferred from agent telemetry.
- **Git / Evidence:** Answer what changed and what verification DevScope surfaced, not
  whether an agent claims to be busy.
- **Handoff / Notes:** A better future candidate for why work stopped or what a next
  participant should know; it is distinct from live status.
- **CLI extension:** May be appropriate when a needed question is still project-derived
  rather than agent-originated.

## Costs and risks

An adapter would add provider coupling, API/version compatibility work, authentication
and permission questions, privacy concerns, session-discovery complexity, platform
differences, failure-mode design, maintenance, and testability cost. It must not parse
private JSONL, internal databases, caches, or undocumented session files. Any future
experiment should use a documented, supported external interface only; no provider API
research is needed for this reassessment.

## Decision criteria

A read-only adapter experiment is justified only when all of the following hold:

1. A concrete user question remains unanswered.
2. Agent-originated data materially improves the answer beyond CLI, Skill, Current
   Work, Handoff/Notes, Git, or Evidence.
3. Best-effort and possibly stale semantics are acceptable.
4. The data can come from a documented supported external interface.
5. The adapter remains optional and does not become a Progress Core dependency.
6. A realistic dogfood scenario needs the signal.

Defer an adapter when it duplicates CLI + Skill, adds only decorative status, relies on
undocumented internals, reclassifies reports as Evidence, creates provider coupling
without clear user value, or lacks a dogfood scenario.

## Reassessment result

No concrete adapter use case is strong enough to prototype now. The strongest remaining
case is identifying concurrent agents and detecting an unexpectedly stopped agent when
that state cannot be inferred from project surfaces. It has not been observed as a
blocking problem in the completed single-agent dogfood workflow.

The reassessment is complete; adapter implementation is deferred. This is not a
permanent decision against adapters. Reassess when multi-agent work causes ownership or
lifecycle ambiguity that Current Work, Git, Evidence, CLI, Skill, and a Handoff/Notes
candidate cannot resolve, and when a supported external interface is available.

## Possible future experiment

If the trigger occurs, first perform focused provider-interface research. Only then
consider one provider, one read-only best-effort lifecycle signal, one explicit user
question, no writes, and no Core dependency. Promotion remains a separate roadmap
slice.

## Deferred questions

- Does a Handoff/Notes experiment address stop reasons better than live telemetry?
- Does multi-agent dogfooding reveal a real ownership or lifecycle ambiguity?
- Which supported external interfaces, if any, are suitable for a narrow adapter?
- Would optional identity improve a human decision rather than decorate the UI?
- Is token/resource telemetry useful enough to justify its privacy and coupling cost?
