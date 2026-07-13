# Monitoring a CTOX pilot

What to watch while running CTOX as an isolated pilot, using only signals the
product itself exposes. Every signal below is read from `ctox status` (JSON on
stdout) or from files under the instance root — no external watchdog is
required for these checks.

## The five signals

### 1. Empty or missing replies

`ctox chat --wait` judges completion per conversation against the durable
assistant outcome. An empty reply body or a structured failure outcome exits
**non-zero** with a typed error; exit code `0` with silent zero output is a
bug, not an operating mode. For scripted use, treat any non-zero exit as a
failed turn and inspect:

- `last_error` in `ctox status`
- `last_agent_outcome` in `ctox status`

### 2. Event-stream health

`ctox status` reports process-lifetime event-stream counters under
`performance.event_stream` (schema `ctox.service.event_stream.v1`):

| Counter | Healthy | Meaning when it grows |
|---|---|---|
| `events_dropped`, `facade_events_dropped` | may grow slowly | Droppable UI events discarded under backpressure; terminal events are never dropped. Sudden growth means a consumer stopped reading. |
| `delivery_events_buffered` / `delivery_events_flushed` | grow together | Turn-completion events parked while a consumer paused, then delivered. A widening gap between the two means a consumer is not catching up. |
| `consumers_gone`, `facade_consumers_gone` | small | Event consumers that disconnected or were declared wedged. Correlates with client kills/restarts. |
| `runaway_terminations` | **0** | A session was force-failed because its delivery buffer ran away. Any non-zero value is worth a look at the service log. |
| `facade_lag_markers` | small | Consumers were told they missed droppable events. |

A wedged event stream no longer requires a service restart: sessions with an
unrecoverable stream fail explicitly and the service rebuilds them on the
next turn. The counters exist so you can see how often that happens.

### 3. Restarts

`performance.process` carries `pid`, `boot_id` and `started_at`. A changed
`boot_id` between two probes means the daemon restarted in between. If you
did not restart it, check the service log before anything else.

### 4. Database growth

Durable state lives in SQLite files under `<root>/runtime/`:

- `ctox.sqlite3` — conversations, queue, evidence
- `business-os.sqlite3` and `business-os-rxdb.sqlite3` — Business OS records
  and sync state
- `ctox-runtime.sqlite3` — runtime configuration

Track their sizes (plus `-wal` siblings) over days, not minutes. Steady
growth proportional to real work is expected; runaway growth while idle is
not.

### 5. Failed or stuck jobs

From `ctox status`:

- `pending_count` / `blocked_count` — queue depth and blocked tasks; a
  `blocked_count` that never drains needs attention
- `busy` plus `worker_phase` — what the worker is doing right now
- `last_completed_at` — the last finished job; if it stops advancing while
  `pending_count` is non-zero, the queue is stuck

## Suggested pilot cadence

Poll `ctox status` once a minute and alert on: non-zero
`runaway_terminations`, a `boot_id` change, `blocked_count > 0` for more
than an hour, or `last_error` staying non-null across probes. Everything
else above is for weekly review.
