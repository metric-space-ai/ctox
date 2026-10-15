# Native Grok Build account model checks

The existing native Grok OAuth record is adopted as configured metadata with a
private generation binding. Native "authenticated" status means a readable
credential is configured, never provider health or successful inference. Only
Owner/Admin adoption and the existing admitted model-check path expose it.

Catalog requests use the actual Grok Build /models endpoint and the native CLI
headers. The reader has an eight-second deadline, rejects redirects and ambient
proxies, bounds JSON, rejects secret echoes and keeps only validated IDs and
closed failure metadata. It creates no account, changes no default and refreshes
no credential. An expired record reports a local refresh requirement without an
upstream status; normal gateway authentication owns refresh. Changed generations
require fresh holder discovery before another check.

The normal Responses gateway still serves the Hi request. Success requires its
actual account acknowledgement, real inference status, requested model and
completed assistant output. The Grok wrapper now retains that real status and
closed error class while discarding raw error bodies. Catalog preflights never
become inference witnesses. Failed/incomplete HTTP 200 output is not success.

Publication pins the encrypted OAuth generation and excludes shadow runtime
configuration and concurrent encrypted-record writes during the bounded commit. Reads of encrypted metadata occur on preopened connections inside
the admitted issuer/Core/Policy fence, without network or credential decryption.
Rotation, deletion, account/consumer revocation and model-policy changes retire
results. Existing native auth ownership and mutation controls remain unchanged.
API-key xAI accounts keep their separate account-bound route.

This source support covers the existing single native Grok OAuth account on its
holding instance. Multi-account OAuth ownership and remote-holder forwarding
remain separate work; neither installed green checks nor live OAuth acceptance
are implied by mocked upstream tests.
