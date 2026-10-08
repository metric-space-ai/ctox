
The nonvoting target does not call `validate_ownership` on A: the WorkerAuthorityClient deliberately denies foreign executor reads. The existing signed quorum TakeOver validates A's expected generation and B's protected copy atomically; the shared native caller then requires a fresh Applied result and quorum-validates B's new generation. A regression uses three independent durable Raft nodes, real signatures and the actual nonvoting WorkerAuthorityClient; it verifies A's read is denied, B takes over the same job, A becomes stale and a replay cannot grant fresh activation. This is a component regression, not installed continuation acceptance.
# Native checkpoint quorum protection

## Protected target guest enrollment

After a completed native `handoff-take-over <binding-digest>`, Linux targets
use `ctox sync handoff-enroll-guest <binding-digest>`. The private same-UID
checkpoint IPC accepts only that binding identifier. It resolves the current
target account, Receive/Resume policy, exact completed takeover row and fresh
quorum ownership of the original job. The protected checkpoint must be complete,
clean, current and durably copied at this target.

The original guest ID and guest-service session come from the bounded,
hash-checked machine artifact in that same checkpoint. The target creates its
own new controller and private import directory while preserving those original
names, original Core session/ExecutionSpec, checkpoint digest and next ownership.
No operator-supplied guest ID, process ID, host path or copied controller can
replace them. An exact retry returns the same retained controller; foreign
checkpoint/binding identities and retired controllers require reconciliation.

The existing `guest-enroll` command remains the fresh-source enrollment path.
A protected target enrollment cannot acquire the ordinary fresh-Create admission.
Its response remains `resumed:false`: enrollment starts no provider, QEMU or
Core turn and cannot substitute for the actual completed GuestImportReceipt,
original-session Core factory, restored original service and current execution
activation. Those production connections and installed two-host proof remain
separate outstanding work. Frozen Mac037 is not an approved capture runtime.


The current native checkpoint copy creates verified local artifacts. Execution takeover also needs signed complete-copy receipts committed by the existing quorum. These operator commands add that connection on the running host's private checkpoint socket; they never start another authority, peer, provider or guest.

After `ctox sync handoff-copy <binding-digest> <source-route>`, the target runs:

```sh
ctox sync handoff-acknowledge-copy <binding-digest> > target-copy-receipt.json
```

This native execution-handoff operation requires the target binding's current Receive and Execute decisions, its own current direct account, current issuer, live host and actual configured node/scope. It rechecks every durable artifact through the shared checkpoint signer. Missing or unknown/pending effects deny the signature. The output contains public signed receipt metadata plus `resumed:false`; no checkpoint content or credentials are emitted.

The source consumes an array of independent target `receipt` objects on stdin:

```sh
jq '[.receipt]' target-copy-receipt.json |
  ctox sync handoff-protect-checkpoint <binding-digest>
```

Only public receipt metadata may be delivered to the source operator; artifacts still use the protected native peer path. The source resolves the enrolled capture, current disclosure decision and its own actual configured execution authority. It verifies its original complete copy and adds its own signed receipt. A native policy transaction commits the exact pending request, receipt set, binding revision and principal epoch before the quorum await. The request nonce is the quorum request ID. The existing quorum validates all copy signatures, current member/data-replica roles, exact job/ownership and checkpoint sequence; matching strings or local files cannot provide membership or protection.

The source accepts only a fresh Applied receipt, confirms that same complete current job via validate_ownership, then rechecks current account, issuer, native disclosure binding and host/operation lifetime before publishing its durable Protected state. Cancellation, denial, replay, lost response or changed authority leaves pending evidence requiring reconciliation. It does not automatically retry another quorum request for the same binding/checkpoint/generation. A fresh signed disclosure decision has the existing60-second validity window; this is not an atomic recall of a decision already sent to a remote quorum.

The same-UID IPC frame is bounded to32KiB and at most8 independent receipts. Existing copy/reconstruction/import request forms remain valid. Mixed operations are rejected. Protection and acknowledgements report `resumed:false`; they grant no target admission, clean-effects certificate, Core continuation or VM activation.

The currently captured native Core state still carries unknown external effects, so these commands deliberately reject that capture. Architecture still owns authoritative effect reconciliation and original-session target factory/activation; the explicit ownership operation is described below. VM supplies protected RAM/disk staging, Transfer supplies artifact transport. Goals15/16/18 require installed independent-host continuation, stale-source rejection, reconnect and abort; unit/fixture results cannot establish them.

This source is stacked on #396. Compiler, focused production-path tests, Clippy and RxDB/browser checks must run on the final head through the canonical Linux gate before normal merge. No installed runtime or merged revision is claimed by this draft documentation.

## Target ownership of the existing job

After complete native copy and source quorum protection, the target operator may run:

```sh
ctox sync handoff-take-over <binding-digest>
```

The command derives the original job/session, source ownership and checkpoint from the
native target binding. It requires current target Receive and Execute decisions, the actual
target account, issuer, configured execution node/scope and live host. It verifies the complete
received manifest and all durable content before submitting anything; unknown/pending native
effects remain a rejection. Full content hashing releases publication locks.

The original source job must still have exactly the enrolled ownership and protected
checkpoint, with no pending effects or refresh requirement; the target must hold a protected
replica. The native policy store commits the exact request, old/new ownership, original spec,
binding revision and principal epoch as Pending before submitting the existing quorum
TakeOver command with a fresh signed Resume permit. Only a fresh Applied receipt followed by
the same current quorum job and fresh native authority may publish Owned. Replayed, lost,
denied or cancelled requests stay Pending and require reconciliation, rather than a second
ownership attempt. The committed quorum transition excludes the old ownership generation.

The public receipt reports original jobId/sessionId, next ownership and resumed:false.
Ownership alone does not construct a provider, restore Core, import a guest, or resume QEMU.
Those operations must freshly validate the resulting ownership and actual completed import
before execution. This step supplies no installed continuation claim; the remaining native
source effect proof and original-session target activation are still tracked in issue183.
