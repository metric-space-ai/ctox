# CTOX provider account affinity

CTOX's native Claude, Codex and Antigravity account pools and Codex search account authority use one standard account setup. New sessions select a stable eligible account. A continuation retains its account despite another account gaining priority. Disabling/removing an account, a model/account cooldown or exclusion from the current attempt can cause a failover; the replacement is retained when the original account recovers.

The HTTP ingress resolves the client's explicit session header, body session/cache key or existing Claude session metadata. The binding key hashes the provider, canonical model and session identifier. The existing local SQLite payload store persists only that digest and the configured account ID. Pool rebuilds and process restarts retain the binding. No provider credential or prompt content is stored here or replicated.

An explicit account/model check does not borrow or overwrite the chat binding. Every retained account is checked against current configured candidates, disabled state, model support and cooldowns. A binding-store failure is reported; there is no silent downgrade to volatile routing. First selection and failover use the canonical SQLite writer fence, while a healthy continuation needs only a read. The store admits up to 100,000 bindings and reports capacity rather than evicting a healthy session.

The portable upstream strategy APIs and serialized strategy field remain for compatibility and conformance. Native CTOX pool construction uses the standard setup regardless of that legacy field. This integration adaptation does not promote the CLIProxyAPI upstream pin or prove installed acceptance.

Remaining work toward the requested standard setup: rank fresh sessions by actual reported limit expiry/reset windows, wire equivalent behavior into Workjet's separate provider gateway, and verify real worker continuation/account usage on the installed stack. Unknown limits remain unknown. The present slice implements durable affinity and removes native round-robin account rotation; it does not claim the limit-ranking or installed acceptance is complete.

Delivery constraint: prepare and qualify the change now; changes on the Supervisor execution path remain held until after the Monday 12 October 13:00 Jour fixe, under the existing Supervisor-only merge and single-writer delivery rules.
