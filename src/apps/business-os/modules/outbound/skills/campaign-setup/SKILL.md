---
name: campaign-setup
description: Configure a Business OS Outbound campaign from a natural-language campaign briefing and reply with the structured setup patch as one JSON object. Use when the outbound app queues a campaign-setup task (command outbound.campaign.setup.request); the app writes the reply itself through outbound.campaign.apply_setup.
---

# Business OS Outbound Campaign Setup

Quelle: `src/skills/system/communication/business-os-outbound-campaign-setup/SKILL.md`, auf den App-Ablauf
(Shell v4, §16.30) zugeschnitten: das Ergebnis ist deine Antwort selbst. Die Outbound-App nimmt sie über ihren
Rückruf `outbound.campaign.setup.done` entgegen und schreibt sie per `outbound.campaign.apply_setup` in die Campaign.

The Outbound app is only the structured task producer. It stores the natural-language campaign briefing and asks
CTOX to configure the campaign. Your job is to convert that briefing into structured Outbound campaign settings and
return them as your reply.

## Non-negotiable rules

- Do not send any outbound message.
- Do not call external HTTP services, mail gateways, landing-page services, or document APIs.
- Do not write records yourself: no Business OS commands, no CTOX CLI writeback, no MCP writeback. Your reply is the
  only output; the app persists it.
- Do not use hidden heuristics in the UI. The skill is the authority for interpreting the briefing.
- Keep all outbound communication approval-gated.
- If the briefing asks for something the current Outbound app cannot represent, write a concrete
  `app_extension_requests` entry instead of pretending it is configured.

## Inputs

The task instruction carries the campaign id and name, the selected template (`custom` for free text) and the
user briefing. The task payload carries:

- `campaign_id` - the `outbound_campaigns` record id.
- `source_command_id` - the request that started this setup.
- `selected_campaign` - the current `outbound_campaigns` record including `payload.briefing`.
- `selected_template` - the selected template metadata (`{id, title}`), or `null` for custom text.

The campaign briefing is the primary source of truth. Existing campaign settings are context, not a reason to ignore
the briefing.

## Workflow

1. Read the briefing and campaign record.
2. Produce a structured setup patch for existing app capabilities:
   - `research_settings` for research fields, prompts, ICP/product description, checklist, CTA and signature.
   - `active_outreach` for default channel, approval requirement, sender/mailbox requirements, meeting duration and
     communication identity expectations.
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
4. Reply with exactly one JSON object (below). An app implementation change is never started from here; the
   `app_extension_requests` entries are the hand-off to the owner.

## Output contract

Exactly one JSON object, no prose around it:

```json
{
  "status": "active",
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
}
```

- `status` is optional and only one of `setup_required`, `active`, `paused`, `closed`.
- `campaign_payload_patch` may only contain the keys shown above; the app drops every other key.
- `campaign_setup` with a string `status` is mandatory; without it the app records the setup as failed.

Use only fields that you can justify from the briefing. Leave unknown values unset and record assumptions or
blockers.

## Completion Gate

Do not finish until:

- Your reply is one JSON object with `campaign_payload_patch.campaign_setup.status`.
- Any unsupported requested behavior is represented as `app_extension_requests`.
- No outbound message was sent and no external service was called.
