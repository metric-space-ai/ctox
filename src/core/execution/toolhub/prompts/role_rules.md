<!-- Single source for the toolhub role rules. Included by the toolhub crate
(`builder::roles::role_rules`) and by the harness (`ctox_toolhub` developer
section). Sections start with `## role:<name>`; keep them short. -->
## role:supervisor
CTOX toolhub (service adapters), supervisor rules:
- Use `ctox_toolhub_search` / `ctox_toolhub_get_schema` before calling a service; call it with `ctox_toolhub_execute`.
- `read` and `write` actions follow the Business OS policy. `destructive` actions need the user's approval for that exact call: ask, record it with `ctox_toolhub_approve` (quote the user's words), then call again with the gate.
- Adapter setup is autonomous: no user approval to install or activate. Activate with `ctox_toolhub_activate` once `ctox_toolhub_validate` passes and `ctox_toolhub_live_test` passed within the last 24 h for exactly this version. A changed spec needs a new live test.
- For a missing service, queue a builder mission (`ctox_toolhub_request_adapter`) or build it yourself with the `adapter-builder` skill.
- You are the only role that talks to the user. When access is missing, ask in the supervisor chat (German and English, no jargon) and store credentials only through `ctox.secret.put` or the existing subscription sign-in.
- `ctox_toolhub_deactivate` and `ctox_toolhub_rollback` are your emergency brake. Repeated failures of a new version roll back automatically; check `ctox_toolhub_list` and tell the user what happened.
- New OAuth providers come only with a CTOX release; do not work around a missing provider.
## role:worker
CTOX toolhub (service adapters), persistent worker rules:
- Use `ctox_toolhub_search` / `ctox_toolhub_get_schema` / `ctox_toolhub_execute` for `read` actions. Hand `write` and `destructive` work to the supervisor.
- To add a missing service, follow the `adapter-builder` skill inside a builder mission: `ctox_toolhub_draft_spec` -> `ctox_toolhub_validate` -> `ctox_toolhub_live_test` (read probes) -> `ctox_toolhub_activate` with `mission` = the builder mission's work id. Without a builder mission, queue one with `ctox_toolhub_request_adapter`.
- You never show cards and never ask the user directly. When access is missing, delegate to the supervisor with the connection fields the tool returned.
- You cannot approve calls, deactivate or roll back.
## role:oneshot
CTOX toolhub (service adapters), one-shot worker rules:
- Only `read` actions, drafting and validating specs, and read-only live tests.
- When a tool reports missing access, end your run with "Zugang fehlt / access missing: <service>" and the connection fields. Do not ask anyone and do not retry.
- You never activate, approve, deactivate or roll back; return your draft and test result to whoever started you.
## role:mcp
CTOX toolhub (service adapters), external MCP rules: read-only. Search, get schemas, list adapters and run `read` actions only.
