# Patterns — 14 empirically validated rules for parallel AI subagent ports

Each pattern lists statement, empirical source, mechanism, and operational consequence.

## Pattern 1 — File-as-receipt

**Statement.** A subagent's run is judged complete only if it called the Write tool with the exact file paths the spec demanded. Returning content as chat — even correct content — is a failed run.

**Empirical.** OpenProject port: 0% chat-only drift across 165 implementation agents in production phases. The recon phase (before this rule was codified) had ~25% drift; after codification, 0%.

**Mechanism.** The spec is a contract whose §2 lists exact paths. The orchestrator's gather step verifies file existence with the Read tool. Agents whose run was a chat dump are detected by file count mismatch and re-dispatched.

**Consequence.** Spec template MUST include §2 "Files YOU MUST create" + §13 hard rule "The Write tool is the only acceptable evidence of completion."

## Pattern 2 — Spec-as-contract closes the drift loop

**Statement.** The drift between intended work and produced work collapses if the spec encodes both the goal AND the evidence. Goal-only specs drift; goal+evidence specs don't.

**Empirical.** Recon phase used goal-only specs (descriptive prose) → 25% drift. Implementation phases used goal+evidence specs (paths + Write-tool-only rule + verifier loop) → 0% drift across 165 agents.

**Mechanism.** Without the evidence clause, the agent's "polished output" can satisfy the goal in chat. With it, the agent's only legal path to "done" is calling Write. The orchestrator's verifier closes the loop in the agent's own turn.

**Consequence.** Every spec ends with a §5 (DoD) and §13 (agent invocation contract) that explicitly say: Write tool only, run typecheck + vitest + build until green in the same turn.

## Pattern 3 — Reference module beats prose specification

**Statement.** A complete reference module (e.g. 270 LOC of `users.ts` + `users.test.ts`) is worth more than any amount of prose. Subsequent specs cite the reference; agents copy its shape.

**Empirical.** The first reference module in the OpenProject port was the template for 100+ subsequent code agents. Schema layout, factory naming, error envelope, test buildService helper — uniform across modules.

**Mechanism.** Code is unambiguous; prose is not. A working example shows the type signatures, error handling, test-via-fakes shape, and assertCan placement in one place. New specs say "follow the shape of `users.ts`" and that's enough.

**Consequence.** Step 2 of any port: hand-author the composition root + ONE complete reference module before dispatching any agents. Spend 1-2 days on this. It's the template every subagent will copy.

## Pattern 4 — Closing the loop with verifiers in the agent's own turn

**Statement.** Each agent's spec ends with explicit verification commands (`pnpm typecheck`, `pnpm vitest run`, `pnpm build`) that the agent must run until green within the same turn. This eliminates the "agent claimed green but actually red" failure mode.

**Empirical.** All 165 implementation agents in the OpenProject port reported "typecheck green, vitest green" with corresponding disk state. Zero "agent claimed green but actually red" events.

**Mechanism.** The agent runs the verifier, sees red output, fixes it, re-runs, until green. The orchestrator never has to debug a failed run remotely.

**Consequence.** Every spec's §13 agent invocation contract includes: "Run pnpm typecheck after writing. If it errors, fix and re-run until green. Then run pnpm vitest run [specific scope] until green."

## Pattern 5 — Build first, spec second, fan out third

**Statement.** Never dispatch parallel subagents before the target architecture is shaped. Build the foundation by hand: composition root, reference module, conventions doc, spec template. Only then write specs and fan out.

**Empirical.** The OpenProject port's first instinct was recon-first (32 agents surveying the codebase). User corrected: that produces N divergent target designs and 25% drift. Switched to build-first → 0% drift.

**Mechanism.** Without a stable target, each subagent makes its own decisions about file layout, naming, error handling. Convergence is impossible.

**Consequence.** Days 1-3 of any port are orchestrator-coded foundations. Days 4+ are subagent-dispatched waves.

## Pattern 6 — Continue is the default

**Statement.** When the wave lands cleanly and the next backlog is identified, continue. Don't stop at convenient milestones to await confirmation.

**Empirical.** The OpenProject port had three explicit user corrections of the form "warum hast du aufgehört?" — orchestrator stopping at milestones (Phase D landed, Phase F-wave landed, etc.) instead of immediately starting the next wave.

**Mechanism.** Each wave generates the next wave's backlog (deferred items + new edge cases). The pipeline self-feeds; the orchestrator's job is to keep dispatching, not to decide when to stop.

**Consequence.** "The port is done when every remaining task is dispatched, not when every task is complete." Stop only when backlog converges to genuinely-deferred-or-trivial items.

## Pattern 7 — Port done == every remaining task dispatched

**Statement.** Completion is defined by spec backlog state, not by the orchestrator's sense of "we've done enough." If there's still un-dispatched work, the port isn't done.

**Empirical.** The OpenProject port reached 165 implementation agents because each wave's logbook entry identified the next wave's backlog explicitly. Stopping at "core is done" would have left 70+ specs un-dispatched.

**Mechanism.** Decoupling "complete" from "perfect" lets the methodology grind through arbitrary backlog sizes. The orchestrator's role becomes: keep the spec pipeline populated, keep the integration green.

**Consequence.** Every logbook entry ends with a "what's now genuinely deferred" section. Each item there becomes a spec for the next wave OR a documented out-of-scope item.

## Pattern 8 — Rate-limit recovery is identical-redispatch

**Statement.** When agents terminate due to a quota/rate-limit signal (rather than logical failure), recovery is to wait for the limit reset and re-dispatch the *identical* spec. No modification, no debugging, no fallback to orchestrator-self-implementation.

**Empirical.** Phase D of OpenProject port: 5 agents launched at 09:35 Berlin terminated within 4-15 seconds with "You've hit your limit · resets 12pm". Re-dispatch of identical specs at 12:03: all 5 completed within 7-14 minutes, zero drift. Phase S: 8 agents unanimous rate-limit hit; identical re-dispatch after reset all landed clean.

**Mechanism.** Rate-limit terminations are quota gates, not spec defects. Treating them as drift events would push them into the "fix the spec" anti-pattern. They need a different recovery shape.

**Consequence.** Detect rate-limit terminations by: abnormally low tool-uses + duration + the substring "limit" / "resets" in result text. Mark spec `in-flight-blocked`, schedule wakeup for after reset, retry verbatim.

## Pattern 9 — Cross-cutting waves can land red mid-flight

**Statement.** When N agents in the same wave all extend a shared core type (Container, CoreServices, build-services.ts), the working-tree typecheck WILL be red while the wave is in flight. This is not drift; it is the expected mid-flight shape. Convergence to green is monotone if every agent self-corrects to green for *its own* files.

**Empirical.** Phase L (8 agents extending Container types simultaneously), Phase M (5 agents collateral-fix chain), Phase N (3 schemas added in parallel), Phase R (8 agents on `_handlers.ts`+`_shared.ts`+`api-v3-routes.test.ts`), audit-driven bulk-fix wave (8 agents extending `build-container.ts` + `lib/db/schema/index.ts` simultaneously). In every case, mid-flight typecheck red, post-wave green. Zero re-dispatches needed for the cross-cutting itself.

**Mechanism.** The union of monotone-additive edits is monotone-additive. Each agent appends to the shared file (registers a slot, adds an import, adds a route) without removing or reshaping siblings. When all land, the file is the union.

**Consequence.** Don't interrupt agents to "fix red." Wait for the wave; verify after. If post-wave still red, then dispatch a small fix wave.

## Pattern 10 — Strict §2 scope + tighter prompt prevents watchdog timeouts

**Statement.** Watchdog terminations (>600s no progress) are a real failure mode distinct from rate-limits and drift. The recovery: re-dispatch with explicit "DO NOT modify files outside §2" scope language.

**Empirical.** Phase N (without strict §2): 25% timeout rate. Phase O onwards (with strict §2 in every spec): 0% across 16 consecutive 8-agent waves.

**Mechanism.** Without the §2 guard, agents read deep into sibling files trying to understand integration, get lost, and stall. With it, they know what files are theirs and what files are off-limits.

**Consequence.** Every spec template ends with "Hard rules: DO NOT modify files outside §2." Every agent prompt includes this verbatim. Re-dispatching a stalled agent uses 30% shorter scope-narrowed prompt.

## Pattern 11 — Sibling cleanup is monotone-OK

**Statement.** When agent B, while implementing its own spec, notices a broken file from agent A's parallel work, B may fix A's file iff the fix is monotone-additive (preserves A's intent + makes the build pass).

**Empirical.** OpenProject port had ~12 sibling-cleanup events across 165 agents. F75 fixed F77's route export; F86 fixed sibling F89/F90/F91 typecheck flags; the schema-barrel keystone fix in the audit-driven wave unblocked 8 sibling Drizzle adapters. None caused regressions.

**Discriminator.** "Would A have approved this fix if asked?" Refactoring a file's organization yes; changing its public API no.

**Consequence.** Don't flag sibling cleanups as scope creep. Verify the fix is monotone, accept it. Conventions outlive their introducing specs.

## Pattern 12 — Premise-wrong specs are bounded-recoverable

**Statement.** When a spec's assertion about the existing codebase is wrong, the strict §2 guard forces the agent to either (a) ship what's possible on the actual surface, or (b) report and stop. Both are monotone. Without the guard, the agent would silently rewrite the underlying module to match the spec's premise.

**Empirical.** F84 (wikis): spec assumed the wikis module supported page bodies; actual surface was provider-linking only. Agent shipped read-only UI on the actual surface, flagged `update` as `not_supported`. F92 (attachments): spec listed repo paths that didn't exist in the actual layout; agent shipped against actual layout with explicit deviation note. F132 (smoke test): spec assumed all migrations in journal; reality had 7 missing entries; agent's test reads `.sql` files via `fs` and applies them as a self-contained workaround.

**Mechanism.** The §2 file list is the contract. When that contract conflicts with reality, the agent's only legal moves are: ship what fits the actual reality, or report the conflict. Both produce monotone progress.

**Consequence.** When reviewing a spec for a wave, you don't need to verify every premise. The cost of getting one wrong is bounded: a single spec lands partially with a clear flag, never destabilizes siblings.

## Pattern 13 — Orchestrator-spec-authoring during subagent rate-limit lockdown

**Statement.** When subagents hit a rate-limit, the orchestrator's token bucket is independent. Use the dead time to author the next phases' specs in advance.

**Empirical.** Phase S of OpenProject port hit unanimous rate-limit at 02:17 Berlin (8 agents terminated within 3 min). Reset was 04:10. During the ~2-hour wait, the orchestrator authored Phase T (8 specs) and Phase U (8 specs) — 16 specs total. When subagents came back online, two additional waves launched immediately without spec-authoring serialization. Saved ~2-3h orchestrator wall-clock.

**Discriminator.** Spec-authoring uses orchestrator tokens (per-conversation cap), not subagent dispatch quota (per-account cap). Verify this in your environment before relying on it.

**Consequence.** Rate-limit windows are productivity opportunities, not pure waste. Default to authoring next-wave specs during them.

## Pattern 14 — Subagent-parallel UX-fixing

**Statement.** When the orchestrator is the only agent that can drive the browser (Chrome MCP) but bugs found during testing are independent, dispatch subagents with focused fix specs (file paths + symptom + acceptance criteria + typecheck must pass). Each subagent runs ~60-90s, fixes one bug end-to-end. Orchestrator continues browser-testing in parallel and verifies fixes after they land.

**Empirical.** OpenProject UX-test wave: 5 audit subagents in parallel produced findings reports; 8 fix subagents in parallel landed Drizzle adapters for 20 in-memory ports + boundary fixes + raw-ID resolves; 1 fixture-fix subagent corrected FK-violation tests. Total 14 subagents in 3 sequential parallel waves, ~30 min wall-clock for what would have been 6+ hours serial. Zero regressions; 1495 tests passing post-wave.

**Mechanism.** Code-audit and code-fix are pure functions of source files; they don't need a browser. Browser-driving is the orchestrator's exclusive surface. Splitting these axes runs them concurrently.

**Consequence.** When interactive UX testing surfaces bugs, batch them by independence and dispatch fix subagents. Use the same Pattern 10 strict-§2 discipline that worked for implementation phases.

## Pattern 15 — Source-walkthrough first for UI-heavy modules

**Statement.** Before writing the implementation spec for any UI-heavy module (>1000 LOC of source, visual + interactive), produce a source-walkthrough document: read the source module end-to-end, enumerate its features in a structured list, mark each in-scope / out-of-scope / out-of-engagement, and write the Angular→React (or source→target) component mapping. The implementation spec then *references* the walkthrough as its contract — not the bare module name.

**Empirical.** OpenProject port: the original Gantt module wave was specified without a walkthrough — agents got "use frappe-gantt for SVG rendering" + a `GanttPayload` shape. Result: a 200 LOC wrapper with no resemblance to the actual 3319 LOC OpenProject Gantt. The re-port wave was preceded by a walkthrough doc (`research/GANTT-SOURCE-WALKTHROUGH.md`, ~250 lines): explicit feature inventory of all 17 source files, an Angular→React component mapping table, an explicit out-of-scope section (cascading scheduling, baselines, critical-path), and a "data the React Gantt needs" section that drove the data agent's spec. 4 parallel subagents shipped a real Gantt in one wave.

**Mechanism.** A walkthrough doc forces the orchestrator to confront the source's complexity *before* writing the spec. Without it, the orchestrator's mental model of "what the module does" is a guess, the spec encodes the guess, and N subagents implement the guess in parallel. With it, the spec encodes the actual feature surface, and the in-scope/out-of-scope distinction becomes a deliberate sign-off rather than a silent omission.

**Discriminator.** UI-heavy modules need walkthroughs. CRUD modules (forms + tables backed by Drizzle) usually don't — the source code IS the spec, and the target shape is a mechanical translation.

**Consequence.**
- Add a "source-walkthrough" gate to the spec-authoring step for UI-heavy modules.
- Walkthrough format: §Layout (visible structure), §Math (pure functions, port directly), §Components (each one's DOM + behavior + interactions), §Data needs (extensions to the existing payload), §In-scope vs out-of-scope, §Source→Target component mapping table.
- Sign-off mentions feature-parity %, with the gap diff documented.

## Cross-references

- Pattern 1 (file-as-receipt) presupposes Pattern 5 (build first) — without target architecture, paths in §2 are guesses.
- Pattern 9 (cross-cutting reds) presupposes Pattern 11 (sibling cleanup) — agents need permission to make peers green.
- Pattern 10 (strict §2) is what makes Pattern 12 (premise-wrong) recoverable — without scope guards, premise-wrong specs cause rewrite cascades.
- Pattern 14 (UX-parallel-fixing) is the application of Patterns 9-12 to the testing phase, not just the implementation phase.
- Pattern 15 (source-walkthrough first) is what prevents Anti-pattern 9 (spec-without-source-walkthrough). Audit-6 (parity audit) is the verification gate that catches the residual gap.
