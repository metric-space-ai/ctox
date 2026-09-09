# Domain application receipts and command recovery

Status: shared native boundary implemented; PR80 domain-handler integration and
its complete user-flow fault tests are pending. This does not certify production
readiness, browser delivery, portable sessions or tenant migration.

## Ownership

Core owns command admission, immutable intent, lifecycle, retry evidence and
terminal results. The Business OS domain transaction owns the fact that it
applied a local mutation. `business_command_domain_effects` stores that fact
in the same SQLite transaction as the affected `business_records`. It is
not an independent command status, outbox, poller or permission source.

The receipt binds command ID, the existing canonical payload hash and the
centrally authenticated user ID. The additional actor binding is required
because the existing audit-derived intent hash does not include actor identity.
It retains the original result and native-selected collection/record references.
It never retains projection snapshots or credentials.

## Handler interface

The command plane creates `DomainEffectAdmission` only for a new Core claim
after central authorization. The dispatcher exposes it through
`prepared.domain_effect_admission.as_ref()` to the opted-in handler:

```rust
let mut conn = open_store(root)?;
let applied = admission.apply(&mut conn, |tx| {
    // All local domain mutation uses this transaction.
    Ok(AppliedDomainEffect {
        result: original_result,
        projections: vec![DomainRecordRef {
            collection: collection.to_owned(),
            id: record_id.to_owned(),
        }],
    })
})?;
Ok(applied.result)
```

The domain adapter receives the admission by reference from the dispatcher;
it does not manufacture one or make a second claim. The shared command plane
publishes and completes after the handler. The old domain-specific post-commit
publisher is removed from migrated command paths. External effects, Core writes,
RxDB writes and separately opened database connections are forbidden inside
this local mutation closure.

The initial opt-in is project upsert and the six project/chat/worker-binding
commands under PR80. The existing project upsert handler has not yet switched
to the transaction helper on this base; the PR80 owner integrates that change
together with its domain transaction. Other command paths keep their existing
behavior. Additional domains require an explicit opt-in and equivalent tests.

## Recovery

Only commands carrying a receipt can pass the prior uncertain/terminal replay
shortcut into the centrally authorized recovery path. No receipt means no
automatic mutation replay. Intent or actor mismatch is rejected before result
delivery. Current policy still applies; denial cannot rewrite an already
applied effect as a failed command.

Recovery reads the current source records (including tombstones), publishes
complete replacements through native RxDB and then finishes through Core's
existing command completion. Removed fields are not deep-merged back from old
projections. A revision check after publication retries at most three times if
a concurrent domain commit changed the source. Missing source/collection,
corrupt receipt or continued source churn fails delivery and retains the proof.
No cross-WAL atomicity is claimed.

The original command result is preserved even when subsequent domain edits
changed the referenced records. Core completion is idempotent. A receipt that
conflicts with an existing failed/cancelled terminal outcome is stopped for
reconciliation, never silently reopened. Missing result storage remains a
delivery error even after Core completion and can be repaired on same-ID replay.

Native intake exhaustion uses Core's existing failure journal but cannot create
a terminal mutation failure when the domain owner has a receipt. There is no new
retry scheduler. Automatic retry availability across the full native intake
lifecycle remains an integration gate.

## Evidence and remaining gates

The actual standalone receipt module has six passing local Rust tests, including
a child process exiting immediately after COMMIT, SQLite ABORT when writing the
receipt, mutation rollback, actor/intent conflicts, immutable replay and concurrent
application. These component tests do not execute the complete CTOX binary.

The full-host CI now includes real command-plane tests for recovery after a
projection ABORT, preserving a later domain title, missing result storage after
Core completion, deleting obsolete projected fields, native tombstones, changed
actor/payload, no-receipt uncertainty and intake exhaustion. Their execution
results are required before declaring this boundary verified.

PR80 additionally owns real post-commit handler fault injection, a fresh process
through the full command path, profile/chat domain invariants, distinct create
IDs and revoked membership. Browser/WebRTC E2E and measured command/boot budgets
remain required; the existing failing context and warm-command gates are not
waived by these tests. No production tenant was modified.
