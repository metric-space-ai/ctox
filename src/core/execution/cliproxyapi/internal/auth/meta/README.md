# Meta device authorization and credential snapshots

This package follows CLIProxyAPI v8.0.13, frozen commit
`d7914afdedca7af95ee974a42453dc49fc1388ce`, specifically
`internal/auth/meta/meta.go` and its authentication regressions.

The selected execution owner supplies the HTTP client and its proxy/TLS policy.
Device authorization and polling preserve Meta's official client ID, grant,
headers and endpoints. The JSON device response cannot replace the token
endpoint. Polling starts after the provider interval, respects pending replies,
adds five seconds on slow_down, terminates on denial/expiry/provider rejection,
and caps authorization at the nearer device expiry or fifteen minutes.

Credential HTTP calls have thirty-second deadlines. Dropping or timing out a
caller drops its upstream response receiver; no task, listener or poller is
detached. CTOX additionally bounds credential response bodies to one MiB.
Endpoint overrides are explicit host configuration rather than ambient
META_MINT_URL. Error and Debug output omit credentials, private endpoints,
identity fields and provider response bodies.

Minting is best-effort after authorization. An API key uses the minted regional
base URL and identity; its lifetime does not inherit the DCA token's expiration.
A DCA-only result retains the advertised DCA expiry for later on-demand recovery.
A mint deadline failure can return that authorized DCA record, matching upstream;
owner cancellation still drops the whole operation before manager persistence.

MetaTokenStorage creates an owned JSON-compatible snapshot. Current metadata
replaces old settings, including deletions. Login records without a current
snapshot may inherit old noncredential settings. Cleared credential/expiry
fields cannot be resurrected from old metadata. Credential filenames hash the
original identity and retain a bounded readable basename, preventing account
collisions and filename traversal.

Only the existing injected SDK Manager/AuthStore may accept and persist the
snapshot. This package creates no filesystem credential writer or new store.
The native SDK MetaAuthenticator now presents the device challenge through an
injected host presenter and returns the owned account to the existing Manager.
Cancellation takes precedence over login work and drops active device/poll
receivers without a write. Metadata retains subscription observations and the
separate DCA deadline; no scheduled API-key expiry is fabricated.
Production host/presenter registration, the executor, on-demand mint preparation
and manager refresh acceptance still need integration. Synthetic native guards
and frozen Go oracles do not establish live Meta, provider hub or App acceptance.
