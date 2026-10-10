# Native provider account registry

Provider-model selection and per-account model exclusions ship in Workjet #247.
This native increment records account existence and explicit consumer withdrawals
for one CTOX instance. It does not replace the installed Models page or its
computer-held gateways.

The existing authenticated Business OS command plane exposes:
- `ctox.workjet.providers.adopt_native {}`: Owner/Admin metadata adoption of
  the instance's existing subscription/coding-plan accounts. Stable logical
  UUIDs map to private holder-local account selectors in the native policy DB.
- `ctox.workjet.providers.list {}`: management readback, including holder,
  enablement, credential readiness and observation time. No secret, local
  selector, preset/model IDs or inferred inference health is returned.
- `ctox.workjet.providers.withdraw {account_id, computer_id, withdrawn,
  expected_revision}`: change one explicit withdrawal under current
  Owner/Admin authority. The target must be a currently possession-bound,
  assigned computer owned by the same verified owner. Restoration removes
  the withdrawal; enrollment and account creation never create opt-in grants.

All three require the normal IntegrationsManage policy check. Mutation handlers
recheck the current active Owner/Admin actor in their domain transaction, resolve
only issuer-verified managed aliases, and commit the mutation plus immutable
Core-command replay receipt together. Payloads cannot select an owner, holder,
credential, model list or private account selector. A native account already
adopted by another logical owner cannot be reassigned by adoption.

An absent observation retains the account and withdrawals. Credential readiness
is taken from the existing native gateway status adapter; it is not a live
provider model list, successful Hi response, quota availability or holder
reachability. No configured/preset model IDs are copied into the registry.
Adoption creates no OAuth session, changes no account/credential and performs no
network inference. It must run through the ordinary authorized owner command,
never direct production SQL or an arbitrary browser DTO.

The native-only `with_consumable_account` seam uses Architecture's
`AdmittedConsumerAuthority` (CTOX #508), preserving the actual captured
possession-bound connection. Current transport, actor, device and assigned
computer revisions are revalidated under its bounded policy fence. Account
revision, owner and withdrawal are read in the same transaction. Every current
enrolled consumer is allowed by default, including computers/accounts added
later. A forwarding node's identity must never replace the original consumer.

This callback permits a bounded local operation only. It is not a serializable
grant and permits no await, network operation, secret-store reentry or distributed
holder fencing. Execution must separately bind a proven live model, actual
holder/config/credential revisions and current task authority, revalidate before
physical IO/publication and acknowledge holder revocation. Remote account adoption,
live catalog observations, authoritative Sync projections, signed forwarding,
holder deletion and unified Models UI are subsequent adapter work; no installed
federation acceptance is claimed by this increment.

Validation covers stable adoption, exclusion of private selectors/model guesses,
missing-holder retention, default consumption, isolated withdrawal/restoration,
foreign/stale/revoked enrollment, stale account revisions, strict payloads and
atomic command receipt replay. Linux checks run on the gpu3 lane, together with
the existing consumer-authority and command-plane guards and RxDB suites.

