# Native API-key model routing

Owned API-key accounts use the current holder-local catalog observation in the
provider registry. Provider-wide model selection and per-account exclusions
apply to the same instance gateway used for inference. The creation-time model
snapshot in the private integration config is not the routing catalog.

An owned account must be adopted on its actual holder, enabled, credential-ready,
and bound to its current configuration and credential generation. A recorded
catalog for that account revision is required. Missing registry state never
bootstraps a new policy store or supplies a wildcard model. Existing unowned
legacy configurations retain their explicit model lists.

Cached handlers retire when configuration, credentials, canonical account,
owner authority, catalog observation or model-selection witness changes. They
recheck before transport and before publishing buffered or streamed results.
A remove/re-add cannot revive a stale handler merely because model names match.
The gateway keeps its existing persistent account selector and session binding;
this change introduces no round-robin policy or new network discovery on requests.

This availability read does not authorize a consumer: the existing admitted
consumer/gateway authority remains required. It reads metadata only and exposes
no secret references, key values, private account bindings or arbitrary endpoint.

Regression coverage uses synthetic credentials and existing wire-fixture model
IDs. It exercises actual gateway request dispatch with a mock provider, live
catalog selection, withdrawal before/during transport, owner revocation, account
disable/remove and unchanged legacy routing. These tests are not installed
provider acceptance or a real upstream Hi-check result.
