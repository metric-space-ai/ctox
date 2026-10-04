# Domain application receipts and command recovery

Status: shared native boundary and automatic receipt-backed intake implemented.
PR80's domain-handler integration is included in this base. Full native tests
for the combined automatic intake and domain handlers remain pending. This does
not certify production readiness, portable sessions or tenant migration.

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
commands integrated from PR80. Their dispatchers pass the new admission into
the domain transaction; signed owner-alias migration for project upsert uses
that same transaction. Their former post-commit publishers are removed.
Other command paths keep their existing behavior. Additional domains require
an explicit opt-in and equivalent tests.

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
retry scheduler. The existing intake selects nonterminal accepted/completed/failed
projections for the seven opted-in command types only when their domain receipt
exists. It reads receipts through a read-only SQLite attachment, preserving the
existing oldest/newest intake fairness. Missing stores or legacy schemas are
neither created nor migrated by selection. Candidate discovery uses the existing
command-type index; unrelated accepted history must not force a status-index scan.

The complete ordered intake query separates pending, background and applied
candidates into disjoint indexed branches. Each branch takes at most the requested
oldest/newest page before the final merge. It preserves the existing two-ended
fairness and command limit. The previous combined predicate is a test-only
semantic oracle, not an alternative executable intake path.

Before interpreting a candidate's browser-authored payload, type or age, native
intake looks up its application identity by command ID. It reconstructs the
command from Core's canonical intent, verifies the receipt hash, loads the still
active native user bound to the receipt and applies the same central policy.
This delivery-only path cannot enter a handler or create a new admission. It
needs no retained browser bearer token. Normal browser requests still require
their existing authentication. An inactive/unknown actor, revoked policy or
identity conflict stops recovery without changing an applied effect to failure.

Delivery exhaustion reuses the existing paced intake retry without repeatedly
rewriting accepted projections. A receipt protects the command from terminal
failure by ID, even if an incoming document has a falsified command type. Native
selection, authorization and completion together still need the full-host gate.

## Evidence and remaining gates

The actual standalone receipt module has six passing local Rust tests, including
a child process exiting immediately after COMMIT, SQLite ABORT when writing the
receipt, mutation rollback, actor/intent conflicts, immutable replay and concurrent
application. Three additional selection tests pass: read-only attachment and
nonterminal filtering, absent/legacy stores, and indexed selection with 20,001
commands. In the local 30-sample selection-only comparison, corrected p50/p95
were 33/81 microseconds versus 15,273/27,502 for the broad status-index query.
This excludes connection setup and the complete ordered intake query; it is not
a browser roundtrip benchmark. These tests do not execute the complete binary.

Two further tests cover the complete ordered query: the old combined OR query
visited 1,000,305 SQLite VM steps for five candidates among 20,005 records. The
bounded branches visit 454 steps. In the isolated 30-sample local comparison,
ascending p50/p95 changed from 27,115/29,464 to 112/177 microseconds; descending
from 27,645/34,407 to 108/130. The full 11-test component run under concurrent
test load measured 296/698 and 291/452 microseconds respectively. Those query
measurements include prepare/result decoding but exclude opening databases and
network/runtime delivery. A separate 2,352-row state matrix checks exact results
against the old predicate, with and without receipts, both directions and four
limits. All eleven component tests pass; full-host performance remains separate.

The full-host CI now includes real command-plane tests for recovery after a
projection ABORT, preserving a later domain title, missing result storage after
Core completion, deleting obsolete projected fields, native tombstones, changed
actor/payload, no-receipt uncertainty and intake exhaustion. Those shared-boundary
tests passed in full-host CI run 34305942733 at e80c66e2e8. New tests exercise
automatic native intake of an old accepted command without a browser token,
untrusted intake payload/actor, current source projection, terminal removal from
the queue, inactive users and conflicting Core identity. They have not yet run
in the complete binary for this change.

PR80's native tests cover real post-commit handler fault injection, a fresh process
through the command path, profile/chat domain invariants, distinct create IDs and
revoked membership. Its independent run 34306615560 passed 45 tests, including
those recovery cases. The combined branch must pass them again together with
automatic intake. Browser/WebRTC E2E and measured command/boot budgets
remain required. Run 34305942733 passed its warm-command and critical-boot gates
but failed the context-app workflow; an earlier run also missed the warm-command
budget. These component results waive no gate. No production tenant was modified.
