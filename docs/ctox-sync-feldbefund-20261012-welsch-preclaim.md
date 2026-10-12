# WELSCH pre-claim delay: measured bounds and intake failures

Measured 12 October 2026 at 09:41:11–09:41:12 UTC using managed-tenant SSH,
SQLite mode=ro and query_only. WELSCH still runs native-main-c7ff9ac3af41:
the /proc/255152/exe hash is
3d0034fbf94447bfd65e3a4b68d34788fa7ca81914c7eb32e67d53855192155b.
Workjet's public Info.plist independently reports 0.0.89. No production
restart, configuration change, command insertion or protected app-file read
was performed.

## The 37 failures have a concrete cause

Seven ctox.workjet.providers.list commands failed with
"provider commands do not accept record_id".
Six ctox.workjet.session.create commands failed initial intake with
"record_id does not match deterministic Workjet session id".
Their further 24 attempts report "canonical command replay remained nonterminal".
Those six aggregates still have execution_phase=accepted and
terminal_status=none. Thus 7 + 6 + 24 = 37 failure attempts over 13 commands.
This is a client/native command-contract mismatch, not proof of the cause
of the project-list push delay. Instances owns the client identity repair;
native replay terminalization belongs to the command-plane owner.

## Historical wait is before claim construction, with a telemetry gap

The two supplied IDs ff4c5a33-4289-4a9b-b126-4bf2069880d4 (meeting) and
81a9c30d-34f2-40ba-a37d-3ef152f3ddbb (KPIs) exactly match native aggregates.
The public Desktop trace places both starts at 10:12:36.864 UTC on 11 October;
claims were constructed at 10:15:13.882 and 10:15:14.814 UTC.
Both completed natively in 26/30 ms after the claim. The desktop requests
already timed out after 35.134 seconds, marked desktop_delivery.
The shared guest preparation took 5.688 seconds; authentication refresh and
managed launch completed much earlier than the claim burst.

The exact deployed command_plane.rs authenticates the replicated session and
stamps its identity before constructing the core claim. The optional
command_intake_sample timing probe records these phases, but none of the
469 selected commands in the fresh 24-hour sample requested it. The journal
has no first-arrival event for these IDs. The initial native document
timestamp has since been overwritten by canonical projections.

Consequently the 157–158 second wait cannot be assigned honestly to guest
push, native pre-claim authorization, or intake scheduling from these
historical records. Owner/source metadata also does not identify one guest.
A physical document's later lastWriteTime and an outbox delivery timestamp
are not first ingress or browser receipt.

## Evidence and next attributable measurement

Durable evidence outside the repository:
- devops/welsch-guest-correlation-20261011.json
- devops/welsch-preclaim-readonly-20261012.json
- devops/welsch-preclaim-public-desktop-trace-20261012.json
under /Users/michaelwelsch/.codex/task-evidence.

Instances/Shell must preserve a stable command ID plus push-start/ACK/result
receipt for one current request; Architecture must pair first native ingress
and the existing command_timing_probe authentication/identity timestamps.
These are the minimum missing observations to separate the pre-claim stages,
not a new release gate. No historical root-cause or installed sync pass is
claimed. The goal-5 runner separately reuses the exact published c7 binary
in a fresh synthetic tenant with two real Chromium clients.

## Current recheck and native timing bound

The public 0.0.89 Desktop trace contains 47 completed WELSCH project.list
requests and no failed list request in the measured window after 08:15 UTC.
Nineteen meeting/KPI commands correlate exactly by ID with the native sample:
start-to-claim is 93–1936 ms. The fresh native subset contains 27 list
commands, all completed/count12; claim-to-terminal p50/p95 is 29/40 ms.
The original 158-second delay was not reproduced in this window.

At 10:12:08 UTC the same native PID255152 reports a process-cumulative
business_commands loop maximum of 620 ms. The exact c7 source times and
awaits the full source-stamp read, pending-document scan and native acceptance
(including spawn_blocking scheduling/authentication). Its static atomic
maximum increases and snapshots only load it. This bounds completed intake
iterations; it excludes a 158-second synchronous authentication/handler wait
inside those iterations, but does not distinguish guest push, incoming
replication, idle notification or a gap before a new iteration starts.
No other long-running aggregate overlapped the original burst.

The same sample has two open data channels, but 47,902 queued native frames
with oldestQueuedAgeMs=83,721. That is a measured transport backlog requiring
Architecture's inspection, not a retrospective proof that this exact queue
caused the two historical command delays. Per-command first-ingress and
push/ACK observations are still missing.

## Goal 5's fresh installed measurement

09:55:09–09:56:11 UTC: the initial isolated startup hit a native key creation
conflict. Sequential native invitation bootstrap before starting the peer
and static server removes this startup failure in a new prefix.

10:01:03–10:02:04 UTC: the second fresh prefix uses the exact c7 release
binary and canonical packaged browser DB/sync modules in Chromium148.
The peer is alive, signaling socket connected and join accepted, but
peer_authenticated/data_channel_open/replicationUp are false.
The browser sees no matching native descriptor, no RTC connection and no
sent frame. Baseline A=1/1 exact (local write5.2ms), native=0/1, B=0/1 after
60 seconds. The 200-write rounds were not run; goal5 is pass:false.

A, B and the bootstrap invitation have identical fingerprints for instance,
native peer ID, room and signaling URLs. No private invitation, secret key,
cookie or app catalog is exported. The discovery/handshake boundary remains
an owned Shell/Architecture dependency; native keys must not be rewritten
to hide it. All browser/native groups were stopped. No production tenant
was used as the fault fixture.

Durable goal receipt:
 /Users/michaelwelsch/.codex/task-evidence/teilziele/05-current-c7-native-discovery-20261012.json
