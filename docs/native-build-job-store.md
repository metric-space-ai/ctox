# Native build-job store

`business_os::build_job_store::BuildJobStore::open(root)` uses the existing
native Business OS SQLite store (`runtime/business-os.sqlite3`). Its private
`native_build_jobs` table is not a browser collection or an HTTP data path.

Each `(owner, job_id)` has immutable admission JSON, mutable progress JSON,
a monotonically increasing revision and lease generation. `create` rejects
reuse, including terminal job identities. `read` and bounded, keyset-paginated
`list` always require an owner. The control plane must derive that owner from
authenticated identity: this library does not authenticate a string or prevent
authorized native processes from opening SQLite directly.

`claim(owner, job_id, revision, lease_ms)` atomically claims an admitted or
expired running job, returning the envelope and lease capability. Leases last
1–300000 ms. A live claim cannot be replaced. Claims increment revision and
generation and allocate a fresh UUID token. `checkpoint(lease, revision,
progress, lease_ms)` writes progress and renews in one transaction.
`complete(lease, revision, progress, terminal)` accepts `succeeded`, `failed`
or `cancelled`, clears the lease and permanently prevents relaunch. Every
write requires the exact owner, job identity, revision, generation, claimant
token, running state and unexpired lease. Conflicts change nothing; reread
and reconcile rather than retrying an old write blindly.

SQLite immediate transactions serialize competing connections. The persisted
claim survives reopen; reclamation fences the old token even if the old
process remains alive. Renewal returns the next revision. A SQLite trigger
also enforces admission immutability. Lease time uses native Unix wall-clock
milliseconds; clock rollback can delay reclamation and forward jumps can
expire leases. This store fences state writes, not remote OS processes:
orchestration must reconcile the prior remote execution before relaunch and
route authoritative writes through this API.

Tests use real SQLite files and independent connections for reopen, concurrent
claims, owner isolation, expired/reclaimed writer fencing, revision conflicts,
checkpoint renewal, terminal exclusion and lease bounds.
