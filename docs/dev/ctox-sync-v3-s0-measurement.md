# CTOX Sync v3 S0: fixture and independent byte oracle

First S0 slice against the binding RFC v2, PR586 source
`6954a1fd36e0f4fc056d7f276b41fd0dd78c3ccb`.
The RFC/contract owner is Claude's “Outbound app Funktionsproblem (fork)” session;
Architecture is the single implementation lane. This is measurement tooling,
not an engine replacement, data-schema migration or performance acceptance.

## Reproduce through the admitted Linux lane

```sh
bash scripts/sync-v3-release-gate.sh /absolute/path/to/pinned/playwright
```

The existing release workflow runs this script after its locked Business OS
JS dependency installation. It requires the admitted runner's `TMPDIR`, writes
one fresh private fixture directory there, and removes only that directory on
exit. No database, service, tenant or existing browser profile is contacted.
The standalone generator can retain a fixture in a new output directory:

```sh
node scripts/measure-sync-v3.mjs --fixture /owned/scratch/new-fixture
node scripts/measure-sync-v3.mjs --verify-fixture /owned/scratch/new-fixture
```

The committed descriptor generates 850 lead envelopes of 80,000 bytes, 6,000
command envelopes of 29,500 bytes (177 MB), 400 task envelopes of 27,000 bytes,
and 2,000 chat envelopes of 1,024 bytes: 9,250 documents, 257,848,000 JSON bytes.
KB/MB are decimal UTF-8 bytes. Chat size is a provisional explicit assumption;
the draft specifies only its count. Each row has deterministic pseudo-random
ASCII filler so a repeated character does not compress away the bulk load.
Generation streams with backpressure; verification independently streams the
files and checks row lengths/counts and SHA-256 against their manifest.
Existing output directories are rejected. These are `fixtureOnly` size envelopes,
not executable `business_commands` or valid production schema seeds. The next
slice must map them into the actual isolated tenant's canonical schemas through
its authorized native seeding path before measuring real app queries.

## Byte semantics and oracle

`DataChannelByteMeter` consumes actual `RTCPeerConnection.getStats()` reports.
It samples each supplied connection once regardless of how many collection
handles refer to it, and keeps distinct channels and distinct connections even
when their stats IDs or labels coincide. Counter intervals bind the same meter,
connection and channel; missing stats, reset, reconnect, disappearance or unsafe
numeric totals reject the interval. Unknown bytes never become zero.

The counter measures **SCTP application payload bytes** from `RTCDataChannelStats`,
not Ethernet/IP/TURN/DTLS overhead or uncompressed logical document bytes. It
does not add the repeated per-collection `frameTransport.receivedBytes` values.
Supply connections from **one endpoint only**; observing both endpoints counts
both sides intentionally and is not a one-way traffic measurement. One snapshot
is sequential across supplied peers, not an atomic cross-peer network snapshot.

`counter-oracle.browser.mjs` opens one owned Chromium and a temporary loopback
static page. Two real DataChannels in the same browser send known UTF-8 and
binary payloads. Receiver delivery length and message count are independent
oracles for `getStats`; twenty aliases of the receiving connection must still
report one connection and exactly the sent payload bytes. The same measured
interval also reproduces naive per-collection aggregation: 1,146,880 bytes for
twenty aliases versus 57,344 bytes for the unique connection and independently
delivered payload. This is a counter correctness comparison, not an installed
engine performance before/after. Peers, browser and
HTTP listener close on completion/error; the browser has a 30-second bound.
This tests no customer or native CTOX instance and claims no relay performance.

## Remaining S0

This release check gates fixture integrity and counter correctness only. It
**does not** claim RFC §4 latency/size budgets are achieved, nor substitute a
synthetic self-test for installed measurement. Still required: canonical tenant
seed/import, actual shell start phases and app-visible-data markers, native RPC
histograms and write acknowledgements, baseline before/after reports, 300-ms
and 600-ms relay impairment, browser heap/IndexedDB baseline, server CPU and
write amplification baseline, 24-hour lock observation, and a measured regression gate in
the release pipeline. Whole-network overhead needs an independent transport
capture/counter if the RFC owner chooses that unit for the 2 MB budget.
No WELSCH live measurement before 12 October 2026 13:00 Europe/Berlin.

The controlling sequence is S0 measurement → S1 content-hashed module graph
and generated inventories → S2 six-behavior profiles, known/unknown/stale,
declared ownership and one business_commands writer → S3 session authority
context → S4 one projection writer per collection and retirement of shadow
tables → S5 lifecycle/transport → S6 specification. S1 is its own stage directly
after S0; hand-maintained asset stamps are retired there, not hidden inside
old-path cleanup. Pending local writes must drain before the RFC's IndexedDB
contract-change reload. This PR changes neither runtime assets nor stamps.
Merge follows final-head checks and the RFC owner's gate review.

## Preserved Transfer/peer-group work

PR579 is merged at `7e21f6052b633d5efaa85e8c5d6c1b420d527076`; frozen branch
`codex/native-peer-group-provisioning-wip` retains that source on origin. There
was no uncommitted group provisioning implementation and no runtime mutation.
PR560/566's native original-key enrollment/lookup and PR579's protected original
Core continuation are retained. Goals15/16/18 are not claimed complete.
The durable operator contract remains
`~/.codex/task-evidence/ctox-sync/goal15-current-receiver-contract-20261010.md`;
Transfer's actual prefix readback is
`~/.codex/task-evidence/ctox-transfers/goal15-public-prefix-readback-20261010.json`.
Both installed c565 prefixes have only their existing pairing user, no established
authenticated Owner or provider/workspace bindings; Linux has no native execution
group, Mac runtime configuration readback is unknown. Transport keys alone do
not grant capture/activation. No fixture voter, grant or copied credential will
substitute for the normal real Owner/group/controller provisioning. That open
work is sequenced inside v3, not pursued as a parallel Sync core change.
