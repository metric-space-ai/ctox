# Workjet Dev/Ops: native task bridge

Workjet Dev and Business OS Ops must link to the same durable CTOX command and
execution task. The CTOX menu entry will dispatch to the daemon belonging to the
bound instance; external harness app tools use the typed Business OS MCP channel.
This document records the first native prerequisite, not completion of that adapter.

## Native delegation retry contract

`business_os.create_app` and `business_os.modify_app` accept an optional
`idempotency_key`: a non-empty string, at most 256 UTF-8 bytes, without control
characters. The server trims surrounding whitespace. An omitted key retains the
existing behavior of creating a new logical request.

`business_os.execute_action` implements the same contract for
`action_id: "ctox.delegate_task"`. This supports durable module-scoped work
beyond app-source changes. Its title, objective, record and payload form the
native command intent; the existing action allowlist and policy still apply.
The person-research action retains its separate retry contract. Other action
kinds reject this top-level key instead of silently treating a retry as new work.

A client allocates and persists the key before its first send. Reuse the same key
and identical arguments after an uncertain response. The server derives the command
identity from the resolved actor, workspace and key, independently of HTTP/JSON-RPC
request ids. The actual daemon store supplies the instance boundary. The queue
atomically claims the command and task and rejects a changed intent for that key.
Permission checks still run on each request; the key grants no authority.

A repeated claim returns its original command/task ids and current canonical
status/result. It does not reset compatibility projections to `accepted`, rerun
app starter materialization, or reassign the Crew member. Terminal failures remain
failures. A deliberate new attempt is a new logical request and needs a new key.

Clients must discover support in the tool's input schema before relying on this
contract. Older daemons may silently ignore unknown arguments. Workjet PR #54
implements keyed app/general-task delegation with explicit schema negotiation
and durable client request-key storage. Its finite `delegate_task` operation
fixes the native action to `ctox.delegate_task`; it cannot select an arbitrary
action or override the authenticated actor. The native CTOX provider adapter
and its menu/session/event integration remain pending.

## Verification

CI explicitly runs the `mcp_app_retry` tests and the existing queue-claim identity
test. They exercise actor/workspace scoping, changing transport request ids,
invalid keys, real create/modify/general-task delegation, terminal-failure preservation and
changed-intent rejection. They do not yet prove the complete Workjet Dev/Ops UI,
concurrent desktop sends, Crew context transport or cross-harness learning.

Local Rust parsing was checked using rustfmt 1.93.0 without writing formatting
changes. Cargo checks/tests and the native/browser RxDB suites remain unexecuted
locally: the shared host admission gate reports insufficient tmp space, excessive
swap and load. Full type and behavioral verification remains required.
