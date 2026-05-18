# Audit checklist — what unit tests structurally cannot catch

After functional completion (all specs landed, tests green, build green), run a 5-audit pass with parallel subagents. **Each audit produces a findings report; orchestrator triages; fix subagents land in parallel.**

This is the wave that catches the defect class unit tests miss: composition-layer wiring bugs, in-memory ports that were never replaced with Drizzle, server/client boundary violations, raw ID leaks. None of these surface in unit tests because the tests intentionally use the same memory adapters that production was incorrectly using too.

## Audit-A — Form-wiring gaps (BUG-1 family)

**Question.** Are all `<form>` elements wired to a Server Action OR an `onSubmit` handler? Or do some default to GET on the current URL, leaking input as query params?

**Method.**
1. List every `actions.ts` file under `src/app/(authenticated)/` and `src/app/(public)/`.
2. For each exported `*Action` symbol, search for `<form action={fooAction}>` usage. If absent, the action is dead code OR the form is broken.
3. List every `<form` JSX in `src/app/` and `src/components/`. For each: is there an `action={...}` prop OR an `onSubmit` that calls a server action? Plain `<form>` with neither = bug.

**Empirical example (BUG-1).** OpenProject port login form had `<form>` with no action — submitted as GET to `/login?login=admin&password=...`, leaking credentials. Fixed by wiring a `loginAction` Server Action.

**Severity.** Blocker (silent fail OR security leak).

## Audit-B — In-memory ports in production (BUG-11 family)

**Question.** Does `lib/composition/build-container.ts` wire in-memory adapters into `Repositories` slots that should be Drizzle-backed?

**Method.**
1. `grep -n "createInMemory\|createMemory" src/lib/composition/build-container.ts src/lib/composition/build-services.ts`.
2. For each match, classify:
   - **Stale import (LOW)** — symbol imported but actual wiring uses a Drizzle factory. Delete the import.
   - **Active in-memory (BLOCKER)** — service writes go to a `Map<>` per lambda. On Vercel, every write returns ok and disappears next request.
3. For each blocker, check: does `modules/<name>/drizzle/repositories.ts` exist? If yes, wire it (easy fix). If no, write the Drizzle adapter (medium fix).

**Empirical example (BUG-11 family).** OpenProject port had **20 production repository slots wired to in-memory adapters**: boards, webhooks, meetings, documents, wikis providers, backlogs, costs (adapter existed, just unwired), budgets, storages, reporting, auth_providers, BIM, github_integration, gitlab_integration, two_factor (security-critical), recaptcha, job_status, calendar, grids/dashboards, avatars. **Plus** the `queryRepository` gateway at composition-root level. Every user-visible write to any of these would have been silently lost on Vercel deployment.

**Severity.** All BLOCKER. This is the audit that returns the most findings; budget for it.

**Fix shape.** Each module needs: `<module>/drizzle/repositories.ts` with the Drizzle factories matching the memory adapter's interface 1:1. Then swap the wiring in `build-container.ts`. Use `db.transaction(...)` for multi-table writes. See [composition-root.md](composition-root.md) for the Db type pattern.

## Audit-C — Raw ID leaks (BUG-4 family)

**Question.** Where does the UI render raw integer IDs to the user instead of human labels?

**Method.**
1. `grep -rn "{[a-z]\+\.\(authorId\|assignedToId\|projectId\|userId\|recipientId\|memberId\|principalId\|categoryId\|priorityId\|statusId\|typeId\|parentId\)}" src/app/`
2. `grep -rn '<dd>{[a-z]\+\.[a-zA-Z]*Id}' src/app/`
3. Inspect column-builders in `src/lib/queries/` — any `String(wp.userId)` instead of resolved name?
4. Inspect activity stream + notifications + dashboard widgets — actor names or ids?

**Empirical example (BUG-4 family).** OpenProject port had 9 user-visible raw-ID leaks at HIGH severity:
- `columns-dsl.ts` (the WP list table) — type/status/priority/assignee/responsible all `String(wp.xxxId)`. Biggest one.
- `TimeEntriesList.tsx` — `user #${userId}`.
- `NotificationsList.tsx` — `#${projectId}`.
- `reminders/page.tsx` — `WorkPackage #${remindableId}`.
- `work-packages/new/page.tsx`, `team_planner/new/page.tsx` — raw "Assignee ID" / "Project ID" number inputs instead of pickers.
- `search/page.tsx` + `SearchBar.tsx` — attachment container subtitles.
- `admin/email-in/page.tsx` — `#${event.workPackageId}`.

**Plus** ~25 LOW-severity fallback sites where `User #<id>` appears only when the resolver missed.

**Severity.** HIGH for user-visible primary labels; LOW for fallbacks after a real lookup attempt.

**Fix shape.**
- For column-builders: extend renderers to accept optional resolver maps (`typesById: Map<number, string>`, `principalsById: Map<number, string>`). Consumer pre-fetches batched lookups, passes maps.
- For one-off renders: resolve the ID via `container.repositories.<module>.findById(id)` in the parent server component, pass the resolved label.
- For raw-ID inputs: replace with the existing pickers (`AssigneePicker`, `PriorityPicker`, project `<select>`).

## Audit-D — Server/client boundary violations (BUG-16 family)

**Question.** Do any server components or server-action files import runtime values from `"use client"` modules?

**Method.**
1. List all `"use client"` files: `grep -rln '"use client"' src/components/ src/app/`.
2. For each, list named exports: `grep -E "^export (function|const|class)" <file>`.
3. For each non-component export E from a client file F, search for imports of E in:
   - `src/app/**/page.tsx`
   - `src/app/**/route.ts`
   - `src/app/**/layout.tsx`
   - any other server-component file (no `"use client"` at top)
4. Type-only imports are fine. Runtime imports (function calls, constants used at runtime) = BLOCKER.

**Empirical example (BUG-16 family).** OpenProject port had 3 such violations:
- `assigneeLabel` exported from `AssigneePicker.tsx` (`"use client"`), called from server-side WP detail page.
- `decodeFiltersFromUrl` exported from `QueryBuilder.tsx` (`"use client"`), imported by `work-packages/actions.ts` + `save-query/page.tsx`.

**Symptom.** `Error: Attempted to call decodeFiltersFromUrl() from the server but decodeFiltersFromUrl is on the client.`

**Severity.** BLOCKER (page crashes at runtime).

**Fix shape.** Extract the pure helpers to a non-`"use client"` sibling module. Re-export from the client component for backwards compatibility. Update server callers to import from the new module.

## Audit-E — Markdown rendering + revalidatePath after mutations

**Question 1.** Do server actions that mutate DB state call `revalidatePath` (or `redirect`)? Without it, the UI shows stale data after submit.

**Method.**
1. List all `actions.ts` files.
2. For each exported async function that calls `services.*.{create,update,delete,add,remove}` or similar mutation methods, check that it ALSO calls `revalidatePath(...)`, `revalidateTag(...)`, or `redirect(...)`.
3. Functions that mutate but neither revalidate nor redirect = bug.

**Question 2.** Are there places using plain `<textarea>` for fields that are semantically markdown (description, body, comment, content) and should use the rich `MarkdownEditor`?

**Method.** `grep -rn "<textarea" src/app/ src/components/`. For each: is the field semantically markdown? Replace with `<MarkdownEditor name="..." defaultValue="..." />`.

**Empirical example (Audit-E).**
- 0 revalidate bugs in the OpenProject port (the wave-by-wave specs explicitly required it).
- 3 textareas to swap to MarkdownEditor: forums new-thread `content`, forums reply `content`, BCF comment form `body`.

**Severity.** LOW (UX polish).

## Audit-6 — Source-vs-target parity for UI-heavy modules

**Question.** For each UI-heavy module (>1000 LOC of source, visual + interactive), does the rendered port resemble the rendered original *in the browser*, or does it just exist?

**Why this audit is structurally needed.** Audits A-E are composition + UX-polish. They cannot detect "the rendered Gantt has 3 bars and looks correct" vs. "the rendered Gantt has 3 bars but bears no resemblance to OpenProject's Gantt." Tests can't either — they verify the spec, and if the spec was a strawman, 100% test coverage means nothing for parity.

**Method.**
1. List UI-heavy modules. UI-heavy = visual + interactive + likely 1000+ LOC in the source. Examples from OpenProject: Gantt, Boards, Calendar, Team Planner, BIM viewer, Backlogs, Wiki page editor.
2. For each module, open the original side-by-side with the port in two browser windows.
3. Walk through the same user story in both:
   - For Gantt: open a project, view the Gantt, drag a bar, change zoom, hover for tooltips, add a dependency.
   - For Boards: open a board, drag a card between columns, edit a card inline, toggle WIP limits.
   - For Calendar: navigate months, drag an event to reschedule, click an event for details.
   - (and so on per module)
4. Document each gap as: missing (feature absent), shaped-differently (feature present but visually/behaviorally divergent), works (parity).
5. Severity:
   - **BLOCKER** — feature is core to the user's mental model of the module (Gantt without drag-resize, Calendar without drag-reschedule, Board without drag-and-drop).
   - **HIGH** — feature is expected by anyone evaluating "is this OpenProject?" (Gantt without zoom levels, Calendar without recurrence, Wiki without inline link autocomplete).
   - **MEDIUM** — feature is power-user (Gantt baselines, Board WIP limits, Calendar quick-create modal).
   - **LOW** — feature is cosmetic (specific font, exact pastel colors, animation timing).

**Empirical example (OpenProject port).** Caught the Gantt parity gap in user-led testing AFTER the port was declared complete. Pre-Audit-6 (the discipline didn't exist): 1 BLOCKER missed (Gantt = thin frappe-gantt wrapper, no resemblance to OP). Cost of recovery: ~6 hours of focused re-port + this skill update. Adding this audit class to the workflow catches the same defect at minutes-per-module instead of hours-per-module after the fact.

**Severity at engagement scale.** If Audit-6 returns even one BLOCKER, the port is NOT production-ready regardless of test counts. The fix is a re-port wave for that module (preceded by a Pattern 15 source-walkthrough), not a polish wave.

**Fix shape.** Same as Pattern 15: source-walkthrough doc → spec referencing it → parallel subagent wave (one agent per coherent component slice: container/header, bars/cells, relations/overlays, data-extension). The OpenProject port's Gantt re-port wave used 4 parallel subagents and shipped in a single ~30 min cycle once the walkthrough doc was written.

**When to schedule.** As soon as functional completion is reached on UI-heavy modules — BEFORE Audit-A through E, because this audit may force a re-port wave that re-introduces composition-layer changes that A/B/C/D/E need to verify.

## Triage and dispatch

After all 5 audits land:

1. **Read each report's headline findings.**
2. **Classify by severity:** BLOCKER (security, data loss, runtime crash), HIGH (user-visible defect), MEDIUM (UX polish), LOW (cosmetic / dead code).
3. **Dispatch fix subagents in parallel** — one per coherent module group, with strict §2 (Pattern 10). The OpenProject port used 8 fix subagents covering: boards+backlogs Drizzle, meetings Drizzle, costs+documents+budgets+wikis Drizzle, auth+two-factor security-critical, BIM+GitHub+GitLab+storages, smaller ports + queries gateway, boundary fix, raw-IDs + textareas.
4. **Cross-cutting alert:** the `build-container.ts` file will be edited by 6+ subagents simultaneously. Pattern 9 (additive merges) applies. Inform each agent.
5. **Test fixtures may regress** — when memory adapters become Drizzle adapters, FK constraints get enforced. Have a final fixture-fix subagent ready (in OpenProject port: 4 FK-violation tests, fixed by seeding roles before members + threading `db` through pglite test container).

## What this audit pass costs and yields

**Cost.** ~30-45 min orchestrator wall-clock for 5 audit subagents + 8 fix subagents + 1 fixture-fix + browser verification + report writing.

**Yield (OpenProject port).** 20 BLOCKERs + 14 HIGH + 40+ low findings, 36 fixed, 0 regressions. Pre-audit: 1487 tests green but ~20 deploy-blockers hidden in production composition. Post-audit: 1495 tests green, all writes persist, pool bounded, schema/journal in sync, no boundary violations.

## When to schedule the audit pass

After functional completion (every spec dispatched, every wave landed, build green) but **before** declaring production-ready. The audit pass is what converts "feature-complete" to "deploy-ready."
