# Native computer endpoints v1

Contract ID: `ctox.computer-endpoints.v1`. Instances owns this endpoint registry
and the capability resolver. Transfer owns SSH/SMB IO, durable jobs, cancellation
and resume. Build adapters consume the same authority; endpoint registration
does not install a daemon, establish connectivity or reserve a build slot.

## Native commands and storage

The verified native Owner/Admin session supplies ownership. These commands
require `integrations.manage`; payload ownership and inline secret fields are
rejected:

- `ctox.workjet.computer.endpoint.upsert`:
  `{endpoint_ref, computer_id, connection}`.
- `ctox.workjet.computer.endpoint.disable`: `{endpoint_ref}`.
- `ctox.workjet.computer.endpoint.list`: `{limit?}`, at most 100 owned entries.
- `ctox.workjet.computer.ssh_key.ensure`: `{computer_id}`. Explicit native
  Owner/Admin setup also requires secrets.manage. The computer must already
  be assigned to that verified owner. Native generates a fresh Ed25519 key in
  SecretStore and returns only contract ctox.workjet.computer-ssh-key.v1,
  computer_id, private_key: {scope,name}, public_key, and public_key_sha256.
  The stable owner/computer tuple reuses the stored key on repeat or lost
  acknowledgement; it never imports or overwrites an existing credential.
  A preexisting unissued record fails closed. Authorize only the returned
  public key on the intended target, then use the reference in endpoint.upsert.
  The action does not install a key on the target or prove network readiness,
  and it does not route the desktop's private key elsewhere.

The computer must currently be assigned and owned, with hosting mode workstation
or self_hosted. An agentless NAS is eligible for storage. The endpoint's owner
and opaque computer binding cannot be changed in place. Repeating an identical
upsert or disable preserves its revision. Connection edits or re-enablement
produce a new revision. Configuration may precede credential installation;
resolution fails closed until all referenced credentials exist and decrypt.

Records live only in native `workjet_computer_endpoints`; no endpoint or
credential reference is projected into browser RxDB. Workjet must use the
normal typed command plane and existing data boundary. The shell computer control bridge exposes computer.ssh_key.ensure through business_commands over RxDB/WebRTC, with correlated command/computer identity and public-only output. It rejects injected ownership, inline secret fields and unsupported result fields.

SSH connection example (all identities, host, pin and paths are examples):

```json
{
  "protocol": "ssh",
  "host": "build.example.test",
  "port": 22,
  "username": "metricspace",
  "root": "/srv/build-lane",
  "host_key_sha256": "SHA256:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
  "host_key_algorithm": "ssh-ed25519",
  "private_key": {"scope": "computer-access", "name": "build-key"},
  "passphrase": null
}
```

An SSH SHA256 pin must encode exactly 32 bytes using the OpenSSH unpadded form.
The adapter must verify it during connection; no trust-on-first-use fallback.
Optional `host_key_algorithm` constrains negotiation to `ssh-ed25519`,
`ecdsa-sha2-nistp256`, `ecdsa-sha2-nistp384`, `ecdsa-sha2-nistp521`,
`rsa-sha2-256` or `rsa-sha2-512`. It never replaces the pin check. Omission
or null keeps library-default negotiation and preserves legacy serialized
connections and fingerprints. A constraint edit changes native endpoint/job
authority. Adapters must apply a supplied constraint before connecting and fail
closed if unsupported. Never retry with another algorithm or pin after
authentication failure. This names the server host key, not the client key.
Private key and optional passphrase are existing SecretStore `{scope,name}`
references. Credential values never belong in endpoint metadata or receipts.

SMB connections contain `protocol: "smb"`, host, positive port, username,
share, absolute root within that share, and `password: {scope,name}`.
Hosts accept DNS names or IP addresses, never URLs, command options or embedded
usernames. Roots must be normalized absolute paths without empty, dot or parent
components. NFS remains a declarable capability but is unsupported by this
endpoint/Transfer increment, and is explicitly rejected here.

## Rust consumer API and resume boundary

`business_os::computer_endpoints` exports:

- `ComputerEndpointRequest {owner_user_id, computer_id, endpoint_ref, usage}`.
  Owner must come from the verified native session or durable authorized job.
- `EndpointUse::Build` or `EndpointUse::Storage {purpose}`.
- `resolve_computer_endpoint(root, &request)` returns
  `ResolvedComputerEndpoint`: owner/computer/ref, endpoint_revision,
  capability_epoch, typed connection, the selected typed capability grant,
  usage and non-secret fingerprint.
- `with_current_computer_endpoint(root, &request, expected_fingerprint, callback)`
  rereads current authority and holds endpoint/grant and secret rotation fences
  through one synchronous bounded protocol operation. Credential slices are borrowed in
  order: SSH key then optional passphrase, or SMB password.

Resolution requires the current assigned, undeleted, owned computer and enabled,
matching endpoint. Build requires a non-agentless build grant and SSH; storage
requires the matching protocol and purpose. The grant root must equal or lie
within the endpoint's approved root, with path-component boundaries.

Persist the resolved fingerprint with the job before executing. Every IO chunk
and resumed chunk must pass the original fingerprint into the bounded callback.
Connection edits, disable/re-enable, capability change/revoke/regrant,
unassignment/reassignment, credential-reference edits, credential rotation or
deletion prevent the old job from continuing. A changed authority requires an
explicit new job; do not overwrite the frozen fingerprint during resume.
Owner/computer identity and the request usage are included in the fingerprint.

The credential revision hashes randomized encrypted records and their references,
never plaintext password/key hashes. Native `capability_epoch` increases for
grant changes and assignment revocation, remains stable for cosmetic/liveness
updates, and is excluded from schema-v1 browser projection.

Acquire endpoint authority before worker/Core/controller locks. Never await,
reenter registry/secret APIs, retain credentials, or run a full transfer while
inside the callback. A callback protects one bounded logical operation only:
connect/auth, one data range of at most 1 MiB (including its bounded parent/open/
read/write/flush/close sequence), or one metadata/rename operation. Adapters bound
call counts and paths. The libraries enforce 10-second timeouts per connection/IO
call; compound operations can take longer and have no promised 10-second total
deadline. Reusing an authenticated transport is allowed; secret byte buffers must
be dropped/zeroized before callback return,
and every next operation must revalidate the original fingerprint. Adapters must
independently enforce paths against remote symlinks, destination permissions,
per-artifact budget admission, available-space checks where supported, host pin
verification, stop/resume and atomic publication. Declared quota_gib does not
establish an aggregate server quota; a hard total guarantee requires a real
server-enforced quota. Use null for the NAS until one is enrolled.
A registry descriptor is authority metadata, not evidence of reachable storage.
