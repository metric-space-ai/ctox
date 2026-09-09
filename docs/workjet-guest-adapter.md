# Guest desktop adapter boundary

This is an implementation seam for the Workjet VM tool, not an enabled VM feature. It adds a guest-local X11 driver and typed bounded actions next to browser_runtime. No browser operation, controller lease, Sync scheduler, transport, provisioner or collection is changed. No model-facing tool or native dispatcher entry is registered until the real authority connector exists.

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

Next integration must describe a separate typed Guest dispatcher extension using the existing native auxiliary transport. It must not reinterpret ctox.browser.live.v1's current human-controller semantics. The agreed native authority supplies the GuestAuthorization implementation and the existing frame/delivery path; only then can Workjet advertise VM in its capability catalog and expose its surface when enabled.
