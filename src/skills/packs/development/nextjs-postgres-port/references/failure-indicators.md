# Failure indicators — leading signs the methodology is breaking down

If you see any of these, **stop dispatching new waves** and address the root cause first. Each indicator maps to a specific failure mode with a known fix.

## Indicator 1 — Drift rate climbing above 5% per wave

**Signal.** More than 1 in 20 agents reports content as chat instead of calling Write, or completes the spec but their files differ from §2.

**Root cause.** Agents reading too much before writing. Spec is too vague, §2 file list is too long, or prompts missing the "Write tool only" hard rule.

**Fix.**
1. Tighten spec §2: max 12 file paths per spec; split if more.
2. Rewrite agent invocation prompt to lead with "The Write tool is the ONLY evidence of completion."
3. Add §13 verifier loop: "After writing, run typecheck + vitest + build until green."

**Empirical.** Recon phase of OpenProject port had ~25% drift before §13 hard rule was codified. Post-codification: 0 drift across 165 agents.

## Indicator 2 — Cross-cutting reds that don't converge

**Signal.** End-of-wave shows red typecheck/vitest, and the next wave inherits the same red.

**Root cause.** Pattern 9 violation. An agent's edit was non-monotone — refactored an existing convention or deleted a sibling's symbol.

**Fix.**
1. Identify which spec wrote the offending file.
2. Re-dispatch ONLY that spec with a tightened §2 forbidding the offending change.
3. If the spec's intent genuinely conflicts with another sibling's intent, you have a spec design error — split the responsibility.

**Empirical.** Zero non-monotone divergence events in the OpenProject port. Cost of one would be ~30 min orchestrator time.

## Indicator 3 — Watchdog timeouts above 10% per wave

**Signal.** More than 1 in 10 agents stalls (no output for 600s) and gets terminated by the runtime.

**Root cause.** Agents reading too many sibling files trying to understand integration. Pattern 10 ("strict §2 scope") not codified or not followed.

**Fix.**
1. Add explicit "DO NOT modify files outside §2" to every spec.
2. Add explicit "DO NOT read files unrelated to your spec" guidance.
3. Re-dispatch failed specs with the tighter prompt (Pattern 10 retry shape).

**Empirical.** Phase N of OpenProject port saw 25% timeout rate (no Pattern 10). Phase O+P+Q+R+S+T+U with Pattern 10 saw 0% across 16 consecutive 8-agent waves. Audit-driven bulk fix wave had 0% across 14 subagents.

## Indicator 4 — Rate-limit hit on dispatch

**Signal.** All agents in a wave terminate within seconds with "You've hit your limit · resets at <time>".

**Root cause.** Quota exhaustion. **NOT a methodology failure.**

**Fix.** Pattern 8 — wait for reset, identical re-dispatch. During the wait, author the next phases' specs (Pattern 13).

**Empirical.** Phase D unanimous rate-limit at 09:35 Berlin, reset 12:00, re-dispatched cleanly. Phase S unanimous rate-limit at 02:17 Berlin, reset 04:10. During Phase S wait, authored Phase T+U specs (16 specs) — saved ~2-3h orchestrator wall-clock.

## Indicator 5 — Premise-wrong specs producing partial landings repeatedly

**Signal.** Agents repeatedly ship "not_supported" envelopes or flag spec deviations.

**Root cause.** Orchestrator authoring specs without verifying the underlying surface exists.

**Fix.**
1. Before each wave, spend 30 min reading the surface to be specced.
2. Pattern 12 still applies on individual cases, but pattern-of-occurrence means systemic spec quality issue.

**Empirical.** F84 (wikis) and F92 (attachments) hit Pattern 12 in the OpenProject port. Both shipped successfully via Pattern 10 + 12. No systemic pattern detected.

## Indicator 6 — Orchestrator self-implementation creep

**Signal.** Orchestrator starts writing implementation code instead of dispatching specs.

**Root cause.** Anti-pattern 7. Orchestrator hit a "small enough to do myself" item and didn't dispatch.

**Fix.** Stop. Write the spec. Dispatch. The 3-min savings of self-implementation cost ~5x more in coherence loss across waves.

**Empirical.** OpenProject port had 1 such incident (F109 TimeEntryRepository.delete fix, ~5 min orchestrator code). Tolerable as a one-off but should not become a habit.

## Indicator 7 — User correcting the orchestrator more than once per phase

**Signal.** The user says "no, do X instead" and the orchestrator has to back out work.

**Root cause.** Orchestrator making decisions that should have been clarified up-front. The spec template's "definition of done" was insufficient.

**Fix.** For the next phase, run a 2-3 sentence intent-check past the user before dispatching. ("This phase will ship X via Y; reasonable?") Less dispatch volume, less rework.

**Empirical.** 5 user corrections in OpenProject port, all in foundation phases (Logbook 0004-0010). Post-Logbook-0010: zero corrections needed across 16 implementation waves.

## Indicator 8 — Tests passing but build failing for >2 waves

**Signal.** vitest green, typecheck green, but `pnpm build` red consistently.

**Root cause.** Next.js typed-routes are stricter than tsc. An agent exported a non-route symbol from a `route.ts`, OR an agent's component imports a runtime function from a `"use client"` file into a server component (BUG-16).

**Fix.**
- For route exports: the `_handler.ts` adjacent-file convention. Move all exported symbols other than `GET`/`POST`/etc. to `_handler.ts`.
- For server/client boundary violations: see [audit-checklist.md](audit-checklist.md) Audit-D. Extract pure helpers to a non-client sibling module.

**Empirical.** 3 incidents in OpenProject port (F77 download route, F93 email-in route, F124 health route). All resolved via the `_handler.ts` convention.

## Indicator 9 (UX-test phase only) — Same defect class repeatedly

**Signal.** Browser-testing finds 5+ bugs of the same shape (e.g. 5 different forms with no Server Action wiring; 5 different pages showing raw IDs).

**Root cause.** A pattern was missed during implementation. Likely a missing rule in CONVENTIONS.md or the spec template.

**Fix.** Run an audit subagent for that defect class (see [audit-checklist.md](audit-checklist.md)). The audit produces a findings list; dispatch fix subagents in parallel (Pattern 14).

**Empirical.** OpenProject port UX-test wave: Audit-B found 20 in-memory ports (BUG-11 was not isolated). Audit-C found 9 raw-ID leaks (BUG-4 was not isolated). Audit-D found 2 more boundary violations (BUG-16 was not isolated). All fixed in one parallel wave.

## When to abandon

If 3 or more indicators above are firing simultaneously and persisting across 2+ waves, the methodology has hit a structural limit for the codebase at hand.

Consider:
- A refactor wave to make the surface more decomposable.
- Switching to a smaller-parallelism approach (2-3 agents) for problem areas.
- Hand-coding the structurally-stuck portion and resuming parallel waves elsewhere.

This engagement (OpenProject port) never hit this state. The skill can't claim to know what it looks like beyond the indicators.

## Quick triage checklist

When something feels wrong:

1. **Drift rate?** Check last wave's agent reports. Count chat-only failures vs file-write completions.
2. **Timeout rate?** Check for "no progress for 600s" in agent statuses.
3. **Rate-limit?** Check agent termination messages for "limit / resets" substrings.
4. **Cross-cutting red?** Run typecheck + vitest. Are the failing files all in the current wave's scope?
5. **Build failing while tests pass?** Check for `_handler.ts` violations or boundary imports.
6. **User corrections accumulating?** Stop. Listen.

Each item above maps to one of the 9 indicators. Most can be diagnosed in <5 min from agent reports + git status + test output.
