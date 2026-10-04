# Devin account-status port

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

OAuth login/token exchange, native AuthRefresher/context binding and production
provider registration remain separate unfinished work. This component and its
synthetic transport regressions do not prove real provider or app acceptance.
