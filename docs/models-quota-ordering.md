# Provider-reported quota ordering

The native CLIProxyAPI standard setup uses provider-reported shared subscription resets for **new** sessions and failover, layered on the durable affinity in #624. A usable window that expires sooner takes precedence; unknown/equal windows use the stable account ID. No round robin, weights, or manual priority choice is introduced. A healthy continuation retains its account even when another account has an earlier reset.

Claude pool responses (buffered and streaming) supply the explicitly named Anthropic unified 5h/7d status/reset/utilization headers. The native host stores only their normalized observations in the existing private SQLite payload store. Missing utilization stays absent; unified billing/overage resets, Retry-After, client metadata, malformed fields and expired observations never become quota estimates. Existing cooldown/error classification remains authoritative for request failures. Other providers without an implemented real observation source remain unknown and use stable selection.

The isolated HTTP test rebuilds the native pool between turns, learns windows from synthetic upstream responses, selects the sooner-expiring account for a fresh session, and retains both existing sessions. It is source-level verification, not genuine provider or installed acceptance. UI limit readback, additional provider adapters, account-removal cleanup of observations and installed acceptance remain follow-ups.

This changes account selection defaults and is held for Supervisor delivery **after Mon 12 Oct 13:00 Berlin**. It must not be inserted into the Monday meeting execution path before then. The strict upstream-promotion baseline remains separate (issue #279).
