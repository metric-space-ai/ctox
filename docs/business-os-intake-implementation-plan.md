# Bewerbungseingang — implementation plan

Generischer Mehrkanal-Eingang: normalisierte Bewerbungen aus Karriereseite/Jobbörse/E-Mail/QR.

## Status

- Collection(s): `applications` — activated + verified (browser run-all.mjs + native parity).
- Engine: `src/apps/business-os/modules/intake/core/` — pure, unit-tested.
- UI: functional create-form + record list wired to the engine core.
- Server-authoritative gates (where applicable): native `ats_gates` via `ats.*.check` commands.

## Remaining (live-instance)

Browser round-trip verification (persist/render/gate-decision display) and board/detail polish, against a running CTOX stack.
