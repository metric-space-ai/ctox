# RFC 0008: Business OS Active Outbound Approval Gate

**Status:** Accepted for implementation planning
**Date:** 2026-05-25
**Affects:** `src/apps/business-os/modules/outbound/`,
`src/apps/business-os/shared/command-bus.js`,
`src/apps/business-os/shared/sync.js`,
`src/core/business_os/business_os_schema_contract.json`,
`src/core/business_os/business_os_schema_hashes.json`,
`src/core/business_os/rxdb_peer.rs`,
`src/core/business_os/store.rs`,
`src/core/business_os/importer.rs`,
`src/core/communication/email_native.rs`,
`src/core/service/service.rs`

## 1. Decision

Active outbound communication in Business OS is approval-gated by construction.
CTOX may prepare drafts, follow-ups, reply responses, and scheduling messages
automatically, but every outbound communication message must be explicitly
approved by a user before any send command can use it.

The hard safety invariant is:

```text
No provider send call may execute unless it is bound to an outbound_messages row
whose approval_status is approved and whose audit trail contains an approval
decision for the same message revision.
```

The browser UI is not trusted as the enforcement boundary. The backend command
handler must reject sends that do not satisfy the invariant, even if the browser
or a replicated command document is malformed.

## 2. Baseline

The existing Outbound module is a full-workspace Business OS module. Its current
scope is campaign sourcing, company qualification, pipeline handoff, contact
research, lead qualification, outreach draft generation, and Mailserver domain
or user administration.

Current Outbound module files:

- `src/apps/business-os/modules/outbound/module.json`
- `src/apps/business-os/modules/outbound/schema.js`
- `src/apps/business-os/modules/outbound/index.html`
- `src/apps/business-os/modules/outbound/index.css`
- `src/apps/business-os/modules/outbound/index.js`

Current replicated Outbound collections:

- `business_commands`
- `outbound_campaigns`
- `outbound_sources`
- `outbound_companies`
- `outbound_pipeline_items`
- `outbound_research_runs`

Current Outbound commands handled by backend importer paths:

- `outbound.source.import`
- `outbound.company.research`
- `outbound.pipeline.contact_research`
- `outbound.pipeline.lead_qualification`

Current Mailserver commands handled through `business_commands`:

- `ctox.mailserver.get_config`
- `ctox.mailserver.save_domain`
- `ctox.mailserver.delete_domain`
- `ctox.mailserver.save_user`
- `ctox.mailserver.delete_user`

Current CLI Mailserver commands:

- `ctox mailserver add-domain`
- `ctox mailserver list-domains`
- `ctox mailserver add-user`
- `ctox mailserver list-users`
- `ctox mailserver send-email --from --to --subject --body`

Current channel send commands:

- `ctox channel send ... --reviewed-founder-send`
- `ctox channel send ... --reviewed-communication-send`
- `ctox channel founder-reply --message-key ... --body ...`

Current browser command path:

1. Module code dispatches through `createCommandBus`.
2. `shared/command-bus.js` writes a `business_commands` document with
   `status = pending_sync`.
3. `shared/sync.js` replicates the collection over WebRTC unless it is a
   read-only projection collection.
4. `rxdb_peer.rs` consumes pending commands from native RxDB SQLite.
5. `store.rs::accept_rxdb_business_command` validates and executes recognized
   command types.
6. Results are written back into `business_commands` and replicated to browser.

Current communication baseline:

- `communication_messages` already exists in the Business OS schema contract
  and is used by communication adapters as the durable message timeline.
- `email_native.rs` contains SMTP send and sync logic.
- RFC 0001 defines a reviewed outbound/founder communication target model, but
  the Business OS Outbound module does not yet own an active engagement model
  or per-message approval gate.
- Business OS pulls `communication_messages` through the channel store path,
  not as generic `business_records` rows.
- The channel send path already persists an outbound `communication_messages`
  row before provider send and marks provider failure as `send_failed`.
- The native email adapter does not enforce approvals itself; approval safety
  is currently enforced by the channel wrapper path.

Current safety gates:

- External outbound through `ctox channel send` is review-gated for email,
  Teams, Jami, WhatsApp, and meeting channels.
- Founder/owner/admin email through the generic send path is blocked and must
  use a reviewed path.
- Reviewed sends are matched by digest over exact body, recipients, CC, subject,
  and attachments.
- Reviewed founder send records a core transition from `Approved` to `Sending`
  with body and recipient hashes.
- Terminal no-send review verdicts are persistent and must not be converted into
  provider sends.

Known baseline gaps:

- The Outbound module has no productive `outbound_engagements`,
  `outbound_messages`, `outbound_approvals`, `outbound_sequences`,
  `outbound_sender_assignments`, or `outbound_meeting_requests` collections.
- The Outbound module has no active communication commands such as
  `outbound.message.prepare`, `outbound.message.approve`, or
  `outbound.message.send_approved`.
- The reviewed send gate is tied to communication review rows, not to an
  Outbound-domain approval model.
- `ctox mailserver send-email` can enqueue directly to Mailserver storage and
  does not use the Business OS approval model.
- Mailserver domain/user management is not yet connected to sender health,
  campaign assignment, daily limits, suppression, bounces, or send windows.
- Business OS Mailserver configuration commands currently expose DKIM private
  key material in command outcomes; active outbound must not replicate secrets
  into browser state.

## 3. Module Pattern Baseline

The following Business OS modules were inspected before implementation work, as
required by the CTOX app-development contract:

- `notes`: reusable patterns include module-level state, `loadModuleMessages`,
  `CtoxResizer`, direct DOM rendering, and cleanup during unmount.
- `spreadsheets`: reusable patterns include local `state`, `ensureStyles`,
  `fetch(new URL('./index.html', import.meta.url))`, RxDB realtime
  subscriptions, command/runbook seeding, debounced rendering, and final draft
  flush on unmount.
- `documents`: reusable patterns include systematic runbooks, dynamic vendor
  module loading, localized static labels, realtime subscriptions, context menu
  cleanup, and final save cleanup.

The Active Outbound implementation should reuse these patterns instead of
introducing a build step, iframe boundary, or alternate state-management stack.

## 4. Target Collections

The active communication layer introduces these collections:

- `outbound_engagements`
- `outbound_messages`
- `outbound_approvals`
- `outbound_sequences`
- `outbound_sender_assignments`
- `outbound_meeting_requests`
- `outbound_suppression_entries`
- `outbound_account_limits`

`communication_messages` remains the cross-channel communication timeline and
provider-observed message store. `outbound_messages` is the Outbound-owned
workflow object for draft, approval, planned send, and strategy state. Sent
outbound messages must link to `communication_messages.message_key` once the
provider accepts or syncs the message.

## 5. Command Contract

The initial command surface for active outbound is:

- `outbound.engagement.create`
- `outbound.engagement.assign_sender`
- `outbound.sequence.save`
- `outbound.message.prepare`
- `outbound.message.update_draft`
- `outbound.message.request_approval`
- `outbound.message.approve`
- `outbound.message.reject`
- `outbound.message.send_approved`
- `outbound.message.pause`
- `outbound.message.cancel`
- `outbound.reply.classify`
- `outbound.scheduling.prepare`
- `outbound.scheduling.mark_booked`

Commands that prepare drafts may be triggered automatically by CTOX. Commands
that send messages must be explicit, idempotent, and backend-gated.

## 6. State Contract

Engagement statuses:

- `ready_for_assignment`
- `assigned`
- `sequence_active`
- `draft_prepared`
- `awaiting_approval`
- `approved_for_send`
- `scheduled_to_send`
- `sent`
- `waiting_for_reply`
- `reply_received`
- `reply_draft_prepared`
- `scheduling`
- `meeting_booked`
- `nurture`
- `closed_not_fit`
- `paused`
- `failed`

Message statuses:

- `drafting`
- `draft_prepared`
- `awaiting_approval`
- `rejected`
- `approved`
- `scheduled`
- `sending`
- `sent`
- `send_failed`
- `cancelled`
- `superseded`

Approval decisions:

- `requested`
- `approved`
- `rejected`
- `commented`
- `superseded`

Stop reasons:

- `reply_received`
- `meeting_booked`
- `manual_pause`
- `manual_close`
- `bounce`
- `unsubscribe`
- `do_not_contact`
- `account_limit`
- `campaign_paused`
- `sender_unavailable`
- `strategy_superseded`

## 7. Enforcement Rules

Backend command handling must enforce:

1. `outbound.message.send_approved` requires an existing `outbound_messages`
   row.
2. The row must have `approval_status = approved`.
3. The row revision must have a matching `outbound_approvals` row with
   `decision = approved`.
4. Recipient, sender account, campaign, and engagement must match the approved
   message.
5. Suppression, unsubscribe, bounce, manual pause, and account limit checks run
   immediately before provider send.
6. Failed provider sends preserve the approved body and remain retryable by
   message id, not by free-form recomposition.
7. A new draft revision supersedes prior unapproved approval requests.

## 8. UI Contract

The Outbound UI must expose:

- Lead Queue for qualified leads without active engagement.
- Sender Assignment dialog with account health warnings.
- Engagement Cockpit with lead context, communication timeline, and next action.
- Approval Inbox containing all pending outbound messages.
- Message Review Editor for subject/body editing, approve, reject, comment.
- Campaign Settings for strategy, sequence policy, approval policy, senders,
  scheduling, CTOX Playbook, and compliance.

The UI may optimistically display prepared drafts, queued actions, and scheduler
decisions, but must present provider send as pending until backend result is
replicated.

## 9. Open Implementation Notes

- Sensitive mail credentials must not be replicated to browser RxDB. Browser
  sender account rows may contain only non-secret metadata and health status.
- Existing Mailserver commands currently expose DKIM material in command
  outcomes; Welle 6 must decide whether active outbound needs a safer sender
  account projection before exposing accounts in the new UI.
- The existing `communication_messages` projection should be reused for reply
  timeline display, but reply matching needs an Outbound-specific engagement
  link table or payload metadata.
- Schema additions must update both browser module schema and native schema
  contract/hash fixtures.
- New active Outbound collections should use `primaryKey: "id"` because generic
  Business OS push/pull handling recognizes `id` or `command_id`; `message_key`
  is reserved for communication-specific paths.
- Existing Outbound collection primary keys and required fields should not be
  changed without versioned browser migration strategies.
- High-volume collections should keep required fields small and place flexible
  data in `payload`, mirroring the existing Outbound schema style.
- The local `.git` metadata in this checkout is currently not readable through
  `git status`; implementation should continue with direct file inspection until
  repo metadata access is repaired.
