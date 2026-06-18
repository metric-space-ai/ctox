# E-Signatur — implementation plan

Generische Signatur-Anfragen: Dokument an Unterzeichner routen und Status verfolgen.

## Status

- Collection(s): `signature_requests` — activated + verified (browser run-all.mjs + native parity).
- Engine: `src/apps/business-os/modules/esign/core/` — pure, unit-tested.
- UI: functional create-form + record list wired to the engine core.
- Server-authoritative gates (where applicable): native `ats_gates` via `ats.*.check` commands.

## Remaining (live-instance)

Browser round-trip verification (persist/render/gate-decision display) and board/detail polish, against a running CTOX stack.
