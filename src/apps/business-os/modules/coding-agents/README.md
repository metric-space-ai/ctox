# Coding Agents

Business OS module for managing CTOX coding-agent provider sessions.

The module dispatches through the shared `business_commands` shell collection
and receives native Core outcomes for `ctox.coding_agent.*` commands. Native
Core also projects durable provider state into the module collections declared
in `collections.schema.json`:

- `coding_agent_workspace_grants`
- `coding_agent_sessions`
- `coding_agent_events`

Provider execution is routed through the native CTOX `coding_agents` core
module. The implementation includes provider discovery, provider-owned auth
guidance, workspace grants, session create/continue/list/get/stop, and the
durable mock-contract provider used by CLI, UI, and RxDB command tests.
