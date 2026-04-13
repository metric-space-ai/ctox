---
name: communication-context
description: Use when CTOX must reconstruct the relevant communication state before replying on email, Jami, or TUI, especially when approvals, blockers, prior promises, or cross-channel context may change the answer.
metadata:
  short-description: Search and reconstruct communication state before replying
---

# Communication Context

Use this skill before replying when the latest inbound message may depend on earlier thread history or on communication that happened on another channel.

## Goal

Do not answer from the newest message alone. Reconstruct the relevant state first, then answer from that state.

## Tool Contract

Prefer active lookup over passive wrapper context.

1. Read the current thread:
   - `ctox channel history --thread-key <key> --limit 12`
1a. Prefer the structured reconstruction view first when available:
   - `ctox channel context --thread-key <key> --query <text> --sender <addr> --limit 12`
2. Search for related communication across channels:
   - `ctox channel search --query <text> --limit 12`
   - add `--channel <name>` or `--sender <addr>` when you need to narrow the search
3. If prior TUI/operator turns may matter, search the LCM:
   - `ctox lcm-grep <db-path> all messages smart <query> <limit>`

## Query Heuristics

- Start with the exact `thread_key` for direct history.
- When possible, build one structured communication-state view first with `ctox channel context` and then drill into the raw hits only if needed.
- Search by sender address plus the operational topic or service name.
- Search by concrete blocker or approval terms when the new message looks like a follow-up.
- If you already know a queue title, service name, host, or deployment target, include that in the search query.

## Decision Contract

Before replying, explicitly decide whether earlier communication changed any of these:

- current approval state
- current blocker state
- whether CTOX already promised follow-up work
- whether the owner already supplied missing values
- whether a prior answer is now stale or contradicted
- whether there are still unanswered owner questions in the active thread

If yes, answer from the updated state, not from the newest inbound line alone.

## Reply Contract

- Acknowledge relevant prior state when it materially affects the answer.
- Do not re-ask questions that the owner already answered.
- Do not restate an old blocker if later communication resolved it.
- If the communication state is still ambiguous after search, say exactly what remains unclear and what evidence you checked.
