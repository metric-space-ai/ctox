# Office commit upload ordering — 2026-09-07

Target: Documents and Spreadsheets on https://welsch.ctox.dev/.
This is a repair checkpoint, not a production-readiness certificate.

## Verified defect and change

The shared bridge staged editor chunks, then used `flushBridgeSync` without
`pushOnly`. A missing bridge state could return immediately. The normal
`pushToRemotePeers` sweep reports rejected peers and schedules a later retry
but resolves its promise, allowing the native commit command to race the blob.

Commit now uses the existing `flushSourceLease` path also used by prepare.
For the current direct replication state this awaits an open peer and calls
`pushToPeer` before dispatch. Missing/pending/follower states request a direct
bridge. Transport rejection propagates, while the lease is released in `finally`.
Blob identity, bytes, hash, base version, write policy and command transport
are unchanged. No HTTP data fallback, runtime configuration or permission change.

## Evidence

- Twelve new deterministic regressions failed against the old bridge and pass
  after the change, covering both Office kinds, acknowledgement ordering,
  transport rejection, pending/follower/missing bridges and unavailable push.
- Combined Office engine and upload suite: 74 passed, zero failures/skips.
  Many pre-existing tests inspect source/evidence contracts; this count is not
  a claim that 74 live browser flows were executed.
- Native embedded Pi code-only review completed with one gateway response,
  `applied_files: []`, `message_count: 2`. Durable operator evidence:
  `ctox-dev/output/welsch-office-pi-proposal-1788740426389.json`.
- Earlier gateway fix is separately deployed from ctox-dev main `1cebb53`.
  Both live editors rendered after it, but the own spreadsheet test edit
  C1=23 failed to save. Its test tab was no longer present on continuation;
  that edit is not claimed persisted. User tab16 was not changed.

## Remaining acceptance boundaries

This corrects a demonstrated upload-ordering defect, not a proven complete
diagnosis of the live save failure. The reused helper retains legacy fallback
methods for states without direct-push support. `pushToPeer` also has cancellation
and terminal-rejection reconciliation semantics; resolution alone must not be
marketed as an unconditional durable-write receipt.

Release/activation and real create/edit/save/reload tests remain required.
Do not call either application production ready until those and the full
Office acceptance matrix pass. Native OOM/SIGKILL restart history and the
separate read/reconnect failures remain open findings, not resolved by this patch.
