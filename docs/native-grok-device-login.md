# Native Grok Build device authorization

`execution::cliproxyapi_xai::CtoxXaiLogin` is an instance-owned native capability.
The authorized controller retains one object for its CTOX root and calls:

- `start().await`: returns `login_id`, public `verification_uri`, `user_code`, and `expires_in`.
- `poll(login_id)`: returns `pending`, `accepted`, `cancelled`, or `failed`.
- `cancel(login_id)`: cancels only that pending login.
- `discover_models(root).await`: fetches model IDs from the authenticated subscription's live catalog.

The outer control transport must authenticate the actor and enforce the existing
instance-management permission before calling these functions. This module is
not a renderer or unauthenticated HTTP endpoint. Never serialize its controller,
private device code, token exchange, encrypted credential record, or errors from
upstream token bodies into replicated command documents.

One pending login is allowed per retained controller; controller destruction
cancels its poll. Discovery and each token request are bounded; the full polling
task expires after 30 minutes. A fresh login refuses an already installed account.
There is no browser launch, ambient config toggle, token-file import, or provider
default mutation. Michael follows the public URL and approves the device himself.

Accepted credentials are encrypted in CTOX's secret store under the
`provider-subscriptions` scope, record `xai-instance-oauth`. The projected account
ID is `xai-instance-primary`. `authenticated` means a credential was accepted; it
is not a live model-health result. The live catalog and a successful check remain
required before selecting a model.

The native instance Responses router accepts explicit provider `xai`, honors the
requested account, and verifies each model against authenticated live `/models`.
It uses the existing portable xAI request/header preparation against the Grok CLI
subscription proxy, refreshes expiring credentials in native memory, and writes
refresh results back encrypted. If Grok is the sole subscription, the listener
starts with no implicit default provider. Existing defaults remain unchanged.

SSE is currently bounded-buffered as for the existing native Kimi route, with a
60-second HTTP bound and 32 MiB response bound. Progressive SSE delivery requires
a generic streaming owner in the portable server and is not claimed here.
Production login, deployment and installed acceptance are operator/parent duties.

## Local operator fallback

On the holding instance host, run `ctox runtime grok-login` under the same
local operator and original CTOX root used by its Secret/Runtime CLI. Follow the
printed public verification URI and approve its user code in your own browser.
The process stays attached until acceptance, cancellation or device-code expiry
(at most 30 minutes). Ctrl+C or SIGTERM cancels its retained controller. It prints
no device secret, access/refresh token or upstream error body, launches no
browser and leaves the selected runtime model unchanged.

This direct CLI uses existing local root/master-key access, not a synthesized
Business OS Owner role or a workaround for a denied remote MCP request.
Device credential installation compares expected absence and writes in one
encrypted-store transaction; a concurrent account installation wins unchanged.
Existing accounts are refused rather than rotated or replaced.

After acceptance, `ctox runtime grok-models` reads the real subscription catalog
with an eight-second bound. Credential presence or catalog membership alone
does not prove model health: run the normal Workjet model check before the
supervisor performs an explicit runtime switch. These commands are available
only once this source is delivered in the installed native binary.

## Authorized Workjet native control

The retained daemon controller registers `ctox.workjet.grok.v1` with capability
`ctox-workjet-grok-v1`. The shell `workjetProjectControl` forwards
`instance.grok.read`, `.start`, `.poll`, `.cancel`, `.check`, and `.remove` over
`requestNative`. Native admission uses the actual current peer and its admitted
capability token; Owner (`chef`) or Admin is required. Login polling watches the
creator's authority independently of renderer requests and revalidates it
immediately before encrypted credential storage. Peer retirement cancels pending
polling. Poll/cancel require the original token and retained login ID.

Requests have `version: 1`, `action`, and UUID `operationId`. Poll/cancel also
require UUID `loginId`; check requires `modelId`. No other fields are accepted.
The public reply echoes those first three fields and contains `installed`
(credential presence only), `accountLabel`, `login` (nullable), `models` (live IDs),
and `check` (nullable). Login contains `loginId`, `phase`, `verificationUri`,
`userCode`, and integer UTC milliseconds `expiresAt`. Phases are `pending`,
`accepted`, `cancelled`, `failed`, and `expired`. The check contains `modelId`,
`status` (`ok` or `error`), integer UTC milliseconds `checkedAt`, nullable
`latencyMs`, and nullable fixed `errorCode` (`timeout`, `missing_credential`,
`model_unavailable`, `invalid_response`, or `request_failed`).

Read and accepted polling fetch the authenticated catalog with an eight-second
bound. Hi checks have a total twenty-second async budget including discovery,
use the shared native instance Responses router with explicit xAI provider, and
require actual nonempty output text. Persisted checks contain a private secret
content binding; a changed/deleted credential invalidates the previous result.
Removal cancels retained login and deletes only the subscription credential under
the same serialization used by native refresh/inference, then clears its check.
Once the native live catalog confirms a requested Grok model, an explicit `ctox runtime switch` selects the instance subscription route (`ctox_subscription`, provider `xai`) rather than retaining the cloud proxy. Discovery has an eight-second bound before persistence; missing credentials, unavailable models and discovery failures leave the current selection intact. The transaction saves public provider/URL metadata so rollback restores the previous route. This implementation does not perform that production switch; the operator switches only after installed login and a real Hi check.

No enabled toggle is exposed. Fixed errors do not promise upstream error-class
fidelity, and this control does not change the instance default model.
