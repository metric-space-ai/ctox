# Scoped credential metadata contract

Core-only change. The Credentials frontend is NOT migrated by this PR and full
cross-scope visibility is not yet a completed user-facing feature. Its current
`mergeEntries(catalog, extra)` discards identity fields and keys rows/actions by
name. A pi-owned app change must consume `scoped_catalog.entries`, retain the
tuple identity, display scope, and render unsupported writes as read-only.
Do not flatten these entries into the old name-only list.

## Native API

The existing `ctox.secret.list` command remains behind the workspace
`SecretsManage` gate and returns its outcome through Business OS commands and
CTOX Sync/WebRTC. No HTTP data endpoint or value-reveal operation is added.

The legacy `scope`, `catalog` and `extra` fields keep their runtime-credential
meaning and shape. New `scoped_catalog` contains:

```json
{
  "schema": "ctox.credentials.scoped-metadata.v1",
  "complete": true,
  "entries": [{
    "id": "[\"account:crew\",\"OPENAI_API_KEY\"]",
    "scope": "account:crew",
    "name": "OPENAI_API_KEY",
    "reference": {"scope": "account:crew", "name": "OPENAI_API_KEY"},
    "description": "Provider credential",
    "is_set": true,
    "status": "set",
    "updated_at": "2026-09-13T00:00:00.000Z",
    "source": "extra",
    "write_support": {"put": false, "delete": false, "reason": "unsupported_scope"}
  }]
}
```

Entries cover every stored secret tuple, including custom/system/account scopes,
plus known unset runtime slots. An unset slot is runtime-scoped even when another
scope has the same name. Rows sort by `(scope, name)`. `id` is the JSON encoding
of that two-string tuple (not an authorization token); rotation does not change
it, and delimiters/Unicode cannot collide. `reference` is an exact structured
locator, not a secret value or a new textual secret-substitution grammar.

Only explicit metadata fields are projected: scope/name/identity/reference,
description, set status, update timestamp, catalog/extra source and operation
support. Arbitrary `metadata_json`, nonce, ciphertext and record values are never
serialized. Listing uses the existing metadata query without decryption. `set`
means a stored row exists, not that the credential decrypts, logs in, or passes a
provider test. A failed list remains an error, not a complete empty catalog.

## Mutations remain runtime-only

Existing `ctox.secret.put {name,value}` and `ctox.secret.delete {name}` keep
their meaning: only `credential_scope()` (currently `credentials`) is targeted.
No secret-store/runtime_env getter, setter, deletion or merge API changes.

Scope-aware clients can additionally send `scope`, `id`, and/or `reference` with
the top-level `name`. Every supplied selector must identify that same runtime
tuple. Other scopes, mismatched identities, malformed selectors and unknown
payload fields fail closed. They never fall back to a same-name runtime secret.
Null/omitted optional selectors use the existing runtime-only default; they do
not select all scopes. Key and value validation remain in force. Commands do
not route by display labels or grant rights from catalog flags.

`write_support` describes API support, not the actor's authorization. The native
`SecretsManage` gate still runs for list/put/delete. Non-runtime scopes and
noncanonical runtime names are read-only here; owners of those integrations
retain their existing dedicated credential lifecycle. Put intake/value-redaction
paths remain intact, including denied/failed requests. Invalid deserialization
retains the existing empty-default mutation failure without echoing raw input.

## Parent verification (not run locally)

No Mac compiler, dependency install, index, deployment or native test was run.
Only Rust formatting/checks and source diff review were performed locally.
Parent runs these on THESEN after review, under its native-verification lease,
using its existing release cache and at most two workers. Use the exact new PR
revision (optionally composed with reviewed PR139/142); do not treat an older
existing binary as evidence for these new tests.

```sh
cargo test --release --locked --bin ctox --jobs 2 scoped_credential_catalog_ -- --test-threads=1
cargo test --release --locked --bin ctox --jobs 2 ctox_secret_ -- --test-threads=1
cargo test --release --locked --bin ctox --jobs 2 replicated_secret_put_document_is_stripped_before_persistence_and_still_applied -- --test-threads=1
```

The first filter selects six new native tests: complete metadata and same-name
scope isolation; stable delimiter-safe identity; no-decryption/read-only status;
user denial; rejection of wrong/malformed put/delete targets without mutation;
and modern plus legacy runtime-only writes/env consumption. The other filters
retain existing redaction and lifecycle guards. No guard is weakened.

No changes to PR139's secret generator, PR142's authenticated MCP authority,
browser app files, adapters, tenant app files or live runtime stores. Parent owns
review, native execution, PR comments and later integration/deployment.
