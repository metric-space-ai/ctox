# Installed b48220: baseline remains local before an authenticated native channel

## Exact run and scope

The single authorized diagnostic rerun was
`ctox-installed-sync-acceptance-20261008T053355Z` on gpu3. Runner PR425 was
normally merged as `befb06d609d82d2c389c81a7c08f27bf42f11e0d` after the final
`418ad5d8bb302b8c5c5c6122eb57fa64bead9475` syntax, Python AST, four privacy/metadata
tests and whitespace checks passed. Runner script blobs are unchanged by merge.

The installed native package remains `native-main-b48220db385d`, binary SHA256
`f07578594780e137a3c00dda58da5a6953909647f45df758eb6b3e0a1b512053`.
A new synthetic prefix and canonical actor UUID were created; neither customer
state nor an existing production identity was copied. Two contexts used the
installed canonical DB/schema/sync modules in matched Playwright1.60 Chromium148.
Workjet0.0.54 was independently re-read as context; this was a browser module
measurement, not a visible Workjet/Shell acceptance or qualified worker fixture.

## Reproduction and counts

The first assertion reproduced at baseline, before any fault. Baseline started
2026-10-08T05:35:26.209Z and failed at05:36:26.343Z; the convergence loop elapsed
60120.722ms. The subsequent diagnostic snapshot completed at05:36:29.934Z,
with3590.741ms of snapshot overhead, outside that convergence interval.

| Phase | A acknowledged writes | A found/exact | Native found/exact | B found/exact |
|---|---:|---:|---:|---:|
| Baseline, one document | 1 | 1/1 | 0/0 | 0/0 |
| All15 fault/catchup phases across the three criterion rounds | not run | not measured | not measured | not measured |

The local baseline upsert took9.600ms. This single write is not evidence for the
200-write latency requirement. Native counts come from mode=ro
`business-os-rxdb.sqlite3::ctox_business_os__desktop_icons__v0`, not the legacy
`business_records` projection. In the retained original attempt, a later
read-only inspection found zero rows in both tables too.

The missing document ID is
`acceptance-ctox-installed-acceptance-5-e508d1f1-4181-4192-9de0-7e484fc0d13a-connected`.
A has HLC `_meta.ctoxHlc = muz3ro5g:0:0ecee38e-d468-4053-beeb-2e0160206fc4`
and `_meta.lwt = 1791437726209`; its returned payload has no `_rev`, recorded
as null. Native and B have no document, therefore no revision/HLC to report.
No arbitrary document payload or credential is exported.

## Established diagnostic boundary

At the divergence snapshot the native process and command consumer were alive,
heartbeat fresh, signaling socket connected and join accepted. However,
`peer_authenticated=false`, `data_channel_open=false`, `replicationUp=false`.
Both browser diagnostic histories include `peer_connect_timeout`; their last
collection status is connecting, `initialReplicationAt=null`, and neither has a
successful observed wire pull. A reports one unsynced desktop_icons write and
zero conflicts; B reports zero unsynced writes and zero conflicts.

The exported native log excerpt at05:35:25.115Z contains the vocabulary WebRTC /
replication,175bytes, SHA256
`62cc1d46435075dec995d4951f1ba694d902464b61a0e5dbe362183a293222a0`.
That startup phrase is not proof of an authenticated channel. Raw rolling
native excerpts stay in the private isolated host prefix. No browser console or
pageerror matched the bounded capture vocabulary; the observed timeout codes
come from the structured browser diagnostic history. Empty console excerpts do
not prove absence of a failure.

This establishes failure before authenticated native delivery. It does not yet
separate a fixture/reachability issue from a browser or native handshake defect,
and does not establish data loss, a merge bug, or a clock-skew bug. Changing the
synthetic actor to a UUID did not remove this symptom; causation is still unknown.
Shell owns browser diagnosis and Architecture owns native diagnosis. DevOps has
not patched either production implementation or started a second rerun.

## Original preemption and cleanup

The earlier run's baseline failed at03:38:18.954462Z. At03:39:44.626497Z DevOps
manually stopped its own P1 GPU unit so a waiting P0 GPU fixture could support
Main's already admitted short Mac UI window. Mac and GPU gates are separate;
there was no shared reservation or automatic cross-host preemption. That later
prioritization decision does not explain the already recorded baseline failure.
The present rerun was not interrupted: controller and canonical gate terminated
with a negative measurement, all owned native/browser groups were verified absent.
Both actual screenshots were retained and inspected; they show the manifest
page used as the browser module host, not a Shell UI. Private state/log custody
remains for responsible-owner diagnosis. No retry, watcher or customer fault ran.

## Evidence

- `~/.codex/task-evidence/teilziele/05-installed-sync-b48220-one-rerun-20261008.json`
- `~/.codex/task-evidence/devops/installed-sync-b48220-one-rerun-20261008/`
- `~/.codex/task-evidence/devops/installed-sync-one-rerun-control-20261008.json`
- `~/.codex/task-evidence/devops/pr425-final-head-check-receipt-20261008.json`

Host custody:
`/mnt/nvme1/ctox-acceptance/devops-sync-b48220-diagnostics-20261008`.
Only one diagnostic rerun was authorized and consumed. Goals5/6/7 remain open.
