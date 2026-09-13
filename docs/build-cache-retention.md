# Disposable build caches

Source builds and operator verification can accumulate compiler output outside
managed release retention. `src/scripts/build-cache.py` provides a small cache
lease, admission check and sweep. The installer calls it from `run_build_module`
before each compilation or dependency-build command. No background service is
installed. Python 3 is required for this source-build admission path on Linux and
macOS; prebuilt release installation does not require this helper.

Defaults are seven days after recorded completion, a **10 GiB soft budget**, and
**20 GiB available** on both cache and working-directory filesystems before a new
build. The soft budget covers the registered `builds/` namespace, including its
unknown entries. It is not a disk-exhaustion guarantee: active and unknown data
can exceed it, and a running build can consume space after admission. Reports
expose measured bytes, protected entries, selected/deleted names, remaining
excess and failures. Monitor `over_soft_limit`, admission failures and cleanup
errors through the existing job logs. Other cache namespaces and backups are
outside this helper's deletion scope.

## Use

Run an isolated verification with an owner and a fresh cache entry name:

```sh
python3 src/scripts/build-cache.py --cache-root "$HOME/.cache/ctox" run \
  --owner verification-job-123 --entry verification-job-123 \
  --cwd "$PWD" --timeout-seconds 3600 -- cargo test --jobs 2 selected_test
```

The helper creates `builds/verification-job-123`, sets the child process's
`CARGO_TARGET_DIR` and `TMPDIR` there, and records explicit completion after the
command and its process group finish. Commands must run synchronously. Source
checkouts, dependency stores and caller-provided `env CARGO_TARGET_DIR=...`
overrides remain outside this managed entry and are the operator's responsibility.
Never place unique source, credentials, runtime data or backups in a disposable
entry. Preserve useful test evidence elsewhere before completion. Failed commands
also complete their disposable cache lifetime; interrupted commands remain
protected. Existing paths cannot be adopted or silently reused.

Installer builds without `--entry` keep their existing output locations and
managed-release lifecycle; they gain space admission and opportunistic sweeping
of explicitly registered entries. Existing arbitrary operator/Cargo directories
are **not automatically classified or deleted**. Future operator jobs must use
the named-entry invocation above to participate in retention.

Inspect or apply the same sweep manually, or invoke it from an existing scheduled
maintenance job (no additional scheduler is created):

```sh
python3 src/scripts/build-cache.py --cache-root "$HOME/.cache/ctox" sweep
python3 src/scripts/build-cache.py --cache-root "$HOME/.cache/ctox" sweep --apply
```

Sweeps also run before admitted builds. Thus seven days is an eligibility age,
not an expiration SLA while no maintenance/build runs. Completed entries are
removed oldest-first when expired or over budget. Active, missing-owner,
unclassified and future-completion entries are protected. Invalid metadata,
symlinks, mount crossings, special files or inventory-budget exhaustion stop the
sweep without deleting anything. Traversal is limited to 250,000 objects and ten
seconds. A shared nonblocking filesystem lease excludes concurrent cooperating
builds/sweeps; its descriptor is inherited by the child. Uncooperative writers
must not mutate managed entries.

Override policy using the existing cache root's `build-cache-policy.json`:

```json
{"completed_days": 7, "soft_bytes": 10737418240, "min_free_bytes": 21474836480}
```

Only these positive finite fields are accepted. No runtime environment toggle is
introduced. Policy file symlinks are rejected. Admission fails closed on cleanup
or policy errors. Nonzero child exit codes propagate to the installer.

## Recovery and rollback

Unknown/active entries need owner review, not automatic adoption or deletion.
Retain the current update snapshot, manual and pre-deletion recovery copies.
SQLite consistency checks do not prove a complete restore or off-host recovery.
That policy is independent of disposable build caches.

To roll back prevention, revert the installer/helper change and stop invoking the
sweep from any existing job that adopted it. Protected state remains untouched;
already removed disposable artifacts require rebuilding. Validate with
`python3 tests/test_build_cache.py`; fixtures stay under the caller's `TMPDIR`.
