# Desktop layout authority and restart boundary

The dependency direction is:

`app.js` strict native reader -> desktop `index.js` adapter ->
`layout-authority.js::ensureDesktopLayoutWithAuthority` -> supplied collection
and insert-only seed operation.

The shell supplies the bounded native reader over the existing sync path.
The adapter supplies the document id, defaults, seed operation and diagnostic
callback. It does not implement a second layout authority or error policy.

The exported resolver owns these decisions:

- A native document wins without local reads or writes.
- Missing reader, rejected native read, or a non-null missing/malformed answer
  means unknown authority: show defaults without publishing them.
- Only explicit native `null` permits one seed insertion. A conflicting insert
  adopts the stored winner; without a winner, the original conflict propagates.
- Local database-closing errors during insertion or winner lookup return
  display defaults without retrying writes. A later call reads authority again.
- Other local failures preserve their original error identity.

`isDatabaseClosingError` is the single desktop classifier, shared by layout,
icon and command handlers. It classifies the message, not the exception's
prototype, so plain-object errors from another realm retain the same behavior.
The layout diagnostic callback observes local restart fallback only; native
read rejection remains unknown authority and does not imply native absence.

The private resolver implements authority selection; the exported boundary
wraps it with local restart recovery. Keep both in this module so moving mount
code cannot silently remove recovery from the runtime path.

Verification: `layout-authority.test.mjs` covers native precedence, unknown
versus absent and conflict winner adoption; `tests/layout-restart.test.mjs`
imports the actual exported resolver and exercises insert/findOne/exec failure,
recovery, original error identity and a conflict without a winner.
`registry-launch-smoke.mjs` retains the adapter/import wiring guard. These
checks do not prove live sync, authentication or production browser readiness.
