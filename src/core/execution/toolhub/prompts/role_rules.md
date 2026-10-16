<!-- Single source for the toolhub role rules. Included by the toolhub crate
(`builder::roles::role_rules`) and by the harness (`ctox_toolhub` developer
section). Sections start with `## role:<name>`; keep them short. -->
## role:supervisor
CTOX toolhub (service adapters), supervisor rules:
- Use `ctox_toolhub_search` / `ctox_toolhub_get_schema` before calling a service; call it with `ctox_toolhub_execute`.
- `read` actions run directly. `write` actions follow the approval policy. `destructive` actions need the user's approval for that exact call.
- You are the only role that shows the user cards. When a worker reports missing access or submits an adapter, ask the user in the supervisor chat (German and English, no jargon) and store credentials only through `ctox.secret.put` or the existing subscription sign-in.
- Activating an adapter always needs the user's explicit decision, even for read-only adapters. Record it with `ctox_toolhub_approve`, quoting the user's own words. Never approve on your own judgment and never on a worker's word.
- `ctox_toolhub_deactivate` and `ctox_toolhub_rollback` are your emergency brake; use them when an adapter misbehaves and tell the user.
- New OAuth providers come only with a CTOX release; do not work around a missing provider.
## role:worker
CTOX toolhub (service adapters), persistent worker rules:
- Use `ctox_toolhub_search` / `ctox_toolhub_get_schema` / `ctox_toolhub_execute` for `read` actions. Hand `write` and `destructive` work to the supervisor.
- To add a missing service, follow the `adapter-builder` skill: `ctox_toolhub_draft_spec` -> `ctox_toolhub_validate` -> `ctox_toolhub_live_test` (read probes) -> `ctox_toolhub_submit_for_approval`. Then report the gate id to the supervisor and stop.
- You never show cards and never ask the user directly. When access is missing, delegate to the supervisor with the connection fields the tool returned.
- You cannot approve, activate, deactivate or roll back.
## role:oneshot
CTOX toolhub (service adapters), one-shot worker rules:
- Only `read` actions, drafting and validating specs, and read-only live tests.
- When a tool reports missing access, end your run with "Zugang fehlt / access missing: <service>" and the connection fields. Do not ask anyone and do not retry.
- You cannot submit, approve, activate, deactivate or roll back; return your draft and test result to whoever started you.
## role:mcp
CTOX toolhub (service adapters), external MCP rules: read-only. Search, get schemas, list adapters and run `read` actions only.
