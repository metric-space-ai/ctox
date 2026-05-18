# Anti-patterns — 8 traps to avoid

Each anti-pattern names the failure mode + how to recognize it + what to do instead.

## Anti-pattern 1 — Speculative recon-first

**Trap.** Spawning many "recon" agents to survey the source codebase before the target is shaped. Each agent makes its own decisions about what matters; the orchestrator then has to reconcile N divergent target designs.

**Recognition.** You see yourself dispatching agents whose task is "summarize/analyze/document the source code" before any target file has been written. You're collecting prose dossiers instead of code.

**Empirical cost.** OpenProject port's first wave: ~32 recon agents, ~25% drift (8 returned dossiers as chat instead of writing to disk), divergent dossier shapes that the orchestrator had to manually reconcile. Cost: ~1.5 days wasted before the user pivoted to build-first.

**Instead.** Build the composition root + reference module + spec template by hand FIRST. Use the source code as a read-only reference (Read tool) when authoring specs. The orchestrator does the recon, not the subagents.

**Exception.** Genuinely opaque source codebases (no docs, no obvious structure) may benefit from a small targeted recon — 2-3 agents producing structured JSON about specific subsystems. Even then, prefer orchestrator-driven recon.

## Anti-pattern 2 — Verb hijacking by content gravity

**Trap.** When an agent generates a long polished output, the act of producing the text crowds out the explicit instruction to publish via the tool channel. The agent does the work and replies in chat, rather than calling Write.

**Recognition.** Agent's report is detailed, well-organized, and appears complete — but the corresponding file paths in §2 don't exist on disk.

**Empirical cost.** Recon phase of OpenProject port: 8 of ~32 agents drifted this way before the file-as-receipt rule was codified.

**Mitigation (Pattern 1+2).** Hard rule in spec: "Write tool is the only acceptable evidence." Verifier loop in agent's own turn (run typecheck + vitest until green). Orchestrator's gather verifies file existence before counting the agent as landed.

## Anti-pattern 3 — Singletons "until later"

**Trap.** `export const db = drizzle(client, ...)` at module scope. Convenient at scale 1, lethal at scale N. Postponing the refactor "until needed" means refactoring N modules instead of 1.

**Recognition.** Module exports an instance, not a factory. Tests can't replace it without `vi.mock()`. The composition root is partial or missing.

**Empirical cost.** OpenProject port had this initially; user corrected it after 1 module. Refactor of the existing module took ~30 min. Doing it after 28 modules would have been ~14h.

**Instead.** Every cross-cutting dependency is a factory: `createDrizzleClient(config)`, `createUsersService(deps)`, etc. The composition root is the only place these are wired. See [composition-root.md](composition-root.md).

## Anti-pattern 4 — `process.env` in business logic

**Trap.** Reading env vars outside the composition root creates uncontrollable side effects in tests. Each module that reads env becomes un-fakeable.

**Recognition.** `grep -rn "process.env" src/lib/services src/modules` returns hits. Tests have to set env vars to run.

**Empirical cost.** Same as anti-pattern 3 — caught early, easy fix; caught late, expensive.

**Instead.** Strict rule: env reads happen ONLY in `lib/composition/build-container.ts`. Every service factory takes its config as a typed `deps` argument. The composition root reads env once at boot, builds the container, passes it everywhere.

## Anti-pattern 5 — Wide types invite false confidence

**Trap.** Specs that type a parameter as the full record (e.g. `position?: GridWidgetRecord`) when only a subset is read. Invites agents to construct full records with placeholder values, surfacing typecheck noise.

**Recognition.** Agent reports include "I had to add fields like `createdAt: new Date()` to satisfy the type, even though the function doesn't use them." Or: typecheck errors after a wave land in places that should not have been touched.

**Empirical cost.** One drift incident in the OpenProject port; recovered with a 2-edit fix.

**Instead.** Type to what is *consumed*, not what is *convenient to pass*. If the function reads `position.id` and `position.x`, type the parameter as `{ id: number; x: number }` not the full record.

## Anti-pattern 6 — Premature milestone-asking

**Trap.** Stopping at "we've reached Phase X" to confirm with the user before proceeding, when the next backlog is obvious and the spec pipeline is intact.

**Recognition.** Orchestrator finishes a wave, writes a logbook entry, then waits for "go ahead" instead of dispatching the next wave.

**Empirical cost.** OpenProject port had this 3-4 times. Each pause cost 30 min - 6 hours of wall-clock that the methodology could have used.

**Instead.** Pattern 6 (continue is the default). Stop only when the backlog converges to genuinely-deferred items OR when a user correction is needed.

## Anti-pattern 7 — Orchestrator-self-implementation creep

**Trap.** The orchestrator hits a "small enough to do myself" item and doesn't dispatch. Saves 3 minutes; costs 5x more in coherence loss across waves.

**Recognition.** Orchestrator's token usage has more `Edit`/`Write` calls on production code than spec-authoring or verification. The spec backlog shrinks while the orchestrator's own commits grow.

**Empirical cost.** OpenProject port had 1 incident (TimeEntryRepository.delete fix, ~5 min orchestrator code) deemed acceptable. More than that becomes a habit and degrades the methodology.

**Instead.** Even small fixes go via subagent if they touch business logic. Reserve orchestrator-direct edits for: composition root (only), conventions doc, spec authoring, logbook entries, handbook updates.

## Anti-pattern 8 — Treating a rate-limit as a drift event

**Trap.** When agents terminate due to a rate-limit signal, "fixing the spec" or modifying the prompt because the agent "didn't do its job."

**Recognition.** Multiple agents in a wave all terminate with abnormally low tool-uses + duration + a "limit / resets" substring in result text. Orchestrator starts editing specs to "make them simpler."

**Empirical cost.** Caught in the OpenProject port before any spec edits happened — but had it been mistaken for drift, would have caused unnecessary spec rewrites.

**Instead.** Pattern 8 (identical re-dispatch after reset). Rate-limits are quota gates, not spec defects.

## Anti-pattern 9 — Spec-without-source-walkthrough produces feature-shaped stubs

**Trap.** Writing a spec for a UI-heavy module by naming the module ("port the Gantt") + picking a library ("use frappe-gantt for SVG rendering") without first reading the source module to enumerate its actual features. Subagents then implement *the spec* — and the spec is a strawman.

**Recognition.** The spec for module X says "thin wrapper around library Y" or "minimal version of Z" without citing line ranges of the source. Tests for module X cover only the wrapper's surface, not parity with the source. The orchestrator declares X "ported" without ever opening the source's UI in a parallel browser tab.

**Empirical cost.** OpenProject port: the Gantt module was specified as a `frappe-gantt` wrapper (~200 LOC). The actual source is 3319 LOC implementing hierarchical task tree, 5 zoom levels, drag-resize handles, dependency arrows, milestone diamonds, derived-children-duration clamps, today line, non-working-day stripes, selection mode for adding relations, and parent-derived scheduling. Tests passed (the wrapper rendered bars), the audit pass missed it (audits cover composition, not parity), interactive UX testing missed it (read-paths and CRUD only). User saw the result, called it correctly: "das hat absolut gar nichts mit dem original zu tun." Cost: ~6 hours of re-port work AFTER declaring the port complete — the most expensive class of correction in the engagement.

**Likely also-affected modules.** When this anti-pattern fires in one place, audit every UI-heavy module in the same engagement: Boards, Calendar, Team Planner, BIM viewer, Backlogs, Wiki page editor, anything that wraps a third-party library.

**Mitigation (Pattern 15).** Before authoring the spec for any UI-heavy module, do a source-walkthrough first — read the source module end-to-end, write a feature inventory document with explicit in-scope/out-of-scope marking, and reference that document as the spec contract.

**Audit gate (Audit-6).** A source-vs-target parity audit (browser-side comparison of the rendered original against the rendered port) catches what tests cannot.

**Instead.**
1. List UI-heavy modules in the source upfront. UI-heavy = visual + interactive + likely 1000+ LOC.
2. For each: spend 30-60 min on a source-walkthrough doc before writing the implementation spec.
3. The spec references the walkthrough as its contract.
4. Sign-off mentions feature-parity %, not just "ported."

## How to recognize when multiple anti-patterns are firing

Three or more of the following simultaneously, persisting across 2+ waves, means the methodology has hit a structural limit for the codebase at hand:
- Drift rate > 5% per wave (anti-pattern 2 not mitigated by Pattern 1)
- Watchdog timeout rate > 10% (anti-pattern absent of Pattern 10's §2 guards)
- Cross-cutting reds that don't converge (Pattern 9 violated by anti-pattern 3 or 5)
- Repeat sibling-cleanup events that change public APIs (Pattern 11 violated)
- User correcting the orchestrator more than once per phase (Anti-pattern 6 active)

When this state hits: reconsider the surface. Maybe a refactor wave is needed. Maybe the source codebase isn't hexagonally-decomposable enough. Maybe the target stack lacks a DI/composition story that the methodology requires.

This engagement (OpenProject port) never hit this state. The handbook can't claim to know what it looks like beyond the indicators.
