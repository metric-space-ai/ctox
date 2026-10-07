# Native admission for a remote Workjet worker

`business_os.remote_worker_admission` is a source-side Business OS MCP tool.
It is invoked through Workjet's authenticated source CTOX connection; target
computers never receive that connection's Owner bearer or a model secret.

Its native checks require the persisted active source Owner/Admin, current
`CtoxTaskCreate` and `IntegrationsManage` policy, the source owner's active
project and its exact repository, and an assigned non-agentless workstation or
self-hosted computer belonging to that owner with a valid typed native `build`
entry in `capability_config`. Missing, storage-only or malformed settings deny
issue/claim/revalidation/renewal; removing or changing that build configuration
fences an existing permit. The native capability contract has no separate
enabled/ready flag: its validated operational configuration is eligibility;
the existing build adapter still probes endpoint readiness, acquires its slot
and checks resource floors before execution. Browser capability chips alone
are not worker permissions. Unknown native users cannot
be created by a claimed MCP actor, and command-scoped sessions cannot admit
independent workers.

## Wire contract

Tool: `business_os.remote_worker_admission`.
Actions:

- `issue`: `{action:"issue", binding, ttl_seconds:300}`. TTL is 1–300 seconds.
- `claim`: `{action:"claim", permit_id, binding, execution_id}`.
- `revalidate`: `{action:"revalidate", permit_id, binding, execution_id}`.
- `renew`: `{action:"renew", permit_id, binding, execution_id, renewal_sequence:1, ttl_seconds:300}`.
- `revoke`: `{action:"revoke", permit_id, binding}`.

`binding` has exactly these camelCase fields:

```json
{
  "requestId": "unique-request-id",
  "requestDigest": "64-lowercase-hex-SHA256-of-the-immutable-Workjet-request",
  "sourceEnvironmentId": "source-workjet-environment",
  "sourceSupervisorThreadId": "actual-source-supervisor-thread",
  "sourceInstanceId": "authenticated-source-CTOX-instance",
  "projectId": "native-owned-project-id",
  "targetEnvironmentId": "paired-target-workjet-environment",
  "targetConnectionId": "actual-target-connection",
  "targetInstanceId": "actual-target-instance",
  "targetComputerId": "opaque-native-assigned-computer",
  "repositoryUrl": "https://github.com/example/repository",
  "repositoryHead": "40-or-64-lowercase-hex-Git-commit",
  "workspaceKey": "unique-request-id",
  "credentialRef": {"environmentId":"source-workjet-environment","accountId":"opaque-account"},
  "providerRef": {"environmentId":"source-workjet-environment","provider":"provider"},
  "modelRef": {"environmentId":"source-workjet-environment","provider":"provider","modelId":"model"},
  "capabilities": ["repository_read","repository_write","run_checks","open_pull_request"]
}
```

The source instance must equal the authenticated gateway workspace. All three
model references must name the source environment, and model/provider names
must match. Repository URLs are HTTPS without embedded credentials, queries
or fragments; comparison normalizes host spelling and trailing `/`/`.git`.
`workspaceKey` equals the path-free request ID. It is not a host path.

The receipt has contract `ctox.workjet.remote-worker-admission.v1`, `permitId`,
`ownerUserId`, `authorityEpoch`, `authorityFingerprint`, `expiresAtMs`, the exact
`binding`, `state` (`issued`, `claimed`, `revoked`) and `executionId` (initially
null), and `renewalSequence` (initially zero). The permit ID is a locator in the source policy database, not an offline
credential. Replaying issue/claim cannot extend expiry, create a second permit,
or claim a different execution. Revoked/expired requests cannot be reissued.

The source owner can keep the same claimed execution alive by renewing before
expiry. Each renewal rechecks the current native account/epoch, project,
computer and policy; its TTL is again 1–300 seconds. The next sequence must be
exactly `renewalSequence + 1`. Repeating that sequence returns the existing
deadline and cannot extend it again; older/skipped sequences and other
execution IDs fail. Expired leases cannot be revived. A long worker therefore
renews its existing lease, rather than reissuing or starting a second attempt.
Loss of source connectivity or renewal authority requires the target/gateway
to stop using the expired lease. Revocation is idempotent for the exact current
authenticated source owner, including after expiry/computer/project retirement.

## Required production consumers

The Workjet source Broker supplies the actual supervisor, source Git HEAD and
SHA256 of its immutable request. The target Receiver uses its authenticated
EnvironmentRegistry connection to return the exact binding to the source MCP
channel, claims one execution ID, then revalidates before thread/first-turn
publication and after reconnect. It must keep durable target request/receipt
idempotency and reject a changed binding before touching its worktree.
The target confines `workspaceKey` to its own protected worker-worktree root,
checks canonical paths/symlinks, and never interprets it as an arbitrary path.

Model references only constrain this native delegation. They do not establish
that a gateway account exists, is enabled or is granted to the target. The
source ProviderGateway must separately resolve the exact current scoped
catalog grant for `connectionId`/`instanceId`/`computerId`, account, provider and
model, and retain the actual credentials at source. Every source gateway use
must also revalidate this claimed native permit; a catalog row alone does not
replace either authority. A target without that production integration must
fail closed, rather than using a target-local Owner or copying credentials.

The native claim and current native policy checks linearize in one IMMEDIATE
policy transaction. The response proves authority at that point; it does not
hold a remote process across an await or fence arbitrary target tools. The
Receiver/gateway own boundary revalidation and cancellation/expiry enforcement.
No remote process or installed acceptance is claimed by this source delta.
