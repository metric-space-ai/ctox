# Browser live surface binding

The Business OS Browser sends ephemeral frames and inputs over the authenticated
`ctox.browser.live.v1` WebRTC channel. Durable session/tab projections remain
in CTOX DB. This contract adds no HTTP data path or browser-selected authority.

Native `session.start`, navigation, `live`, and `input` responses include:

```json
{
  "binding": {
    "session_id": "browser_session_...",
    "tab_id": "browser_tab_...",
    "runtime_generation": "native-generated runner incarnation",
    "active_tab_id": "runner tab id from nav.active_tab_id"
  }
}
```

The durable `tab_id` and runner `active_tab_id` are different namespaces.
Never label a returned image with whichever tab happens to be selected locally.
The frontend must first match the response to its still-current session,
navigation epoch and lease, then bind the displayed frame to the native
generation and `nav.active_tab_id`. A session change invalidates the old image
and pending inputs. Reconnection requires a new confirmed frame before input.

A client can send `runtime_generation` and `active_tab_id` from that frame.
Native checks the generation and obtains the runner's active tab under the same
handle lock used to deliver the operation. Mismatches fail before input delivery.
These fields are optional for older clients; present values must be nonempty
strings. Supplied input-event `session_id`/`tab_id` must match the requested
session and its durable tab. Batches larger than 64 are refused, never truncated.

The native queue budget is three seconds for input, five for live frames and
thirty for navigation. Runner operations first acquire an asynchronous per-session
permit; live callers stop waiting when their budget expires. The permit stays
with the blocking IO until it completes even if its caller disappears. The
budget is checked again after acquiring the handle and after the tab lookup.
An expired queued operation is refused before delivery. This
does not cancel an operation already executing or prove exactly-once input
delivery after a lost response. The registry's liveness sweep uses a nonblocking
handle probe: a busy runner is neither declared dead nor waited on during lookup
of other sessions.

A fulfilled transport promise alone is not an input acknowledgement. Check
`ok` and each `results[index].ok`; indices refer to the submitted batch. Native
input watermarks exclude failed and unprocessed entries. Disconnection, rejected
bindings and failed input must not leave the UI claiming that control is ready.

## Evidence and remaining acceptance

The regression cases cover foreign event sessions/tabs, oversized batches,
old runner generation, wrong active tab, missing/failed navigation replies,
expired queued input, busy-handle liveness and failed-input watermarks.
An additional handler regression uses native-issued signatures, actual actor
and lease authorization and a real isolated CTOX DB. It refuses foreign event
bindings, another signed owner, a wrong lease and malformed generation before
runner access, preserving the session row. Its valid-identity control reaches a
deliberately absent runner; it does not execute browser input. An async-lock
regression verifies queue expiry and that timeout cannot release the operation
already holding the permit. These are source regressions, not an installed-browser
acceptance result.

Required final verification: native typecheck and targeted tests, existing
RxDB native/browser checks for the composed source, then an owned isolated
Browser session with click/type/scroll, two distinguishable sessions/tabs,
switch/reconnect and delayed old responses. Bind every result to the actual
served module and installed native revision. Do not reuse customer login or
research sessions, clear their profile, or infer acceptance from an artifact.
