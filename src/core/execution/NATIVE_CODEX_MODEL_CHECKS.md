# Native Codex account model checks

Owned, explicitly configured Codex OAuth accounts use the existing admitted
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

Inherited ChatGPT auth snapshots, shared credential references and remote holder
forwarding are deliberately unsupported here. They need their own existing
owner-authorized transport; this path does not read app-protected authentication
files or substitute another account. A token refresh during a probe invalidates
that generation; account metadata/discovery must be refreshed before retrying.

This is backend support. An installed Workjet green check, OAuth operator
acceptance and all-provider federation still require installed-stack testing.
