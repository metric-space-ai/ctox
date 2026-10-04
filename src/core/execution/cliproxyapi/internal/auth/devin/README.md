# Devin OAuth and account-status components

The status service follows CLIProxyAPI v8.0.13, commit
`d7914afdedca7af95ee974a42453dc49fc1388ce`, specifically
`internal/auth/devin/user_status.go` and the executor's account refresh.
It reuses the existing bounded protobuf reader and Go UTF-8 conversion.

The execution owner must supply the selected account's HTTP transport, including
its proxy and TLS policy. Requests use raw unary protobuf, the selected session
token and device seed, a fixed 30-second deadline and a 4 MiB response bound.
Timeout, cancellation or read failure drops the upstream receiver. Nothing
retries, persists credentials, creates a global client or changes authentication
eligibility here.

Account refresh returns a new Auth value; the native owner remains responsible
for persistence. Failed status requests leave the input unchanged. Identity
fields update only when returned; routing cooldown fields are preserved.

Quota percentages retain presence separately from their numeric value. This is
an intentional CTOX adaptation: upstream defaults omitted percentages to zero,
but Models must show an unavailable API observation as unknown. A successful
refresh replaces the quota observation snapshot, removing stale missing limits
and reset dates. Known zero remains zero. Wire timestamps keep signed Unix
seconds and UTC formatting rather than losing out-of-range dates.

The OAuth component now provides 64-byte PKCE/S256, exact authorization URL
ordering/escaping, single-use token exchange, profile lookup and owned Auth
record construction over the same selected transport. Exchange/profile calls
have a fixed 30-second deadline and 1 MiB body limit, matching the upstream
HTTP limits. Code exchange never retries. Profile/status enrichment is
best-effort; it does not turn quota-reading failure into authentication rejection.
Profile identity wins over status identity; unknown identities have distinct
hashed filenames, and unsafe/oversized profile names cannot escape a filename.
The native owner still persists the returned Auth; no new credential store or
background OAuth listener is created here.

The callback component binds only IPv4 loopback and accepts `/callback` with the
owner-supplied state. It checks state in constant time before accepting either
a code or a provider error; unrelated requests cannot consume the login. A valid
callback claims the session once. The five-minute deadline starts at listener
creation. Both the listener and bounded connection tasks belong to the consuming
wait and close on completion, timeout, cancellation or drop. No detached listener,
browser launcher, credential store or background poller is created.

The native owner still needs to connect its PKCE/login-session lifecycle to this
callback component, AuthRefresher/context/status binding and production provider
registration. Synthetic component regressions do not prove real browser OAuth,
provider or app acceptance.
