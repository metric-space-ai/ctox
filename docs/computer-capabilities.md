# Computer capabilities v1

Contract ID: `ctox.computer-capabilities.v1`. A computer is an opaque Workjet
identity assigned to a CTOX instance, not a hostname or an environment. Build,
storage and GPU configuration lives in its native `workjet_computers` record.
The registry is shared by native build selection and TransferEngine consumers.

## Assignment and authority

`ctox.workjet.computer.assign` retains its existing identity, display name,
hosting mode and optional agent capability names. It additionally accepts:

- `capability_config`: an array of the tagged descriptors below; at most one of
  each kind. Omission preserves existing settings; an explicit empty array
  removes operational capabilities.
- `agentless`: a boolean. Omission preserves the existing value; initially false.

Supplying either field requires native `integrations.manage` policy approval and
a verified Owner (`chef`) or Admin role. The signed session supplies the owner;
payloads cannot assign a different owner. Even Admin cannot silently take over a
computer belonging to another owner. Unassignment makes all capabilities
unavailable to native routing. Managed backend hosts remain ineligible.

Legacy assignment refreshes preserve typed settings. Operational names in the
`capabilities` string array are derived from the typed descriptors; toolchain
names such as `codex` and `claude` remain supported. A name alone never grants
execution. Endpoint references are opaque identifiers in the native Instances-owned
[computer endpoint registry](computer-endpoints.md), not hostnames, URLs,
passwords, private keys or command strings. Its resolver checks current grants,
SSH/SMB connection details and SecretStore references; Transfer owns IO and
job lifecycle. NFS is explicitly unsupported in this endpoint increment.
This capability contract contains no credentials.

## Typed descriptors

```json
[
  {
    "kind": "build",
    "ssh_endpoint_ref": "endpoint-gpu3",
    "slots": 3,
    "jobs": 6,
    "lane_root": "/mnt/nvme1/build-lane",
    "disk_floor_gib": 60,
    "toolchains": ["rust-1.93", "node-22"]
  },
  {
    "kind": "gpu",
    "model": "NVIDIA RTX A4500",
    "vram_gib": 20
  }
]
```

Build requires an SSH endpoint reference, 1–32 slots, 1–64 jobs per slot, an
absolute lane root, a positive free-disk floor and 1–16 toolchain identifiers.
Toolchains and descriptor ordering are canonicalized. The adapter must enforce
capacity and disk floors, acquire the remote slot lease, keep a target directory
per source/PR, and produce exit status/logs that survive SSH interruption.
Public source uses GitHub plus the exact local diff; private source uses the
existing authenticated transfer path. Credentials must not be copied with
source. `~/.codex/bin/gpu-build-run.sh` is the current execution prototype;
registry integration does not replace its detached runner yet.

Native `load_registered_computer_capabilities` returns only assigned, undeleted
computers of the requested owner and validates persisted settings before use.
`select_build_target` matches the required toolchain and picks the most free
slots, with an opaque-ID tie-break. Native slot probes must be at most 30 seconds
old, not future-dated, and within declared slot capacity. Selection is not a
reservation: the build adapter must still acquire the remote flock before
spawning. Browser liveness and capability-name tags are not slot observations.

An agentless NAS is assigned with `hosting_mode: "self_hosted"`,
`self_hosted_colocation: false`, no agent names and exactly one storage descriptor:

```json
{
  "kind": "storage",
  "endpoint_ref": "endpoint-flashstore24-nas",
  "protocol": "ssh",
  "root": "/volume1/artifacts",
  "quota_gib": null,
  "purposes": ["artifacts", "backups", "exchange"]
}
```

Storage protocols are `ssh`, `smb` and `nfs`. The root is an absolute normalized
remote path, without `.`/`..` components. Quota is a declared positive-GiB budget
or null. This increment uses it for per-artifact admission; it does not establish
an aggregate limit across files/clients or claim measured free space. A hard total
limit requires an actual server-enforced quota. NAS acceptance uses null until
such a quota is enrolled. At least one purpose
is required; purposes are deduplicated. The endpoint's protocol must match this
configuration when TransferEngine resolves it. TransferEngine owns protocol
support, cancellation, resume and the actual transfer. Declaring a protocol here
does not prove its endpoint or credentials are installed. Agentless computers
cannot carry build/GPU/agent capabilities, daemon co-location, worker-profile
bindings or session execution targets.

GPU descriptors require a nonempty model label and positive VRAM in GiB. GPU
metadata alone never grants permission to run builds or agents.

## Compatibility and delivery

The current Workjet/RxDB computer collection is schema v1 with a string array
and `additionalProperties: false`. Native typed settings and the agentless flag
are retained in the authoritative record but excluded from that projection;
capability names continue to replicate over the existing WebRTC data plane.
There is no schema version/hash change or HTTP data bridge in this increment.
The expanded UI schema and editing workflow are coordinated with Main after
Workjet 0.0.35 is installed. Native registered endpoint resolution now uses the
owner-bound registry and a frozen per-job authority fingerprint. Native
capability_epoch is excluded from the v1 projection along with typed settings.
Actual build dispatch, protocol IO and deployed UI acceptance remain integration
work.

Acceptance for the complete capability outcome remains: gpu3 and gpu4 registered
for build, ASUSTOR `flashstore24-nas` (10.0.0.28) registered as agentless storage,
a CTOX-routed CTOX build on gpu4, a gpu3 artifact transferred to the NAS through
TransferEngine, and visible capabilities in installed Workjet. Example endpoint
IDs and paths above are contract examples, not claims of completed registration.
Michael supplies the NAS SSH key; the supervisor handles its Tailscale admission.
