# Project Supervisor Luma binding

The project stores an optional `supervisor_luma_id`, an opaque identifier of an
instance-wide configured `workerProfiles[].id`. It is not an account identifier,
a computer identifier, a credential, or evidence that a provider executed a turn.
The native project writer retains its existing Owner identity, domain admission,
idempotency and projection checks. A caller cannot change another Owner's project
or inject a route/model through this field.

`ctox.workjet.project.upsert` uses the existing partial-update semantics:

- Omission preserves the stored selection; a new project remains unselected.
- A nonblank string (at most 160 characters) stores the reference.
- Explicit `null` removes it, restoring the existing instance-default choice.

The shared fixture is `src/core/rxdb/tests/fixtures/workjet-supervisor-luma-v1.json`.
`build_workjet_jour_fixe_contract.mjs` generates its native and browser validators.
The `workjet_projects` schema advances to version 3; every intermediate migration
preserves all existing fields without synthesizing a Luma selection. Module schema
files and native/browser schema hashes are generated together.

## Delivery boundary

This first slice stores the reference only. It deliberately does not change
`create_supervisor_ai_request`, queue admission, provider selection or narration.
A UI must not describe a stored reference as the model that actually executed.
The complete Supervisor route implementation remains a separate delivery; the
Owner's Monday 12 October 13:00 merge restriction applies to it.

The execution slice must resolve the selected profile and route from current
native instance configuration under project Owner policy, validate the real
account's current catalog, and retain configuration revision and logical
profile/route/account references in the admitted turn. Missing, stale, withdrawn,
unsupported or ambiguous selections must fail explicitly instead of silently
using the instance default. An absent selection retains today's execution path.

Selected and actual execution are distinct facts. Actual model, route, harness,
attempt and provider turn must come from the native producer's durable witness,
including any real fallback; neither UI labels nor model prose are evidence.
No secret or private account selector belongs in the project or turn projection.
The existing guarded execution observer is the transport for those facts.

Crew owns the native binding/execution contract; Main owns its Workjet setting
and actual-model display; Models owns live catalogs and route authority;
Instances owns remote harness dispatch. The configured profile includes a
computer today; choosing a different eligible network computer remains an
explicit native dispatch decision, not a client-supplied execution permission.
