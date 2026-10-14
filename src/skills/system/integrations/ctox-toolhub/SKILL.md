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

Use the CTOX Toolhub for third-party apps; never call provider APIs with your own tokens and never ask for secrets in chat.

- `ctox toolhub list` shows connected accounts (no secrets). `ctox toolhub catalog` lists toolkits, actions and input schemas. Read the schema before calling.
- `ctox toolhub exec <toolkit>.<ACTION> --json '<args>'` runs one action and prints JSON. Writes need `--confirm`, only after the change is approved.
- Exit codes: 0 ok; 2 invalid arguments (fix them); 3 credential_required (no usable connection); 4 provider error (report it, do not loop); 5 approval_required (get approval, then repeat with `--confirm`); 1 internal.
- Report results only from the returned JSON. A failed or skipped call is not done.
- If `ctox` is not on PATH here, the Toolhub is not reachable from this harness: say so, do not improvise.
  <!-- toolhub-prompt:role:supervisor -->
  Role rule (Supervisor): only you raise user cards. On exit 3, or a worker's `needs_user`, run `ctox ui request credential --toolkit <slug>` (or `connect` for OAuth apps, `confirm` for risky actions) once; requests are deduped. Wait for it, then resume the parked persistent worker or re-dispatch the one-shot worker with the same task. Never ask for the secret in chat. Skill: ctox-ui-requests.
  <!-- toolhub-prompt:role:persistent_worker -->
  Role rule (Persistent worker): never raise user cards; `ctox ui request` is refused here (exit 6). On exit 3, take the `needs_user` object from the output (kind, toolkit, reason, fields), hand it to your supervisor, and park. Continue other independent work and resume when the supervisor reports it resolved.
  <!-- toolhub-prompt:role:one_shot_worker -->
  Role rule (One-shot worker): never ask anyone and never wait. `ctox ui request` is refused here (exit 6). On exit 3, stop and end your run with the `needs_user` object (kind, toolkit, reason, fields) as your result; the supervisor asks the user and re-dispatches you.
  <!-- toolhub-prompt:role:operator -->
  Role rule (Operator shell): no team session is bound. You may raise a card with `ctox ui request credential --toolkit <slug>`. Team workers must have CTOX_SESSION_ID or CTOX_TEAM_ROLE set.
  <!-- toolhub-prompt:end -->

## Details

- Per-connection fields (Jira site, Zendesk subdomain, Salesforce instance URL, …)
  appear in `catalog` as `connection_fields` and on the credential card; CLI:
  `ctox toolhub connect … --credential-stdin --param site=acme`.
- Role detection and enforcement: `src/core/execution/toolhub_role.rs`; card
  lifecycle: skill `ctox-ui-requests`.
- Spawners export `CTOX_SESSION_ID` (the Workjet thread id) and `CTOX_TEAM_ROLE`
  (`supervisor` | `persistent_worker` | `one_shot_worker`) into every worker.
