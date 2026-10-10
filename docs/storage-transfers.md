# Registered storage transfers

Storage jobs extend the existing `ctox_transfer_jobs` request and share its worker
lease, pause/resume/cancel controls, restart reconciliation and content receipts.
A job pins its owner, computer, endpoint reference, endpoint configuration digest,
purpose, relative path, direction, SHA-256 and length. Native registry resolution
must revalidate that snapshot before each bounded storage operation. A changed
endpoint cannot retarget a resumed job. Secret values never enter requests or
receipts; the native host resolves SecretStore references at connection time.

The Instances owner supplies the authoritative computer-bound endpoint resolver
on top of `ctox.computer-capabilities.v1`. SSH storage uses SFTP with a pinned
SHA-256 server key and an in-memory private key. SMB storage requires encrypted
SMB3, disables DFS referrals and rejects reparse points. NFS is not supported by
this increment. Neither adapter reads ambient credentials or executes shell text.
SSH authentication and each complete SSH storage operation have a ten-second
deadline, including parent checks, range I/O, flush and close. SMB uses ten-second
connection and individual library I/O timeouts; an SMB storage operation can
contain several calls, so its total deadline is not ten seconds. Cancellation is checked between operations, after the current
operation settles and before the worker lease is released.

Uploads first import the pinned local content into the private object store.
They write a request-specific temporary remote file, flush each range before
checkpointing its offset, and verify the retained remote prefix before resuming.
Publication never replaces an existing destination. A full remote readback checks
SHA-256 and length before and after publication; a matching already published
object recovers a publication-before-receipt crash. The source artifact remains
local; offload completion does not itself authorize deletion.

Downloads checkpoint flushed local ranges, truncate uncommitted tails on reopen,
and use the existing hash-checked object publication. Wrong content cannot produce
a completed receipt. Remote flush acknowledgements establish protocol-level
persistence, not a claim about NAS hardware power-loss guarantees.


## Local operator commands

`ctox transfer storage-upload ID OWNER COMPUTER ENDPOINT PURPOSE SHA256 SIZE RELATIVE_PATH FILE`
imports an existing local artifact and queues its upload. `storage-download` uses
the same arguments without `FILE` and publishes verified content in the local
object store. `PURPOSE` is `artifacts`, `backups` or `exchange`. The computer and
endpoint must already be registered for that owner; command arguments never carry
credential values. The native resolver freezes the endpoint, capability and
credential-version fingerprint when the job is admitted.

Use `ctox transfer status ID`, `pause ID`, `resume ID` and `cancel ID` for the
existing durable lifecycle. `ctox transfer run SECONDS` runs a bounded local
worker. Cancel is terminal; pause can resume after process restart. A changed
endpoint or credential version requires a newly admitted job.

A configured `quota_gib` is a declared storage budget checked against each
artifact size at admission. It does not enforce aggregate usage across files or
clients. A hard total quota must be configured on the storage server; NAS
acceptance uses no declared quota until that server setting is enrolled.

## Integration and acceptance status

The native resolver, CLI and daemon worker are wired. Component tests and real
loopback SFTP/SMB3 pause/restart/upload/download/no-replace fixtures passed on gpu3.
SSH uses russh/russh-sftp to avoid the former libssh2 OpenSSL/BoringSSL link
collision.

The real gpu3 → ASUSTOR `flashstore24-nas` offload passed with native revision
`037a47cf8a2bb602085ccaaba177dcfe8f592c2c` and native Owner enrollment revision
`44e5520234577b8038d762f3d056aeb2cc348835`. An owned 833,736,360-byte artifact
was paused after 2,097,152 bytes, then completed after worker restart using the
same upload job and endpoint fingerprint. A fresh download, with the local
upload cache removed, independently read the NAS and matched SHA-256
`15ce709d47e74da3c3659d6bb1aee1f18dc10a4c3294edf387371d012a3a837c` and length.
The continuation and download took 863.824 seconds; the source stayed unchanged
and all owned workers stopped. These measurements apply to that isolated native
CLI run, not to later revisions or Workjet UI acceptance.

The retained operator evidence is
`~/.codex/task-evidence/ctox-transfers/nas-offload-037a47cf8a2b-continued-receipt.json`;
the preceding bounded attempt remains recorded separately as a negative result.
Both completed receipts use `ctox-storage-v1`. The destination was the approved
`/volume1/Build-Tmp/build-lane-offload` with its pinned SSH endpoint. Native Owner
enrollment and SecretStore provisioning are owned by Instances; credentials
never enter transfer requests or receipts.

## SSH/SMB dependency compatibility

The SSH adapter uses russh 0.64.1. SMB 0.12.1 is included locally with one
manifest change: its SSPI dependency is updated from 0.21.3 to 0.23.0. The old
SSPI release pins crypto prereleases that conflict with patched russh releases.
The compatible pair avoids both native OpenSSL linking and a vulnerable SSH
downgrade. Original SMB Rust sources are unchanged; provenance, the exact patch
and MIT licensing are documented in `src/core/transfers/vendor/SMB-UPSTREAM.md`.
