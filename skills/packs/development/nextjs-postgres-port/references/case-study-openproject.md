# Case study — OpenProject → Next.js port

Empirical record from the engagement that produced this skill. **Do not invent metrics; cite from this file.**

## Substrate

- **Source:** OpenProject (Rails 8.1.3, ~1M LOC, 924 models, 614 services, 28 modules)
- **Target:** Next.js 15.5.15 + Drizzle ORM + Postgres + Auth.js + Inngest + Vercel Blob + Resend
- **Engagement duration:** 5 calendar days, ~30h orchestrator-active time
- **Final state:** ~120k LOC (compression ratio ~1:8), 1495 tests passing, deploy-ready

## Final cumulative numbers (post UX-test bulk-fix wave)

| Metric | Value |
|---|---|
| TypeScript files | 770+ |
| LOC | ~125k |
| Test files | 161 |
| Tests passing | **1495** (1 skipped — opt-in real Postgres E2E) |
| Test runtime | ~45s |
| Next.js API routes | 56 |
| Next.js page routes | 105 |
| Drizzle migrations | 15 |
| Specs written lifetime | 169 |
| Implementation agents lifetime | 165 (regular waves) + 14 (UX-test bulk-fix wave) = **179 total** |
| Chat-only drifts (lifetime) | **0** |
| Watchdog timeouts (lifetime) | 3 (1 in Phase R, 2 pre-Pattern-10) |
| Rate-limit incidents (lifetime) | 2 (Phase D + Phase S, both recovered identically) |
| Production build | green |
| Real Postgres E2E (F132 smoke) | green (25-step end-to-end) |

## Wave-by-wave breakdown

| Wave | Phases | Specs | Agents | Tests added | Notes |
|---|---|---|---|---|---|
| Foundations | E1-E3, F1-F7 | 10 | 10 | +250 | Composition root + reference module + first 7 modules |
| F-wave | F8-F11 | 4 | 4 | +50 | Inngest + composition extensions + first smoke |
| Phase D | F12-F17 | 6 | 6 | +100 | Recovered from rate-limit incident #1 |
| Phase E | F18-F23 | 6 | 6 | +80 | First wide-fan-out wave |
| Phase F | F24-F30 | 7 | 7 | +90 | Calendar/Gantt foundations |
| Phase G | F31-F37 | 7 | 7 | +85 | Auth deepening (OAuth, WebAuthn) |
| Phase H | F38-F45 | 8 | 8 | +95 | Project copy, queries, audit log |
| Phase J | F46-F53 | 8 | 8 | +110 | Storages, custom fields, RBAC matrix |
| Phase K | F54-F61 | 8 | 8 | +88 | Timezone, captcha, audit-log viewer |
| Phase L | F62-F69 | 8 | 8 | +77 | First high-collision cross-cutting wave |
| Phase M | F70-F77 | 8 | 8 | +95 | WP detail polish, comments, GDPR-export |
| Phase N | F78-F85 | 8 | 8 | +91 | First with strict §2 (Pattern 10 codified) |
| Phase O | F86-F93 | 8 | 8 | +89 | Wikis page bodies, OAuth-apps, BIM |
| Phase P | F94-F101 | 8 | 8 | +94 | Wiki tree, OAuth grants, GitLab UI |
| Phase Q | F102-F109 | 8 | 8 (+1 retest) | +75 | API v3 fanout (8-way collision on 1400-line test file) |
| Phase R | F110-F117 | 8 | 8 | +92 | Boards, news, forums, observability — recovered from rate-limit #2 |
| Phase S | F118-F125 | 8 | 8 | +132 | Production hardening (BIM 3D, deploy runbook, OP-data import) |
| Phase T | F126-F132 + F133 | 8 | 7 + orchestrator | +91 | Final polish + handbook finalization |
| **UX-test wave** | (audit + fix) | (audit reports) | 14 | +8 | Audit-driven bulk fix — 20 BLOCKERs caught |

## The 5 user corrections (load-bearing)

These are the corrections without which the methodology would have failed. Each became a pattern or anti-pattern.

### Correction 1 (Logbook 0004) — Don't fan out before target is shaped

> *"Du hast wohl zu früh, zu naiv und zu unüberlegt einfach die Subagents angeworfen, das führt wohl nur zu Chaos."*

**Translation.** "You dispatched subagents too early, too naively, too thoughtlessly — that just produces chaos."

**Context.** Orchestrator's first instinct was 32-agent recon-first. Produced ~25% drift + divergent dossier shapes. User intervened.

**Became.** Pattern 5 (build first, spec second, fan out third) + Anti-pattern 1 (speculative recon-first).

### Correction 2 (Logbook 0005) — Refactor on the fly is mandatory at scale

> *"Bei einem riesen parallel-port-Projekt ist ein on-the-fly-Refactoring quasi Pflicht."*

**Translation.** "In a massive parallel-port project, on-the-fly refactoring is essentially mandatory."

**Context.** Orchestrator was preserving source idioms (Rails callbacks, Active Record patterns) into TypeScript. User pointed out: at 1M LOC, source-fidelity is a recurring cost; refactor cost is one-time.

**Became.** CONVENTIONS.md §6 (refactor-on-the-fly translation table) + spec-template requirement.

### Correction 3 (Logbook 0006) — Hexagonal or it isn't a module

> *"Ein Modul ist nur ein Modul, wenn es für sich geschlossen vollständig (!) testbar ist und alle Side Effects (wenn überhaupt noch vorhanden) simuliert werden."*

**Translation.** "A module is only a module if it's fully self-testable in isolation and all side effects (if any) are simulated."

**Context.** Orchestrator had module-level singletons (`export const db = ...`). User pushed back: the 28-modules scale + parallel agents requires hexagonal isolation.

**Became.** Pattern 3 (reference module beats prose) + CONVENTIONS.md §7 (forbidden constructs) + Anti-patterns 3 + 4.

### Correction 4 (Logbook 0007) — Don't stop at milestones

> *"aber warum hast du aufgehört, wenn der port nicht abgeschlossen ist?"*

**Translation.** "But why did you stop, when the port isn't complete?"

**Context.** Orchestrator finished a wave, wrote the logbook, then waited. User pointed out: no waiting needed; the next wave's backlog is in the logbook.

**Became.** Pattern 6 (continue is the default) + Anti-pattern 6 (premature milestone-asking).

### Correction 6 (post-completion, 2026-05-02) — Tests verify the spec, not the source

> *"das hat absolut gar nichts mit dem original zu tun? wie konntest du nur so krass bei Portieren versagen?"*

**Translation.** "This has absolutely nothing to do with the original? How could you fail so spectacularly at porting?"

**Context.** Orchestrator declared the port complete after Phase T (165 implementation agents, 1495 tests green, build green). User opened the Gantt page in the browser and saw a thin `frappe-gantt` wrapper (~200 LOC) bearing no resemblance to OpenProject's actual 3319 LOC Angular Gantt module. The Gantt spec had said "use frappe-gantt for SVG rendering" — agents implemented exactly that, tests verified exactly that, audits never compared against the source.

**Became.** Pattern 15 (source-walkthrough first for UI-heavy modules) + Anti-pattern 9 (spec-without-source-walkthrough produces feature-shaped stubs) + Audit-6 (source-vs-target parity audit). Updated SKILL.md "honest limits" to flag this as a structural risk: test-coverage of the spec ≠ feature-parity with the source.

**Cost.** ~6 hours of focused re-port (source-walkthrough doc → 4 parallel subagents → CSS polish → composition wiring). Most expensive single-correction class in the engagement. Would have been ~minutes-per-module if Audit-6 had existed in the original audit pass.

### Correction 5 (Logbook 0010) — Port done == every remaining task dispatched

> *"du hast die nicht im hauptagent nicht vor augen geführt, dass die arbeit gar nicht abgeschlossen sein kann, wenn für nicht alle tätigkeiten des ports letztlich subagents gespawned und intergiert sind"*

**Translation.** "You didn't tell yourself that the work can't be complete unless every task is ultimately spawned to subagents and integrated."

**Context.** Orchestrator was implementing some items inline rather than dispatching. User pointed out: that breaks the pipeline definition.

**Became.** Pattern 7 (port done == every remaining task dispatched) + Anti-pattern 7 (orchestrator self-implementation creep).

## Specific drift events

### Phase D rate-limit (5 agents, 09:35 Berlin)

5 agents dispatched at 09:35 all terminated within 4-15 seconds with "You've hit your limit · resets 12pm (Europe/Berlin)". Identical re-dispatch at 12:03: all 5 completed within 7-14 minutes, zero drift.

**Lesson.** Pattern 8 — rate-limit recovery is identical-redispatch.

### Phase S rate-limit (8 agents, 02:17 Berlin)

8 agents dispatched at 02:17 all terminated within 3 min. Reset was 04:10. During the wait (~1h53m), orchestrator authored Phase T (8 specs) and Phase U (8 specs).

**Lesson.** Pattern 13 — orchestrator-spec-authoring during subagent lockdown.

### Phase R F105 watchdog stall

F105 (meetings API endpoints, 8 endpoints) stalled mid-test-file edit on a 1400-line test file. Routes + handlers were on disk; only test additions stalled.

**Recovery.** Targeted retest agent appended the missing tests in 162 seconds with a tighter §2 (only 1 file).

**Lesson.** Pattern 10 (strict §2) prevents most stalls; rare ones are recoverable with tighter scope.

### Audit-B finding — 20 in-memory ports in production

Audit subagent grep'd `createInMemory` / `createMemory` in `build-container.ts`. Found 20 production repository slots wired to in-memory adapters. Every user write to: boards, webhooks, meetings, documents, wikis providers, backlogs, costs, budgets, storages, reporting, auth_providers, BIM, GitHub, GitLab, two_factor (security-critical), recaptcha, job_status, calendar, grids/dashboards, avatars — would have been silently lost on Vercel deployment.

**Recovery.** 8 fix subagents in parallel landed Drizzle adapters for all 20 modules + the queries gateway in ~15 min wall-clock.

**Lesson.** This bug class is unit-test-invisible because tests use the same memory adapters intentionally. Only architecture audits catch it.

## What the methodology compressed

| Activity | Single AI agent | 2-3 parallel | 8 parallel + this method | Senior team |
|---|---|---|---|---|
| Wall-clock | ~80h | ~30h | **~30h orchestrator-active** | 6-12 months |
| Token cost | ~3M | ~10M | **~25M** | (salary) |
| Coherence risk | low | medium | low (with patterns) | high (silos) |

The orchestrator's wall-clock is the same as the 2-3-parallel approach but produces ~3x more code. The differentiator is the patterns (1-14), not the parallelism count.

## What was NOT validated by the engagement

- Real Vercel deployment behavior (env-var pickup, lambda cold-starts, Inngest cloud).
- Production Postgres under load (k6 scaffolding shipped, no run).
- Multi-user concurrent operations.
- Mail delivery via real Resend.
- OAuth login flows (no GitHub/Google credentials).
- Drag-and-drop in real browser (synthetic events vary).
- File upload to real Vercel Blob.
- Replication of the methodology without the 5 user corrections (N=1).

These remain validation gaps. A production-validation phase is a separate skill set.

## What WAS validated

- 165 implementation agents, 0 chat-only drifts (file-as-receipt + spec-as-contract works at scale).
- 8-agent waves with cross-cutting collision converge monotonically (Pattern 9).
- Strict §2 scope guards drive watchdog timeouts to <2% (Pattern 10).
- Schema barrel keystone fixes reach across sibling adapters (Pattern 11).
- Audit-driven bulk fix waves catch composition-layer bugs (UX-test wave).
- The full pipeline: foundations → spec-driven implementation → audit → fix → UX-test → fix → final report.

## Reading order for replicating

1. `SKILL.md` (this skill's entry point)
2. `references/composition-root.md` (the foundation)
3. `references/recipes.md` (templates)
4. `references/patterns.md` (the rules)
5. `references/anti-patterns.md` (the traps)
6. `references/audit-checklist.md` (the production-readiness pass)
7. `references/ui-ux-testing.md` (the testing protocol)
8. `references/failure-indicators.md` (the diagnostic guide)
9. This file (the empirical citations)
