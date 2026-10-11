# S0-4: browser storage, peer CPU, bounded lock observation

Builds on #598 with the same 9250-document / 257848000-byte fixture and real
0/300/600-ms datagram relay. No production engine, authority, schema or runtime
configuration changes. S5 targets from the owner: <=10 RTT-equivalents to visible
data at600ms; native write ACK-p95<=3RTT. This slice measures, it does not optimize.

Browser phase marks trigger CDP Runtime.getHeapUsage and origin-scoped
Storage.getUsageAndQuota. Report used/allocated heap, sampled peak and signed
start/end growth; indexeddb usage is Chromium's physical quota estimate, not
serialized row size. Two idle checkpoints10s/20s after the five writes expose
short-runtime growth. No forced GC or memory-leak assertion. The heap includes
shell, app, and bounded measurement observer. Samples and collection overhead
are retained; unavailable meters fail rather than silently report zero.

CPU comes from /proc of the exact harness-owned server PID and named native peer
Tokio/supervisor threads (Linux comm is truncated to15characters). CPU deltas use
actual CLK_TCK and process/thread start identity; reuse or resets fail. Thread births/exits carry explicit CPU lower/upper bounds (stable observed peer work/all process work), not invented exact CPU. Server
CPU and peer-thread CPU are separate. Browser-to-Node callback reception is the
CPU boundary; browser timestamps and meter overhead are recorded. This observes
sys+user CPU, not stacks or exclusive attribution to one asynchronous request. Even a stable observed thread set cannot prove there were no unseen short-lived peer threads: named-thread CPU is the observed sum, with all-process CPU as its upper bound.

The normal lane runner exercises21resource/transport tests, a real SQLite lock
probe self-test, all three browser/native relay cases, and a20s end-to-end soak
on the0ms case. The long mode86400 starts systemd --user unit
ctox-sync-v3-s0-locks-20261010 from inside the same normal admitted lane. The lane
slot stays owned until this bounded unit terminates; no admission bypass. Unit:
CPUQuota200%, MemoryMax6G/no swap, RuntimeMaxSec86900, control-group cleanup.

The soak uses a new isolated prefix and the same published7e21native binary. Eight
concurrent browser upserts every~30s must receive exact native accepted ACKs and
pass independent SQLite readback. Every~5s a bounded writer-admission probe opens
the same fixture SQLite store, attempts BEGIN IMMEDIATE, and immediately rolls
back without changing data. It records actual SQLITE_BUSY/LOCKED retries with
10ms retry sleeps and a500ms budget. Native logged lock errors are counted without
retaining messages. Unlogged native busy-handler wait time remains UNKNOWN;
this is not THESEN-load or customer acceptance. No WELSCH/THESEN restart or fault.

The bounded status JSON is atomically replaced and retains aggregate counts plus
only20recent probes. RUNNING has pass:null; completed coverage is known only after
the deadline. Failure is filed pass:false. Live result on gpu3:
/mnt/nvme1/build-lane/evidence/architecture/sync-v3-s0-resources/soak-20261010/status.json.

Query from the operator Mac:

```sh
ssh lan-gpu3 'systemctl --user show ctox-sync-v3-s0-locks-20261010 -p ActiveState -p SubState -p MainPID; cat /mnt/nvme1/build-lane/evidence/architecture/sync-v3-s0-resources/soak-20261010/status.json'
```

Normal qualification uses scripts/sync-v3/run-native-lane.sh SHARED_ARCHIVE
PINNED_PLAYWRIGHT20 (separate arguments); long observation uses86400. The lane
retains lifecycle/measurement evidence before removing its disposable prefix at
terminal completion. The supervisor evaluates the24h result tomorrow.
