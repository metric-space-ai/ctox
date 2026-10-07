# Native paused QEMU memory checkpoint

The Linux native guest owner can retain a QEMU memory/device stream without
starting a second executor. These are process primitives, not enrollment,
protected transport, a completed import receipt, clean external effects or
permission to execute. They do not yet connect the production Core handoff.

`RetainedQemuDesktop::save_memory_live` checks the actual child/service endpoint,
retires live desktop access, pauses the child and saves RAM/device state through
a native-owned private Unix socket. The process owner records the exact stream
length and SHA256. Export succeeds only after QEMU reports completed migration
and the source is stopped in postmigrate. The source cannot resume through this
owner after an export attempt. The caller must confirm that exact child's exit
before copying its writable overlay or completing the process effect.

`RetainedQemuDesktop::spawn_incoming` uses `-S -incoming defer`.
`load_memory` verifies the entire native-owned readonly stream before feeding
it to QEMU, verifies the fed bytes again, and returns only with QEMU paused.
`activate_restored` is a separate execution effect. Its caller must hold fresh
current native account/worker/policy/controller and execution-ownership guards
through the effect. Readiness requires the original guest-service session from
the protected checkpoint, the actual retained target child and a real capture.
A newly booted service session cannot pass restored readiness.

The socket peer must match the retained QEMU PID. No shell, exec migration URI,
TCP endpoint, renderer-selected path or automatic fallback is introduced.
The stream is capped at1GiB, uses128KiB buffers and has a120-second deadline.
Files must be regular, private, native-owned and unaliased. Restore rejects
mutable files, wrong lengths and wrong hashes. Cancellation or failure retires
the attempt without releasing the child; stop/reap and effect reconciliation
remain mandatory. No failed migration is automatically replayed.

The native checkpoint owner still needs to bind immutable base hash, writable
overlay contents, portable hardware profile, original guest and service identity,
and source execution history into its protected manifest. RAM must be split into
ordered bounded blobs (for example8MiB) for the existing64MiB/blob and1GiB total
transport bounds. Only a verified complete protected target import may supply
these native descriptors. Transport/admission limits are not raised here.

The actual QEMU roundtrip regression proves a memory stream can be exported,
loaded without automatic execution and explicitly resumed; a corrupt stream
retires incoming execution. It uses isolated empty test disks, not a guest OS.
Installed production guest readiness, revocation and GPU3→GPU4 continuation
remain separate outstanding acceptance under issue183.

Protocol references:
- [QEMU QMP migration commands](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html#migrate)
- [Managed startup](https://www.qemu.org/docs/master/system/managed-startup.html)
