---
name: business-os-support-workflow
description: Use when CTOX handles a Business OS Support conversation and must summarize, classify, draft, or propose the next support action through typed Business OS writeback.
---

# Business OS Support Workflow

Use this skill for CTOX work launched from the Business OS Support app.

## Core Rule

Support work is durable Business OS work. Do not treat the prompt as a private
chatbot request and do not send external replies directly.

Read the supplied `record_snapshot`, then produce any structured Support result
through the advertised `writeback_contract`:

```text
support.agent.writeback
```

If a Business OS MCP Channel is available, use `business_os.list_module_actions`
for module `support`, then propose or execute `support.agent.writeback` only
within the server's policy. If MCP is not available, write the structured
result in the final response so CTOX can capture it through the normal Harness
review path.

## Allowed Suggestions

Allowed `suggestion_kind` values:

- `summary`
- `draft_reply`
- `classification`
- `next_action`
- `customer_update`
- `ticket_action`

Every writeback must include:

- `conversation_id`
- `source_command_id`
- `task_id` when known
- `suggestion_kind`
- `summary`
- `payload`
- `confidence`
- `required_human_action`

Use `required_human_action: "human_send_required"` for reply drafts. Reply
drafts are never external sends.

## Workflow

1. Identify the customer issue, channel, status, priority, and latest inbound
   request from the snapshot.
2. Check Support notes, conversation events, related ticket, and recent
   communication messages before proposing action.
3. Produce a concise summary and risk assessment.
4. If drafting a reply, keep it factual, short, and ready for human review.
5. If suggesting state changes, label them as suggestions only.
6. Never claim that the conversation is resolved, sent, assigned, or updated
   unless the Business OS command result confirms it.

## Boundaries

Do not:

- write raw RxDB records
- call HTTP endpoints as a data bridge
- send email, WhatsApp, Teams, or Jami messages
- invent customer or ticket records
- bypass approval or Support permissions

Prefer a small suggestion with clear evidence over a broad unsupported action.
