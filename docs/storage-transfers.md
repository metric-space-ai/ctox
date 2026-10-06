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
Connection and I/O timeouts are ten seconds; cancellation settles the current
bounded operation before the worker lease is released.

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

## Integration and acceptance status

Implementation is in progress. Native endpoint resolver/CLI wiring, final checks
and installed acceptance must land before claiming support. The required real
acceptance is gpu3 → ASUSTOR `flashstore24-nas` at 10.0.0.28, including interruption
and resume of the same durable job. Instances reported that SSH authentication
currently fails; the registered SecretStore reference, host-key pin and actual
NAS destination root are not yet available. Example paths in the capabilities
document must not be used as live destinations.
