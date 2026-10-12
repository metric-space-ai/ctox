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
