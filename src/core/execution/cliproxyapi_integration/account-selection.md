# Account selection for model probes

The native provider listener accepts `X-CTOX-Provider` and an optional
`X-CTOX-Account` opaque configured account ID (1–128 ASCII letters, digits,
`-`, `_`, `.`, `:`). Duplicate, malformed or auxiliary-route selectors fail
before dispatch. Selection remains server-authoritative: the request-scoped pin
narrows scheduler candidates before model, disabled-account and cooldown checks.
Both Responses and Messages, buffered and streaming, use the same pin. A failed
pinned account cannot rotate to another account; unpinned scheduling is unchanged.
Kimi routes also enforce exact configured account/model matching.

Kimi reauthentication retains the account's enablement, configured models,
priority, weight and endpoint profile. It replaces only the encrypted credential
tuple and its private references; a disabled account remains unavailable and
the existing account keeps its position in the stack; other accounts retain their policy.

`X-CTOX-Account-Selected` acknowledges only an actual configured account chosen
by the scheduler/route, including upstream failures after selection. Rejected
selectors are never echoed. IDs are nonsecret; provider credentials remain in
native credential owners and never enter client headers or error text.

Authenticated `/v0/management/runtime-status` exposes
`features.account_selection: true`. Clients must require that capability before
per-account inference and require the selected-account acknowledgement on success.

Pinned probes may also receive `X-CTOX-Error-Class` from a closed whitelist:
`auth` (upstream 401/403), `quota-rate-limit` (402/429), `unknown-model`
(structured upstream model error code/type/status or `error.param = model`),
and `network-provider`. Arbitrary provider messages never determine this class.
Unpinned error behavior and headers remain unchanged. Classification occurs
before provider bodies are collapsed into redacted listener errors.
