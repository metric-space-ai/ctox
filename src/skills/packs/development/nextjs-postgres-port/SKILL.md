---
name: nextjs-postgres-port
description: Use when porting a substantial existing application (Rails, Django, Laravel, ASP.NET, etc.) to a Next.js + Postgres + Drizzle + Auth.js stack using massively parallelized AI subagents. Covers the architectural preconditions, the spec-as-contract methodology, the 14 patterns and 8 anti-patterns that govern parallel agent dispatch, the hexagonal composition root that makes parallelism safe, the audit-driven bulk-fix workflow for production-readiness, and the interactive UX testing protocol that catches the defect class unit tests structurally miss. Empirically validated on a ~1M-LOC OpenProject → Next.js port that landed in 165 implementation agents over 5 days with 0 chat-only drifts and 1495 passing tests.
cluster: development
---

# Next.js / Postgres Port (with parallel AI subagents)

## Overview

Porting a substantial application to Next.js + Postgres + Drizzle + Auth.js is a problem with three reliable shapes:

1. **Greenfield-ish target.** The destination repo starts empty and grows. The source repo is treated as a read-only specification.
2. **Hexagonal architecture as a precondition, not a goal.** Composition root + DI + memory/Drizzle repository pairs are the load-bearing prerequisite for safe parallelization.
3. **Spec-as-contract + file-as-receipt.** Each subagent receives a spec listing exact file paths to write. The Write tool call is the only acceptable evidence of completion. This drives drift to zero across hundreds of agents.

This skill captures the methodology that ported OpenProject (Rails 8.1.3, ~1M LOC, 28 modules) to Next.js 15 + Drizzle ORM + Postgres in 5 calendar days using 165 implementation agents with 0 chat-only drifts and 1495 passing tests at landing.

## When to use this skill

**Yes — apply this skill when:**
- The source codebase can be decomposed into modules where each has clear inputs/outputs and isolatable side effects.
- The target stack is fixed at Next.js + Postgres (or close: Drizzle / Auth.js / Inngest / Vercel Blob / Resend).
- A token budget of ~25M tokens per ~120k LOC is acceptable.
- The orchestrator has continuous attention for ~30 hours of dispatching/integrating.

**No — choose another approach when:**
- The source codebase fights hexagonal modeling (heavy ActiveRecord callbacks, magic global registries, deeply coupled biz/UI logic).
- The target stack lacks DI / composition story.
- Token cost is the binding constraint.
- The work needs single-author signoff per file (the methodology breaks coherence here).

## Workflow

1. Open [references/patterns.md](references/patterns.md) and [references/anti-patterns.md](references/anti-patterns.md). Read both before starting — they encode the load-bearing rules.
2. Open [references/composition-root.md](references/composition-root.md) and build the hexagonal foundation by hand: `Container`, `buildContainer`, `getServices`, reference module. This is the template every subagent will copy.
3. Use [references/recipes.md](references/recipes.md) for: spec template, agent invocation prompt, side-effect surface inventory, wave sizing.
4. Dispatch implementation waves of 8 agents in parallel per wave; iterate.
5. After functional completion, run the audit phase from [references/audit-checklist.md](references/audit-checklist.md) — this is where a class of defect (composition wiring, in-memory ports, server/client boundary) gets caught that no unit test covers.
6. Use [references/ui-ux-testing.md](references/ui-ux-testing.md) for the interactive Chrome-based test protocol with subagent-parallel fixes (Pattern 14).
7. Watch [references/failure-indicators.md](references/failure-indicators.md) — eight leading signs the methodology is breaking down, each with a known fix.
8. Reference [references/case-study-openproject.md](references/case-study-openproject.md) for empirical numbers, specific drift events, and what the user corrections looked like in practice.

## Default operating assumptions

- **Source-walkthrough first for UI-heavy modules.** Before authoring the spec for any module >1000 LOC + visual + interactive (Gantt, Boards, Calendar, Team Planner, BIM, etc.), read the source end-to-end and write a feature-inventory walkthrough doc. The implementation spec then references the walkthrough as its contract. Without this gate, the spec encodes the orchestrator's guess and subagents implement the guess. (Pattern 15, Anti-pattern 9.)
- **Build infrastructure first, spec second, fan out third.** Never dispatch parallel subagents before the target architecture is shaped. (Anti-pattern 1.)
- **Refactor source idioms into target idioms on the way in.** Source-fidelity at scale is a recurring cost; refactor cost is one-time. (Anti-pattern 2 prevention.)
- **Modules are hexagonal or they are not modules.** Every module factory takes injected dependencies. No `process.env`, no `new Date()`, no `crypto.randomUUID()`, no singleton exports in business logic. The composition root is the only env-reading site.
- **Continue is the default.** Don't stop at convenient milestones. The port is done when every remaining task is dispatched, not when every task is complete.
- **Spec is the contract.** The Write tool is the only acceptable evidence. Returning code as chat fails the run. Tests in the agent's own turn close the loop.
- **8 agents per wave is the empirical sweet spot.** 4 underutilizes; 12+ hits cross-cutting collision density faster than convergence.
- **Strict §2 scope in every spec.** Each spec has a "Files YOU MUST create" list and an explicit "DO NOT modify outside §2" rule. Without it, watchdog timeout rate climbs above 25%.
- **Mid-flight reds are normal.** When N agents extend a shared core type simultaneously, the working tree typecheck will be red mid-wave. Convergence is monotone if every agent makes its own files green.
- **Sibling cleanup is monotone-OK.** When agent B notices a broken file from agent A's parallel work, B may fix it iff the fix is monotone-additive.
- **Premise-wrong specs are bounded-recoverable.** When a spec's assertion about the codebase is wrong, the §2 guard forces the agent to ship against reality and flag the deviation rather than rewrite the dependency.

## Reference guide

- [references/patterns.md](references/patterns.md) — 14 patterns with empirical citations
- [references/anti-patterns.md](references/anti-patterns.md) — 8 anti-patterns to avoid
- [references/recipes.md](references/recipes.md) — copy-pasteable templates (spec, agent prompt, composition root, side-effect inventory, wave sizing)
- [references/composition-root.md](references/composition-root.md) — hexagonal foundation: Container, buildContainer, factory pattern, memory/Drizzle pairs
- [references/audit-checklist.md](references/audit-checklist.md) — 5 audit categories that catch what unit tests miss
- [references/ui-ux-testing.md](references/ui-ux-testing.md) — interactive Chrome testing + Pattern 14 (subagent-parallel UX fixing)
- [references/failure-indicators.md](references/failure-indicators.md) — 8 leading signs methodology is breaking, with fixes
- [references/case-study-openproject.md](references/case-study-openproject.md) — empirical results, drift events, specific numbers from the OpenProject port

## Cost economics (per ~120k LOC port)

| Approach | Wall-clock | Token cost | Coherence risk |
|---|---|---|---|
| Single AI agent | ~80h | ~3M | low |
| 2-3 parallel agents | ~30h | ~10M | medium |
| **8 parallel + this method** | **~30h orchestrator-active** | **~25M** | low (with patterns) |
| Senior engineer team | 6-12 months | (salary) | high (knowledge silo) |

The methodology compresses ~100-1000× wall-clock at ~$300-500 token cost per 100k LOC. The bottleneck is orchestrator spec-authoring (~1-1.5h per 8-agent wave), not subagent capacity.

## What the methodology proves AND its limits

Empirically validated:
- 165 implementation agents, 0 chat-only drifts (Pattern 1).
- 8-agent waves with cross-cutting collision converge monotonically (Pattern 9).
- Strict §2 scope guards drive watchdog timeouts to <2% (Pattern 10).
- Schema barrel keystone fixes reach across sibling adapters (Pattern 11).
- Audit-driven bulk fix waves catch composition-layer bugs unit tests structurally cannot see (UX-Test-Report v4 of the OpenProject case).
- Source-walkthrough-first → parallel-subagent-wave delivers a faithful UI port for a 3319 LOC Angular Gantt module in ~30 min wall-clock once the walkthrough doc exists (Pattern 15, Gantt re-port wave).

Honest limits:
- Bias toward greenfield. Existing non-hexagonal codebases need a refactor wave first.
- Bias toward TypeScript. Strong type-checking is the verification gate; weaker-typed targets need a stronger test-as-spec mechanism.
- N=1 large-scale validation. Replicating without comparable user-collaborator corrections is untested.
- Real-Postgres-deploy + production load + multi-user concurrency are NOT validated by this methodology alone. Add a deployment phase.
- **Tests verify the spec, not the source.** UI-heavy modules (Gantt, Boards, Calendar, Team Planner, BIM, Backlogs, rich editors) demand a Pattern 15 source-walkthrough BEFORE the implementation spec, plus an Audit-6 source-vs-target parity audit BEFORE declaring the module ported. Without both, the methodology will produce a wrapper that passes 100% of its tests and bears no resemblance to the source — empirically demonstrated by the OpenProject Gantt initial-port failure (see [references/case-study-openproject.md](references/case-study-openproject.md) Correction 6).
