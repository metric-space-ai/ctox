---
name: business-os-outbound-campaign-setup
description: Configure a Business OS Outbound campaign from a natural-language campaign briefing and write the resulting structured settings back through the CTOX command bus. Use when a CTOX queue task carries suggested_skill business-os-outbound-campaign-setup or a Business OS chat task has required_skills including business-os-outbound-campaign-setup.
cluster: communication
---

# Business OS Outbound Campaign Setup

Use this skill when a CTOX queue task has `suggested_skill: business-os-outbound-campaign-setup` or the task payload requires this skill.

The Outbound app is only the structured task producer. It stores the natural-language campaign briefing and asks CTOX to configure the campaign. Your job is to convert that briefing into structured Outbound campaign settings and write them back through the command bus.

## Non-negotiable rules

- Do not send any outbound message.
- Do not call external HTTP services, mail gateways, landing-page services, or document APIs.
- Do not bypass Business OS RxDB/command-bus writeback.
- Do not use hidden heuristics in the UI. The skill is the authority for interpreting the briefing.
- Keep all outbound communication approval-gated.
- If the briefing asks for something the current Outbound app cannot represent, write a concrete `app_extension_requests` entry instead of pretending it is configured.

## Inputs

The chat task payload contains:

- `selected_campaign` - the current `outbound_campaigns` record including `payload.briefing`.
- `selected_template` - the selected template metadata, or `null` for custom text.
- `writeback_contract` - the allowed writeback command and allowed payload fields.
- `required_skills` - includes `business-os-outbound-campaign-setup`.

The campaign briefing is the primary source of truth. Existing campaign settings are context, not a reason to ignore the briefing.

## Workflow

1. Read the briefing and campaign record.
2. Produce a structured setup patch for existing app capabilities:
   - `research_settings` for research fields, prompts, ICP/product description, checklist, CTA and signature.
   - `active_outreach` for default channel, approval requirement, sender/mailbox requirements, meeting duration and communication identity expectations.
   - `communication_strategy` for target audience, offer, language, tone, channel plan and handoff to Conversations.
   - `sequence_strategy` for retry/follow-up timing and stopping rules.
   - `landing_page` only as desired configuration, not as an external deployment.
   - `campaign_setup` with status, summary, assumptions and open blockers.
3. For capabilities the app does not yet provide, add `app_extension_requests` items. Each item must include:
   - `id`
   - `title`
   - `reason`
   - `requested_behavior`
   - `priority`
   - `source_briefing_excerpt`
4. Write back exactly one `outbound.campaign.apply_setup` command.
5. If and only if an app implementation change is truly needed, create a separate normal Business OS app-modify task for the Outbound module after the setup writeback.

## Writeback

Dispatch one command through CTOX:

```bash
ctox business-os commands dispatch --json '{
  "id": "cmd_outbound_campaign_apply_setup_<unique>",
  "command_id": "cmd_outbound_campaign_apply_setup_<unique>",
  "module": "outbound",
  "command_type": "outbound.campaign.apply_setup",
  "record_id": "<campaign_id>",
  "status": "pending_sync",
  "payload": {
    "campaign_id": "<campaign_id>",
    "source_command_id": "<original_chat_command_id>",
    "skill": "business-os-outbound-campaign-setup",
    "campaign_payload_patch": {
      "campaign_setup": {
        "status": "configured",
        "summary": "...",
        "assumptions": [],
        "blockers": []
      },
      "research_settings": {},
      "active_outreach": {
        "approval_required": true
      },
      "communication_strategy": {},
      "sequence_strategy": {},
      "landing_page": {},
      "app_extension_requests": []
    }
  },
  "client_context": {
    "actor": { "id": "ctox-agent", "role": "agent", "display_name": "CTOX Agent" },
    "source_skill": "business-os-outbound-campaign-setup"
  }
}'
```

Use only fields that you can justify from the briefing. Leave unknown values unset and record assumptions or blockers.

## Completion Gate

Do not report completion until:

- The `outbound.campaign.apply_setup` command returned `ok: true`.
- The patch contains `campaign_setup.status`.
- Any unsupported requested behavior is represented as `app_extension_requests`.
- No outbound message was sent and no external service was called.
