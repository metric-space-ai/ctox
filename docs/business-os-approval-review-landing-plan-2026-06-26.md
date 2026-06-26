# Business OS Approval/Review Workflow — Landing & Gap-Closure Plan

Date: 2026-06-26
Scope: right-click → "request a change I cannot/should not execute myself" →
send to another user for **review** → the change runs **only after approval**,
routed through the **Threads** app (per-user hub) and the central command
dispatcher.

Related (parallel-agent) plans this builds on:

- `docs/business-os-threads-app-implementation-plan.md` (data model, command
  types, right-click approval flow, permission model)
- `docs/business-os-roles-permissions-plan.md` (permission vocabulary, role
  matrix, phase tracker)
- `docs/business-os-threads-roles-remediation-plan-2026-06-25.md` (Phases 1–8
  "implemented locally and verified", 6 release follow-ups)

## TL;DR

The feature is **not missing — it is unlanded**. A cross-layer review (browser
request UI, Threads reviewer app, native Rust pipeline, RxDB schema, roles)
confirms it is **implemented and largely complete in the local working tree**,
but **none of it is on `origin/main`** because it sits inside a large
uncommitted in-flight stack. The real work is: **(A) land it on main**, **(B)
close the role-enforcement gap** (the part the user actually flagged), **(C)
tests + i18n**, **(D) verify the SQLite table is created**, **(E) deploy +
browser-test**, then **(F) the 6 release follow-ups**.

## 1. Existing building blocks (implemented, in-flight-local)

| Layer | Blocks (all in-flight-local, look complete) |
| --- | --- |
| Request UI (`app.js`, `shared/shell-permissions-ui.js`) | approval mode option + impact label; reviewer field backed by a `business_users` datalist; `threads.ctox_approval.request` payload (carries `target_command_type`/`target_payload` = the command that runs after approval) + dispatch; full record/source context; de/en labels; mode-sync show/hide reviewer field |
| Threads reviewer app (`modules/threads/*`) | approvals inbox filter; pending list scoped to `reviewer_user_id`/admin; approve/reject/edit actions + form; approval card in thread timeline; `ctox_task_approval_requests` collection + helpers; RxDB sync |
| Native pipeline (`core/business_os/threads.rs`, `store.rs`) | handlers for `request`/`edit`/`approve`/`reject`/`cancel`; reviewer/admin-only authorization + reviewer-has-target-permission check; **APPROVE → enqueues the real `business_commands` command (the "runs only after review" gate)**; reject/cancel discards it; target immutability; optimistic version locking; audit events; collection ACL |
| Schema / RxDB | `ctox_task_approval_requests` schema (browser + native), schema-hash sync, native-peer replication registration, dist bundle |

Net: the **happy path is built** — request → reviewer sees it in Threads →
approve → command enters the dispatcher and runs; reject → nothing runs.

## 2. Missing / incomplete building blocks

These are the genuine gaps the review surfaced (the role-enforcement ones match
the user's exact concern):

1. **MISSING — "request-only" permission.** There is no permission/role meaning
   *"may REQUEST approval but may NOT execute directly"*. `data.write` is binary.
   Need a permission (e.g. `ctox.task.request_approval` / a `request_only` flag)
   plus role-matrix defaults.
2. **PARTIAL — UI steering.** `buildGlobalCtoxContextModes` offers `approval` as a
   free 4th choice to everyone. For a restricted user it should **replace**
   `data`/`app` (hide self-execute, make "Freigabe anfragen" the path). Needs a
   `canSelfExecute` boolean passed from `app.js` into the mode builder.
3. **PARTIAL — native enforcement.** `request_approval` does not require that the
   requester actually LACKS the target permission, and the chokepoint denies
   direct writes but the UI doesn't pivot. Server must remain authoritative:
   restricted actor's direct mutation DENIED, approval-request ALLOWED.
4. **PARTIAL — integration tests.** Only a payload-signature smoke exists. Need
   request→approve→`business_commands` enqueued; reject→no command; edit→approve.
5. **PARTIAL — Threads i18n.** ~20 approval strings (form labels, buttons, filter)
   are hardcoded German; add `locales/en.json`+`de.json` keys.
6. **VERIFY — SQLite table.** Confirm `ctox_task_approval_requests` is actually
   created in `business-os.sqlite3` (dynamic table creation) — else writes fail.

## 3. Landing strategy (the real challenge)

**Entanglement:** `store.rs` differs from origin by ~5192 lines and `threads.rs`
is a new ~4772-line file, but these are the SUM of ~15 unpushed local commits
(ats, data-write chokepoint, capability tokens, OCR, …) **plus** uncommitted
threads/approval/roles work. The approval feature is **not cleanly separable**
from the parallel agent's broader in-flight stack.

Options:

- **A. Land the whole in-flight stack.** Commit the remaining uncommitted changes
  and push local `main` → `origin/main`. Gets the feature live, but pushes the
  parallel agent's entire unpushed body of work (knowledge, mailserver,
  doc-stack, …), some possibly mid-edit. Large, hard to reverse.
- **B. Coordinate.** The parallel agent commits/pushes its coherent stack (it is
  that agent's change set); this session then verifies + browser-tests on main.
  Safest; lowest collision risk.
- **C. Surgical extraction** of only the threads/approval files onto a worktree at
  origin/main. Likely **infeasible** because `store.rs`/policy changes the feature
  depends on are interleaved with unrelated local commits.

**Recommendation:** B or A by explicit decision — not a silent unilateral push of
~10k lines. RxDB discipline (AGENTS.md): if `collections.schema.json` or native
schema changed, fixtures + schema hashes must be regenerated on both sides and
the `dist` cache-busters bumped before merge.

## 4. Test plan

- **Native:** `cargo check`; targeted `cargo test` for threads/approval + policy
  (request/approve/reject/authorization, approve→enqueue, reject→no-enqueue,
  self-review-blocked); `node src/apps/business-os/rxdb/tests/run-all.mjs`.
- **Browser e2e** (extend the existing Playwright harness used for the right-click
  proof): right-click → "Freigabe anfragen" → pick reviewer → submit dispatches
  `threads.ctox_approval.request`; then as the reviewer in Threads → Approve →
  assert a `business_commands` document was created (and Reject → none).
- **Deploy + verify:** `ctox upgrade --dev` (or static-asset overlay for the
  browser layer) then run the e2e against the live instance.

## 5. Risks

- Pushing ~10k lines of another agent's in-flight, partly-uncommitted work to
  `main`; possible mid-edit/incomplete state.
- RxDB schema drift if hashes/fixtures aren't regenerated consistently.
- A full `ctox upgrade --dev` rebuild is ~26 min per deploy iteration.
- Parallel-agent collision on the same files (working tree may hard-reset).

## 6. Work-off order

1. **Landing decision** (A vs B) — gate; do not push ~10k lines without it.
2. If landing here: verify/regenerate RxDB fixtures + schema hashes; `cargo
   check`; `node run-all.mjs`; confirm `ctox_task_approval_requests` table creation.
3. Close the **role-enforcement** gap: add the request-only permission +
   role-matrix defaults (policy.rs / permissions.js); pass `canSelfExecute` into
   `buildGlobalCtoxContextModes` so restricted users get "Freigabe anfragen"
   instead of self-execute; enforce server-side.
4. Add the integration tests (§4) and the Threads i18n keys.
5. Land (commit + push per the chosen strategy).
6. Deploy + run the browser e2e; capture proof.
7. Close the 6 release follow-ups from the remediation plan.

## 7. Role-enforcement gap — implementation spec (this session's contribution)

Coordination decision (2026-06-26): the parallel agent lands its foundation
stack on `origin/main`; **this session then adds the role-enforcement layer on
top, plus tests/i18n, and runs the live browser e2e.** Steps below assume the
foundation is on main first (they reference `threads.rs`/`policy.rs`/
`shell-permissions-ui.js`).

Model (per `roles-permissions-plan.md`): "request-only" = an actor that LACKS
`data.write` / `ctox.task.create` for the target module. Such an actor must be
**unable to self-execute** and **steered to "Freigabe anfragen"**.

1. **Permission probe (browser).** In `app.js`, where the context menu builds
   `agentScope`/`canModify`, also compute
   `canSelfExecute = permissionsFacade.can('data.write', module) || can('ctox.task.create', module)`
   (extend `shared/permissions.js` with this check if absent — mirror the native
   `policy.rs` decision, do not invent a browser-only rule).
2. **UI steering (browser).** Pass `canSelfExecute` into
   `buildGlobalCtoxContextModes({ canModify, canSelfExecute, labels })`
   (`shared/shell-permissions-ui.js`). When `!canSelfExecute`: drop the `data`
   and `app` modes, keep `ask` (read-only) and make `approval` the default
   selected mode. Add a one-line hint ("Du kannst hier nur eine Freigabe
   anfragen"). Keep it a UI hint only — native stays authoritative.
3. **Native authority (Rust).** Confirm `policy.rs` already DENIES a restricted
   actor's direct `business_os.chat.task`/data-write (the DataWrite chokepoint)
   and ALLOWS `threads.ctox_approval.request`. If a restricted actor's direct
   execute is not denied server-side, add that gate — the UI steering must not be
   the only guard. (Allowing a privileged actor to *also* request review is fine
   and intentional; do not force privileged users into approval-only.)
4. **Reviewer authority (Rust).** Confirm `policy.rs`/`threads.rs` enforces that
   only the assigned `reviewer_user_id` (or an admin / `external.approve`-holder)
   can `approve`/`reject`/`edit`, re-checked at decision time.
5. **Tests.** Native: restricted actor → direct execute DENIED, approval-request
   ALLOWED; non-reviewer approve DENIED; approve → `business_commands` enqueued;
   reject → none. Browser: `shell-permissions-ui.test.mjs` cases for
   `canSelfExecute=false` hiding data/app.
6. **i18n.** Add the ~20 Threads approval strings + the new steering hint to
   `locales/{de,en}.json`.
7. **Live e2e.** Extend the Playwright harness (used for the right-click proof):
   right-click → "Freigabe anfragen" → pick reviewer → submit; then as reviewer
   in Threads → Approve → assert a `business_commands` doc was created; Reject →
   none. Run against `ctox upgrade --dev` deploy.
