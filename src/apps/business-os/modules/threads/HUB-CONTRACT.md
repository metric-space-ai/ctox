# Threads Hub Contract

Binding contract for the Business OS **Threads** hub. It is the single source
of truth shared by three consumers:

- **Native (Rust)** — `src/core/business_os/threads.rs` (event handlers,
  projections, attention state).
- **Browser module** — `src/apps/business-os/modules/threads/` (inbox, timeline,
  actions).
- **App-development skill** — references this file when teaching apps how to
  raise threads, link objects, and request human decisions.

Normative keywords **MUST** / **SHOULD** / **MUST NOT** are used in the RFC
sense. Where this contract and the code disagree today, the divergence is
recorded in **§9 Deviations** — the contract states the target, the deviation
records current drift.

---

## 1. Purpose

Threads is the **personal inbox for every moment work needs a human**: approve,
unblock, hand off, respond to a mention, or review an AI result.

A thread is the **conversation and audit trail _about_ an object** — a
candidate, an invoice, a task, a document. A thread is **never the object
itself**. The object lives in its own app and collection; the thread links to
it (`source_module` + `source_record_id`, `user_thread_links`) and MUST link
back so any decision surface can open the object it decides about.

Every thread carries `kind` (`note`, `mention`, `message`, `approval`,
`ctox_task`, `handoff`), a `status`, its `participant_ids`, and a source
reference. Consumers MUST treat the thread as derived context around a record,
not as primary storage for that record.

## 2. Personal-inbox semantics

The **inbox** (`filter === 'inbox'`) shows only what concretely needs **this**
user, now. It MUST include exactly:

- **My pending reviews** — an approval in `ctox_task_approval_requests` with
  `status === 'pending'` **and** `reviewer_user_id === me`. Someone else's
  pending approval is _their_ inbox item, not mine. Admins (`chef`/`admin`)
  MUST NOT receive every review as a firehose in the inbox; the full review
  queue lives under `approvals`/`team`.
- **My mentions** — a message with `me` in `target_user_ids`, or a notification
  of type `mention`.
- **My unread human threads** — unread `user_notifications` for `me`.
- **Work assigned to me** — `assigned_user_id === me` while `status` is one of
  `open` / `blocked` / `escalated`.

**Machine work** (`kind === 'ctox_task'`) MUST enter the inbox **only on
escalation**: `status === 'blocked'`, `status === 'escalated'`, or a linked
CTOX command/task in a failed state. A "work finished" notification is a
**result to review**, not a call to act, and MUST NOT surface in the inbox by
itself.

Personal relevance applies to **everyone, including admins** — an admin wants
their inbox, not a debug feed. The **wide views** that intentionally show
beyond personal relevance are exactly `all`, `team`, and `system`
(`system` == `kind === 'ctox_task'`).

The filtering predicate and the per-tab counts MUST be computed by the **same
rule** (`threadMatchesFilter`) — a tab's number MUST equal what the tab shows.

## 3. Attention model

Attention is **server-authoritative**, persisted per `(thread, user)` in
`user_thread_states`:

- `attention_score` (number)
- `attention_reasons` (string[], human UI vocabulary)
- `unread_count` (number)

The browser MUST read these stored fields and MUST NOT recompute attention over
full collections when the state exists. Native MUST refresh
`user_thread_states` for every affected user on every relevant event
(`message` / `approval` / `notification` / `status` / `handoff`), via
`refresh_thread_states`.

**Reason vocabulary** (German UI strings, canonical):

| Reason | Raised when |
| --- | --- |
| `Freigabe nötig` | a pending approval where `reviewer_user_id === me` |
| `Erwähnung` | I am in a message `target_user_ids` / an unread `mention` |
| `Übergabe an dich` | an open handoff whose target is me (`assigned_user_id`) |
| `Blockiert` | thread `status === 'blocked'` |
| `Fehlgeschlagen` | a linked CTOX command/task failed |
| `Frist heute` / `Frist überschritten` | `due_at_ms` is today / in the past |

**Score weights** (highest applicable reason wins — **max**, not sum):

| Reason | Weight |
| --- | --- |
| Freigabe nötig | 80 |
| Blockiert / Fehlgeschlagen | 70 |
| Übergabe an dich | 65 |
| Erwähnung | 60 |
| Ungelesen | 40 |

`needs_action` is **derived**, not stored independently: it is true iff
`attention_reasons` is non-empty. Consumers MUST NOT invent a separate
"needs action" signal that can disagree with the reasons list.

## 4. Timeline rules

The thread timeline merges `user_thread_messages` and
`ctox_task_approval_requests`, ordered by time.

- **System events are protocol, not conversation.** Any message whose `kind`
  (or `event_type`) is one of `ctox_status`, `approval_request`,
  `approval_approved`, `approval_rejected`, `handoff`, `status` — or any
  machine event with no `author_user_id` and `actor_type !== 'ai'` — MUST render
  as a **compact single protocol line** (human-readable head first), **never** a
  chat bubble.
- **Every reference is a link.** A timeline entry that points at an object,
  command, or task MUST render a real navigation control, never bare text:
  - object → `#<module>?record_id=<id>` (see §5),
  - command → `#ctox?command_id=<command_id>`,
  - task → `#ctox?task_id=<task_id>`.
- **Every failure line carries a follow-up action.** A protocol line whose body
  reads as failed/blocked MUST offer a one-click rework, dispatched as a real
  AI request `threads.ai.request` (`{ thread_id, goal, risk_class }`) in the
  same thread — not a dead status string.

Human messages (with `author_user_id`, or `actor_type === 'ai'` for CTOX) render
as conversation bubbles with sender and relative time.

## 5. Deep-link format

Object deep links are hash routes into the source app:

```
#<module>?record_id=<id>
```

- `record_id` is URL-encoded. Threads **emits** `record_id`.
- The shell record-focus contract (planned Phase 4) accepts the alias `record`
  in addition to `record_id`; consumers reading a deep link SHOULD accept both.
- An explicit `source_deep_link` on the entry, when present, MUST win over the
  derived form. A `module` of `threads` (or empty) yields no object link.
- Command/task links use the CTOX app: `#ctox?command_id=<id>` and
  `#ctox?task_id=<id>`.

**Record-approval banner contract.** A pending approval MUST be able to surface
**at the object** (the shell banner over the source record), not only inside
Threads. The decision dispatches the same commands Threads uses:

- approve → `threads.ctox_approval.approve`
- reject → `threads.ctox_approval.reject`

Both MUST carry `approval_request_id` and `expected_updated_at_ms` (optimistic
concurrency — the caller MUST refuse to decide without a known version).

## 6. Notifications

Notifications are created **natively** from `target_user_ids`, one
`user_notifications` record per targeted user (`upsert_notifications`). Native is
the only writer; the browser MUST NOT synthesize durable notifications.

Origin of `target_user_ids`: `message.create` / `note.create` carry them in the
payload (native `target_user_ids()` also folds in singular
`target_user_id` / `assignee_user_id`); a `mention` message sets
`notification_type = mention`.

**Notification types emitted today** (canonical, from `threads.rs`):

- Messages/notes: `note`, `mention`, `message`
- Approval lifecycle: `approval_request`, `approval_edited`,
  `approval_approved`, `approval_rejected`
- CTOX status (`upsert_status_notification`): `ctox_status`, `ctox_completed`,
  `ctox_failed`, `ctox_finished`

The browser attention/mention/waiting predicates MUST key off these strings
(`mention`/`mentioned`, `approval_request`/`approval_requested`, `ctox_failed`,
`escalated`, `deadline`) — see §9 for the drift between the plan's short type
list (`approval`/`mention`/`note`/`handoff`/`escalation`) and the concrete
strings above.

**Delivery obligations** (phased):

- Threads tab **badge** — count of `needs_action === true` states (cheap once
  §3 is server-authoritative). *(planned Phase 2)*
- Shell **toast** on a newly arriving approval/mention/handoff for me.
  *(planned Phase 2)*
- Desktop/OS push via `business-os-desktop`. *(planned, later)*

Until the shell path lands, the module MAY raise a browser `Notification` for
allowed unread types above the user's `priority_threshold`, respecting quiet
hours and per-type `notification_preferences` — advisory only, never a
substitute for the durable native record.

## 7. Directed communication

Humans are **picked, never typed**. Pickers (reviewer, handoff target, note
target, @mentions) MUST resolve real people from the `business_users` roster.
Composer `@mentions` are matched against roster ids and names
(`mentionTargetsIn`) and become `target_user_ids` on the dispatched
`threads.message.create`, with `kind: 'mention'`. A raw free-text user id is
not the intended input path.

## 8. Non-goals

Threads is **NOT**:

- **a chat app** — it is human-in-the-loop work coordination; system events are
  protocol lines, not messages, and the inbox is curated by relevance, not a
  feed.
- **a ticket system** — status/attention describe what a human must do now, not
  a workflow board; queue/task state lives in CTOX (`business_commands`,
  `ctox_queue_tasks`), linked, not owned.
- **the object store** — the object of record lives in its own app collection;
  Threads holds the conversation and audit trail about it and links to it.

Threads also MUST NOT bypass the WebRTC/RxDB data plane or become an HTTP data
bridge, and MUST NOT treat browser-asserted actor identity as authority for
approve/reject once native role enforcement (planned Phase 1/5) lands.

## 9. Deviations (contract vs. current code)

Recorded so the contract stays honest against `threads.rs` and the module as of
2026-07-20:

1. **Attention labels/weights.** The browser fallback in
   `attentionReasons`/`attentionScore` currently uses different strings
   (`Freigabe`, `Zugewiesen`, `Blockiert`, `Frist`, `Erwähnung`) and different
   weights **summed** (`Freigabe 100`, `Blockiert 90`, `Frist 80`,
   `Erwähnung 70`, `Zugewiesen 50`), while `whyMeLine` emits yet another set
   (`Freigabe nötig`, `Fehlgeschlagen`, `AI arbeitet`, `Unzugeteilt`,
   `Frist heute`/`Frist überschritten`). §3 is the **target**: server-written
   `attention_reasons`/`attention_score` with max-wins weighting. Phase 1
   reconciles the client fallback to this vocabulary. The browser already
   prefers the stored `user_thread_states` values when present, so writing them
   natively is what makes §3 authoritative.
2. **Notification type vocabulary.** The refactor plan names five short types
   (`approval`, `mention`, `note`, `handoff`, `escalation`). The code emits the
   concrete strings in §6 instead: approvals as `approval_request` /
   `approval_edited` / `approval_approved` / `approval_rejected`; handoffs flow
   through the `message` path and are stored as `notification_type = "message"`
   (there is **no** literal `handoff` or `escalation` notification type today);
   CTOX status uses `ctox_*`. Treat §6 as canonical; the short list is a
   category grouping, not the wire values.
3. **Deep-link alias `record`.** Threads emits only `record_id=`. The `record`
   alias is part of the planned shell record-focus contract (Phase 4) and is not
   yet emitted or parsed by this module.
4. **Native role enforcement for approve/reject** is not yet in force
   (Auth audit: capability token default-off, browser-asserted actor). §5/§8
   state the target; the module's decision buttons are display until Phase 1/5
   makes native the authority.
