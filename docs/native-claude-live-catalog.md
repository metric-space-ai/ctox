# Native Claude account live catalogs

After Owner/Admin domain admission, `ctox.workjet.providers.adopt_native` binds
each installed Claude OAuth account to its holder-private configuration and
credential fingerprint. No selector, fingerprint or secret is projected.

`ctox.workjet.providers.observe_native` accepts only the opaque account ID and
expected account revision. The native holder resolves its own account and makes
a bounded authenticated GET to the official Anthropic model list, outside the
policy transaction. Redirects and ambient proxies are disabled; unsupported
endpoint/proxy overrides fail before token disclosure. The request has an
eight-second deadline, a 128 KiB response limit and a 1024-model limit.
Paginated/incomplete/invalid lists are rejected.

The observer compares the exact account and credential pair before disclosure
and after the response. Changed credentials/configuration discard completion.
Persistence rechecks current Owner, holder, account revision and private binding.
Only allowlisted status/timing/failure metadata and real returned IDs are stored.
Discovery never refreshes tokens, changes cooldowns, enables models, switches the
default route or proves capacity/inference. Failed observations retain previous
IDs as stale metadata.

This supports the local holder's account. It is not remote holder execution
authority, a lease or a Claude Code producer receipt. Supervisor execution still
requires its real durable lease/project fence, selected-account acknowledgement
and actual harness/model witness. No installed acceptance is implied.
