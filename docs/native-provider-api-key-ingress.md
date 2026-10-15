# Native provider API-key ingress

> Status (15.10.2026): the native WebRTC receiver for this request
> (`src/core/business_os/rxdb_peer_provider_api_keys.rs` and its registration in
> `rxdb_peer.rs`) is owned by the CTOX Sync v4 work and is not on `main` yet.
> The gateway side it calls (`install_api_key_account_owned_authorized` in
> `src/core/execution/cliproxyapi_integration/api_key_ingress.rs`), the account
> controls, the live catalog and the routing are on `main`. Until the receiver
> lands, the capability below is not advertised and no API key can be entered.

API keys travel only through the guarded auxiliary WebRTC request
`ctox.workjet.providers.api-key.v1`, advertised by
`ctox-workjet-provider-api-key-v1`. They must never enter a durable
`business_commands` payload or a replicated collection.

The request has one JSON parameter:

```json
{"version":1,"action":"instance.providers.apiKey.add","commandId":"<UUID>","provider":"kimi","secret":"<key entered by the owner>"}
```

`provider` accepts `zai`, `minimax`, `xai`, or `kimi`. The native receiver
requires a current signed Owner/Admin capability, resolves its canonical owner,
checks fixed provider `/models` endpoints, and stores an accepted credential in
the native SecretStore. Kimi tests both Coding and Moonshot plans. The verified
endpoint and exact live model IDs are retained in holder-private configuration.
No caller-supplied owner, actor, endpoint, secret reference or model list is accepted.

Reuse the same `commandId` for a retry of the same creation operation. An
already committed matching operation returns its existing account without key
rotation or duplicate creation. Reusing it with another key or provider fails.
Authority is checked before discovery, before credential/configuration effects,
and again before publication. A retired peer cannot receive a success receipt.
A process failure between the separate secret and configuration stores requires
reconciliation; replay does not overwrite an unexplained existing secret.

The successful response contains `action`, `commandId`, `configured:true`,
`adoptionRequired:true`, `endpoint`, `models`, and `inferenceVerified:false`.
It contains no holder-local account ID, credential reference, fingerprint or key.
An accepted catalog is not a successful inference check. A discovery rejection returns configured:false and a key-free discovery object containing the checked endpoint profiles, genuine HTTP statuses (or null for local failures), and failure classes.

Next invoke the existing admitted `ctox.workjet.providers.adopt_native` command
with an empty payload. That path imports only API accounts bound to the current
canonical owner into the existing provider registry. Legacy API accounts with
no verified owner binding are left unadopted. Secret generation metadata is read
outside the policy writer transaction; only owner-scoped metadata is projected.

Renderer integration, public API-account controls, persisted live observation
and inference acceptance remain separate work. The endpoint is not proof that
an installed Workjet flow has passed.
