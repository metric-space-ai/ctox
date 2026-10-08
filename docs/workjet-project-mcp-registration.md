# Typed Workjet project registration over MCP

`business_os.upsert_project` creates or updates an owned project through
`ctox.workjet.project.upsert` and its native durable command admission.
Supply the saved `project_id`, `name` and a stable `idempotency_key`.
Never allocate a second ID to retry a lost response. The response contains the
native `command_id`, completed status, owned project and default group-chat ID.
The same actor/project/key returns the same receipt; changed intent is rejected.

Optional `repo_url`, `description` and `info` use the existing native project
contract. Omitted/null optional values preserve stored values. An explicit
`info` object replaces that object, so read the owned project first and preserve
its other info fields. `info.goal` stores the agreed goals (up to4096 bytes).
It does not prove that a Workjet Supervisor has received a message or executed
the goals. Supervisor handoff and execution require their own measured receipt.

The tool cannot set a website, archive state, schedule, working copy, owner,
secret, terminal input or server update. Existing values of those fields survive
an update. Foreign projects are denied even for an actor with an administrator
role. Native identity resolves enrolled same-person aliases without changing
the stored owner.

Managed clients need read and write admission, the tool if allowlisted, module
`ctox` if module-scoped and collection `workjet_projects` if collection-scoped.
Both gateway and native enforce the project read scope. Command-scoped Crew or
restricted Supervisor sessions cannot create independent projects. Do not widen
an existing connector to make a denied call pass.

For Molecularity, preserve project ID
`d8dfa343-c928-4d3c-a05e-58be3514ceb1` and repository
`https://github.com/metric-space-ai/molecularity`. The owner goals are Engine,
Demos and a GitHub project page. Do not infer a website URL from the project name.
Deployment, admitted MCP read/write and the actual Supervisor handoff are
separate evidence from unit tests or a merged source change.
