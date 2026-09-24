# Delegated credential row presence (draft)

This native path lets a delegated worker ask the daemon whether exact secret
rows exist. The worker does not open the store, decrypt values, list a catalog,
or receive descriptions or arbitrary secret metadata.

When invoking the existing `ctox.delegate_task` action, include this object in
its action `payload`. The action proposal stores it as
`command.payload.input.metadata_read_contract`:

```json
{
  "metadata_read_contract": {
    "schema": "ctox.worker.credential-presence.v1",
    "credentials": [{"scope": "credentials", "name": "EXAMPLE_API_KEY"}]
  }
}
```

Selectors are examples, not a Leadfeeder configuration contract. One to eight
unique exact scope/name pairs are allowed. Blank, wildcard, malformed and
unknown contract fields are rejected. No contract means no metadata grant.

At queue execution, only `ctox.delegate_task` with this explicit contract
receives the metadata session. The existing signed command session binds its
actor, role, command ID, canonical payload hash, workspace and expiry. Native
admission additionally requires the current `SecretsManage` permission and
exact equality with the stored contract. It cannot combine this session with
action/collection grants. A metadata-only session rejects other MCP tools.

The worker invokes `business_os.read_credential_presence` with `{}`. Caller
arguments cannot widen the selectors. The daemon revalidates actor, payload,
contract, permission and nonterminal command state before and after accessing
the store, and checks expiry at both boundaries. Any lookup failure is an
error, never an empty or absent result. The response contains only scope, name,
`present` and `provider_readiness: "not_checked"`.

A present row does not prove decryption, valid credentials, configured account,
selected API/auth mode, or provider readiness. Account/mode configuration
selectors remain pending the existing Leadfeeder implementation owner's final
contract. Do not run the old terminal preflight again or grant catalog access
from a module name or natural-language objective.

Verification pending: the new native tests cover exact presence across scopes,
value/description/metadata canaries, malformed contracts, untrusted callers,
selector widening, actor substitution, permission denial, expiry, terminal reuse
and role revocation. They have not been compiled or executed. Queue-to-MCP
integration, sandbox worker execution, and a deterministic concurrent revocation
at response publication still need evidence. The before/after checks are not a
claim of atomic transport publication. Local heavy checks remain behind the
shared resource gate. No deployed readiness claim.
