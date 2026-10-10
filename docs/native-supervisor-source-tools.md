# Native Supervisor worker dispatch

The enrolled execution.v1 receiver offers one fixed native tool, worker_dispatch.
The generated Source contract carries tool_call, native_tool=worker_dispatch,
operation_id and tool_arguments_json. The SDK arguments contain task and optional
title/computer_id/worker_profile_id. They cannot contain a tool/action selector,
actor, project, context, root, token, URL or dispatch key.

The native handler derives the dispatch key from the retained UUID operation,
captures the exact incoming Source and original holding controller, and invokes
the existing registered project dispatcher inside the controller's already held
Core/Policy reservation. It retains the original signed Supervisor session,
current owner/project/role/epoch checks and native lease. It neither opens a
nested writer nor creates a replacement queue, controller or executor.

An acknowledgement lost after commit is replayed against the existing digest and
returns the same intent. Different work with the same UUID is rejected. Outer
native transaction rollback also rolls back the intent; the helper cannot
commit independently. Revoked/replaced/expired native scopes cannot dispatch.

The ordinary MCP dispatcher reuses the same helper and retains its existing
channel policy and transaction wrapper. The Source gets only its one fixed tool
descriptor, never the native restricted session token or a broad managed grant.

This is additive infrastructure. execution_ready remains false and actual_json
remains unset. Startup acknowledgement is not completion or an SDK process/stop
witness. The registered SDK lifecycle and original service producer are separate.
