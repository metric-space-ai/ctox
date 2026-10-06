# THESEN Outbound source

Opt-in customer module; this directory is not part of the standard module catalog.
The production writer installs these files into the existing THESEN local module
only after the THESEN runtime upgrade signal. This is not a replacement for the
unrelated built-in `modules/outbound` app.

Baseline: installed Outbound 1.0.291, captured 2026-10-06; original index.js
SHA256 f5b52b8b97cbb596c24ac29eef81dfd1a905dfd63c93d83dde407a2861d70465.
All nine source files were compared to the typed native file manifest before
editing. No tenant records or credential values are included.

1.0.292 uses collection invalidation subscriptions (Shell v454 API) and scoped,
serialized reloads. Lead synchronization is demand-only; ordinary lead reads
remain complete and paginated, so detail, provenance and actions retain their
existing contracts. Slim projected lists and loading only changed lead IDs are
still pending the runtime projection contract (#277) and a separate App change;
this release does not claim the <1 MB startup goal.

Michael/Claude explicitly authorized direct implementation in a normal CTOX PR
for this performance repair because the embedded Pi model route is unavailable.
This narrow exception does not authorize an alternate Business OS data plane.

Required regression: `node src/apps/business-os/scripts/tests/outbound-scoped-reload.test.mjs`.
Run through the Linux GPU lane; real browser load/transfer acceptance follows
installation on the identified THESEN runtime. No production acceptance yet.
