# Guest desktop adapter boundary

This is an implementation seam for the Workjet VM tool, not an enabled VM feature. It adds a guest-local X11 driver and typed bounded actions next to browser_runtime. No browser operation, controller lease, Sync scheduler, transport, provisioner or collection is changed. Business OS commands `ctox.guest.observe` and `ctox.guest.input` are registered on the command plane. Public RxDB intake passes `GuestRuntimeInjection::Unregistered` and fails closed. A registered owner is an explicit extra argument on `accept_rxdb_business_command_with_guest_runtime`; this slice does not install a production owner. No model-facing tool or production VM provisioner is enabled.

## Required authority

GuestRequest contains only a guest ID and an action. GuestScope and GuestCaller are separate native inputs. Their strings are context claims that the injected authority must resolve against canonical state, not proof of permission. The scope preserves instance, human owner, project, thread, WorkerProfileId and guest identity; human session and worker execution/provider-session identities are separate.

GuestAuthorization has no default implementation:

- begin_observation checks current observe rights before capture.
- publish_observation rechecks the same binding/epoch after capture and publishes through the existing delivery owner. It returns the canonical frame ID. Pending frames must be invalidated by the existing delivery path on takeover/revocation.
- apply_input validates the current frame and input rights and owns the effect callback. An earlier successful ownership check is insufficient: the callback must execute under the current controller/fence and serialize or cancel against human takeover, expiry and revocation.

The adapter never returns raw frame bytes to a caller for later unchecked publication. It only acknowledges the authority's publication and returns that frame's identity. Native authorization integration and in-flight transport invalidation remain with the architecture task. A mock authority test does not prove those production guarantees.

## Typed driver operations

| Action | Bound |
|---|---|
| Observe | PNG; at most16MiB,4096px per axis and8,388,608pixels |
| Click | Left/middle/right; nonnegative pixel coordinates within current display |
| Type | Nonempty UTF-8 up to16KiB; no null byte; passed on stdin |
| Scroll | Four directions;1–20steps; coordinates checked against current display |
| Key | Finite navigation/editing key enum; no arbitrary helper arguments |

Inputs include a frame ID. The authority must resolve it to a still-current observation of this exact guest, including viewport and fence validity. Unknown action fields, actor overrides, arbitrary commands and invalid bounds are rejected.

X11GuestDriver runs only on Linux inside an already provisioned guest. It uses fixed /usr/bin/maim and /usr/bin/xdotool helpers, a native-provided local DISPLAY and absolute Xauthority path. Those are child-process settings from typed native configuration, not ambient feature switches or model payload fields. The driver verifies current display dimensions before effects and capture, bounds stdout/stderr and operation duration, kills/reaps its own child on failure or timeout, and uses kill_on_drop on cancellation. Typed text is not placed in process arguments or logs. There is no SSH hop, listener or background polling loop.

The provisioned guest image must provide the helpers and a dedicated X11 session. No install/provisioning is performed by the adapter or by enabling a future tool option. Hosting/guest registration, confirmed membership and authorization remain distinct. An arbitrary connected workstation must never be silently treated as a VM guest.

## Verification and remaining integration

Tests exercise pre-capture denial, revocation before publication, distinct observe/input rights, rechecking on repeated input, wrong guest/project/frame rejection, action parsing, input bounds and stdin argument safety. Linux CI additionally starts one owned Xvfb display and xev client, obtains a real PNG, checks pointer position and observes an actual typed keyboard event. Both processes are killed/reaped on completion; no user display is touched.

That test is a local virtual-display driver test, not proof of VM isolation, provisioning, production authorization, human takeover races or two independent guest targets. The latter remain required before release.

The command connector is `src/core/business_os/guest_commands.rs`. Payloads are bounded metadata only: a guest ID, optional scope claims, a frame ID for input, and a typed `GuestInput`. Receipts return `command_id`, `guest_id`, `outcome`, and an opaque `frame_id` for observe. Raw PNG bytes and helper arguments are rejected. GuestScope/GuestCaller are native owner/session inputs; payload strings are claims, not proof.

The command plane carries `GuestRuntimeInjection` on the prepared command. Ordinary `accept_rxdb_business_command` / `accept_rxdb_business_command_with_origin` pass only `Unregistered`, so dispatch fails closed. `accept_rxdb_business_command_with_guest_runtime` is the explicit injection boundary: `Registered` holds a type-erased `GuestCommandExecutor` that calls `dispatch_guest`. Tests use that boundary; they do not install a process-global owner, and this slice does not register a production lifecycle owner.

This path must not reinterpret ctox.browser.live.v1's current human-controller semantics. Remaining unowned work: VM provision/guest readiness, a production GuestAuthorization plus frame/delivery path, P2P streaming attachment, takeover fencing, and two-host restore. Only after those exist and are verified can Workjet advertise VM in its capability catalog.

## QEMU monitor adapter

The private qmp module adds a bounded local monitor client for an already-owned QEMU child. It negotiates capabilities, queries status, pauses/resumes CPU execution, requests guest powerdown and requests QEMU exit through the documented [QMP protocol](https://www.qemu.org/docs/master/interop/qmp-spec.html). Commands are fixed methods; no arbitrary command/argument or network endpoint is accepted. Unix connection paths must be absolute and selected by the native lifecycle owner.

Responses are correlated by monotonically increasing IDs. Interleaved events and unrelated replies are bounded and never published as authoritative lifecycle state. Frames are limited to 64 KiB and each operation to five seconds. Dropping an in-flight future, timeout, malformed data or disconnect retires the connection and preserves an unknown outcome. Effects are never retried automatically. Explicit correlated errors expose fixed diagnostics rather than guest-supplied details.

A powerdown acknowledgement does not prove guest shutdown; quit does not replace waiting for the owned process. Production integration still needs authorized child creation, protected monitor endpoint, image selection, readiness, resource limits and ongoing ownership fencing. This module starts no production VM and adds no scheduler, persistence or transport.

Eight added tests cover packet fragmentation, correlation, events, malformed/oversized/incomplete responses, bounded timeout, cancellation after write, greeting rejection and a local Unix socket. The Linux test starts one QEMU process with one TCG vCPU, 32 MiB memory, no guest OS/disk/network/display and an initially paused CPU. It exercises actual monitor status, resume, pause, powerdown request and exit, then reaps its captured child on success, failure or a 15-second deadline. The test does not establish a usable guest desktop, hardware acceleration, startup latency, cross-platform provisioning or state migration.

## Owned Linux QEMU process

The private qemu module creates and retains the actual child before its first asynchronous handshake. Its native-only PreparedQemuGuest inputs refer to an already prepared standalone raw base and native-created qcow2 overlay. QEMU receives explicit JSON block-device nodes: read-only raw base, writable overlay and a named backing node. It does not discover a backing path from the overlay header. Existing per-file locking rejects a second local writer; this is not the distributed ownership fence.

Creation starts paused with one or two vCPUs and a bounded memory allocation. Production acceleration is explicitly KVM on Linux x86 guests; software emulation is a test-only option, not an automatic fallback. There are no default network devices, host shares, display listener, shell command or inherited process environment. An explicit virtual GPU is available to a later provisioned guest. These conservative device choices do not yet connect the guest to the existing P2P stream.

The monitor listener is created in a private temporary directory and accepts only the peer PID belonging to the captured child. Cancelled or failed negotiation retires the listener. The QemuProcess value still owns the child, so the caller must retain it and call stop/wait even after cancellation. Each monitor/effect operation belongs inside the future native authority connector; this process primitive grants no authorization.

stop forces termination and awaits the owned child. A timeout/error retains the child and must not release its native ownership fence. request_shutdown is only a guest powerdown request, and forced stop is not an application-consistent checkpoint. Dropping the owner invokes Tokio's kill-on-drop cleanup as a last resort; it is not confirmed exit or permission to transfer ownership. Only the monitor directory is disposable; base and overlay files remain intact.

Five new tests cover resource/path validation and aliases, a real QEMU with prepared block nodes and local duplicate-writer rejection, cancellation with retained child ownership, rejection of a monitor connection from another process, and prompt failure when the child exits before connecting. The real test uses empty raw data and a qcow2 overlay with a deliberately unusable embedded backing path; it must start through the explicitly bound base and preserve the base/overlay after exit. It does not boot Linux or prove application restoration. Both QEMU children remain explicitly retained and stopped even when the test deadline expires.

The native lifecycle caller still needs image provenance/setup, host-wide admission, guest readiness, network/streaming attachment, production authority, crash-consistent export and two-host relocation. No model-facing tool or operational provisioning service is enabled by this addition. Process invocation follows the documented [QEMU block-device interface](https://www.qemu.org/docs/master/system/invocation.html); no shared CTOX transport or contract is replaced.

## Fresh worker disk preparation

The private Linux image module now creates a small qcow2 overlay for an already verified raw base through the documented [qemu-img create interface](https://www.qemu.org/docs/master/tools/qemu-img.html). It creates a unique directory with0700 permissions under the native owner's admitted state parent, never overwrites an existing disk and does not copy or modify the base. The unresolved backing label requires QemuProcess to bind the base explicitly. Image downloads, recipe selection, architecture compatibility and digest/provenance verification remain with the native provisioner.

QemuOverlayPreparation captures the helper before the first await. finish is a single attempt with a ten-second process deadline; cancellation retires the attempt while its caller retains the helper. abort kills/reaps that exact child before deleting unpublished files. Successful preparation flushes the overlay and its directory entries, then retains the disk for native registration; a later abort or dropping the preparation cannot erase it. Failed or cancelled callers must retain the preparation until abort confirms exit. Drop remains best-effort cleanup, not a completed lifecycle transition.

Four tests cover invalid bases and existing-state preservation, failed helper cleanup, cancellation with retained process ownership, and two distinct real qemu-img overlays with an owned paused QEMU using one of them. The real fixture contains no guest OS and measures no desktop startup latency. This is image preparation for the lifecycle integration; it is not a complete operational provisioner. Worker contents will still use the existing versioned checkpoint/file-transfer contract, not replication of the whole base or an opaque overlay as a substitute for portable state.
