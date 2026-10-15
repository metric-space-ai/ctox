# CTOX transport bench (M3 / M4)

This test-only fixture connects the unchanged native RxDB WebRTC transport to
three independent headless Chromium peers. No Business OS apps, customer stores,
production signaling, TURN credentials, or product writes are involved.

## Components and revisions

- `native/`: a standalone Cargo workspace wrapping the existing
  `WebRTCRsConnectionHandler`; stdin controls synthetic traffic, stdout records
  receipts and send completion. It does not replace the transport implementation.
- `browser.mjs`: adapter for the existing `createCtoxWebRtcNativePeer`. A later
  transport may provide a replacement adapter through `--browser-adapter`.
- `runner.mjs`: owned signaling server, static runtime loader, Chromium, TURN,
  lifecycle and raw JSON evidence. One fresh browser context per peer/profile.
- `counters.mjs`: exact ID-set reconciliation; a zero-sample quantile is null.
- `scripts/sync-v4/bench-netns.mjs`: disposable network namespace and netem.
- `scripts/sync-v4/bench-stage.mjs`: materializes only the test's source inputs
  for the lane when transferring the full repository is impractical. It records
  the source revision and source hashes; it never invents a git revision.

Use `--native-bin`, `--native-revision`, `--browser-root`, `--browser-revision`
and optionally `--browser-adapter` to compare old and phase-4/5 implementations.
The binary, browser transport and adapter hashes are included in every result.
The native executable must implement the fixture's stdin/NDJSON protocol; select
this fixture rebuilt against the candidate transport, not a production daemon.
A replacement browser adapter exports `start(config)` and sets `window.bench`
with `ready`, `begin`, `drop`, `stop`, `snapshot`, and `close` methods. Its snapshot
must retain the same ID/event/counter schema so comparisons cannot omit losses.

Changing a declared revision does not rebuild the binary: supply the matching
binary and verify the recorded hashes.

## Canonical execution

Run through the shared GPU admission lane, never directly on the Mac:

```sh
greppy bash-smart -- ~/.codex/bin/gpu-build-run.sh \
  --owner YOUR_THREAD_ID --task v4-bench-transport \
  --target-key v4-bench-transport --host gpu4 --jobs 2 --src YOUR_SOURCE \
  -- bash -lc 'ctox-prep.sh && \
    cargo build --release --locked --manifest-path tests/sync-v4-bench/native/Cargo.toml --jobs 2 && \
    node --test tests/sync-v4-bench/counters.test.mjs && \
    node scripts/sync-v4/bench-run.mjs --owner YOUR_THREAD_ID --out OWN_ARTIFACT_PATH'
```

The lane must have Playwright and `ws` available; `--modules-root` selects their
existing node_modules directory. `--chrome` selects an installed Chromium.
The default module path is the pre-existing GPU4 acceptance dependency cache;
there is no npm installation at runtime. The fixture extracts coturn and its
libraries from the host's configured official apt repositories into its own
artifact directory, without installing a service. `--turnserver` and `--turn-libs`
can select a previously extracted copy. These are test dependencies only.

A short diagnostic run requires `--smoke --duration 35 --profiles 20 --peers 1`.
It is explicitly ineligible as a ten-minute baseline. Default profiles are
20, 150 and 600 ms; each offers load for 600 seconds, followed by bounded drain.
The complete matrix is about 33 minutes, plus compilation and establishment.

## Isolation and TURN proof

Passwordless `sudo` for a **new network namespace** is required for netem.
The helper refuses the host network namespace, adds a private dummy interface,
then drops to the original uid/gid before launching Chrome/native/TURN. It does
not modify host routes or qdiscs, install a system service or use customer data.
Chrome uses `iceTransportPolicy: relay`; sampled selected-candidate pairs must
show relay or the run fails before load. Native gathers host candidates inside
that namespace. All browser/native traffic therefore crosses local coturn.
The native fixture is built in release mode. Keep the same optimization profile
for both baseline and candidate; `--native-profile` records a supplied binary's
declared profile, which is not inferred from its filename or hash.

Signaling is local and is not delayed; this measures the transport data path.

Only UDP packets to/from the TURN listening socket are shaped. The requested
RTT is split into two one-way delays, with 1% loss in each direction by default.
The actual ICE RTT distribution is recorded separately: configured delay is
not substituted for measured RTT. TURN's native-facing local leg is unshaped.
There is no WAN/TURN deployment claim. The optional `--loss` argument changes
only this isolated fixture. All fixture processes have a bounded deadline and
are terminated on exit; the namespace disappears with them.

## Traffic and counter contract

Each of three browser peers offers control pings every 100 ms and command,
interactive, live and initial messages every second. Each side continuously
sends one deterministic 5,000,000-byte ASCII bulk body per peer, starting the
next body after transport completion (native) or application ACK (browser),
or after the recorded bounded failure/timeout.

These are workload labels mapped onto the **existing** transport's methods and
priorities, not a claim that the legacy implementation has six channels.
Control maps to `ctoxProtocol`; other classes to `masterWrite` on synthetic
`bench_<class>` collections. Legacy unreliable audio/video is not exposed and
is explicitly listed as unsupported rather than simulated with reliable data.

Every 30 seconds all browser transports close, stay down one second, and reopen
with a fresh peer generation. No application replay masks transport loss.
This is an explicit connection-close test, not a silent-blackhole simulation.
The default `--drop-mode factory` records full transport shutdown/recreation.
For M3's automatic reconnect path use `--drop-mode channel --profiles 20`:
only the main DataChannel closes, retaining the same peer factory and its
real onclose/reconnect machinery. No application replay is added. Fault attempts,
actual `fault_applied` events and `fault_target_not_open` are distinguishable;
an already broken link is never counted as another successful channel fault.
Use the same mode for baseline/candidate comparisons. Both modes can run all
three RTT profiles; a single-profile run does not prove the omitted profiles.
Reliable IDs accepted by send must match unique receiver IDs. Failed admission,
missing send result, duplicates, accepted-but-unreceived and unexpected receipts
are separate fields. A disconnect can legitimately reject send; those rejects
do not silently disappear from attempted counters. Browser send's boolean proves
queue admission only, whereas native completion awaits the existing send API.

Control latency is request/ACK round trip on the browser's monotonic clock
or from the native controller's offer to its ACK receipt (including stdio delay),

under load. Successful samples, ACK timeouts and send failures must be considered
together: low percentiles alone do not establish absence of starvation.
Browser ACK matching times out after 10 seconds; native send completion after
35 seconds. Late browser ACKs beyond that matching deadline do not turn a
timeout into a successful ping. Those IDs remain without a successful ACK.
After the matrix, run `node scripts/sync-v4/bench-summarize.mjs OWN_ARTIFACT_PATH`.
This derives full offer-to-ACK latencies from raw timestamps on each sender's
clock, including synchronous admission and ACKs in the bounded drain. It writes
`derived-latency.json` beside each immutable raw result and prints a German
table. The executor's `controlPingUnderLoadMs` instead counts ACKs completed
inside the load interval (browser timing starts after its synchronous send).
The derived table avoids censoring those slow tail completions or omitting
synchronous admission. No successful ACK and unavailable drain timing stay
explicitly absent rather than becoming zero.
Throughput is useful body bytes received **inside the load interval**, decimal
MB/s, aggregate across peers per direction. Wire byte counters include framing,
protocol and ACK traffic. Drain receipts are available in reconciliation but
are excluded from under-load throughput and latency.

`maxBrowserBufferedBytes` samples physical `bufferedAmount` every 20 ms. Native
only publishes `lastBufferedAmount`; its value is not claimed as a physical
maximum or proof of the v4 128-KiB association budget.

## Evidence and outcomes

`matrix.json` identifies each profile, child PID, exit status and terminal state.
`rtt-N/result.json` holds raw event IDs, send/receive counters, p50/p95/p99, ICE
proof, byte counts, revisions and netem parameters. Stderr/turn logs remain beside
it. Exit 0 means measurement completed, **not** that M3/M4 passed. Read exact
counter differences and missing samples before judging a result. Interrupted or
failed runs remain evidence with their error and cannot become a baseline.

No report or customer evidence belongs in the public repository. The controller's
private `docs/baseline-transport.md` links the measured matrix and limitations.
