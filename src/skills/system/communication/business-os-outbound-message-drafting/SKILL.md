---
name: business-os-outbound-message-drafting
description: Draft personalized Business OS outbound outreach (initial message plus two follow-ups) for a pipeline contact and write the result back onto the command bus. Use when a CTOX queue task carries suggested_skill business-os-outbound-message-drafting or an outbound.pipeline.outreach_draft command, so the agent produces approval-gated drafts without ever sending a message or calling an external service.
cluster: communication
---

# Business OS Outbound Message Drafting

Use this skill when a CTOX queue task has `suggested_skill: business-os-outbound-message-drafting` or the prompt contains an `outbound.pipeline.outreach_draft` command.

The Business OS UI is only a structured task producer. It never calls an email gateway and never talks to an external drafting service. Everything stays on the CTOX RxDB command bus. Your job is to turn the queued drafting request into a personalized draft and persist it back through one command.

## Non-negotiable rules

- Do not send any message. You only produce drafts. Sending stays behind the explicit Business OS approval gate.
- Do not call any external HTTP service, gateway, or email API. Work only with the payload you were given and CTOX-native tooling.
- Do not invent facts. Use only the supplied ICP/product description, the company research summary, and the person data. If a detail is missing, write around it instead of fabricating it.
- Match the prospect's language and register. Default to German, formal "Sie".
- Respect suppression, bounce, and opt-out state. If the payload marks the contact as suppressed or opted out, do not draft — report and stop.

## Inputs

The command payload carries everything you need:

- `pipeline_id` — the outbound pipeline item id.
- `contact_index` — which contact in that item you are drafting for.
- `drafting_request` — the drafting context:
  - `Produkt_und_Dienstleistungsbeschreibung` — the ICP / product description.
  - `CTA` — the desired call to action.
  - `Signatur` — the signature to append.
  - `Checkliste_Landingpage` — landing-page checklist for tone/claims.
  - `message_prompt_template` / `extract_prompt_template` — optional operator prompt templates; honor them when present.
  - `company` — `{ name, website_url, homepage_summary }`.
  - `person` — the target contact's known fields.
- `writeback` — the exact writeback contract: `command_type`, `pipeline_id`, `contact_index`, and `message_fields`.

If you want the persisted skillbook guidance, read it with the MCP tool `business_os.get_record` for collection `outbound_skillbooks`, id `business-os.outbound.message_drafting.v1`. Treat its `mission`, `non_negotiable_rules`, `workflow_backbone`, and `routing_taxonomy` as the operating contract.

## Workflow

1. Read the command payload and confirm `pipeline_id` and `contact_index`.
2. Pick the single strongest, defensible anchor between the product/ICP and this company + person, using the homepage summary and person data.
3. Draft four fields:
   - `message_mail_subject` — short, specific, no clickbait.
   - `message_mail_body` — personalized opener, the anchor, one clear CTA, then the signature.
   - `message_followup_1` — short, polite nudge that references the first message.
   - `message_followup_2` — final, low-pressure follow-up with an easy opt-out.
4. Keep every field free of invented claims and external links you cannot justify.
5. Persist the draft with exactly one command (see below). Do not write partial fields across multiple commands.

## Writeback (the only persistence step)

Dispatch the writeback command through the CTOX command bus. It is processed synchronously and writes the four fields onto the pipeline contact's `messages` object, clears `outreach_generating`, and sets `outreach_status = "drafted"` so the UI spinner resolves over sync.

```bash
ctox business-os commands dispatch --json '{
  "id": "cmd_outreach_writeback_<unique>",
  "command_id": "cmd_outreach_writeback_<unique>",
  "module": "outbound",
  "command_type": "outbound.pipeline.write_outreach_draft",
  "record_id": "<pipeline_id>",
  "status": "pending_sync",
  "payload": {
    "pipeline_id": "<pipeline_id>",
    "contact_index": <contact_index>,
    "messages": {
      "message_mail_subject": "...",
      "message_mail_body": "...",
      "message_followup_1": "...",
      "message_followup_2": "..."
    }
  },
  "client_context": { "actor": { "id": "ctox-agent", "role": "agent", "display_name": "CTOX Agent" } }
}'
```

You can also pass the document with `--input <path>` to a JSON file. Use `command_type` and field names exactly as the `writeback` contract specifies.

## Completion gate

Do not report the drafting task as done until:

- All four message fields are non-empty and consistent with each other.
- The single `outbound.pipeline.write_outreach_draft` command returned `ok: true` and `status: "completed"`.
- No message was sent and no external service was called.

If the writeback fails, report the `pipeline_id`, `contact_index`, and the error. Do not retry by inventing new content; re-dispatch the same corrected document.

## Guardrails

- One contact per drafting task. Do not batch unrelated contacts into one writeback.
- Never bypass the command bus by editing RxDB collections directly.
- Never escalate to sending; that is a separate, approval-gated command (`outbound.message.send_approved`).
