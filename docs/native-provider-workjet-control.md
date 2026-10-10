# Native provider accounts in Workjet

Workjet's instance provider controls use the existing Owner/Admin Business OS
command bus over CTOX Sync. They do not require a project, a worker computer,
an HTTP data bridge or a renderer credential.

The shell accepts version 1 requests with a UUID `operationId` and these
actions:

| Action | Fields beyond action, version and operationId | Native command |
| --- | --- | --- |
| `instance.providers.read` | none | `ctox.workjet.providers.list` |
| `instance.providers.adopt` | none | `ctox.workjet.providers.adopt_native` |
| `instance.providers.observe` | `accountId`, `expectedAccountRevision` | `ctox.workjet.providers.observe_native` |
| `instance.providers.models.select` | `provider`, `models`, `expectedRevision` | `ctox.workjet.providers.models.select` |
| `instance.providers.models.exclude` | `accountId`, `expectedAccountRevision`, `models`, `expectedRevision` | `ctox.workjet.providers.models.exclude` |

Reads do not adopt accounts or refresh credentials. Adoption explicitly records
existing native accounts without copying their secrets. Observation performs the
existing bounded live provider catalog request on the holding instance. Selection
applies once per provider; exclusions apply only to the exact native account.
The native command handler retains policy, identity, revision and live-catalog
validation. The request cannot provide an owner, holder selector, endpoint or key.

A successful response contains `version`, `action`, `operationId` and
`registry`. It preserves the authoritative `nativeAccountReference`
(`accountId`, `holderInstanceId`, `accountRevision`) from the exact registry
row. A Workjet-local gateway account ID is a separate identity: do not infer a
mapping from an email address or provider name.

Only allowlisted public metadata reaches the renderer. A live GET /models
success reports catalog discovery; `inferenceVerified` remains false. It is not
a green inference check, a signed execution permit, a coding harness session or
proof that the requested Claude Code Luma ran.

The shell captures the current session, database, command bus, Sync object,
actor and instance. A changed scope retires delivery. The desktop-facing wait is
bounded to 25 seconds and does not retry or replay a command. An already admitted
native command remains a durable intent; after timeout, refresh reads current
native state rather than assuming that the mutation did not happen.

This change is additive. Native Claude self-login transport, the Workjet consumer
and an actual admitted Claude Code holding executor are separate increments.
No installed account login or Supervisor execution is claimed by these contracts.
