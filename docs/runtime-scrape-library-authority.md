# Runtime scrape library authority

Outbound registration uses the SQLite target's activated revision, preserving
configuration, revision and hash, including rollback to an older revision.
The nonempty persisted body and materialized file must match the active hash.
Missing or invalid materialization requests generation/repair; it never imports
a source-tree provider script over the runtime library.

Missing targets are created from the adapter manifest or generic prospect schema.
First use without a usable script queues existing target-scoped repair admission,
which deduplicates open work and preserves cancellation/retry limits. The leaf
uses the registered target workspace and existing register-script/execute relay.
No daemon-root write access, new endpoint or provider-specific backend is needed.

## Invocation contract

App tests require an explicit `test_input.company` or `company`. Provider labels
and source IDs never supply company identity. Configuration and secret references
come from `CTOX_SCRAPE_MANIFEST_PATH`'s registered `config`; concrete query input
comes from `CTOX_SCRAPE_INPUT_JSON`. The native app caller forwards its own command
ID as `task_id`, discarding input-supplied task, owner and session claims. This ID
is correlation, not a credential. Generated workers use their actual harness
queue task reference. Generation receives explicit JSON input-file instructions;
without valid company input it registers the script but must not invent a query.

`test_input.operation_timeout_ms` defaults to 90000 and accepts integers from
1000 through 300000. Scripts pass this budget to authenticated browser automation.
The outer scrape runner gets ceil(operation milliseconds / 1000) + 30 seconds
for completion/evidence serialization. This is an operation budget, not a cap on
the lifetime of a task. Runtime scripts must honor the same input contract.

## Native owner resolution

Task-link, spawned-task metadata and direct command lookups require native
admission evidence and revalidate current actor activity, role, scope and policy.
Recoverable control commands use their original control permission instead of a
queue-command permission. Client-context and payload owner claims cannot replace
that evidence. Terminal requesting queue tasks are rejected. The explicitly
trusted local auth-assist interface retains its existing local-owner fallback;
public authenticated automation does not enable it. Signed command-session
validation remains available without issuing new sessions to generated workers.

This is admission-time revalidation. It does not prove atomic revocation during
an ongoing browser operation, task-scoped executor interruption or stale artifact
publication fencing. Those remain separate acceptance requirements.

## Evidence limits

Regression source covers activated revision preservation, invalid materialization,
novel first-use generation and native registration/execution, explicit company
input, rejected operation budgets, discarded authority claims, native control
permission checks, inactive actors and spawned-task owner resolution. Synthetic
fixture-generated JavaScript is not real model/browser/tenant acceptance.
Formatting and diff checks passed; Cargo compile/tests/clippy and live runtime
proof remain required before readiness or deployment claims.
