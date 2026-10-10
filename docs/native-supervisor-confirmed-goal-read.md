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
descriptor. It then exposes the fixed native tool
`confirmed_goal_read` with empty arguments. It uses the existing original
controller reservation and guarded publication, alongside `worker_dispatch`.
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
