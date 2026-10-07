# Workjet Jour fixe v1

This is the shared wire contract requested in the 2026-10-07 project target picture.
It enables the meeting UI and speech gateway work to proceed against the same
fixture. This change defines types and validation, not live command handlers,
persistence, a working meeting room, or installed product acceptance.

Source: `src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json`.
Generate both consumers with `node src/core/rxdb/tools/build_workjet_jour_fixe_contract.mjs`.
`--check` detects drift. Native and browser tests consume the same positive and
negative cases. Generated consumers must never be edited separately.

## Identity and persistence

A meeting belongs to one native `workjet_projects` record and its authenticated
owner. `SupervisorRef` contains the explicitly registered Workjet thread UUID
AND its verified CTOX execution `thread_key`. Titles, directories and arbitrary
client UUIDs cannot establish this binding. The current Main opening path only
proves the Workjet UUID; Harness supplies the normal execution binding.

`Meeting` is the bounded assembled read model. Native persistence stores meeting
metadata, slides, comments, final transcript turns and to-do proposals as separate
rows linked by `meeting_id`; projections expose bounded pages over CTOX DB/WebRTC.
The nested fixture is not permission to store an unbounded transcript in one RxDB
document. Each child and referenced goal/file must belong to the same authorized
project/meeting. Reads remain scoped through that native relationship. No business
data or audio bytes become an HTTP data bridge.

## Presentation and conversation

States: `planned -> preparing -> ready -> live -> review -> confirmed`.
Cancellation and a named preparation failure are explicit terminal alternatives.
Failure retains the prior goal. Preparation is a native CTOX schedule before the
configured meeting time (initial lead time two hours), using the project's IANA
zone and concrete supervisor binding. Reconciliation must be idempotent and stop
on archive or cleared `jour_fixe`. The schedule invokes the common JourFix skill.

Slides carry stable IDs and positions. A deck revision is immutable after
publication. `ready` requires complete slides and playable narration receipts,
not an optimistic UI label. Audio references the existing authorized CTOX file
store with byte hash, exact narration-text hash, format, model, run ID, duration
and measured synthesis time. There are no bearer tokens, base64 blobs or arbitrary
audio URLs in this contract. The player resolves these file IDs through the
existing permitted file/chunk path. Native publication validates receipt hashes.

Comments reference a slide and its exact deck revision; x/y are normalized 0..1.
The authenticated owner supplies feedback. Native storage stamps author identity
and emits one durable event to the bound supervisor; `supervisor_event_id` is the
receipt. Replacement decks cannot silently move or discard old comments.

The speech gateway supplies ordered `TranscriptEvent` values per stream.
Partials are transient and may update that stream's current display; only final
events become immutable `TranscriptTurn` rows. Duplicate stream/sequence or turn
IDs are idempotent; older sequences cannot overwrite final text. Store text and
speech from both the owner and supervisor. Speaker identity is checked against
authenticated origin, not accepted as authority from a payload. Preserve source
run IDs and measured `sentence_end_latency_ms`; batch realtime factor is not live
latency evidence. Models owns measured sentence-end-to-transcript latency <~1.5s.

## Commands and owner-confirmed goals

The fixture lists `ctox.workjet.jour_fixe.*` command names, request types and
required origin: owner, bound supervisor, or one of those. These are contracts
for subsequent native handlers, not already installed tools. They use the
existing typed Business OS command/MCP path and existing policy checks. Managed
clients need explicit bounded grants; this contract widens none.

Every mutation has an operation ID, meeting ID and expected meeting revision.
Retrying an operation returns its saved receipt; conflicting or stale revisions
fail before effects. Nested slide/turn/meeting IDs must match. Commands cannot
supply owner/confirming-user identity. `transcript.append` accepts only verified
final events for the matching speaker/stream.

`todos.propose` creates a versioned proposal with acceptance criteria and source
evidence IDs. `todos.revise` lets the owner edit it, producing a NEW proposed
revision. `todos.confirm` requires that exact proposal and the expected prior
goal revision. Confirmation derives the owner from authentication and commits
the confirmed list plus the supervisor's durable CTOX goal definition atomically.
It returns `GoalRef`. A stale confirmation cannot replace a newer goal. A proposed
list never changes goals or launches work. Confirming an empty list is an explicit
owner decision, not an inferred completion. The next preparation reads the
confirmed goal revision and reports progress, merged PRs, metrics and decisions
against it. Goal installation preserves normal continuation/review invariants.

## Installed acceptance still required

A real ctox.dev test appointment must prepare a deck automatically, play its
audio, deliver a clicked comment to that same supervisor, persist a spoken
sentence, and save the owner-confirmed list as the durable goal referenced by
the next preparation. Source fixtures and merges alone do not prove those steps.

## Project configuration and weekly-report schedule

`ctox.workjet.project.upsert` also accepts `info.summary` (bounded to 4096
characters); legacy `info.description` and `info.status` remain compatible.
Missing fields preserve values, explicit null clears the whole info/appointment.
The native and browser `workjet_projects` v2 schemas and complete identity
migration chain retain prior ownership, revisions, deleted rows and configuration.
Both sides consume the project configuration corpus next to the wire fixtures.

Before a native schedule tick, committed active projects with a registered
supervisor UUID and an active, permitted native owner are reconciled into one
weekly schedule each. The configured IANA zone and local appointment time feed
the existing calendar. The native `supervisor.turn.submit` command invokes the
existing Threads producer, so the report is a durable task/message in that same
supervisor chat. An occurrence ID is stable across retries, including a crash
between command acceptance and the schedule receipt. No browser timer, guessed
thread identity, new provider grant or external effect is introduced.

Unchanged deadlines and operator pauses survive reconciliation. Archive, deletion,
cleared appointment or lost native binding/owner authority disables the schedule.
Restoring configuration does not silently reverse that pause; the explicit native
`ctox schedule resume --task-id <id>` command re-enables it. Dispatch rechecks the
current project, registered thread, appointment and owner policy. Historical runs
remain retained. The test path is scoped to one task:

```sh
ctox schedule tick --task-id <id> --at <RFC3339-test-time>
```

Use an isolated test root; this is an operator action and actually admits the
report when due. Omitting `--task-id` with a test time is rejected. It uses the
normal native command/receipt/queue path, without running the model in tests.
This slice supplies the requested weekly report at the appointment. T−2 h deck
preparation, the common JourFix skill/tools, meeting persistence and confirmed-goal
installation remain the meeting-runtime follow-up described above.
