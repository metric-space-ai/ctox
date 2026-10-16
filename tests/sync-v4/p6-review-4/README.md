P6-REVIEW-4 regression targets

Run through the assigned gpu3 lane, after `ctox-prep.sh`:

```sh
python3 tests/sync-v4/p6-review-4/run.py
```

The default command exits nonzero while any requested contract is red.
`--only regressions|r10|r13|r14|scenarios` narrows the run.
 `--expect-red` is for qualifying
known assertion failures; it does not declare the product or scenarios green.
Individual exits, source bindings and complete logs remain under the order's
`CARGO_TARGET_DIR/p6-review4-<UTC>/`.

- R10 drives the real `Foreground::start` cleanup after a later factory fails.
  An earlier engine refuses drain while its real child effect remains alive.
  Maintenance must not obtain the daemon's IPC/file custody during that time.
  The fixture kills/reaps its own child even when the assertion fails.
- R13 checks an allowed reply first, then commits a native rights revocation
  while a second reply is held at the existing `SidecarRunner` boundary.
  The real CodingPanel must preserve the draft and expose a revoked result.
  This is a component contract; it does not claim genuine model/sidecar E2E.
- R14 traces actual filesystem syscalls during each poll of the production
  `OwnerAdapter::attach` future. Poll markers carry kernel thread IDs, so task
  migration and Tokio blocking workers cannot be confused. A file identity
  probe on the polling async thread is a failure; Store/reader/blocking threads
  remain allowed. `strace` is required; absence is unverified, never a pass.

The root tests wire private test modules in a disposable source copy under the
lane target. Supplied production sources remain untouched; copied wiring is
also restored in `finally`. No production fix or public test hook is installed.
The targets use current policy sources on the updated train7 (a6dda5aab, including E21 library/app partition).
`--train <sha>` binds the immutable product revision and refuses source drift
in src/, Cargo.toml or Cargo.lock. The old #827 composition is retained only
for reproducing the original 59849cee5 baseline without its array error.
The actual train Foreground, OwnerAdapter and CodingPanel remain unchanged.
Only session/credential/crew signatures are compile stubs; policy, Store and
Draft writeback are production code. This composition is separately recorded.

K13/K21/K22 use the existing P6 service entry and isolated real-daemon runner.
Missing web/bootstrap/native fixture bindings remain `not_run`; startup and
syntax/oracle checks cannot establish incident acceptance. Genuine scenario
mutation proofs remain pending until the native incident path is reachable.
