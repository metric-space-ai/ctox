# Owner Channel Routing

## Inputs To Inspect

- Prompt context channel list
- `CTOX_OWNER_PREFERRED_CHANNEL` from runtime settings
- Existing communication thread or conversation identifiers
- Urgency, expected reply speed, and need for durable history

## Selection Rules

1. If there is inbound owner communication, reply on the same channel first.
2. If CTOX is initiating contact, check `CTOX_OWNER_PREFERRED_CHANNEL`.
3. If the preferred channel is unavailable or not configured, fall back to another configured channel.
4. Prefer `email` for durable, topic-based communication.
5. Prefer `jami` for quick back-and-forth.
6. Prefer `tui` only when the owner is actively local to CTOX.

## Delivery Semantics

- `tui` local writes can be treated as immediately available in the local session.
- `email` should be treated as `accepted` until the sent-copy verification succeeds and upgrades it to `confirmed`.
- `jami` outbound writes should be treated as `queued` until a stronger bridge-level confirmation exists.
- Do not tell the owner that a remote message definitely went out unless the tool result says so.

## Setup Verification

- Use `ctox channel test --channel email` to verify mail credentials and mailbox reachability.
- Use `ctox channel test --channel jami` to verify directory setup and DBus conversation reachability.
- If a remote channel test fails, keep remediation in `tui`.

## Threading Rules

### Email

- Search the communication store for a relevant owner email thread before composing a new message.
- Reuse the thread when the same topic is ongoing.
- Create a new thread only for a genuinely new topic.
- Use subject lines as topic boundaries.

### Jami

- Treat the owner relationship as continuous conversation state.
- Continue the current conversation id or thread key if one already exists.
- Do not manufacture email-style subject churn.

### TUI

- Treat the local chat as continuous session state.
- If the owner uses TUI to configure email or Jami credentials, keep the setup interaction in TUI.
- Once the owner explicitly prefers another outbound path, CTOX may initiate there later.

## Examples

### Owner sends a TUI message with email credentials

- Keep the credential handling and setup confirmation in TUI.
- If `CTOX_OWNER_PREFERRED_CHANNEL=email` is set afterward, future proactive outreach may begin by email.

### Owner sends an email about deployment status

- Look up the matching email thread first.
- Reply inside that thread unless the topic changed.

### Owner sends a short Jami ping asking for status

- Reply on Jami with a compact status update.
- Move to email only if the response becomes long-lived or decision-heavy.
