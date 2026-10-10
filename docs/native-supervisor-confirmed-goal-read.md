# Confirmed project goal in a native Supervisor session

The restricted `business_os.jour_fixe_read` tool accepts
`{"action":"read_confirmed_goal","request":{}}`. The currently signed native
Supervisor lease selects the Owner, project and binding; callers cannot supply
another project, goal, actor, root, lease or command. An unconfirmed project
returns `confirmed_goal: null`, without creating or guessing a definition.

The result reads the accepted Core confirmation and its actual planned steps:
accepted items, goal revision/status, step statuses, saved result excerpts,
emission attempts, message keys and persisted timestamps. This uses the same
definition/progress reader as the next meeting deck. Saved partial evidence is
not converted into completed status. The ordinary MCP read is a read-only
DEFERRED snapshot; it does not initialize schemas, emit work or take a writer
reservation.

The selected-Luma Source advertises the additional tool only when its claim sets
`include_confirmed_goal_read: true`; older claims retain their one existing
descriptor. It then exposes the fixed native tool `confirmed_goal_read`.
Start with `tool_arguments_json: "{}"`. Each continuation copies the previous
`next_cursor` into `tool_arguments_json: JSON.stringify({cursor: next_cursor})`;
only optional `cursor` (at most 128 characters) is accepted. A new call may use a
fresh operation UUID while retaining the original offer and controller.

The result is a typed `ctox.workjet.supervisor.confirmed_goal_page.v1` page:
`state`, `project_id`, `supervisor_thread_id`, `snapshot_id`,
`document_sha256`, `document_bytes`, `byte_offset`, `byte_length`,
`json_fragment`, `captured_at_ms`, `document_complete`, and nullable
`next_cursor`. State `page` contributes at most 24,576 UTF-8 bytes. Join the
fragments in byte-offset order; verify each fragment length, the stable
snapshot/scope/hash, final document size and SHA256 before JSON parsing.
`document_complete` is snapshot framing EOF, not task, worker, SDK-turn or goal
completion. The whole original Source reply stays below its 65,536-byte bridge.

A snapshot retains the captured native step progress. A changed goal reference
or status returns `snapshot_changed`; a missing/expired snapshot returns
`snapshot_unavailable`. Discard partial fragments and start a bounded fresh
empty read under the same current Source. `capacity_unavailable` is explicit
capacity, not a reason to retire a valid Source or truncate a large goal. The
cache holds at most eight documents of at most 1,052,672 bytes (the existing
1-MiB native goal budget plus bounded wrapper). Only expired, completed or
invalidated snapshots may be displaced; expiry is the original offer deadline.

Every page revalidates the existing original controller, signed project binding
and DataRead policy and uses guarded publication alongside `worker_dispatch`.
It never re-enters a store/writer inside that reservation, manufactures an
execution controller, confirms Owner todos or creates a second plan/schedule.

The existing Core confirmed plan remains responsible for emission, retry and
completion; the registered Supervisor/worker Source remains responsible for
delegation. Harness must map the additional fixed descriptor to the matching
native Source operation, rather than a generic caller-selected MCP tool.

Regression fixtures exercise actual typed Owner confirmation, Core step
emission/lease, partial progress, reader reopen beside a writer, empty
definitions, changed leases and extra caller authority. These are component
tests; a full installed Molecularity cycle remains a separate acceptance.
