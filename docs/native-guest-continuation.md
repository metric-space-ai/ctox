# Protected native guest continuation

The production receiver, rather than a general Transfer job, owns the target
checkpoint binding. Transfer metadata, copied bytes and reconstructed files
cannot create execution authority.

The private host's current signing identity, account, policy, controller and
checkpoint receiver must remain live. The Sync execution group requires three
distinct real voters and at least two executor/DATA voters. Each host provisions
its own account credentials; credentials are never transferred with a checkpoint.

## Operator and application path

Use the existing `ctox sync configure` and `ctox sync configure-guests`
interfaces with host-local typed configuration on stdin. Guest configuration
selects the actual model/account/harness, workspace and Linux QEMU/base/profile.
It becomes active when that isolated host starts.

The current protected handoff uses the original binding throughout:

1. Enroll the actual source capture with `handoff-enroll-source`.
2. Receive it using `handoff-copy`, reconstruct the private workspace using
   `handoff-reconstruct`, then issue the verified `handoff-acknowledge-copy`.
3. `handoff-protect-checkpoint` consumes actual independent signed DATA
   receipts. `handoff-take-over` changes ownership of the original job.
4. `handoff-enroll-guest`, `handoff-import-guest` and
   `handoff-restore-guest` retain the original guest/service session,
   protected import receipt and real QEMU child at the target.
5. The target's authorized `business_os.chat.task` producer uses the matching
   configured profile, project/chat, model/account and workspace. Its actual
   Core constructor loads the original Core UUID from the protected receiver.
   The native provider then admits the original Owned job and binds that
   producer to the retained target child before the first Core turn.

The final step is an application command through the existing native command
plane. It is not a new activation CLI, a new Create job or a Transfer-completion
callback. The original account/model/harness/session tuple must match; a changed
tuple is rejected. The original target ownership, checkpoint digest and sequence,
completed import effect and exact pending QEMU process effect are revalidated
around quorum awaits. Unknown effects remain rejected.

## Runtime ownership

The receiver retains the actual original Core owner and host lifetime. The
verified immutable imported journal is copied to a separate private writable
Core journal. The retained working journal identity is checked before target
publication; it is never adopted from an arbitrary request path.

Only the native machine restore can publish its readiness receipt. Admission
physically checks that the exact retained child and guest endpoint are still
live. Binding moves that child and its existing I/O runtime into the registry
once. Core turns do not boot another QEMU process. The assigned workspace writer
lease remains required.

The actual worker/account/policy/controller and receiver fences protect guest
commands and frame publication. Capability and native signing records are read
under one encrypted issuer fence for frame operations. They are not acquired
recursively while holding a worker transaction. A failed first-turn binding
retires the target attempt and stops only its retained child, preserving its
pending process effect for reconciliation.

The final native Core configuration pins the existing canonical backend URL
after profile/CLI overrides, in addition to disabling inherited background
execution. Ordinary non-native sessions keep their existing configuration.

## Evidence limits

These connections require a Linux source with the real source boot/capture
implementation and a Linux target with protected import, machine restore,
original Core construction and original-job admission. A Darwin transport build
does not supply the Linux machine witness.

Import and machine-only CLI responses remain `resumed:false`. Successful
constructor or source tests do not prove installed continuation. Native Core
observations still preserve generic unknown effects; a successful turn, empty
caller-provided list, completed import or stopped child cannot mint a clean
effect receipt. Until that production clean-effects boundary is implemented
and measured, goals 15/16/18 remain open.

Installed acceptance must demonstrate the original session continuing on B,
A rejected as stale after takeover, reconnect and abort, on isolated tenants.
No customer fault test or baseline build is implied by this document.
