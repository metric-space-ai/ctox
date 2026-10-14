---
name: ctox-toolhub
description: Use connected third-party apps (Gmail, Slack, GitHub, Jira, HubSpot, Notion, … 39 services) through `ctox toolhub list/catalog/exec`, and hand missing credentials to the supervisor as `needs_user`. Single source of the harness system-prompt section for every role.
cluster: integrations
---

# CTOX Toolhub

This file is the **single source of truth** for the Toolhub/UI-card section that
every harness gets in its system prompt. The text between the `toolhub-prompt`
markers is rendered verbatim: the `common` block for every role, plus exactly one
role block. CTOX native (`toolhub_role::prompt_section`) and Workjet
(`packages/workjet-capabilities/skills/ctox-toolhub/SKILL.md`, a verbatim copy
checked by its generator) both read these markers. Edit here, then copy.

<!-- toolhub-prompt:common -->

## CTOX Toolhub (connected apps)

Use the CTOX Toolhub for third-party apps; never call provider APIs with your own tokens and never ask for secrets in chat. Two equivalent ways, same instance, same connections:

- Tool `ctox_business_os` (no local `ctox` needed): `toolhub_list`, `toolhub_catalog` (optional `toolkit`), `toolhub_exec` with `tool` = `<toolkit>.<ACTION>`, `arguments` and `confirm`, and `toolhub_needs_user` with `toolkit` and `reason`.
- CLI, only if `ctox` is on PATH: `ctox toolhub list`, `ctox toolhub catalog`, `ctox toolhub exec <toolkit>.<ACTION> --json '<args>' [--confirm]`.
- Read the action's input schema before calling. Writes need confirm, only after the change is approved.
- Exit codes (`exit_code` in tool results): 0 ok; 2 invalid arguments (fix them); 3 credential_required (no usable connection); 4 provider error (report it, do not loop); 5 approval_required (get approval, then repeat with confirm); 1 internal.
- Report results only from the returned JSON. A failed or skipped call is not done. If neither way is available here, say so; do not improvise.

<!-- toolhub-prompt:role:supervisor -->

Role rule (Supervisor): only you raise user cards. On credential_required, a worker's `needs_user`, or before a risky action, call `toolhub_needs_user` (or `ctox ui request credential|connect|confirm --toolkit <slug>`) once; it pins a card in the chat and is deduped. End the turn as waiting, then resume the parked persistent worker or re-dispatch the one-shot worker with the same task. Never ask for the secret in chat. Skill: ctox-ui-requests.

<!-- toolhub-prompt:role:persistent_worker -->

Role rule (Persistent worker): never raise user cards; CTOX refuses them for you and returns a `needs_user` object (kind, toolkit, reason, fields) instead. Hand it to your supervisor with `workjet_escalate` and park. Continue other independent work and resume when the supervisor reports it resolved.

<!-- toolhub-prompt:role:one_shot_worker -->

Role rule (One-shot worker): never ask anyone and never wait. CTOX refuses user cards for you and returns a `needs_user` object (kind, toolkit, reason, fields) instead. Stop and end your run with it as your result; the supervisor asks the user and re-dispatches you.

<!-- toolhub-prompt:role:operator -->

Role rule (Owner chat): you talk to the Owner directly and may raise a card with `toolhub_needs_user` (or `ctox ui request credential --toolkit <slug>`). End the turn as waiting until it is resolved.

<!-- toolhub-prompt:end -->

## Details

- Per-connection fields (Jira site, Zendesk subdomain, Salesforce instance URL, …)
  appear in `catalog` as `connection_fields` and on the credential card; CLI:
  `ctox toolhub connect … --credential-stdin --param site=acme`.
- Role detection and enforcement: `src/core/execution/toolhub_role.rs`; card
  lifecycle: skill `ctox-ui-requests`.
- Spawners export `CTOX_SESSION_ID` (the Workjet thread id) and `CTOX_TEAM_ROLE`
  (`supervisor` | `persistent_worker` | `one_shot_worker`) into every worker.
