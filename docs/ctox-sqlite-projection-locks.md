# SQLite projection writer boundaries

The 2026-10-09 THESEN journal finding reports a 14.4-second cockpit pass and
106 busy/locked deferrals over six hours with eight workers. A pass duration
is not a measurement of a held SQLite writer lock. The current pump has no
transaction spanning `refresh_measured_with`; source records and RxDB mirrors
are written independently. Queue admission and acknowledgements intentionally
retain Core and attached canonical projection writes in one transaction.

Crew retention previously read orphan candidates inside a deferred transaction,
then promoted it to a writer. A concurrent WAL commit between those operations
can cause SQLITE_BUSY_SNAPSHOT immediately, regardless of busy_timeout.
Retention now discovers candidates outside a transaction and reserves an
IMMEDIATE writer separately for each orphan. It rechecks the attempt identity,
start/finalization state, current lease and durable finalization before deleting
the selection event, tombstone outbox, attempt and assignment together.
A candidate started or leased in the meantime survives. Empty maintenance is
read-only: SQLite reserves its sole writer even for an UPDATE or DELETE that
matches zero rows. Finalization evidence and finished-retention candidates are
therefore discovered outside a transaction, and current guards are rechecked
under one short IMMEDIATE reservation per candidate. Reopened routes and rows
that entered the newest-500 window survive. The existing limits
(128 orphans and 128 completed rows; newest 500 completed rows retained) remain.

Batch queue admission previously reserved IMMEDIATE before ranking all pending
messages with window functions. Ranking now happens in a released read snapshot.
The bounded selected rows are reloaded by primary key under IMMEDIATE, checking
current inbound direction, message/channel/account/thread/remote identity, pending
status, retry_not_before and metadata not_before. Concurrent leases, deferrals or
identity changes are skipped without transition proofs. Updated payloads are
returned from the current row. Ranking remains a candidate snapshot; newly arrived
messages participate in the next pass. Lease changes, transition proofs and
attached canonical projection writes still commit together.

Finalization parses/validates bounded retrospective metadata before reserving
the writer. Its finalized_at guard, statistics, learning state and commit
remain atomic. Single-record writes keep their existing boundaries. Cold event
delivery prepares payloads and deduplication outside the writer, then commits at
most 64 source records and 64 mirror records per separate transaction, using the
same per-row merge/envelope functions. Source commit precedes mirror delivery;
notifications follow mirror commit. Batch replication clocks are reserved from the
persisted collection high-water mark under that mirror's IMMEDIATE transaction,
including when another retained writer has advanced it since this cache opened.
Each row is strictly later than the earlier feed cursor; rollback publishes no
reservation. A failed mirror chunk rolls back together,
retains completed chunks in the dedupe cache and restores the unclaimed replay
cursor so its unpublished rows stay eligible. No writer reservation spans a full
pass or both independently delivered stores. Repeated source upserts and mirror
lookups/upserts reuse the connection's bounded prepared-statement cache. The
SQL, bound values, secret redaction and per-execution canonical version guards
remain unchanged; caching does not reuse a policy or write decision.

Cold event replay also looks up the plan at each event's emission. A partial
Core index on task/time/event ID for worker.plan_updated skips unrelated
phase/tool history, including tasks with no plan. The query and its attempt
filter are unchanged; the query-plan regression requires the ordered index.

## Writer diagnostics

Slow native transactions emit `[ctox sqlite writer]` after their own lock
has been released. The fields are operation, primary database path, wait_us,
hold_us, outcome and (for failed acquisition) SQLite error code. No SQL,
document content or credentials are logged. The fixed threshold is 50 ms.

Operations covered: queue.lease_task, queue.lease_batch, queue.ack_attempt,
queue.ack_messages,
crew.retention_orphan, crew.retention_start_evidence, crew.retention_finished,
crew.finalize_attempt, projection.source_upsert,
projection.source_tombstone, projection.rxdb_upsert and
projection.rxdb_tombstone, projection.source_batch and projection.rxdb_batch.
Queue operations may reserve attached projection
databases as well as the reported primary Core database. These labels distinguish
queue ownership, Crew accounting and source/mirror delivery; other transaction
paths are not claimed to be traced.

Cockpit phase logs retain total wall time, including reads and waiting.
Correlate them with writer diagnostics; never equate total_us with hold_us.
The cockpit's existing 100-ms timeout remains a lossy delivery budget.
PR462's independent busy/locked retry change remains its owner's delivery;
this change does not suppress lock errors, drop pending authority or weaken
owner/receipt guards.

## Isolated load validation

The deterministic retention regression commits a competing start and lease
after candidate discovery and verifies both attempts survive and the writer
is released. The short native load regression uses eight SQLite writers,
the actual projection/source/mirror functions, WAL and a populated fixture.

For an explicit one-hour run through the gpu build lane:

```sh
ctox-prep.sh
cargo test --bin ctox cockpit_projection_one_hour_eight_native_writers -- \
  --ignored --nocapture --test-threads=1
```

The test reports CTOX_SQLITE_LOAD with duration, commits per writer,
projection passes, warm-up duration, maximum measured pass and every failure.
It must finish a full hour, have writes from all eight workers, have zero
errors and keep both the populated cold projection and every measured
steady-state pass below one second. Cold timing is reported separately. It uses
no model accounts or external effects.

This is an isolated native fixture, not installed-service or customer acceptance.
An installed proof must identify binary/source revisions, isolated prefix,
host, workload and the complete one-hour measurement. The original THESEN
lock holder and the installed criterion remain unproven until those measurements
are available; a green fixture or source change alone does not establish them.
