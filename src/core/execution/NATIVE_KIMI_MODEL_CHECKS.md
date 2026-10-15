Authoring contract: native Kimi model checks
===========================================

Implementation is in progress. This draft records the verified integration
boundary and acceptance requirements; it does not claim installed support.

Current source routes Kimi model checks into the API-key reservation. Native
Kimi subscription accounts instead have a typed account ID, disabled/priority
state, an access-token reference, optional refresh-token reference, a state
reference and a fixed endpoint profile. Those existing accounts need their own
private model binding; an API-key binding cannot authorize OAuth consumption.

The implementation must read only the existing account and exact encrypted
record generations. No credential values or private bindings enter projections.
A live account catalog must provide model IDs; configured route hints and static
fallbacks are not live capability or health evidence. Missing discovery must
remain explicitly unknown, never a fabricated model list or successful check.

Checks use the existing instance Responses gateway and selected account. A green
result requires actual upstream inference status, exact account acknowledgement,
requested model and completed assistant output. Catalog preflights, local errors
and cooldowns cannot masquerade as credential rejection or inference success.

Publication must reuse the admitted capability issuer fence with preopened
read-only encrypted metadata connections. It must not recursively acquire the
secret-store writer, decrypt credentials, reopen Core or wait for network inside
that scope. Account/configuration, token/state, consumer, holder or model-policy
changes retire results. Disabled and foreign accounts remain unavailable.

Existing subscription account stacking is retained; no account, default model,
provider model ID, endpoint choice or runtime environment toggle is invented.
OAuth refresh and authentication stay in the existing credential provider.
Native account controls and remote-holder forwarding remain separate obligations
of the full Models goal, not implied successes of this component.

Required evidence: targeted native routing, generation/authority and actual
issuer-scope regressions; format/check/Clippy; applicable data-plane checks on
gpu3. Installed acceptance additionally requires a real configured Kimi account
and an actual Hi result through the installed Workjet/native stack.
