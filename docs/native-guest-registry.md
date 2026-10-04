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
identities. Policy revision hashes the actual project/profile/computer/member/chat/
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

Four source regressions use canonical native policy records and SQLite locking:
foreign/duplicate/unregistered enrollment; competing policy writer and changed
policy/controller; replaced store/import directory; archived project/removed
member/closed thread/expired worker. The authority fixture rejects all operations,
and provider metadata is a component fixture: these are policy/controller checks,
not actual quorum/provider/QEMU/two-host acceptance. Tests are UNRUN until the
source-bound shared resource gate admits them. Root compilation, final producer
composition, Linux compilation/real image startup, command/frame delivery,
confirmed-stop effect reconciliation and real two-host acceptance remain required.
