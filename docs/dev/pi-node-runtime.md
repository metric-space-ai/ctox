# Isolated Linux Node runtime for Pi

The locked Pi packages and undici require Node >=22.19.0. The distro Node18
can parse the bundled file yet fail its library import on a dynamic RegExp
`v` flag. A missing sidecar socket is therefore not proof of a transport bug.
The existing browser runtime's Node18 minimum is a different contract.

`src/core/coding_agents/install_node_runtime.py` is an explicit operator
installation/check tool for Linux x86_64 and aarch64, using Python3.9+ standard
library only. It pins Node22.23.2 and official archive SHA256 values, creates a
version directory beneath an operator-selected private root, and publishes it
only after checking the exact supplied sidecar bundle's inert library import.
It never changes system Node, a service unit, a global PATH, the native binary,
app source, secrets or the current release link. It does not download anything.

## Provision and check

Download the matching archive through the deployment owner's existing bounded
artifact-fetch path, subject to host admission/resource policy:

- https://nodejs.org/dist/v22.23.2/node-v22.23.2-linux-x64.tar.xz
- https://nodejs.org/dist/v22.23.2/node-v22.23.2-linux-arm64.tar.xz

The immutable [official checksums](https://nodejs.org/dist/v22.23.2/SHASUMS256.txt)
are compiled into the helper. HTTPS checksum retrieval is not independent
signature verification. Store the archive in designated disposable artifact
storage, not source. Obtain the bundle digest from reviewed release provenance;
never use a freshly calculated unknown digest as approval of unknown code.

```sh
python3 src/core/coding_agents/install_node_runtime.py \
  --install-root /operator/owned/toolchains \
  --archive /designated/artifacts/node-v22.23.2-linux-x64.tar.xz \
  --bundle /reviewed/release/coding-agents/ctox-pi-sidecar.mjs \
  --bundle-sha256 REVIEWED_BUNDLE_SHA256
```

An existing installation is checked without `--archive`. `--print-path` emits
only its checked `bin` directory after successful preflight. The operator may
prepend that directory to PATH for an already authorized, bounded CTOX command;
Pi's current native owner resolves `node` from that PATH and still clears the
child's environment. This is explicit process launch configuration, not a new
ambient CTOX runtime toggle. Do not change a running service's environment or
run a real coding turn merely to validate installation. Long-lived daemon
selection remains a separate reviewed service deployment action.

The preflight verifies Node version and bundle hash before and after import,
clears inherited environment/credentials/NODE_OPTIONS, uses a scratch HOME and
TMPDIR under the chosen root, discards stdout, and kills its process
group after15seconds. Clearing argv[1] before import keeps Pi's direct-entry
socket server inactive. Version and import are separate bounded subprocesses. Failure retains at most
64 KiB of stderr in an owner-only 0600 file under the install root, outside
transient staging. A separate 0600 JSON receipt records phase, exit code,
timeout, truncation and the retained log hash. Only its path is printed;
raw bundle errors are private and must be redacted before sharing. Successful
preflights remove their temporary diagnostic file.
An import result proves only loading, not model access, a successful turn,
provider acceptance or application correctness. The bundle must be trusted;
this helper is not a sandbox for arbitrary JavaScript.

The archive is copied into private staging before hashing. All paths, entry
types and symlink parents are validated before extraction. A checksum/import
failure removes staging and leaves the version target absent. Existing targets
are never overwritten; their receipt and Node executable hash must match.
A nonblocking install-root lock prevents concurrent installs/checks by this
helper. Keep the root owner-controlled; this is not protection against its own
owner deliberately replacing files. Retry a failed check only after diagnosing
its cause. Rollback removes the bounded command's PATH selection; remove a
version directory only once no owned process uses it.

## Evidence

Offline tests cover archive escape/links, checksum rejection, atomic publication,
failed-import cleanup, existing executable tampering, bundle matching and child
environment isolation. They do not download or execute a real Node archive.
The deployment owner must still run this helper against the pinned archive and
exact deployed bundle on Linux and retain its JSON receipt. No real Pi turn is
part of this acceptance gate.
