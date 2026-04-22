---
name: service-deployment
description: Install, configure, start, verify, and hand off a local or external-backed service using inspectable helper scripts, explicit secret classification, and durable completion or blocked state. Use when CTOX must bring a service to a working deployed state rather than only plan a generic change.
cluster: host_ops
---

# Service Deployment

Use this skill when the goal is to install or deploy a service.

Do not use it for:

- broad operational health review: use `reliability_ops`
- generic repo or host scoping: use `discovery_graph`
- low-level secret handling by itself: use `secret-management`
- generic post-install verification by itself: use `acceptance-verification`
- generic narrow config changes without a deployment target: use `change_lifecycle`

This skill uses the shared SQLite kernel via `skill_key=service_deployment`.

For CTOX mission work, only SQLite-backed runtime state counts as durable deployment knowledge. Continuity commits, ticket knowledge, verification runs, communication records, and ticket/self-work state count. Workspace markdown files or ad hoc notes do not count as durable knowledge on their own.

## Operating Model

Treat this skill as:

1. preflight evidence capture
2. explicit deployment-shape classification
3. secret classification
4. bounded execution slices
5. verification and handoff

This skill is responsible for getting the service into a deployable state.
It is not allowed to declare a deployment successful based only on process or port presence.
Use `acceptance-verification` for the final proof that the service is actually usable.

Preferred helper scripts under `scripts/`:

- `deployment_collect.py`
- `deployment_capture_run.py`
- `deployment_store.py`
- `deployment_bootstrap.py`

They are inspectable helpers, not hidden authority. Read or patch them when the deployment shape is unusual.

## Tool Contracts

- `deployment.capture_raw`
- `deployment.store_capture`
- `deployment.store_graph`
- `deployment.bootstrap`
- `secret.classify_and_store`

## Workflow

1. State the concrete service target and the success condition.
2. Capture preflight evidence first:
   - available package/runtime managers
   - occupied ports
   - existing service presence
   - sudo/root feasibility when required
3. Classify the deployment shape before touching credentials:
   - `local_install`
   - `external_integration`
   - `existing_service_repair`
4. Use `secret-management` to classify every credential requirement:
   - generated
   - discovered
   - owner-supplied
   - external reference
5. Generate and store local admin credentials when CTOX can safely own them.
6. Ask the owner only for values CTOX truly cannot derive or generate.
7. Execute the install or rollout in bounded slices, verifying each slice before continuing.
8. Persist the resulting deployment, blocker, or verification state.
9. If the service starts but the acceptance check is still failing, do not report `executed`. Report `needs_repair` or `blocked` with the exact failing verification layer.
10. When the deployment is owner-visible, ticket-bearing, or depends on prior operational knowledge, inspect the active ticket/knowledge plane first. If the source system, source skills, or knowledge domains are absent, treat that as an explicit maturity gap instead of silently substituting workspace notes for durable knowledge.

## Secret Handling

- For local installations, prefer self-generated admin credentials persisted to a local secret reference.
- For external integrations, ask only for the exact external endpoint or credentials that CTOX cannot create itself.
- Never treat a missing credential as owner-supplied by default.
- Never forget generated admin credentials. Store a local reference before reporting success.

## Operator Feedback Contract

Use these exact headings:

- `**Status**`
- `**State**`
- `**Deployment Shape**`
- `**Secret Status**`
- `**Current Findings**`
- `**Autonomous Actions**`
- `**Escalation**`
- `**Next Step**`

`State` must be one of:

- `proposed`
- `prepared`
- `executed`
- `blocked`
- `needs_repair`

## Completion Gate

Do not finish the reply until all of the following are true:

- all eight headings are present
- `Deployment Shape` explicitly says whether this is a local install or external integration
- `Secret Status` explicitly says which credentials were generated, discovered, still missing, or stored by reference
- if `State` is `executed`, verification evidence is named in `Current Findings`
- if `State` is `needs_repair`, the exact failing verification layer is named:
  - `service_process`
  - `listener`
  - `http`
  - `authenticated_api`
  - `admin_identity`
  - `mutating_smoke`
  - `persistence`
- if the deployment is still open, a durable queue or schedule record exists
- if blocked, the exact missing value and accepted reply path are named

## Guardrails

- No claim of success without a working verification probe.
- No claim of success when only a listener or web frontend is up but authenticated or mutating verification is still failing.
- No owner credential request unless `secret-management` says the value is truly owner-supplied.
- No vague "I need variables" blocker messages.
- No silent continuation of multi-step installs.
- No claim that a deployment is operationally understood when the only written record is a workspace file rather than SQLite-backed runtime state.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/deployment-rules.md](references/deployment-rules.md)
- [references/install-patterns.md](references/install-patterns.md)
- [`acceptance-verification`](../acceptance-verification/SKILL.md)
