# Sync v3 S0: native scale startup and write latency

This slice builds on PR585 head `23335748bce2830946989cc28222c2ccfb8687c8`.
It measures the existing shell → browser DB → WebRTC → native SQLite path;
no sync-engine implementation or production authority changes are made.

## Workload and visible-data boundary

The 9,250-document, 257,848,000-byte THESEN size descriptor is mapped to four
isolated local-module collections using the canonical module schema loader.
Native documents retain the declared exact JSON byte sizes including Rx metadata.
Seeding is offline, bounded to 32-row transactions; SQLite independently verifies
counts and byte totals before the native peer starts. This tests actual native
query/write storage with the scale envelope, not production business_commands
execution, queue scheduling, or customer Outbound semantics. No fixture becomes
an executable business command and no production record or policy is modified.

A fresh browser starts the real isolated Business OS shell. The probe obtains
normal collection leases, queries twenty native lead rows, renders their actual
IDs in an on-screen shell overlay and crosses two animation frames. Evidence
separately reports navigation-to-visible, collection-setup-to-visible and
query-to-visible durations. Screenshot and exact row count accompany the report.
The visible boundary is the painted fixture list, not full Outbound app readiness.

Five sequential writes modify an existing 80 KB lead envelope. Local upsert time
is recorded separately. Write latency ends at the actual replication peer's
matching `masterWrite` response with no conflict; an independent native SQLite
readback must confirm each marker. A local commit or optimistic UI is insufficient.

## Actual impaired relay

The existing local test signaller optionally replaces both peers' IPv4 host ICE
candidates with one owned loopback UDP relay per peer pair. Other candidates are
suppressed so no direct path can bypass the relay. ICE/STUN, DTLS and SCTP datagrams
pass through it unchanged. The 300/600 ms cases hold each direction for 150/300 ms;
zero-delay baseline uses the same relay. This is a real user-space UDP datagram
relay, not a TURN server, public network or JavaScript sleep in the query handler.
The signaller's role/token/room admission remains unchanged.

Independent UDP echo tests validate each round-trip delay. Each actual browser
run must select the relay's ICE port, transfer datagrams in both directions and
show the expected per-packet hold floor. Native ACKs cannot precede the imposed
round-trip floor. Queue size, timers, child groups and per-case 240-second runtime
are bounded. Original browser/native processes are owned by the existing smoke
harness and drained on exit; no operator browser or default service is touched.

## Reproduce

Use the normal gpu3 lane with the shared published native7e21 artifact:

```sh
bash scripts/sync-v3/run-native-lane.sh /path/to/ctox-linux-x64.tar.gz /path/to/pinned/playwright
```

The wrapper pins archive SHA256
`98d8c153022eeb83bc6f84a5fade2d838f7bf3749a19b051ba6fe36adb56e64a`
and binary SHA256
`741b7260925c1e27b11cc8100d7877f1f582325523f6d31d1a69e4dac4c4f8ba`.
It compiles no new native baseline. Scratch state stays in the gate's TMPDIR;
bounded JSON, screenshots and logs survive under the lane's Architecture evidence
folder. Source head and binary hash are recorded separately. One fresh prefix and
browser is used per RTT, with five writes each; median/p95 from five samples are
exploratory measurements, not a statistically certified product SLO.

The release workflow runs the same measurement against its already-built native
binary, after locked JS dependency installation. Missing measurements, native ACK,
SQLite confirmation or relay-path proof fail the tooling gate. RFC latency budgets
remain findings, not falsely credited pass flags or loosened existing guards.
No WELSCH measurement before 12 October 2026 13:00 Europe/Berlin.

Before/after numbers belong in the PR from the final-head gpu3 receipt. No customer
performance, installed-stack acceptance or new engine improvement is claimed.
