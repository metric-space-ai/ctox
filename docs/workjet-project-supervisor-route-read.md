# Configured Supervisor route read

The additive Owner command `ctox.workjet.project.supervisor.route.read.v1` accepts only
`{project_id, thread_id, inbound_channel?}`. It returns the separately versioned
`ctox.workjet.supervisor.route-display.v1` fixture; existing turn/capabilities/history
responses stay byte-shape compatible. Use the Business OS command/RxDB path, never an HTTP data bridge.

`configured` is null for the original instance-default behavior. A selected Luma is resolved
against the current native instance configuration, Owner/computer binding and authoritative live account
catalog. Only Luma/configuration revision, harness, route ID, model, computer ID and catalog observation
time are public. No account ID, holder ID, local provider selector, credentials or full configuration is exported.

`source` is null unless the most recent persisted requested route for this exact Owner/project/UUID
matches the current selection, native account revision and catalog snapshot. Its execution key and SHA256
request revision identify an immutable request, with persisted error code/time. This does not authorize execution.
`actual` is always null in this implementation; neither a configured model nor an unverified `actual_json`
column is a producer receipt. The fixture reserves a strict future actual-producer object requiring run, turn
and receipt identities; no such object is emitted until a genuine holding executor can prove it.

Policy requires DataRead plus native project Owner and the exact retained Supervisor thread. Foreign users,
projects and threads are denied. Reads use deferred snapshots and read-only Core connection flags; they never
create the attempts table or take an issuer/writer fence. A selection change suppresses the old request source.

The native/browser types are regenerated together from
`src/core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json` using
`node src/core/rxdb/tools/build_workjet_jour_fixe_contract.mjs`.
This slice does not execute a configured Luma, change the no-selection Monday path or establish installed acceptance.
