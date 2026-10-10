# Native provider account controls

Workjet sends `instance.providers.account.enable` and
`instance.providers.account.remove` through the admitted RxDB command bus.
The shell translates these to `ctox.workjet.providers.account.enable` and
`ctox.workjet.providers.account.remove`. Both requests contain `version: 1`,
`operationId` (UUID), `accountId` (canonical opaque registry identity),
`expectedAccountRevision` (positive integer), and `expectedRevision`
(non-negative policy revision). Enable additionally requires `enabled`
(boolean). Remove rejects that field. Results retain
`{version, operationId, action, registry}`.

Public account metadata optionally contains `controls: {canEnable, canRemove}`.
Both fields are booleans. Omission means unsupported on an older native/shell
version. Current native supports local configured Claude subscription accounts
with a retained private adoption binding. Inherited main routes, unsupported
providers and accounts with an unresolved holder mutation advertise false.
Consumers must not infer support from provider identity or catalog success.
Unknown fields, private selectors, bindings, credential handles, OAuth material,
and raw credential-bearing errors never travel through the public shell result.

## Durable stages and authority

The new admitted Core claim reserves the exact canonical account, holder,
Owner/Admin actor, Policy revision, account revision and private generation in
Policy. Only one unfinished holder control per owner is allowed. Current actor,
account and Core claim are checked again immediately before the external effect.
No Policy transaction or connection is retained across Runtime or Secrets IO.

The holder compares the exact adoption binding and uses a Runtime configuration
CAS. Its topology mutation and immutable private effect proof commit in the same
Runtime transaction. Enablement changes the real configured account's disabled
flag. Removal removes only that topology entry and captures exact encrypted
credential content generations, rejecting shared credential references.

Proof-bearing recovery never repeats the topology mutation. It checks current
Core intent and Owner/Admin authority, the unchanged reserved Policy target,
and the holder's exact applied configuration generation. Removal cleanup holds
Runtime's writer transaction while deleting the captured Secrets generations
in one Secrets transaction. Already absent secrets are idempotent; replacements
are rejected. A changed account or topology stays uncertain and requires
reconciliation, rather than deleting a re-login or claiming completion.

Only then does Policy commit its account update/removal, current registry
projection and immutable final domain receipt together. Removal clears that
account's withdrawals, model observations, exclusions and binding. A shared
provider selection survives while another owned provider account exists.
Core's existing terminal projection repair publishes the current registry and
retains the original correlated command result on replay.

A known rejection before Runtime COMMIT releases the reservation without an
applied receipt. A crash with only the reservation remains uncertain and never
authorizes a fresh external mutation. Cancellation/revocation observed before
the effect rejects it; changes observed after Runtime COMMIT prevent completion
and remain conservatively uncertain. There is no cross-WAL atomicity claim.

## Dormant defaults

Disabling or removing the last default account preserves the stored selected
provider. Typed topology loading accepts the dormant route. The outer Responses
router always supplies that selected provider explicitly; requests report it
unavailable rather than calling a different provider. The portable router's
constructor may use an available internal default, but outer requests never
use that default. The running listener uses a persistent config polling interval
and also revalidates its retained configuration at each incoming connection.
After a change, new traffic receives rebuilt routes; an unavailable rebuild
fails closed. Already admitted traffic keeps its routes. No model or project
execution default is changed.
