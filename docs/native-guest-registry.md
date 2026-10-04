# Native guest registry and controller integration

The native lifecycle constructor is `business_os::NativeGuestRegistry::new` with
the actual already-running ExecutionAuthority and native capability requirements.
It pins the canonical Business OS SQLite file and instance identity. Capability
requirements are requirements for admission, not advertised or measured guest
capabilities. This source does not install a production owner or enable the VM
catalog.

An authenticated human provisioning owner enrolls an existing project, project
chat and worker profile against a private canonical import parent. Enrollment
checks native project ownership/status, active project membership, profile and
assigned computer, and the corresponding open non-archived native thread. The
registry generates guest/controller IDs. Its scope comes from the actual
authority; no client or model can supply an instance/scope/path to admission.
The generated mutable runtime directory is separate from the signed imported
checkpoint. A duplicate assignment or reused import parent is rejected.

`registry.admission(guest_id)` supplies the scoped concrete
NativeGuestAdmissionOwner to Architecture's existing NativeGuestAdmission.
Resolution uses actual worker/provider/account guard, then a separate canonical
Business OS IMMEDIATE transaction, then the shared controller lock. The worker
and policy stores are different files: the held worker transaction alone cannot
fence policy. Policy opens READ_WRITE without CREATE/migration, pins device/inode
before/inside/after publication and rejects replaced instance/store/import-parent
identities. The registry retains the actual runtime-root directory descriptor and
compares it with the live path before/after publication. Admission receives the
opaque provider's actual worker root and rejects another root even when principal,
account and row IDs match. Binding and subsequent guest operations recheck this
same relationship; a claim or witness JSON does not supply it. Policy revision hashes the actual project/profile/computer/member/chat/
thread records. The signed principal, command lifetime and exact admitted native
attempt must match; fresh resolver checks happen around quorum admission.

`bind_execution` validates actual ownership with the running authority, the
immutable provider/account/harness contract, and the actual matching Admitted
worker-store row. It binds the complete admitted destination/policy revision.
GuestRestoreOwner publication invokes exactly once under those same held native
fences. It records Uncertain before the callback; a failed/uncertain attempt
cannot be replayed. Receipt registration requires completed protected checkpoint
effect, exact native effect/target derivation and directory identity. Import
presence alone cannot grant readiness.

The native PersistentSession factory requires this actual registry and an
already enrolled guest ID. Before TurnStart, it consumes its own Admitted row
under the actual borrowed worker/provider transaction and invokes
`bind_execution` for a fresh quorum/provider/account/policy/controller check.
Command authority is rechecked after each admission/binding await. Missing
registry, changed authority, ambiguous binding or a second execution attempt
poisons the session instead of falling back. The retained NativeGuestExecution
is an observation handle; operations must still revalidate their live fences.
Production server lifecycle registration/enrollment and actual guest/controller
workflow acceptance remain unfinished; this constructor is not advertised as a
working installed guest lane.

Linux process startup retains a native-generated guest-process attempt before
the first quorum await, accepts only a fresh exact BeginEffect result, then
retains the actual paused QEMU child before bootstrap. Its runtime/overlay must
belong to this assignment. The unresolved process effect remains open for the
entire actual process lifetime, preventing ordinary remote takeover. No boot,
capture, QMP reply, failed stop or directory can complete it. Native human stop
revokes the shared controller first, stops only the exact retained child, and
keeps its observed exit/pending effect for explicit reconciliation. Worker expiry
does not stop the human from terminating that child.

Readiness requires the registered exact import and fresh actual retained-process,
guest-session and local endpoint/capture observation. Native worker/command expiry
is rechecked after blocking observation and before publication. The native owner
registers exact process-effect metadata from the retained child before bootstrap
and supplies GuestReadyObservation under the shared guard. Architecture's
consumer allows exactly that pending effect and rejects foreign/additional effects.
Unknown old effects, uncertain starts,
missing/replaced guests and non-Linux runtimes deny readiness.

Five source regressions use canonical native policy records and SQLite locking:
foreign/duplicate/unregistered enrollment; competing policy writer and changed
policy/controller; replaced store/import directory; missing/replaced native instance
identity without automatic recreation; archived project/removed member/closed
thread/expired worker. The authority fixture rejects all operations,
and provider metadata is a component fixture: these are policy/controller checks,
not actual quorum/provider/QEMU/two-host acceptance. Tests are UNRUN until the
source-bound shared resource gate admits them. Root compilation, final producer
composition, Linux compilation/real image startup, command/frame delivery,
confirmed-stop effect reconciliation and real two-host acceptance remain required.

## Atomic process checkpoint transition

The native quorum operation `CommitEffectCheckpoint` validates the exact sole
pending effect, current ownership and independently signed durable copies from
at least two eligible replicas, including the owner. The checkpoint sequence
must advance. It installs that fresh checkpoint and completes the effect in one
committed Raft entry and one SQLite state-machine transaction. Rejection leaves
both checkpoint and effect unchanged; a repeated request returns evidence only.

Every newly begun effect persists `checkpointRequiresRefresh`. Completing an
effect alone does not clear this flag and cannot authorize takeover from the old
checkpoint. Ordinary checkpoint protection clears it only with no pending
effects and a strictly newer authenticated checkpoint. Guest staging and commit
also require this freshness flag to be clear: completion between staging and
admission cannot publish the old checkpoint. Legacy serialized jobs
without the flag default to requiring refresh, never to an invented freshness
proof. The previous protected checkpoint remains available as evidence while
the process is running.

The native lifecycle owner must prove the exact retained child's exit and
capture an actually consistent stopped-guest checkpoint before invoking the
atomic operation. Signed copy receipts prove durable bytes and execution binding;
they do not prove guest shutdown or application consistency. No browser/IPC
operation is added, and the registry still retains its effect after stop until
this real lifecycle reconciliation is connected.

New signed-RPC/independent-SQLite regressions race old-checkpoint takeover against
the atomic commit, reject old/forged/incomplete copies and foreign/additional
effects, verify replay and restart persistence, and deny stale takeover after
ordinary effect completion. These source regressions remain UNRUN at this
revision while the existing DevOps resource unit owns the admission gate.
