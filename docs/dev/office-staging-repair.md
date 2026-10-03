# Offline repair of omitted Office staging uploads

This maintenance command removes active, historically corrupted Office upload
staging records through the existing native projection writer. It does not
delete documents, versions, canonical DOCX files, editor snapshots, or command
receipts. It does not reconstruct the omitted upload bytes.

```sh
ctox business-os repair office-staging --dry-run
ctox business-os repair office-staging --apply --expected-sha256 <candidate_sha256>
```

Use the normal CTOX root selection for the target instance. Inspect the dry-run
report before applying. The apply operation requires exclusive ownership of
the existing native-peer process lock: stop the managed peer first, and ensure
the service is restarted even if maintenance fails. Do not stop active work
without reconciling its durable state.

The audit rejects recent uploads (less than one hour old), unsupported record
shapes, missing owning documents/current versions, nonterminal commands
referencing an affected blob, any canonical reference to a damaged blob, and
missing or hash-mismatched canonical/editor files in affected document history.
Only the exact audited candidate digest can be applied.

Before writing, it saves the complete original candidate records and report
under `runtime/office-staging-repair/<digest>.json` with restricted permissions
and durable file/directory synchronization. The normal projection writer emits
deletion tombstones into the native projection store and RxDB. The tombstones
retain replication evidence; historical staged-upload references remain
auditable. A final audit requires zero remaining active omitted staging records.
There is no raw SQL deletion or HTTP data bridge.

The audit also includes already deleted chunks retaining an invalid omission
object. The file projection writer replaces that invalid byte field with an
empty string only on deletion, so the tombstone satisfies the ordinary schema.
The original omitted object remains in the repair backup.

The implementation is `src/core/business_os/office_staging_repair.rs`.
Its real-store tests cover preservation and backup, repeated empty repair,
live-peer exclusion, stale digest rejection, missing canonical data, and
nonterminal command exclusion. A native RxDB integration test additionally
replays the stored tombstone through the actual projection writer and schema.
Browser replication and production file reopening must also be checked after
operational use.

A failed audit is a finding, not permission to bypass its checks. Resolve the
specific missing or inconsistent evidence before retrying. Restore from the
saved records only through a reviewed recovery path; never blindly copy an old
database over newer writes.
