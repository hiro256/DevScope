# Current Work History Proposal

## Status

This is an exploratory semantics proposal for the `Progress history experiment`. It
does not introduce a generic event model, persistence implementation, CLI command,
or TUI surface. The roadmap item remains incomplete until a concrete experiment has
been implemented and dogfooded.

## Purpose and boundary

History is a time-ordered record of explicit, observable state changes made by
DevScope. It is useful as context for resuming short work, reviewing the recent flow
of a slice, and deciding whether a recorded Current Work state deserves attention.
It is not an automatic substitute for maintaining Current Work.

The first concrete source should be **Current Work change history**. Current Work
already has narrow, explicit mutation points, so its event meaning is clear without
agent telemetry, filesystem-diff inference, or automatic state correction.

```text
Current Work file       current Recorded working state and source of truth
Current Work history    supplemental timeline of DevScope-successful mutations
Evidence                observed verification or Artifact result, not this history
```

History must not reconstruct Current Work, act as event sourcing, infer an active
item, complete a Plan task, or prove work happened at the recorded time.

## Initial event scope

The first experiment records only mutations that DevScope itself successfully writes:

```text
devscope work active N       ActiveSet or ActiveChanged
devscope work active clear   ActiveCleared
devscope work done N         WorkCompleted
```

`ActiveSet` applies when no item was active; `ActiveChanged` retains both the prior
and new item references. `ActiveCleared` records an explicit clear operation. A
successful `work done` records `WorkCompleted`, including whether that same atomic
Current Work write also cleared Active.

The initial scope intentionally does not record a file being created, replaced,
directly edited, or changed in Parent/Task metadata. The Current Work detector still
reloads such edits as current state, but no history entry is inferred from them. This
keeps the first history source about DevScope mutations rather than guessed intent.

## Event meaning and timestamp

Each event means: **DevScope successfully persisted this explicit Current Work
mutation at this timestamp.** It does not mean that work began, work ended in the
real world, or that an item is verified complete.

Use an RFC 3339 / ISO 8601 timestamp with an explicit local offset, for example
`2026-09-13T15:00:00+09:00`. It is readable in Windows-oriented local dogfooding,
unambiguous across daylight-saving transitions, and directly parseable later. The
implementation should obtain this value when DevScope confirms the write, not from
file modification time.

## Payload

Every entry should carry a format version, timestamp, and source-specific event kind.
For item-bearing events, store both the one-based display-order number and item text.
The number helps compact CLI output at the time of the event; the text preserves
meaning after reordering. Neither is a stable identity.

The event should also snapshot the Current Work Parent path and Task text so that a
history entry remains intelligible if the current file is later replaced. Active
events include prior and/or new item references as appropriate. `WorkCompleted`
includes `cleared_active: true` only when completion cleared that same item.

```json
{"version":1,"timestamp":"2026-09-13T15:00:00+09:00","kind":"active_set","parent":"docs/roadmap.md","task":"Progress history experiment","item":{"number":2,"text":"Implement history semantics"}}
{"version":1,"timestamp":"2026-09-13T16:00:00+09:00","kind":"work_completed","parent":"docs/roadmap.md","task":"Progress history experiment","item":{"number":2,"text":"Implement history semantics"},"cleared_active":true}
```

No UUID or persistent item ID is introduced for this experiment. Stable identity is a
separate requirement that should not be pulled into Current Work merely for history.

## Atomic completion

`work done` on the active item currently completes the checkbox and removes Active in
one Current Work file write. The history equivalent should be **one**
`WorkCompleted` event with `cleared_active: true`, not a synthetic pair of
`WorkCompleted` and `ActiveCleared` events.

One event faithfully represents one user command and one atomic mutation, avoids a
false ordering between simultaneous effects, and keeps a short timeline low-noise.
An explicit `work active clear` remains its own `ActiveCleared` event.

## Store comparison

| Store | Advantages | Limits | Decision |
| --- | --- | --- | --- |
| Plain text log | Easy to read | Ambiguous parsing and future fields | Reject |
| `current-work.jsonl` | Append-oriented, human-inspectable, parseable, source-specific | Needs line validation | Select |
| Generic `events.jsonl` | Looks extensible | Creates premature cross-source event pressure | Reject |
| SQLite | Queryable and transactional | Unjustified dependency and migration cost | Reject |

The proposed local-only path is `.devscope/history/current-work.jsonl`. JSON Lines
keeps each event independently inspectable and leaves room for malformed-line handling
without inventing a generic event store.

For the first experiment, retain entries with unlimited local append and no rotation.
Current Work writes are deliberately infrequent, so this is the smallest behavior to
dogfood. The experiment must observe file growth and noise; only then should a later
slice compare a latest-N policy or a size cap. It must not delete history
automatically in the first slice.

## Read surface

Prefer `devscope work history` over `devscope history work` or generic
`devscope history`. It follows the existing `work list`, `work active`, and `work
done` namespace and avoids claiming a cross-source history interface prematurely.

Initial output should be compact and read-only, for example:

```text
14:03 Active set      #2 Implement history semantics
14:21 Active changed  #2 -> #3
14:37 Completed       #3 Fix history rendering; Active cleared
14:45 Active cleared  #4 Pause for review
```

Timestamps may initially be rendered in local clock form for compactness while the
stored offset-bearing timestamp remains authoritative. This is not a TUI proposal.

## Relationship to freshness and other sources

History can later provide a fact such as “last Current Work mutation was 42 minutes
ago.” It is a judgment input only: age never automatically means stale, never clears
Active, and never emits a warning in the first experiment.

Plan history is out of scope because Markdown edits are often external and do not
share Current Work mutation semantics. Git already has its own history, so DevScope
does not duplicate it. Build/Test has latest-result persistence but no history in this
experiment; Artifact remains a current re-observation source with no history. Agent
telemetry, Handoff, Notes, and stop reasons remain separate candidates.

“Recorded history” is a useful narrow description: Current Work history records
explicit mutations to Recorded working state. It is neither Evidence nor an agent
report, and it does not create a new broad provenance taxonomy.

## Genericization boundary

Do not introduce `ProgressEvent`, `EventSource`, `EventBus`, a generic event store,
or a timeline abstraction for this slice. Reassess only after another concrete source
has independently demonstrated the same requirements for payload, retention,
timestamp interpretation, and read surface. Similar-looking timestamps alone are not
enough.

## Proposed dogfood and success criteria

The next implementation slice should add only the Current Work JSONL writer and
`devscope work history` reader, then dogfood this sequence:

1. Set Active for an item.
2. Change Active after an interruption.
3. Complete the active item.
4. Explicitly clear a later Active item.
5. Start a fresh session and read the compact history.
6. Judge whether it explains recent work and highlights potentially old Current Work
   without asserting that it is stale.

The experiment is successful when the recent flow is understandable on resumption,
entries remain low-noise and semantically explicit, timestamps help human judgment
without automatic inference, and no generic event model is needed. If the history is
not useful enough to justify its manual write points and local file, stop rather than
expanding it to Plan, Git, Evidence, or agent data.