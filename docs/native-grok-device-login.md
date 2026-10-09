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
