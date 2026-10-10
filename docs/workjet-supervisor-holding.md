# Native selected-Supervisor holding fence

The service captures a NativeSupervisorExecutionLease from its actual signed,
restricted command or confirmed-plan session. It seals the requested Luma under
the original native lease. This is a private Rust value, with no Serialize,
Deserialize or Debug implementation. No route DTO or ConsumerFacts JSON can
construct it. An unset project selection retains the current instance-default
path; a selected route still fails explicitly until the genuine holding producer
is connected. Capture never populates actual_json.

The native guarded holding handler supplies its captured
AdmittedConsumerAuthority to NativeSupervisorHoldingController::claim. The
controller owns that exact admitted connection generation. The current native
Owner and selected computer must match. A durable Core primary key admits only
one controller for an original execution/lease hash. Cancellation or completion
does not allow a second controller to restart that same lease.

NativeSupervisorHoldingController::with_current holds the source transport,
issuer, Core and Policy fences, in that order. It revalidates the original
command/confirmed-plan lease, Supervisor binding/epoch, requested selection,
account revision/catalog/policy and exact controller state before its bounded
callback. Prepare private account/secret snapshots before entry. Inside the
callback do not await, wait for network work, enter secrets/transport/account
APIs, or retain connections. Re-enter before actual dispatch, after every
asynchronous boundary and for every stream/result publication.

The Models holding-native proxy composes this callback with its prepared
private account snapshot and model validation. Its Root consumer receives only
a per-lease opaque capability, never an account OAuth credential. The model
proxy must record the genuine upstream exchange. Harness supplies the real SDK
session/turn/stop witnesses. Neither this controller, account eligibility,
requested route, Source terminal report nor capability retirement proves a
physical model invocation or SDK stop.

cancel permanently retires this private controller even if its original
connection or lease is gone. A DB error cannot restore its in-process permit.
The proxy must also retire its scoped capability, and the genuine producer must
separately stop its turn. No other queue task or controller is affected.

Regression fixtures check single-controller admission, changed Owner/computer,
expired/replaced/cancelled/terminal original leases, changed selection/binding,
account/catalog/policy changes, retired-controller rejection and rollback.
The combined source/Core/Policy test verifies current native issuer/device
policy plus simultaneous writer exclusion. These isolated fixtures are not
installed Molecularity, network-holder or physical execution acceptance.
