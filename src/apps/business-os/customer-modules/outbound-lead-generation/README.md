# THESEN Outbound source

Opt-in customer module; this directory is not part of the standard module catalog.
The production writer installs these files into the existing THESEN local module
only after the THESEN paired runtime upgrade signal. This is not a replacement
for the unrelated built-in `modules/outbound` app.

Baseline: installed Outbound 1.0.291, captured 2026-10-06; original index.js
SHA256 f5b52b8b97cbb596c24ac29eef81dfd1a905dfd63c93d83dde407a2861d70465.
All nine source files were compared to the typed native file manifest before
editing. No tenant records or credential values are included.

1.0.292 uses invalidation-only subscriptions and scoped serialized reloads.
Lead synchronization is demand-only. Lead invalidation reads a paginated id/rev
manifest, then unprojected full records only for changed/new IDs. Missing IDs
are removed; equal or future timestamps do not decide membership. Full records
retain the existing detail/provenance/action contracts. Projected metadata never
becomes a complete Lead or a mutation payload. Strict read tokens fence the
actual bridge generation; unsupported projection fails without a full retry.
Concurrent document-revision changes are rechecked once, then reported as an
error with the existing bounded retry. A replaced App binding refetches full
records rather than reusing its old cache.

Cold startup still hydrates every Lead once. A slim list with lazy detail and
explicit full-data action hydration remains outstanding. This source change
does not claim the <1 MB startup goal or production acceptance.

Runtime dependencies: main PR277's confirmed projection/strict hydration API,
and collection.$.subscribe(callback,{invalidateOnly:true}). The latter exists
in THESEN's live sync-invalidate-v454 overlay but was absent from the merged
query-projection-v454 source. The Shell owner has that concrete compatibility
handoff. Deployment requires the actual paired runtime to support both APIs.

Michael/Claude explicitly authorized direct implementation in a normal CTOX PR
for this performance repair because the embedded Pi model route is unavailable.
This narrow exception does not authorize an alternate Business OS data plane.

Required regressions (through the Linux GPU lane):
- node src/apps/business-os/scripts/tests/outbound-scoped-reload.test.mjs
- node src/apps/business-os/scripts/tests/outbound-lead-revision-loader.test.mjs

Real browser load/transfer, detail, edit, export, live updates and reconnect
acceptance follow installation on the identified THESEN runtime.
