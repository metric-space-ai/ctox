# Native API-account controls

Accounts created by the authenticated API-key ingress use the existing
`ctox.workjet.providers.account.enable` and `.remove` commands after native
adoption. The caller supplies the public account UUID, expected account and
policy revisions, and `enabled` for the enable command. Owner, holder, provider,
private selector and credential binding are resolved by the admitted server path.

API bindings use the private `api-key:v1:` source tag. It does not expose a key,
reference or fingerprint to clients and does not make subscription or inherited
routes editable through the API-key adapter. Existing owned accounts receive
this binding on the next ordinary `adopt_native` refresh.

Enable/disable changes only that account's flag, preserving its key generation,
other accounts, models and defaults. Credential readiness follows whether the
key is configured, independently of the disabled flag. It is not a successful
inference result.

Removal commits the configuration change and exact cleanup proof in one Runtime
transaction. Policy reservation alone cannot perform a new effect during
recovery. Proven recovery deletes only the recorded encrypted key generation,
then removes the account's model observations, exclusions, bindings and registry
entry. The provider's shared selection remains while other accounts exist.

An account with a missing key can still be removed. A replacement key or a new
reference after the Runtime commit prevents stale cleanup and requires normal
owner reconciliation; it is never deleted by that old proof. A committed remove
retires the private account identity in the Runtime journal, so a late creation
retry cannot restore it. A fresh add operation receives a different identity.

This is a native backend contract. Workjet consumer wiring and installed UI,
route and model-check acceptance remain explicit follow-ups.
