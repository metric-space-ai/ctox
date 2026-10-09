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
A candidate started or leased in the meantime survives. The existing limits
(128 orphans and 128 completed rows; newest 500 completed rows retained) remain.

Finalization parses/validates bounded retrospective metadata before reserving
the writer. Its finalized_at guard, statistics, learning state and commit
remain atomic. Source and RxDB projection writers reserve one record at a time;
source commit precedes mirror delivery, and notifications follow mirror commit.
No writer reservation spans a full pass or both independently delivered stores.

## Writer diagnostics

Slow native transactions emit `[ctox sqlite writer]` after their own lock
has been released. The fields are operation, primary database path, wait_us,
hold_us, outcome and (for failed acquisition) SQLite error code. No SQL,
document content or credentials are logged. The fixed threshold is 50 ms.

Operations covered: queue.lease_task, queue.ack_attempt, queue.ack_messages,
crew.retention_orphan, crew.finalize_attempt, projection.source_upsert,
projection.source_tombstone, projection.rxdb_upsert and
projection.rxdb_tombstone. Queue operations may reserve attached projection
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
errors and keep every measured steady-state pass below one second.
Warm-up is reported separately. It uses no model accounts or external effects.

This is an isolated native fixture, not installed-service or customer acceptance.
An installed proof must identify binary/source revisions, isolated prefix,
host, workload and the complete one-hour measurement. The original THESEN
lock holder and the installed criterion remain unproven until those measurements
are available; a green fixture or source change alone does not establish them.
