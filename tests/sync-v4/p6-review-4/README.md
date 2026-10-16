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
The last functional red qualification used train7 a6dda5aab (including the E21 library/app partition).
`--train <sha>` binds the immutable product revision and refuses source drift
in src/, Cargo.toml or Cargo.lock. The old #827 composition is retained only
for reproducing the original 59849cee5 baseline without its array error.
The actual train Foreground, OwnerAdapter and CodingPanel remain unchanged.
Only session/credential/crew signatures are compile stubs; policy, Store and
Draft writeback are production code. This composition is separately recorded.

K13/K21/K22 use `unshare --user --map-root-user --net --mount --pid --fork
--mount-proc` and the real foreground service. The private namespace verifies
the UID/GID maps and user/network/mount/PID identities before setup. Loopback
and netem are configured there; prlimit runs inside it. The process and network
receipts distinguish namespace PIDs from the outer controller PID.

The runner observes actual service readiness before probing the control-plane
bootstrap. It neither starts the V3 server nor seeds owner SQLite. A missing
native/browser/identity adapter remains `not_run`; policy-dependent stages on
the requested train-8 base remain `not_run(cutover-5)`. Every unmeasured criterion
keeps an explicit null actual value. `--namespace-mutation` requires real private
namespaces and cleanup, then removes `--net` from a copy of the real launcher:
the namespace contract must turn red before daemon startup. Namespace/startup
qualification cannot establish incident acceptance. Incident mutation proofs remain pending
until the authenticated native incident path is reachable.
