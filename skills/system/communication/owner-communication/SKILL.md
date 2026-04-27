---
name: owner-communication
description: Use when CTOX needs to communicate with the owner through TUI, email, or Jami, choose the correct communication path, continue an existing owner thread, or decide how proactive outbound owner contact should work.
metadata:
  short-description: Route owner communication across TUI, email, and Jami
cluster: communication
---

# Owner Communication

Use this skill whenever CTOX needs to interpret, continue, or initiate communication with the owner.

For CTOX mission work, only SQLite-backed runtime communication state counts as durable communication knowledge. Messages, sync runs, approvals, ticket state, continuity, and verification records count. Workspace notes or copied email snippets in files do not count as durable knowledge.

## Scope

- Channels are limited to `tui`, `email`, and `jami`.
- Treat `tui` as the local, direct CTOX session.
- Treat `email` as topic-threaded and archival.
- Treat `jami` as continuous conversation flow, similar to chat.

## Channel Selection

1. If the owner contacted CTOX on a specific channel, prefer replying on that same channel.
2. If CTOX initiates contact and `CTOX_OWNER_PREFERRED_CHANNEL` is set in TUI settings, prefer that channel.
3. If CTOX initiates contact and no preferred channel is set, choose the lowest-friction configured channel that fits the urgency and persistence needs.
4. Do not invent a channel that is not configured in the prompt context.

## Channel Semantics

### TUI

- Use for direct local interaction with the owner.
- If the owner enters email credentials or Jami account details in TUI settings, treat TUI as the setup surface, not the long-term remote reply target.
- TUI is continuous within the local session rather than topic-threaded.

### Email

- Email is topic-threaded.
- Accept mail from the configured allowed domain for support, account help, onboarding, troubleshooting, and other non-admin work.
- Treat mail from outside the configured allowed domain as unauthorized unless an explicit profile says otherwise.
- Only the owner and configured admin mail profiles may authorize admin work by email.
- Only senders with explicit sudo authority may authorize privileged local actions by email.
- Secrets, passwords, tokens, root material, and sudo credentials are never accepted by email; move those inputs to TUI.
- Before drafting any email reply, inspect both:
  - the most recent messages of the same thread
  - the most relevant owner communication across channels and recent operator turns
- Answer in the context of the whole recent communication state, not only the newest line or the current thread in isolation.
- Do not rely only on a wrapper that pasted older messages into the prompt. Use the communication tools actively:
  - `ctox channel context --thread-key <key> --query <text> --sender <addr> --limit <n>`
  - `ctox channel history --thread-key <key> --limit <n>`
  - `ctox channel search --query <text> --limit <n>`
  - `ctox lcm-grep <db> all messages smart <query> <limit>` when prior TUI or operator dialogue may matter
- Before sending a new outbound email, first look for an existing relevant owner thread in the communication store.
- Reuse the existing email thread when the topic matches.
- Start a new thread only when the subject materially changes.
- When replying to an inbound email, preserve the thread subject. Do not send `(no subject)` or invent a fresh subject for an existing owner thread.
- If the prior thread or recent owner communication already contains promises, partial work, blockers, approvals, handoffs, or open questions, explicitly account for them in the new reply instead of answering as if the topic started now.
- Use email for durable summaries, approvals, decisions, handoffs, and anything the owner may need to revisit later.
- If the incoming email concerns a critical, risky, or urgent operational topic, reply by email that the owner must continue in the local TUI before CTOX performs the action.
- Do not tell the owner you are "working on" a multi-step or high-impact task unless you also create the explicit durable next-work record for it in CTOX queue or plan state.
- If a promised next step fails or stalls, send a follow-up status instead of silently going quiet.
- If work is blocked on owner input, enumerate the exact missing values, credentials, approvals, or decisions. Do not say only that "something is missing".
- For a blocked owner-visible task, explicitly say how the owner can unblock it:
  - reply to the current email with the exact requested values when email is safe for that case
  - or switch to TUI when the topic is critical, risky, or secret-bearing
- If a task is blocked specifically because the sender lacks sudo authority or admin authority for the requested action, say that plainly.
- Do not imply that the owner should discover or complete hidden manual setup steps on their own. State exactly what CTOX still needs.
- If the blocker is an external approval URL, device-login confirmation, Vercel claim URL, or access-grant link, include the exact link in the owner message and ask explicitly for approval/confirmation. Do not paraphrase the link away.
- Do not send repeated owner emails that only restate the same blocker. If there is no new evidence, no state change, and no new owner question, keep the review internal in queue or schedule state instead of mailing the same status again.
- Never send an email without a real subject. Reuse the existing thread subject when continuing a thread; if no real subject is available yet, create one deliberately before sending.

### Jami

- Treat Jami as an ongoing conversation stream rather than a subject-threaded mailbox.
- Prefer continuing the existing conversation tied to the owner account and conversation id.
- Use Jami for short operational updates, lightweight follow-ups, and rapid clarification when TUI is unavailable.

## Operational Rules

- Keep replies short and stateful.
- Match the owner's current thread or conversation context before opening a new one.
- Read enough recent owner communication to understand the last known state before replying.
- Treat communication lookup as an explicit preparation step, not as a passive prompt garnish.
- Prefer reconstructing one explicit communication-state view first, then drill into raw thread or search hits as needed.
- Never answer as if only the latest inbound message exists when the surrounding communication already contains approvals, blockers, or unfinished work.
- When responding to inbound owner communication, continue the established path unless there is a clear reason to escalate to a more durable channel.
- When escalating from `jami` or `tui` to `email`, explicitly say that the detailed follow-up is moving to email.
- Distinguish clearly between:
  - research or preparation already done
  - work that is actually executing now
  - work that is only queued or planned
- If CTOX is reporting a successful self-improvement or skill refinement, do so only after a review step confirmed the result and documented the learning. The owner report must name the concrete change and the evidence, not just say that CTOX "optimized itself".
- If a prior communication already granted or denied approval, acknowledge that state instead of asking again unless the scope has materially changed.
- Verify the transport state after proactive outbound communication instead of assuming delivery.
- Treat email `accepted` as weaker than email `confirmed`.
- Treat Jami `queued` as not yet delivered.
- Do not leak secrets, passwords, root auth material, or BIOS-protected state into outbound channels unless the owner explicitly requests it and the channel choice is justified.
- If a blocker, approval, or commitment is only present in a workspace artifact or free-form note, treat the communication state as incomplete until it is visible in the SQLite-backed communication or ticket state.

## Communication Shapes

- `tui`: direct answer, immediate clarification, local setup guidance
- `jami`: concise update, quick question, acknowledgement, short coordination
- `email`: durable summary, structured proposal, longer decision memo, explicit approval request

## External Approval Links

When a third-party platform requires the owner to complete an approval step:

1. capture the exact approval URL or claim URL
2. explain in one sentence what the approval unlocks
3. state what CTOX will do immediately after approval
4. continue the same owner thread when possible
5. persist the blocker durably so the work does not vanish while waiting

Examples include:

- Vercel browser approval / device login confirmation
- claim URLs for unmanaged deployments
- access-grant links for project/team membership

## Setup And Health

- Before relying on a configured remote channel, prefer running `ctox channel test --channel email` or `ctox channel test --channel jami`.
- If the test fails, keep setup and troubleshooting in `tui` until the remote path is healthy.
- If the owner entered communication credentials in TUI settings, treat that as configuration input, not automatic proof that the transport works.
- Treat CTOX mail self-tests as technical channel-health artifacts, not as ordinary owner communication. They may be stored for verification, but they must not create normal owner-facing queue work unless a human explicitly asked for mail validation.

## Owner / Founder Outbound Email

Outbound email to owner, founder, or admin recipients (configured via `CTOX_FOUNDER_EMAIL_ADDRESSES` and `CTOX_OWNER_EMAIL_ADDRESS`) is **only** sent through the reviewed founder-outbound pipeline (`send_reviewed_founder_outbound`). The agent does **not** call `ctox channel send` for these recipients — that command is structurally blocked.

The send is triggered automatically after the agent's turn completes successfully **if** the job carries explicit outbound-email intent and at least one recipient is owner/founder/admin per the configured policy.

How to give a job the outbound-email intent (operator-side):

- `ctox chat --to d.lottes@remcapital.de --cc j.kienzler@remcapital.de --subject "INF Yoda Update" "<prompt body>"`
- Equivalent metadata when creating a queue task: populate `outbound_email` with `account_key`, `thread_key`, `subject`, `to`, `cc`.

Agent responsibilities for such a job:

- Produce the email body as the turn reply, in mandantengerechter Sprache.
- Do **not** include internal CTOX vocabulary (`queue`, `runtime/`, `sqlite`, `route_status`, `runtime_env_kv`, host paths, etc.) — these will be rejected by the body cleanliness check.
- Do **not** invoke `ctox channel send` yourself; the service routes the send.
- The reply you produce **is** the email body; the service uses recipients/subject from the job metadata.

## References

- For routing rules and examples, read `references/channel-routing.md`.
- For active communication search workflow, use `communication-context`.
