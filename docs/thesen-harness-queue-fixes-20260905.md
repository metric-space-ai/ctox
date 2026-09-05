# THESEN Queue/Harness acceptance — 2026-09-05

Customer evidence: `thesen-outbound-fehlerprotokoll-20260904.md`, sections 8–11,
including the operator's corrected canonical/projection distinction on September 5.
No customer instance was accessed or changed for this work.

## Befund 1 — withdrawn as a customer incident

The operator's final ID join establishes that all seven alleged lease corpses
are cancelled routing rows, with stale running projections. Befund 1 is
withdrawn. No further lease-sweep changes are part of this follow-up.

The current tree already issues 15-minute leases, renews them every 60 seconds,
and reclaims expired/incomplete leases at boot and in mission maintenance.
However, `run_orphaned_queue_lease_sweep` treated every cached inflight key as
live whenever the global `busy` flag was true. An unrelated active worker
therefore protected an expired orphan from recovery indefinitely. This is a
reproduced independent code defect, not the cause of the customer's seven old
`running` documents. Its existing regression tests and fix remain in place.
The measured incident is now conclusively assigned to Befund 4.

Only `PromptWorkerActivity` now registers and unregisters live worker lease
keys. The sweep protects those and explicitly buffered prompts, rather than
all cached inflight keys. TTL, heartbeat and transactional recovery remain the
existing authoritative mechanisms.

Acceptance tests:
- `incident_lease_heartbeat_preserves_live_work_then_expiry_requeues_it`
- `incident_sweep_reclaims_old_leases_without_expiry_or_worker_id`
- `incident_boot_reclaims_seven_expired_leases_from_previous_workers`
- `incident_sweep_reclaims_orphans_while_an_unrelated_worker_is_busy`

These tests use real isolated SQLite stores. The missing-expiry test injects
both SQL NULL and an empty string, with old leased_at and NULL lease_worker_id,
through a raw SQLite connection immediately before the real sweep. It verifies
pending state, cleared ownership, and idempotent recovery. The seven-row boot
test is a synthetic batch, now including both missing-expiry forms, and checks
that a second boot does not duplicate tasks. The heartbeat test also checks
that the previous owner cannot renew a released row.

### September 5 follow-up: actual origin/main lease fields

Verified against origin/main at 48c351c48:
- Both lease_queue_task and lease_messages write lease_expires_at = now +
  15 minutes in the admission transaction; lease_worker_id initially is NULL.
- PromptWorkerActivity::start calls record_queue_lease_worker after admission.
  This is a separate, best-effort write: an error is logged and execution
  continues. Its heartbeat renews lease_expires_at every 60 seconds.
- open_channel_db runs the routing schema backfill: for a leased row with
  NULL expiry it derives datetime(leased_at, '+15 minutes'). It does not give
  old work a fresh TTL from the time the store opens.
- release_stale_queue_task_leases selects expired leases and explicitly
  selects NULL/blank expiry (also incomplete owner/leased_at). Active worker
  keys and buffered prompts remain protected. Thus absence of an expiry
  alone does not prevent recovery on this source revision.

The operator subsequently confirmed a live canonical lease with expiry and
worker identity. The claim that the tenant never writes these fields is
withdrawn: the event-JSON count was a measurement error. The 18-minute gap
between leased_at and the observed expiry is compatible with heartbeat renewal
of a 15-minute lease; it does not establish an 18-minute base TTL.
Process-event JSON must distinguish an absent key from an
explicit JSON null: json_type(row_before_json, '$.lease_expires_at') returns
SQL NULL for absence and the string 'null' for explicit null. Event triggers
capture the table columns present when installed (process_mining.rs,
install_table_triggers/build_trigger_sql); the event log alone is not a direct
read of today's canonical row. No customer measurement is inferred here.

Follow-up developer validation: cargo test incident_ -- --test-threads=1
passes all 12 incident tests, including the NULL/blank sweep and boot cases;
cargo check passes. cargo fmt --check still reports the documented unrelated
baseline differences; the added test's formatting is corrected.

Baseline: root `cargo check` passes. Root `cargo fmt --check` already fails on
unrelated formatting in the supplied checkout (including pre-existing service,
context, projection and execution files); those changes are not reformatted by
this work. Each slice retains only formatting changes belonging to its edits.

## Befund 2 — bounded scrape repair admission

The scrape failure handler previously called generic queue creation for every
failed probe. Its prompt includes the run identity, so generic request hashing
does not deduplicate a target across runs. Admission now holds an IMMEDIATE
SQLite transaction and reuses any open repair with the same target type/key,
including legacy metadata without target_type. A running repair's own failing
probe therefore returns its current lease instead of creating its successor.

After failure, at most three attempts are admitted, delayed by 5 and 10 minutes.
Further requests return the final failed task with an explicit exhausted-budget
reason. Operator cancellation remains stopped. Successful repair resets the
consecutive-failure budget.

Both real-store tests pass: three distinct submissions produce one open leased
task, and three failures enforce persisted cooldowns plus terminal exhaustion.
Tied timestamps are ordered by persisted attempt number.

## Befund 3 — explicit global serialization

durable_queue_dispatch_blocked_locked and enqueue_prompt deliberately admit no
second durable task while any worker is active. Historical eight running rows
are not evidence of eight live execution slots.

A bounded pool now admits independent business_os.chat.task jobs with explicit
thread identity and isolated harness sessions. It reserves slots before spawning
and preserves one worker per thread. App authoring, external communications,
tickets and shared sessions retain the existing serial policy. Expired startup
reservations are removed by the normal lease sweep.

Typed QueueWorkerCapacity reads queue.worker_capacity solely from the SQLite
runtime store: default 4, supported range 1–8. Operators inspect/change it with
ctox queue capacity [--workers N]; 1 restores serial admission.

The acceptance test creates five independent pending tasks, observes four actual
leased rows and one pending row, and proves a second admission cannot overbook
the pool before worker startup. Each admitted job uses an isolated session.

The original guard is durable_queue_dispatch_blocked_locked in
src/core/service/service.rs (origin/main 9b3f44e09:16379). It checks busy,
app_recovery_active, durable_queue_lease_in_progress and worker_active_count.
It prevents overlapping work in the legacy shared execution/session state and
competing recovery/admission. The bounded pool in service_queue_capacity.rs
retains those exclusion rules for shared work, but admits independently isolated
Business OS research conversations concurrently. Default 4 is intentional;
there is no need to retain default 1 for those isolated conversations. One
thread still has one worker, preserving its ordered context. The operator's
nine pending rows have no retry/dependency holds, consistent with the measured
global serialization. These settings were already landed in 62cc1fd7a.

## Befund 4 — queue CLI projection registration

Queue projection hooks were registered by open_store only. A fresh queue CLI
process could cancel a lease without opening that store, silently leaving its
materialized native RxDB document running. The CLI entry now registers the same
hooks before dispatch; cancellation retains the existing transactional projection.

A subprocess test starts without any open_store call, executes cancel through
the CLI entry, then verifies canonical cancelled state and the native
ctox_queue_tasks document, including a new matching document/column revision.

The RxDB Rust suite passes. The initial root JS run had three baseline guard failures:
two inventory differences absent in the clean origin clone, and an identifier
guard that already fails in six active files on origin/main. No guard was
weakened. No browser replication source was changed by this slice.

### Befund 4 follow-up — stale native-only queue projections

The normal business_command_queue_task_payload and queue_task_payload writers
store QueueTaskView.message_key as the document id. They historically omit a
separate message_key property. Its absence alone does not establish an orphan
or a ticket_self_work_items origin. The final customer ID join proves that all
seven running documents have canonical cancelled rows, acknowledged August
28–31. They are historical cancel/projection mismatches, not missing queue rows.
Newly written queue payloads also include message_key for explicit traceability.

Confirmed IDs and canonical acknowledgement times (UTC):

| Queue ID suffix (`queue:system::`) | Canonical acked_at |
|---|---|
| 5cb6be0045dbced617666180 | 2026-08-28 09:15 |
| a39b9a3f0114ca53298822d3 | 2026-08-28 09:50 |
| 874c7789b4b239a43f296542 | 2026-08-28 10:09 |
| 69ed8448d83beed58b28959a | 2026-08-30 15:12 |
| aa0ab1ab313716ff9d2a8bab | 2026-08-30 18:47 |
| b12733bd9ea7a1c3495ed1b6 | 2026-08-31 08:18 |
| 880197035f207a79bd41ac71 | 2026-08-31 08:18 |

Their projections retained status=running, route_status=leased,
task_status=running and lease_owner=ctox-service. The reconciler must preserve
the canonical cancelled outcome rather than relabel these as failed or requeue
them. A dedicated seven-row test now reproduces precisely that mismatch.
It includes freshly rewritten running documents and a lingering worker key:
neither may override the known canonical cancelled state. Terminal/pending
canonical outcomes are projected immediately; only missing/expired sources
wait for the orphan projection TTL.

The existing repair_queue_projections CLI reads only business_records and is
not an automatic daemon reconciler. Native-only documents can therefore survive
there indefinitely; the mutation hook also skips queue rows with no matching
business_records mirror. The new reconciler scans stale running/leased
projections in both stores at boot and on the 60-second lease maintenance tick.
It uses the existing ten-minute projection recovery TTL, without an environment
toggle. It resolves id/message_key/task_id and business_command_task_links,
protects unexpired canonical leases and current/buffered worker keys, and
projects pending/terminal canonical states. Missing sources or expired leases
produce failed with an explicit repair/error reason. Canonical queue and command
rows are not mutated by this projection-only pass.

Core-state inspection and both projection writes share one attached IMMEDIATE
transaction, fencing concurrent lease admission and avoiding partial repairs.
Revisions and native last-write metadata advance; terminal native documents
and tombstones are preserved, and fresh orphans retain their grace period.
Tests cover seven native-only orphans, a mirror-only
orphan, live leases whose documents lack message_key, command-link resolution,
canonical pending/cancelled outcomes, terminal command outcomes, current-worker
protection, rollback on native write failure, and the actual boot/maintenance
entry points. No browser runtime source changes are required.

Developer verification of this follow-up: all 16 incident tests and all seven
existing store_projections tests pass, including the historical repair guard.
The native RxDB manifest suite passes serially (368 unit, 31 conformance,
one error-contract guard and one idle-budget test). The divergent checkout's
JS suite retains its three baseline inventory failures (110 pass); these
assertions are unchanged. The clean origin clone remains the release check.

Clean-clone follow-up verification also passes: 16 incident tests, seven
existing projection tests, all 113 JS tests, and the serial native RxDB suite
(366 unit + 31 conformance + two guards). cargo check passes. The new tests
use the repository's Rust 2021 formatting; the unrelated Office/LCM formatting
baseline remains outside this slice.

### Final acceptance clarification: closed-store queue mutations

The strengthened fixture exposed an additional merge defect:
enrich_queue_projection_payload removed absent lease_owner/leased_at/acked_at
from its new payload, while upsert_attached_rxdb_record merges into the old
document. Removed fields therefore survived in native RxDB. The projector now
writes explicit nulls for cleared values. Cancel/Fail tests seed an old owner
and lease timestamp and verify they disappear, with acked_at matching the
canonical row. Existing status and completion guards are retained.

The queue CLI registers its projection hooks before dispatch, independently of
open_store. The fresh-process acceptance now covers cancel and fail, then
reopens the Business OS store and checks both its mirror and native RxDB
status/revision. Cancellation also checks the cleared lease owner and persisted
acknowledgement. An unreviewed complete request must fail the existing durable
completion guard and leave the running projection/revision unchanged; the test
checks the guard error, absence of an accepted completion proof and unchanged
state rather than granting an unverified success. The rejected transaction also
rolls back its attempted proof insert.
The existing canonical_queue_ack_refreshes_queue_and_command_without_repair
test separately covers projection from the native failed-ack path.
Automatic historical reconciliation
is already wired at daemon boot and every 60 seconds; it does not require an
operator repair command or a database edit. There is deliberately no recursive
reconciliation call from open_store itself.

### Process-event JSON diagnostic, recorded separately

The operator confirms lease_expires_at and lease_worker_id are absent from the
historical routing event row_after JSON despite populated canonical columns.
process_mining.rs::install_table_triggers/build_trigger_sql capture the column
list at installation. table_triggers_current checks the process schema version
and existence of the three trigger names, not whether table columns changed.
This permits old trigger JSON shapes after additive schema changes. The
historical event count is not a measurement of lease field population. A
column-aware trigger refresh requires a separate migration/recorder regression
check; it is recorded here without changing routing or event capture in this
queue-projection acceptance slice.

## Befund 5 — typed writeback capability and terminal completion evidence

The harness enabled signed Business OS MCP sessions only for nonempty
allowed_actions. The app's mechanism=business_command contract has command_type,
collection and record_ids instead, and was therefore ignored. The semantic-only
review shortcut also treated that contract as requiring no action.

The harness now recognizes the native research writeback contract and exposes
business_os.execute_writeback. It requires a signed internal command session,
revalidates the originating actor, checks server DataWrite/module/collection
policy, and binds record_id, module and research_command_id from the persisted
contract. Cross-record, cross-parent and gap-task substitution are rejected.
The actual write runs through the existing native business command dispatcher;
a build without that handler fails rather than recursively queueing work.
Identical payloads reuse the same native command identity.

Completion requires a completed native writeback receipt for every contracted
record and the exact originating research command. Missing/failed/foreign
receipts yield TerminalQueueFailure, including when the worker claims success
after a sandbox-blocked CLI or SQLite writeback. The semantic-only shortcut is
disabled for business_command writeback contracts. The prompt names the tool
and requires retaining research artifacts on failure.

The developer checkout lacks the research_writeback native handler already
present on origin/main; the clean clone is authoritative for that integration.
Local tests cover command-only session activation, cross-scope rejection,
failed and foreign receipts, and terminal rejection of the observed false
success. Clean-clone validation additionally exercises native dispatch.

### Initial replication pending — cause not established

awaitInitialReplication returns initialReplication. Peer acceptance chains a
pull drain followed by a push drain, then resolves the original deferred.
This is a barrier for both directions; evidence that documents flow alone does
not establish which drain or peer negotiation is outstanding. Existing
checkpoint/reconnect/catchup smoke tests do not reproduce the customer's
five-collection permanent pending state. There is no captured outstanding
request/ack/checkpoint trace from that run in the incident protocol.

No replication implementation change is made without that evidence. In
particular, readiness is not forced true or resolved on the first document.
The next useful evidence is the affected pool's negotiated peers, pull/push
in-progress state, pending request IDs and matching acknowledgements at the
time awaitInitialReplication remains pending.

### Clean-clone validation update

origin/main advanced independently with a3f3e90c5, removing the pre-existing
customer identifiers from active source comments. The landing clone was rebased
onto that update before further pushes. With browser test dependencies present,
the full JS/RxDB suite passes: 113 passed, 0 failed, 0 skipped.

The clean-clone RxDB unit suite initially hit a global db_count assertion in
parallel execution (365 passed, 1 failed). Its serial run passes all 366 unit
tests. No RxDB implementation or guard assertion was changed for this work.

The required clean-clone cargo test --manifest-path src/core/rxdb/Cargo.toml
-- --test-threads=1 also passes: 366 unit, 31 conformance, 1 error-contract
guard and 1 idle-budget test. The 17 existing native research/gap-writeback
tests pass, including completed lead persistence and correlation rejection.

Broader baseline checks are not fully green. Before the writeback slice, the
clean origin clone's 100-test MCP module suite already has 14 failures (86 pass):
missing customers/outbound/support/mcp-inventory fixture modules and related
permission/expected-validation differences. The divergent developer checkout
has one additional existing person-research idempotency failure.

cargo fmt --check was run on both trees. Pre-existing formatting differences
remain in clean-clone office_engine.rs and context/lcm/{mod.rs,tests.rs}.
Formatting introduced by these queue/writeback slices is corrected. These
unrelated baseline failures are not represented as a green overall release
gate and their assertions are not changed or bypassed.
