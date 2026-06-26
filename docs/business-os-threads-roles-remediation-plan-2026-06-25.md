# Business OS Threads/Roles Remediation Plan

Status: local remediation implemented and verified; release follow-ups remain
Date: 2026-06-25
Last updated: 2026-06-26
Scope: Business OS Threads app, role/permission policy, approvals, RxDB/WebRTC access control, command dispatch

Related plans:

- `docs/business-os-threads-app-implementation-plan.md`
- `docs/business-os-roles-permissions-plan.md`

Current implementation status:

- Phase 1 is implemented locally: browser WebRTC now carries the Business OS
  capability token, native collection authorization is default-on, query-fetch
  and file-fetch deny when no auth callback is installed, and the same handler
  collection authorization is used for fetch paths in the Business OS native
  connection handler.
- Phases 2-5 are implemented locally: existing thread writes require
  membership/admin authority, reviewers are resolved to active Business OS
  users, requester self-review is blocked by default, approval decisions/edits
  carry expected update versions, approval targets are immutable, and approved
  commands re-enter the central Business OS command dispatcher.
- Phases 6-7 are implemented locally: active customer/outbound/ticket command
  families have native policy gates, mailserver config/domain/user commands
  require `secrets.manage`, and DKIM/private password material is redacted from
  browser-visible command outcomes and `business_commands`.
- Phase 8 is implemented for the current shell slice: global right-click
  already dispatches notes, mentions, and approval requests to Threads; action
  modes now expose impact levels and the user/reviewer field is backed by
  active `business_users` suggestions while native policy remains authoritative.
- Runtime build artifacts generated during verification were removed from
  `runtime/build` on 2026-06-26.

Remaining release follow-ups:

1. Decide whether the generic `RxWebRTCReplicationPool::new_multi` should stop
   installing demo-style allow-all query/file-fetch callbacks. The registries
   now deny when no callback is present, and the Business OS production path
   installs handler-level collection authorization, but generic pool callers
   should be audited before this becomes a platform-wide fail-closed claim.
2. Stabilize the unrelated `business_module_catalog` startup churn observed in
   browser smokes. The Threads right-click smoke excludes that collection from
   readiness for this mode so the collaboration path can be verified, but the
   catalog merge loop should be diagnosed separately before CI/nightly use.
3. Promote the browser right-click Threads smoke into the regular smoke matrix
   after the catalog churn is fixed or intentionally classified.
4. Finish release/operator documentation for the final role matrix, legacy
   browser peer behavior without capability tokens, and the distinction between
   browser UX hints and native-authoritative policy.
5. Decide whether mailserver DKIM/password storage should move from private
   runtime storage into the CTOX secret store. Browser-visible command payloads
   and outcomes are redacted locally, but the long-term storage boundary still
   needs a product/security decision.
6. Split the current coarse support ticket gate into narrower
   triage/reply/resolve/agent-request permissions if support workflows need
   different reviewer roles.

## Purpose

This plan captures the remaining work from the code review of how the new
Threads/user hub and right-click collaboration flows interact with Business OS
roles and permissions.

The target is not a chat-only system. CTOX already owns durable work, tickets,
context, queues, review, apps, inbound/outbound work, and non-linear lifecycle
continuity. The missing work is to make the new Threads experience safe and
streamlined as the user-facing coordination hub while preserving the existing
Business OS model:

- apps remain the real workrooms;
- chat/threads aggregate work, notes, mentions, approvals, and status;
- server-side policy remains authoritative;
- browser gates are UX mirrors only;
- RxDB/WebRTC remains the Business OS data path;
- HTTP must not become a browser data bridge.

## Review Findings To Close

The review found these role and permission gaps that must be closed before the
Threads/approval surface can be treated as role-safe.

1. Approved commands can bypass the central command policy dispatcher.
   `threads.rs` classifies approval targets from browser-provided target/mode
   metadata, then queues a final command through a trusted-local actor path.
   High-risk command families such as coding-agent, runtime, secrets,
   integrations, mailserver, app install/source/release, and data mutation must
   be classified from the actual command type and checked through the same
   server-side policy as direct commands.

2. Existing threads can be joined or written by a user who knows a `thread_id`.
   Thread visibility is participant-based, but note/message creation currently
   merges the actor into participants for existing threads. Existing thread
   writes must require current membership or admin authority.

3. Pending approvals are too mutable.
   Requester/reviewer/admin edit paths can change target command/module/record
   and payload after an approval was requested. Target-changing edits must
   either be forbidden or reset review state with a new immutable version/hash.

4. Several active app command families bypass module data-write policy.
   Command families such as `customers.*`, `outbound.*`, and `ctox.ticket.*`
   have dedicated handlers before the generic data-write chokepoint. They need
   explicit server-side policy gates.

5. Mailserver commands are not sufficiently gated and can leak secrets.
   Mailserver config/domain/user commands must require explicit policy, and
   command payloads/results/audit records must not replicate passwords, DKIM
   private keys, or other secrets into browser-visible collections.

6. Scoped WebRTC replication expects capability tokens, but the browser peer
   session does not currently carry the required token. This can make scoped
   thread/command/task replication fail or force unsafe fallbacks.

7. Query-fetch is fail-open without collection authorization.
   The query-fetch registry can stream collection data without the same auth
   callback/document filters as normal replication. This must be fail-closed,
   especially for collections that are sensitive at collection level.

8. Reviewer and capability rules are too weak.
   Reviewer identifiers are free strings, self-approval is not blocked by
   default, and tokenless replicated commands can fall back to weak synthetic
   identities. Approval identity and capability handling need stronger defaults.

## Non-Negotiables

- Do not proxy Business OS app/module data over HTTP.
- Do not add UI-only authorization for state-changing actions.
- Do not edit `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` directly.
- Keep policy decisions server-authoritative in `src/core/business_os/policy.rs`
  and native command handling.
- Keep browser permission helpers in sync with native policy, but treat them as
  UX hints only.
- Runtime behavior should move toward typed runtime config or the CTOX stores,
  not new process-environment toggles.

## Phase 0: Baseline And Regression Tests

Tasks:

- Add failing tests for every review finding before broad rewrites.
- Cover native thread membership, approval edit/versioning, approval execution,
  command policy classification, and replication/query-fetch authorization.
- Add browser tests for right-click note/mention/approval affordances where the
  browser can mirror policy state.
- Keep fixtures small and tied to real roles: low-privilege app user,
  experienced reviewer, module owner/founder, admin, and service actor.

Acceptance criteria:

- Each review finding has at least one regression test that fails on the
  current behavior or proves the existing gap explicitly.
- Test names describe the policy invariant, not the implementation detail.

## Phase 1: WebRTC Capability Token And Query-Fetch Auth

Tasks:

- Wire the Business OS capability token into the browser WebRTC `peerSession`
  using the existing control-plane capability issuance path.
- Keep the token short-lived and scoped; avoid long-lived local-storage tokens.
- Validate `peerSession.capabilityToken` natively before scoped records can
  replicate.
- Make query-fetch fail closed when no auth callback is installed.
- Register the same collection/document authorization logic for query-fetch and
  file-fetch that normal replication uses.
- Add explicit collection-level checks for sensitive collections such as users,
  credentials, commands, tasks, files, and thread records.
- Prefer typed runtime config for "require capability token" and "collection
  authz" behavior instead of adding more environment toggles.

Acceptance criteria:

- An unrelated user cannot replicate or query-fetch another user's private
  threads, commands, tasks, notes, or approvals.
- An admin can see administrative records through policy, not through bypass.
- Query-fetch cannot stream `business_users`, credentials, or other sensitive
  collection data without authorization.
- Missing auth callback is a hard denial, not an open path.

## Phase 2: Thread Membership And Mentions

Tasks:

- Split thread creation from existing-thread writes in native thread handlers.
- For existing threads, require the actor to be an existing participant or an
  admin before accepting messages, notes, or target-user additions.
- Allow new threads to be created with participants only when the actor has
  permission to reference the source context.
- Treat right-click "leave note for user" and "bring user into loop" as explicit
  mention/notification flows, not as arbitrary thread takeover.
- Store provenance for contextual notes: source app, module, record id,
  action surface, actor, target users, and created time.
- Add admin-only recovery/merge tools for exceptional cases where a thread must
  be joined after the fact.

Acceptance criteria:

- Knowing a `thread_id` is never enough to write into or join a private thread.
- A participant can invite or mention another user only according to policy.
- Thread participants, mentions, and notifications are auditable.

## Phase 3: Reviewer Identity And Approval Request Hardening

Tasks:

- Replace free-form reviewer strings with active Business OS user references
  wherever possible.
- Reject missing, inactive, unknown, or disabled reviewers.
- Block requester self-approval by default.
- Allow self-approval only through an explicit admin override or documented
  solo-mode policy, with an audit reason.
- Check that the reviewer has the relevant permission for the target command or
  is otherwise allowed to approve that class of action.
- Add a reassignment flow for invalid or unavailable reviewers.
- Preserve low-privilege user ability to ask CTOX questions and request work,
  while requiring experienced-user approval for agent commands they cannot run.

Acceptance criteria:

- A low-privilege user can create an approval request but cannot approve it.
- A requester cannot approve their own request without explicit elevated policy.
- A reviewer who lacks permission for the target action cannot approve it.
- Reassignment is explicit and audited.

## Phase 4: Approval Target Immutability And Versioning

Tasks:

- Make approval target command, module, record, and payload immutable after the
  request enters review.
- If target edits are necessary, create a new approval version and reset review
  state.
- Permit prompt-only or note-only edits by requester only when they cannot
  change the authorized action.
- Record before/after diffs for every approval edit event.
- Add an expected version/hash to approve/reject commands.
- Reject stale approvals when the reviewer acts on an outdated card.
- Show stale state clearly in the browser approval card.

Acceptance criteria:

- A requester cannot swap a harmless target into a privileged target after
  review starts.
- A reviewer cannot accidentally approve a changed payload from a stale UI.
- Approval history explains exactly which version was approved or rejected.

## Phase 5: Central Command Policy For Approved Work

Tasks:

- Build the final `BusinessCommand` from the approval target and run it through
  the same central policy classifier as direct commands.
- Derive required permission from the actual command type and payload, not from
  browser-provided target/mode metadata.
- Remove trusted-local shortcuts from externally approved command execution.
- Explicitly gate high-risk command families:
  - `ctox.coding_agent.*`
  - runtime and model-runtime control
  - secrets and credentials
  - integrations
  - mailserver configuration
  - app install/source/release/rollback
  - module data writes
  - file mutation and export
- Keep read-only ask/help flows separate from commands that can mutate data or
  delegate agent work.
- Audit both approval decision and final command authorization result.

Acceptance criteria:

- A command denied through the direct command path is also denied through the
  approval path unless the approval itself grants exactly the required policy.
- `ctox.coding_agent.*` cannot be launched by disguising it as a generic chat
  task.
- Approval execution records the actor, reviewer, permission decision, command
  id, and resulting task id.

## Phase 6: Policy Gates For Active App Commands

Tasks:

- Add explicit server-side permission checks to active app command families that
  currently run before the generic data-write gate.
- Gate CRM/customer commands through module ownership, founder assignment,
  explicit grants, or admin role.
- Gate outbound commands through the relevant outbound/module permissions.
- Gate ticket commands through support-specific permissions or the existing
  module data policy until a narrower ticket policy exists.
- Keep internal service actors explicit and narrow; do not let replicated
  browser commands fall into service paths.
- Add tests for denied low-user commands and allowed owner/admin commands.

Acceptance criteria:

- App buttons and right-click actions cannot mutate module data unless native
  policy allows the actor.
- Browser permission helpers and native policy produce consistent UX states.
- Every command family that mutates Business OS state has a visible native gate.

## Phase 7: Mailserver Secret Hygiene

Tasks:

- Add explicit permissions for mailserver domain/user/config commands.
- Store passwords, DKIM private keys, and comparable secrets in the CTOX secret
  store or private runtime storage, not in replicated command payload/result
  records.
- Redact secrets before writing `business_commands`, thread artifacts, audit
  events, projections, and browser-visible records.
- Return only public DNS material and secret references to the browser.
- Add regression tests that scan command payloads/results for leaked secret
  fields.

Acceptance criteria:

- A non-admin/non-integrations user cannot run mailserver mutation commands.
- No browser-replicated collection receives raw password or DKIM private key
  material.
- Audit logs preserve evidence without exposing secrets.

## Phase 8: Browser UX And Right-Click Collaboration

Tasks:

- Add a unified right-click action model for every app element:
  - ask CTOX a question;
  - ask CTOX to manipulate data or run an app action;
  - leave a note for another user;
  - mention another user in context;
  - draft a CTOX prompt that requires another user's approval.
- Label actions by impact level: read-only question, note/mention, approval
  required, data mutation, privileged agent work.
- Show only relevant actions for the current user, but keep disabled actions
  explainable.
- Use active `business_users` for reviewer/recipient pickers instead of raw
  strings where possible.
- Show approval cards with command type, target app/module/record, payload
  summary, requester, reviewer, version/hash, policy result, and stale warning.
- Route all generated approval requests, notes, and mentions into the Threads
  hub without making chat the owner of the work.
- Preserve app/workroom context links so users can jump back into the real
  work surface.

Acceptance criteria:

- A low-privilege user can involve a more experienced user without needing
  direct agent-command rights.
- A reviewer can understand exactly what CTOX will do before approving.
- Notes and mentions appear in the relevant user's hub and link back to the
  originating app element.

## Phase 9: Documentation And Rollout

Tasks:

- Cross-link this plan from `docs/business-os-threads-app-implementation-plan.md`
  and `docs/business-os-roles-permissions-plan.md`.
- Document the final role matrix for Threads, notes, mentions, approvals, app
  commands, and privileged agent delegation.
- Document capability-token behavior and default-on expectations.
- Update browser permission helper docs whenever native policy changes.
- Add rollout notes for existing local installations and legacy browser peers.

Acceptance criteria:

- Developers can see which checks are native-authoritative and which are
  browser UX mirrors.
- Operator docs explain what happens when a browser peer lacks a capability
  token.
- The Threads app documentation describes the hub as coordination/visibility,
  not as a replacement for app workrooms.

## Verification Matrix

Run the narrowest relevant checks while implementing each phase, then run the
combined matrix before declaring the remediation complete.

Latest local verification on 2026-06-26:

- PASS `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --target-dir runtime/build/threads-target business_os::threads::tests:: -- --nocapture`
- PASS `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --target-dir runtime/build/threads-target business_os::store::tests::active_app_command_families_require_native_policy_gates -- --nocapture`
- PASS `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --target-dir runtime/build/threads-target business_os::store::tests::mailserver_commands_require_secrets_manage_and_redact_payload_secrets -- --nocapture`
- PASS `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
- PASS `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- PASS `node src/apps/business-os/rxdb/tests/run-all.mjs`
- PASS `cargo build --manifest-path src/core/rxdb/Cargo.toml --release --example v15_wire_daemon --target-dir runtime/build/cargo-target`
- PASS `node src/apps/business-os/rxdb/tests/cross-process-wire-smoke.mjs`
- PASS `node src/apps/business-os/rxdb/tests/cross-process-file-fetch-smoke.mjs`
- PASS `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs`
- PASS `node src/apps/business-os/modules/threads/tests/threads.test.mjs`
- PASS `node --check src/apps/business-os/app.js`
- PASS `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- PASS `CTOX_VOXTRAL_BUILD_GGML=0 SMOKE_MODE=business-os-threads-rightclick-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js`
  - Verified right-click note, mention, and approval creation.
  - Verified native projections into `user_threads`, `user_thread_messages`,
    `user_notifications`, and `ctox_task_approval_requests`.
  - Verified reviewer hub rendering and approval action availability.
  - Browser warning/error/request/resource/cache counts were zero.
- PASS `git diff --check` for the files touched by this remediation.

```bash
CTOX_VOXTRAL_BUILD_GGML=0 cargo test --quiet --target-dir runtime/build/threads-target --bin ctox business_os::threads::tests::
cargo test --quiet --manifest-path src/core/rxdb/Cargo.toml
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/rxdb/tests/run-all.mjs
node src/apps/business-os/rxdb/tests/cross-process-wire-smoke.mjs
node src/apps/business-os/rxdb/tests/cross-process-file-fetch-smoke.mjs
node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs
node src/apps/business-os/modules/threads/tests/threads.test.mjs
node --check src/core/rxdb/tools/browser_rust_smoke.js
CTOX_VOXTRAL_BUILD_GGML=0 SMOKE_MODE=business-os-threads-rightclick-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
```

Add targeted tests as implementation lands:

- native approval policy bypass attempts;
- thread join/write by non-participant;
- approval target edit/version stale approval;
- query-fetch collection authorization;
- mailserver secret redaction;
- app command data-write gates;
- browser right-click approval and note flows.

## Recommended Implementation Order

1. Add failing tests for the review findings.
2. Fix capability-token wiring and query-fetch authorization.
3. Fix thread membership and mention semantics.
4. Harden reviewer identity and approval target versioning.
5. Route approved command execution through central command policy.
6. Gate active app command families and mailserver commands.
7. Add secret redaction for replicated command records and audit artifacts.
8. Polish browser right-click, reviewer, note, and approval UX.
9. Update docs and run the full verification matrix.

## Open Decisions

- Should requester self-approval ever be allowed outside explicit admin override
  or solo mode?
- What is the exact support-ticket permission model: one coarse ticket
  permission or separate triage/reply/resolve/agent-request/apply grants?
- Is `business_os.chat.task` ever read-only, or should every CTOX task request
  be treated as delegated work requiring data-write or approval policy?
- How should short-lived capability tokens rotate during long browser sessions?
- What compatibility behavior is acceptable for legacy browser peers without
  capability tokens?

## Done Criteria

This remediation is complete when:

- arbitrary thread join/write by known id is impossible;
- notes, mentions, approval requests, and app context links are auditable;
- approval target changes cannot bypass review;
- approved command execution uses the same central policy as direct commands;
- low-privilege users can request work but cannot run privileged agent actions;
- scoped replication and query-fetch enforce the same access model;
- browser-visible records do not contain raw secrets;
- app/workroom commands are server-gated, not only UI-gated;
- the Threads hub gives each user a focused personal work view without becoming
  the owner of durable work semantics.
