# Isolated installer / M7 rehearsal

These tools run only inside the canonical Linux build lane. They do not deploy
to a tenant, publish a release, or authorize a production upgrade. Fixtures must
carry the existing synthetic-owner marker and live below the lane artifacts
directory. Never point them at a customer state root.

## Release custody

`m7-release.py --output <new-artifact-directory>` builds the CLI with the normal
release profile and target from `.github/workflows/release.yml`, then packages
its runtime assets. It writes binary/package digests and an asset inventory.
`--reuse-build <terminal-release.json>` avoids another compiler run only when
the compiled product source tree and immutable binary digest still match.

`m7-artifact-qualify.py <release.json> <new-output> [owned-profile]` removes the
real `app.js` archive entry and requires receiver rejection, followed by the
unchanged positive control. With an owned profile it also removes a real
synthetic history tombstone from a SQLite-API copy and requires the full-row
readback guard to reject that loss before restoring the copy.

## Supported installer and recovery

`upgrade-controller.mjs` creates a private PID/mount/network namespace and a
real isolated user service manager. It never substitutes a successful
`systemctl`, uses the host service bus, or bypasses failed namespace admission.
The current installed image executes recovery; the historical producer must
not keep running the old rollback implementation after a release switch.

`m7-data-readback.py` fingerprints every row and column of every synthetic app
table, including tombstones. Rollback and forward recovery must match the
before snapshot. This does not establish coverage of an actual tenant.

## One bounded qualification

Run `m7-dress-qualify.py <producer-release.json> <absolute-UTC-deadline>` through
the task's canonical gpu3 command. The historical fixture/reference paths are
explicitly pinned in that driver. Every child has a timeout and cleanup; the
absolute deadline bounds the combined run.

The driver records syntax, release spawn liveness, actual archive/readback
corruption, the private rehearsal, and a static HTTP component measurement
using the V3 reference slot. That reference is **not** a signed V4 shell.

`m7-dress.py` writes `m7-go-nogo.json`, including all criteria even when an early
phase fails. Its statuses are `pass`, `fail`, or `not_run` plus the responsible
owner. Exit 2 means missing acceptance prerequisites, not a successful M7.
The qualification driver's exit 0 can establish that its executed contracts
hold while `fullM7Accepted` remains false. Read the decision matrix.

A fresh-state foreground startup control is explicitly separate from an
installed upgrade, persistent signing-key verification, populated-profile
migration, rights binding, app hot-plug and Workjet bridge acceptance. A
SQLite-only profile copy is not booted as a complete sealed-state restoration.
An authorized frozen profile, productive import/coverage path, ordered V4
transport and signed V4 slot must be supplied before the full rehearsal can
pass. The production `upgrade --dev` path remains main-only.
