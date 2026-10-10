# Native provider account control contract

Workjet sends `instance.providers.account.enable` and
`instance.providers.account.remove` through the admitted RxDB command bus.
The bridge translates them to `ctox.workjet.providers.account.enable` and
`ctox.workjet.providers.account.remove`. Both requests contain `version: 1`,
`operationId` (UUID), `accountId` (canonical opaque registry identity),
`expectedAccountRevision` (positive integer), and `expectedRevision`
(non-negative policy revision). Enable additionally requires `enabled`
(boolean). Remove rejects that field. Results retain
`{version, operationId, action, registry}`.

Public account metadata may contain `controls: {canEnable, canRemove}`.
Both fields must be booleans. The bridge strips unknown control fields.
An omitted controls object means unsupported; consumers must not infer
mutation support from provider identity or catalog success.

## Current support

This change prepares the shell contract only. Native handlers and native
capability advertisement are deliberately absent. Existing accounts therefore
remain read-only. Calling a prepared action does not establish native support
and cannot be reported as a successful account mutation without the usual
matching completed receipt.

## Required native implementation

The native handler must resolve the canonical UUID under current Owner/Admin
command authority to the owned local Claude account, exact holder, account
revision, policy revision, and private configuration/credential generation.
Inherited main routes and unsupported providers must remain unsupported.

A safe implementation cannot wrap the existing disconnect API inside
`DomainEffectAdmission::apply`: that callback is Policy-only, while topology
is in the Runtime database and encrypted credentials are in Secrets.
The current Core replay path only resumes a domain mutation when its final
applied receipt exists; a crash after an external effect but before that
receipt otherwise becomes an uncertain command. A durable holder-effect
protocol must retain stage-specific identity and proof, recover only effects
already applied, reject changed credential generations, and avoid replaying
a deletion against a re-login. Public results and replicated metadata must
never include selectors, bindings, credential handles, OAuth material, or
raw errors containing secrets.

The existing disconnect helper also retries changed configuration revisions
and changes the default provider when its last enabled account is removed.
`validate_default_provider` currently requires the selected default to have
an enabled account. Supporting disable/remove without a default switch
therefore requires an explicit dormant-default representation and routing
behavior, or a concrete refusal of that operation; it must never silently
choose another provider.

Enablement must persist in the actual topology, survive refresh and
re-adoption, and affect routing. Removal must delete exactly the selected
account's topology, encrypted credential tuple, federation binding,
withdrawals, model observation, and model exclusion. Provider-level model
selection belonging to other accounts must be preserved. These obligations
require native integration tests, including stale revisions, foreign ownership,
credential/config replacement races, and crash/replay boundaries, before
advertising either capability.
