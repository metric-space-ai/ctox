# CI per line: V2 checks on `release/v2`, v4 counterparts everywhere else (E37)

Decision E37: no check is switched off. Every workflow picks the line with
`${{ github.base_ref || github.ref_name }}`. On `release/v2` the V2 steps run unchanged;
on every other line (main, v4/*, PRs against them) the v4 counterpart of the same step runs.
V2 test files stay byte-identical. A v4 counterpart checks the SAME invariant against the v4
source; an invariant without meaning in v4 is listed in the script header with a reason.

Measured baseline (2026-10-16, branch `shell-v4/followup-1`, V2 scripts run unchanged):

| V2 script | Result on this branch | First cause |
|---|---|---|
| `assert-rxdb-only.mjs` | crash ENOENT | `modules/desktop/schema.js` deleted by the port |
| `assert-module-conformance.mjs` | red | every ported module: no `schema.js`, `mount(root, api, …)`; plus db-isolation-inventory stale desktop_apps |
| `assert-command-consumer-inventory.mjs` | red | dispatch_consumers drift (V2 consumers moved to `api.command`) |
| `assert-ctox-crew-map.mjs` | red (timeout) | the ported ctox module imports `ctox:shared/…`; the V2 harness serves no such scheme, `__ctoxCrewReady` never set |

Inventory per script: see the sections below (written as each counterpart lands).
