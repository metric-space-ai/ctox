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
Native root checks and installed acceptance remain open. The root linker exposed
an OpenSSL/BoringSSL collision in libssh2; SSH now uses russh/russh-sftp and its
protocol fixtures must pass again. The required real acceptance is gpu3 → ASUSTOR
`flashstore24-nas` at 10.0.0.28, including interruption and resume of the same
durable job. The operator approved `/volume1/Build-Tmp/build-lane-offload` and the
pinned SSH endpoint. Native Owner enrollment and SecretStore provisioning remain
owned by Instances; no credential value is stored in this document.
