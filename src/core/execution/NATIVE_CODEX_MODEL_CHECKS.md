# Native Codex account model checks

Owned Codex OAuth accounts and the existing native CTOX auth snapshot use the admitted
`ctox.workjet.models.check.v1` path and the instance Responses gateway. The
request pins the private account and provider. Success still requires the real
upstream status, selected-account acknowledgement, requested model and completed
assistant output; local errors are never inferred credential failures.

Before a check, the existing Owner/Admin `providers.observe_native` command can
load that account's live Codex model list from the official ChatGPT Codex
endpoint. The bounded reader uses the existing pinned Codex client version,
rejects redirects, endpoint overrides and ambient/account proxies, and retains
only validated IDs and classified status metadata. Tokens, provider bodies and
private bindings never enter the public registry. Discovery is not inference.

The reservation requires a fresh, selected, non-excluded model on the same
admitted holder. It pins the exact native configuration and encrypted identity,
access and refresh token records (plus a configured proxy record). Configuration,
credential, consumer or model-policy changes retire publication. Runtime writes
are excluded during the bounded physical send; there is no network I/O or
credential decryption inside the publication fence.

The inherited native account reads only the existing CTOX secret-store auth
snapshot. Its encrypted snapshot generation is pinned in place of synthetic
individual token references. A same-ID configured override retires that binding.
Its model-only binding never grants account enable/remove authority; native auth
remains managed by its existing owner. No app-protected authentication file is
read. Shared credential aliases and remote holder forwarding remain unsupported.
A token refresh during a probe invalidates that generation; account discovery
must be refreshed before retrying.

This is backend support. An installed Workjet green check, OAuth operator
acceptance and all-provider federation still require installed-stack testing.
