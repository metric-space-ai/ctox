---
name: external-review
description: Run an external, read-only verification pass for a CTOX slice by gathering mission, ticket, communication, runtime, and live-surface evidence directly from tools instead of relying on executor-provided context.
cluster: review
---

# External Review

Use this skill for a standalone review run.

Treat the review assignment as target metadata only.
Gather everything else yourself.

## Core Contract

The review run:

- uses read-only inspection only
- rebuilds its own understanding from the runtime store and live surfaces
- evaluates the reviewed slice against mission state, done gates, claims, and public-surface quality
- returns a verdict, failed gates, open items, evidence, and when needed a handoff for another review run

## Primary Sources

1. Runtime store: `runtime/ctox.sqlite3`
2. Workspace under review
3. Live public/runtime URLs
4. Ticket/self-work state
5. Relevant communication facts
6. Service/runtime/log state

## Suggested Workflow

1. Read the review assignment carefully.
2. Resolve the target conversation/thread in the runtime store.
3. Discover:
   - mission line
   - done gate
   - latest claimed slice/result
   - current blockers
   - active/open related work
4. Inspect related ticket/self-work state.
5. Inspect relevant communication facts if the work is owner-visible.
6. Inspect the live surface and critical routes.
7. Inspect the relevant files/runtime/logs needed to settle the claims.
8. Decide PASS / FAIL / PARTIAL from evidence.

## Canonical Read-Only Commands

Use the local CTOX CLI first where it gives a structured answer.

### Continuity and mission state

```bash
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> focus
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> anchors
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> narrative
```

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT conversation_id, mission, mission_status, blocker, done_gate, next_slice, is_open
FROM mission_states
WHERE conversation_id = <conversation-id>
ORDER BY updated_at DESC;
"
```

### Latest conversation activity

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT message_id, role, created_at, substr(body,1,400)
FROM messages
WHERE conversation_id = <conversation-id>
ORDER BY message_id DESC
LIMIT 12;
"
```

### Queue and self-work

```bash
ctox queue list
ctox ticket self-work-list --limit 20
ctox ticket cases --limit 20
```

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT message_key, source_label, status, thread_key, priority, preview
FROM queue_messages
ORDER BY created_at DESC
LIMIT 20;
"
```

### Communication facts

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT channel, direction, sender_address, subject, substr(preview,1,220), created_at
FROM communication_messages
WHERE thread_key = '<thread-key>'
ORDER BY created_at DESC
LIMIT 12;
"
```

### Live/public verification

```bash
curl -I <public-url>
curl -sS <public-url>
curl -i <critical-route>
```

Use a browser for owner-visible or public surfaces whenever possible.

## Public Launch Failure Conditions

Return FAIL when any of these are true:

- internal instruction text is visible
- planning or operator text is visible
- admin or backoffice surfaces leak into the buyer flow
- critical route or dependent API is broken
- the page is technically up but commercially not credible
- the layout, hierarchy, or copy is visibly not launch-worthy

## Review Handoff Rule

Normal review compaction is disabled.

If the review grows large enough that another review run should continue, stop and return:

- `VERDICT: PARTIAL`
- decisive facts gathered so far
- remaining checks
- best next verification targets

The handoff must be sufficient for another reviewer to continue without the original run.

## Output Contract

Return exactly:

- `VERDICT: PASS|FAIL|PARTIAL`
- `MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR`
- `SUMMARY: ...`
- `FAILED_GATES:`
- `OPEN_ITEMS:`
- `EVIDENCE:`
- `HANDOFF:`
