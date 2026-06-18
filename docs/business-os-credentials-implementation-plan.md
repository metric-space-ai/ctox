# Nachweise — implementation plan

Generischer Nachweis-/Zertifikats-Tresor: ablaufende, verifizierte Artefakte (Zertifikate, Lizenzen, Arbeitserlaubnis) je Subjekt mit Ablauf-Warnung und Einsatz-Gate.

## Status

- Collection(s): `business_credentials` — activated + verified (browser run-all.mjs + native parity).
- Engine: `src/apps/business-os/modules/credentials/core/` — pure, unit-tested.
- UI: functional create-form + record list wired to the engine core.
- Server-authoritative gates (where applicable): native `ats_gates` via `ats.*.check` commands.

## Remaining (live-instance)

Browser round-trip verification (persist/render/gate-decision display) and board/detail polish, against a running CTOX stack.
