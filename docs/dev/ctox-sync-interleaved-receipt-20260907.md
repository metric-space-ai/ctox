# Native WebRTC send receipt regression — 2026-09-07

Status: transport defect reproduced, minimal native correction implemented;
browser acceptance results are recorded below. The tenant upgrade is owned by
the operator. This work does not activate a release on thesen.ctox.dev.

## Causal change and incident scope

The defective sender lifetime was introduced by
`f972b3ed611fba8335c7774f9fe48433fe7219d8`
(`Extract native Sync lifecycle and add quorum execution authority`,
committed **2026-09-06 00:48:53 UTC**).
This is part of the Sync architecture rework, not a tenant data/configuration
mistake. It is in the interval after `391508cd8` and before the reported
`branch-main-20260906T223502Z` upgrade.

The relevant source is
`src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs`:
`await_queued_send`, `drain_send_queue`,
`send_framed_text` and `drain_high_priority_inline_frames`.

A sender waiting for its receipt can take ownership of the shared peer queue
drainer. The first queued message need not be that sender's message. If it is
a large response for another collection, its framed transfer can interleave
small high-priority messages. One such small message can be the drain owner's
own message. Completing its receipt caused the biased `tokio::select!` to
return and drop the drainer **in the middle of the other collection's response**.

The other response's result sender is dropped too. The pre-fix code reports
`RC_WEBRTC_PEER: WebRTC send queue result dropped` with
`expectedPeerTeardown: true`, even though the peer is still connected.
The browser has a transfer start and some chunks, but no complete response.
Its incomplete-transfer watchdog and outstanding request deadlines therefore
have nothing to complete. A shared multiplexed connection can have pending
pulls and query fetches from many collections simultaneously: transport
liveness (`activePeerCount=1`, native replication “up”) does not prove that
these application messages have been delivered.

This is a concrete mechanism consistent with the reported incoming-transfer
stalls and `masterChangesSince` timeouts. It is not proof that every historical
timeout, or every pending browser write in that tenant, has this sole cause.
We did not instrument/replay the actual thesen session.

Chronology matters:

- `40873f0d6` (06 Sep 13:41:40 UTC) adds retirement checks at awaited
  background-transfer boundaries. It does not prevent the ordinary successful
  receipt from cancelling a foreign transfer.
- `ad93f4d21088ab201ccc40a750f8c1188ee71942`
  (“Preserve large source projections across restart and bound live replication
  bursts”) was committed **07 Sep 03:03:38 UTC**, after the first affected
  06 Sep 22:35 upgrade. It bounds large live events by requesting a checkpoint
  resync; this is not a time-based rate limiter. It cannot explain the initial
  upgrade's regression.
- Tombstone/schema normalization and preservation of large source projections
  are separate changes. The reproduced failure requires neither malformed
  documents nor a SQLite lock.
- The earlier field document already records stalled initial pulls and the
  Threads module query storm before this upgrade. Its later correction
  explicitly retracts several “zero server writes” claims due to a SQLite
  REAL/TEXT comparison error. Do not reinterpret the older release as an
  unconditional healthy baseline.
- The reported `cockpit-projections` CPU load remains a separate finding.
  This transport regression reproduces without cockpit projection work;
  CPU saturation is not required to trigger it.

Source incident reports:
[initial pull field report](../ctox-sync-feldbefund-20260906-erstpull.md) and
[upgrade acceptance](../thesen-outbound-abnahme-nach-upgrade-20260905.md).

## Minimal correction

A `QueuedTransferGuard` marks the interval from dequeue through the complete
send outcome of one queued message. While that guard is active,
`await_queued_send` does not consume the draining caller's own ready receipt.
It can return the receipt at the existing yield between whole queued messages.

This keeps inline priority preemption on the wire: a small command response
still arrives before the last chunk of a large response. It changes only
when the caller can finish owning the drain. Existing cancellation,
connection-generation retirement and reconnection guards remain in force.

No schema, checkpoint, browser database migration, protocol change, timeout
increase, extra retry layer, HTTP data path or IndexedDB wipe is required.
This does not make arbitrary external cancellation of the drainer impossible;
it fixes successful completion of an interleaved receipt cancelling a foreign
message.

## Deterministic negative and positive control

New regression:
`interleaved_own_receipt_preserves_the_other_collections_transfer`.

It executes the actual native queue, frame splitting, priority preemption and
ACK parser. Only the SCTP link is replaced with a recording/ACKing test channel.
A large `user_thread_states` response is queued before a small
`business_commands` response; the small response's caller owns the drain.

- Pre-fix implementation + new test: **fails in 0.01 s** with
  `WebRTC send queue result dropped`, `expectedPeerTeardown: true`.
- Fixed implementation: passes; both receipts complete, the reconstructed
  large response matches every byte, the small response precedes the final
  large chunk, and retry count is zero.
- Full native crate suite: **431 tests passed**, zero failures/ignored.
  Breakdown: 394 + 31 + 1 + 1 + 4 across test executables.
- Browser runtime suite: **119 passed, zero failed, zero skipped** with
  `--require-wire-daemon`, repeated successfully after incorporating
  `origin/main` at `6d70b8ba0`. The native wire fixture executable is the
  existing Mac build; the changed native sender was separately compiled
  and tested on Linux.

Reproduction:

```sh
greppy bash-smart -- cargo test --manifest-path src/core/rxdb/Cargo.toml \
  interleaved_own_receipt_preserves_the_other_collections_transfer
greppy bash-smart -- cargo test --manifest-path src/core/rxdb/Cargo.toml
greppy bash-smart -- node src/apps/business-os/rxdb/tests/run-all.mjs --require-wire-daemon
```

Use task-specific build/temp paths as required by AGENTS.md.

## Browser acceptance and performance fixture

`SMOKE_MODE=multiplex-reload-browser-to-rust` extends the existing
`browser_rust_smoke.js` harness. The helper
`tools/multiplex_reload_probe.js` seeds **only its isolated test database**.

It uses Chromium, the actual Business OS shell, browser IndexedDB,
WebRTC replication, the native daemon and native SQLite. It first runs the
existing 21-file / 11,130,431-byte source preservation test and restarts the
native daemon. Then it performs three browser reloads in the same context:

- Start 21 additional collections, compete with demand reads and ten real
  policy-gated `ctox.provider_subscription.status` commands per reload.
- Require all 21 collections to report initial replication complete.
- Read all 3,911 thread-like records through bounded 200-row demand pages.
- Check 36 live lead-like records on the first reload.
- While the page is disconnected, advance one fixture-native record from
  revision 1 to 23 and tombstone 17 others. Require revision 23 and 19 live
  records on both subsequent reloads, keeping IndexedDB intact.
- Enforce a 60-second deadline measured from navigation start per reload.
  Record per-command round trips and total reload durations.

The fixture source writes, revision advance and tombstones are **test setup**,
not a product repair path and never executed against a tenant database.
The fixture validates the browser data plane; it is not a full acceptance of
the tenant's Outbound research workflow, external integrations or every app UI.

Remote isolated test root:
`/home/ctox/.cache/ctox/file-preservation-fbeca32b7`.
The optimized baseline binary is `fcce19763` (still containing the regression),
SHA-256 `62b88db86ce03ebcba8f3cd6571018c56f110b3dfd45ab7862f90cd034edba4e`.
The fixed full daemon was built from that same source snapshot plus this native
file correction with `cargo build --bin ctox --no-default-features`,
SHA-256 `38c28b2afaf4793cc507ddda87a8b50ab87505d80f199831f8268dc7d92b31a6`.
Both use signed shell `0.1.46-beta.18` in their isolated runtime roots.
The fixed candidate is a **debug** build; differences against the optimized
baseline are not an apples-to-apples performance comparison.
It must not be activated against a newer shell/schema contract.

Initial fixture attempts were rejected, not counted as acceptance: the
diagnostics object was incorrectly called as a function, the test module
lacked its static entry assets, and a single demand page was mistaken for the
entire 3,911-row collection. These fixture errors were corrected before the
reported measurements.

## Measured results

The complete raw reports are versioned next to this document:
[baseline browser](evidence/ctox-sync-interleaved-receipt-20260907/baseline-browser.json)
and [fixed browser](evidence/ctox-sync-interleaved-receipt-20260907/fixed-browser.json).

| Test build | Reload 1 | Reload 2 | Reload 3 | Result |
|---|---:|---:|---:|---|
| Optimized baseline, fcce19763 | 15,292 ms | 18,290 ms | 19,397 ms | 21/21 complete, 30 commands completed |
| Debug candidate with fix | 23,209 ms | 28,103 ms | 16,447 ms | 21/21 complete, 30 commands completed |

In every round, all 3,911 thread-like rows arrived. The lead count changed
36 → 19 and revision 1 → 23 on the second reload and stayed correct on the
third. The large source-file hashes also remained intact across native restart.
Both runs reported zero browser warnings/errors, failed requests, asset errors,
cache repairs and unknown process signals. Neither run logged
`ctox_webrtc_incoming_transfer_stalled`, `masterChangesSince` request timeouts,
`peer_connect_timeout` or `WebRTC send queue result dropped`.

**The browser comparison is positive acceptance, not a deterministic negative
control:** this baseline browser run also passed. The actual negative control
is the forced native queue interleaving test above. Three successful reloads
do not establish a production percentile or exclude other causes of tenant
stalls. The optimized/debug comparison must not be presented as a speedup.

The native crate is byte-for-byte unchanged between fcce19763 and main
6d70b8ba0 before this correction (`git diff fcce19763..6d70b8ba0 -- src/core/rxdb`
is empty). The corrected handler file SHA-256 matches locally and in the
Linux test build:
`0ca99f5234a09648b3debee9c9527aef5e6a06bfc644626e45f7c45262e1d041`.
Newer unrelated native Business OS/Crew changes were not in that isolated
full-daemon test binary. Native builds emitted pre-existing warnings;
the test commands and full daemon build exited successfully.

A separate run of the existing
`SMOKE_MODE=command-roundtrip-timing-browser-to-rust` fixture with the fixed
debug candidate completed all 30 measured commands after one warm-up:
**p50 431 ms, p95 576.7 ms, maximum 590 ms**, no browser errors.
[Full seven-stage timing report](evidence/ctox-sync-interleaved-receipt-20260907/fixed-warm-command.json).
The <300 ms warm-command target was **not met in this debug run**. No optimized
fixed-binary benchmark or critical-collection boot p95 was executed in this
incident fix; do not claim those architecture performance gates passed.

## Operator upgrade acceptance

Build/release the merged main with its matching native/schema/shell contract.
The transport fix itself does not require a shell change or storage reset.
Keep the existing browser journal and IndexedDB so queued writes and actual
checkpoint recovery are tested, rather than hidden by a wipe.

After the operator upgrades thesen.ctox.dev, measure three authenticated
reloads (including one fresh browser context), all active collections reaching
complete in under 60 seconds, lead `lead_1qjm3z5` at the server's researched
revision/content, delivery of the known tombstones, and journal acknowledgments.
Capture incoming transfer/RPC errors and timings. Compare actual payloads and
command receipts, not only “peer up” or aggregate frame counters.

The tenant's CPU profile and the architecture programme's separate warm
command p50 <300 ms / critical-collection boot p95 <5 s remain separate
measurements; a 60-second tenant recovery check does not replace them.
