---
name: message-drafting
description: Draft personalized Business OS outbound outreach (initial message plus two follow-ups) for one pipeline contact and reply with the four message fields as one JSON object. Use when the outbound app queues an outreach-draft task (command outbound.pipeline.outreach_draft); the app writes the reply itself through outbound.pipeline.write_outreach_draft and never sends without approval.
---

# Business OS Outbound Message Drafting

Quelle: `src/skills/system/communication/business-os-outbound-message-drafting/SKILL.md`, auf den App-Ablauf
(Shell v4, §16.30) zugeschnitten: das Ergebnis ist deine Antwort selbst. Die Outbound-App nimmt sie über ihren
Rückruf `outbound.pipeline.outreach_draft.done` entgegen und schreibt sie per
`outbound.pipeline.write_outreach_draft` an den Pipeline-Kontakt.

The Business OS UI is only a structured task producer. It never calls an email gateway and never talks to an
external drafting service. Your job is to turn the drafting request into a personalized draft and return it.

## Non-negotiable rules

- Do not send any message. You only produce drafts. Sending stays behind the explicit Business OS approval gate.
- Do not call any external HTTP service, gateway, or email API. Work only with the data you were given.
- Do not write records yourself: no Business OS commands, no CTOX CLI writeback, no MCP writeback. Your reply is the
  only output; the app persists it.
- Do not invent facts. Use only the supplied ICP/product description, the company research summary, and the person
  data. If a detail is missing, write around it instead of fabricating it.
- Match the prospect's language and register. Default to German, formal "Sie".
- Respect suppression, bounce, and opt-out state. If the request marks the contact as suppressed or opted out, do
  not draft: reply with `{"skipped": "<reason>"}` and stop. The app records it as a failed draft.

## Inputs

The task instruction carries everything you need:

- `pipeline_id` and `contact_index` — which pipeline contact you are drafting for.
- `drafting_request` — the drafting context:
  - `Produkt_und_Dienstleistungsbeschreibung` — the ICP / product description.
  - `CTA` — the desired call to action.
  - `Signatur` — the signature to append.
  - `Checkliste_Landingpage` — landing-page checklist for tone/claims.
  - `message_prompt_template` / `extract_prompt_template` — optional operator prompt templates; honor them when
    present.
  - `company` — `{ name, website_url, homepage_summary }`.
  - `person` — the target contact's known fields.
- `skillbook` — the persisted drafting skillbook of the app (`outbound_skillbooks`,
  `business-os.outbound.message_drafting.v1`). Treat its `mission`, `non_negotiable_rules`, `workflow_backbone`, and
  `routing_taxonomy` as the operating contract. Its writeback step is done by the app from your reply.

## Workflow

1. Confirm `pipeline_id` and `contact_index` from the instruction.
2. Pick the single strongest, defensible anchor between the product/ICP and this company + person, using the
   homepage summary and person data.
3. Draft four fields:
   - `message_mail_subject` — short, specific, no clickbait.
   - `message_mail_body` — personalized opener, the anchor, one clear CTA, then the signature.
   - `message_followup_1` — short, polite nudge that references the first message.
   - `message_followup_2` — final, low-pressure follow-up with an easy opt-out.
4. Keep every field free of invented claims and external links you cannot justify.

## Output contract

Exactly one JSON object, no prose around it:

```json
{
  "messages": {
    "message_mail_subject": "...",
    "message_mail_body": "...",
    "message_followup_1": "...",
    "message_followup_2": "..."
  }
}
```

All four fields are mandatory, non-empty strings. A reply without them is recorded as a failed draft.

## Completion gate

Do not finish until:

- All four message fields are non-empty and consistent with each other.
- No message was sent and no external service was called.

## Guardrails

- One contact per drafting task. Do not draft for other contacts.
- Never escalate to sending; that is a separate, approval-gated command (`outbound.message.send_approved`).
