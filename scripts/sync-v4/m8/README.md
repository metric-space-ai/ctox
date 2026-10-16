# Native M8 component measurement

Run only through the canonical Linux lane:

```sh
~/.codex/bin/gpu-build-run.sh --owner 01a0e259-5815 \
  --task v4-m8-harness --target-key v4-v4-m8-harness --host gpu3 --jobs 2 \
  --src "$PWD" -- bash -lc 'ctox-prep.sh && bash scripts/sync-v4/m8-native.sh --out "$CARGO_TARGET_DIR/m8-<unique-run>"'
```

Until integration contains P9, add
`--component-ref 96d3dc3a60a40884ab6404c1eda4f5802125e6a6`.
This materializes only the identified test composition into the output directory;
it never checks out or edits production files in the caller's clone. GitHub is a
build-input source only. After P9/CUTOVER integration, omit that option to compile
the caller's current source. A reused output directory is refused.

The runner always materializes a disposable component source copy. It appends
only a module declaration attaching the new test-only scheduler_bridge.rs to
that copy: this exposes the same private accepted-channel constructor used by
the in-module lane4 test. The pristine/compiled scheduler hashes, declaration
and adapter hash are retained. No production algorithm or tracked production
file changes. No production acceptance claim can be made with this test seam.

Before cargo it touches the actual copied Rust sources, forcing recompilation
even after a mirror preserves old mtimes. The native --source-marker prints a
SHA-256 compiled from those source bytes; the runner verifies it against the
inputs before starting any profile and retains source-marker.txt. A mismatched
binary is rejected instead of measured.

Optional `--deadline-utc 2026-10-16T03:05:00+00:00` bounds the measurement
budget after lane admission. The runner records requested/effective per-profile
limits, reserves time for qualification, and leaves timed-out or unstarted
profiles failed. It never lowers the required artifact size or latency targets.
The native process has one Core and two clients, two independent associations,
production V4 schedulers/framing and real native DTLS/SCTP data channels.
Its channel setup follows the green lane4 native test: only the offering endpoint
creates channels; the answerer accepts that same set. Both sides consume the
scheduler-owned incoming lanes (no competing raw DataChannel poll task). Each
direction sends and hashes at least 50 MiB of incompressible artifact data on the
production Transfer lane (its enum index is 5 because Interactive is a separate
priority lane; architecture logical lane 4). Bulk continues throughout command
and terminal probes. Commands traverse the real ordered store writer, Core
command state machine, executor fence, projection and commit-driven signed
subscription. Admission and task execution are isolated component fixtures.
This does not exercise AppRuntimeIngress, the real app executor, browser Kernel,
enrolled participants, or installed CUTOVER-3.

Where an unprivileged user/network namespace supports tc, netem runs only on
that new namespace's lo: half-RTT egress delay, 1% random loss and a shared
64 Mbit/s link. The helper refuses its parent's namespace. Otherwise the native
runtime wraps UDP *below* DTLS/SCTP: half-RTT egress delay, seeded 1% Bernoulli
loss and 8 MiB/s pacing per direction. SCTP performs its real retransmission;
the harness never repairs application messages. The shim starts after handshake;
netem also covers handshake. Queues are bounded. No shared host interfaces,
firewall, privileged host command or live tenant is touched. Namespace probe
results and local coturn availability are retained; a direct connection is never
labeled TURN. TURN without a proven relay route remains explicitly unmeasured.

Each profile is a bounded child process. Fatal native errors, timeout, absent
samples, missing peers, incomplete artifacts, lost or duplicate logical messages,
or a threshold miss fail the component result. Raw samples, source/binary hashes,
UDP drop counts, per-direction artifact hashes and log paths are retained.
SCTP retransmission statistics are not collected by this harness and remain
null; application replay/duplicate counts are separate. Reconnect replay is not
exercised by this profile. Clock measurements share one process monotonic clock;
there is no fabricated cross-host clock synchronization claim.

Outputs: `m8.json`, `m8.md`, raw `profile-{20,150,600}.json`,
`provenance.json`, logs and `threshold-mutation.json`.
Percentiles use nearest rank. Terminal p50/p95/p99 are always reported; the
100 ms terminal limit applies at RTT 20. Live limits apply to every profile, so
RTT 600 may legitimately fail. No limits are silently adjusted.

The threshold proof lowers one actually green latency budget to zero and executes
the baseline and mutant against the same retained native samples: an originally
green latency criterion must change from exit 0 to exit 1. Baseline outputs are
preserved. A missing green baseline is not claimed as a successful mutation.

`pass` and `product_acceptance` remain false for this **component profile,
pre-cutover**, even when `component_profile_pass` is true. Rerunning a component
fixture on a merged CUTOVER revision alone cannot certify production bindings.
The production acceptance adapter must invoke the admitted policy/participant/
executor path before product M8 can be claimed.

The same entry point can reassess retained native samples without a second
transport run: `m8-native.sh --qualify-only "$CARGO_TARGET_DIR/<run>"`.
Runtime provenance and raw profile hashes stay intact; assessor hashes and the
new threshold proof are recorded separately. Assessment still returns nonzero
for any failed component criterion and cannot grant product acceptance.
