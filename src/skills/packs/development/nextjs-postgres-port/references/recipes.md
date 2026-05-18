# Recipes — copy-pasteable templates

## Recipe 1 — Spec template

```markdown
# Spec F<NN> — <one-line module name>

## 0. Status
ready-for-agent

## 1. Goal
<one paragraph: what this module does, why this spec exists>

## 2. Files YOU MUST create

```
port/src/modules/<name>/
  schema.ts                                  Drizzle table(s)
  repositories.ts                            interface(s)
  drizzle/repositories.ts                    real impl(s)
  memory/repositories.ts                     in-memory impl(s)
  service.ts                                 createXxxService(deps): XxxService
  service.test.ts                            tests using only fakes
  setup.ts                                   permission registration, menu, jobs registry
```

## 3. Behavioral contract
- When <event>, then <outcome>.
- ...

## 4. Files agent MAY READ (for context, not modification)
- port/CONVENTIONS.md (sections 6, 7, 8 critical)
- port/src/lib/services/users.ts + users.test.ts (reference module pattern)
- port/src/lib/composition/types.ts, build-container.ts
- ...

## 5. Files agent MAY NOT MODIFY
Anything under:
- port/src/lib/ (core)
- port/src/app/ (route layer — orchestrator wires)
- port/CONVENTIONS.md

If you need a new core helper, STOP and report.

## 6. Type signatures
```ts
export interface XxxRepository { ... }
export interface XxxServiceDeps { ... }
export function createXxxService(deps: XxxServiceDeps): XxxService;
```

## 7. Refactor-on-the-fly table
| Source pattern | Target pattern | Why |
|---|---|---|
| `acts_as_journalized` | Explicit `recordJournal()` call | TS prefers explicit |

## 8. Allowed dependencies
```
drizzle-orm, zod, react, next         (always allowed)
@/lib/*                               (core helpers)
@/modules/<this-module>/*             (this module's own files)
```

Forbidden: other `@/modules/<other>/*`, new npm packages, `process.env` reads, singleton exports, `vi.mock()` of project files.

## 9. Definition of Done
- All files in §2 exist.
- `pnpm typecheck` passes.
- `pnpm vitest run port/src/modules/<name>` passes; runtime under 1 second.
- Service factory takes only injected deps; no env reads, no singletons, no `new Date()`, no `crypto.randomUUID()`.
- Memory repository implementation passes the same tests as the Drizzle one (where applicable).

## 10. AGENT INVOCATION CONTRACT (paste into agent prompt verbatim)

> You are implementing a port-module spec for OpenProject Next. The spec is at `port/specs/F<NN>-<slug>.md`. **Read it.** It is the contract.
>
> The Write tool is the **only** acceptable evidence of completion. Returning content as chat is a failed run — even if the content is correct. Verify each file in §2 with the Read tool after creating it. Don't summarize at the end; just write the files.
>
> Follow `port/CONVENTIONS.md` strictly. Read `port/src/lib/services/users.ts` + `users.test.ts` first as the reference pattern; copy its shape. Hexagonal: every side effect is an injected dep. Tests use only fakes. No env, no globals, no singletons, no `new Date()`, no `crypto.randomUUID()`.
>
> Run `pnpm typecheck` after writing. If it errors, fix and re-run until green. Then run `pnpm vitest run port/src/modules/<name>` until green. Only then are you done.
```

## Recipe 2 — Subagent dispatch prompt (orchestrator-side)

```
Implement spec **F<NN>** at `<repo>/specs/F<NN>-<slug>.md`.

**Working directory:** `<repo>`

**Read first:**
1. The spec.
2. `<repo>/CONVENTIONS.md`
3. `<repo>/src/lib/services/users.ts` and `users.test.ts` — reference module pattern
4. `<repo>/src/lib/composition/{types,build-container,build-services,test-container}.ts` — wiring sites
5. <any spec-specific context files>

**Pattern 9 alert:** F<NN> may extend shared core types (Container, CoreServices). ADDITIVE merges only. If you encounter another agent's edits, MERGE — preserve theirs and add yours alongside. Convergence is monotone — your job is just to make YOUR files green.

**Hard rules (Pattern 10):**
- Write tool only. DO NOT modify files outside §2 of the spec.
- DO NOT add new npm packages.
- Hexagonal: no env / globals / singletons / `new Date()` / `crypto.randomUUID()` in business logic.
- Tests with fakes only. No `vi.mock()` of project files.

**Workflow:**
1. Read the spec.
2. Read references.
3. Write each file in spec §2 with the Write tool.
4. Run `pnpm typecheck`. Fix iteratively until green.
5. Run `pnpm vitest run <scope>`. Fix until green.
6. Run `pnpm build`. Fix until green.
7. Report files + final test count.

Begin.
```

## Recipe 3 — Subagent type choice

| Task | Subagent type |
|---|---|
| Recon / open-ended search | `general-purpose` (NOT Explore — Explore reads excerpts and misses content) |
| Spec implementation | `general-purpose` |
| Targeted lookup with known query | `Explore` (single grep / find) |
| Architecture planning | `Plan` |
| PR review | `code-reviewer` if available |

The OpenProject port discovered this empirically: early use of `Explore` for code-implementation work produced ~25% drift because Explore reads partial files. Switch to `general-purpose` and drift drops to 0.

## Recipe 4 — Composition root pattern

See [composition-root.md](composition-root.md) for the full template. Key points:

- `lib/composition/types.ts` — `Container`, `Repositories`, `Gateways`, `Config` types.
- `lib/composition/build-container.ts` — env reads happen ONLY here. Returns `Container`.
- `lib/composition/test-container.ts` — same `Container` shape with all fakes.
- `lib/composition/build-services.ts` — `CoreServices` factory taking `Container`.
- `lib/composition/index.ts` — `getContainer` + `getServices` as **`globalThis`-symbol-keyed singletons**, NOT `React.cache`. (Pool-leak fix from BUG-12.)

## Recipe 5 — Side-effect surface inventory

Before authoring specs, walk every module and list:
1. **DB writes:** which tables does the module write to? Each one needs a Drizzle adapter.
2. **External API calls:** mail, blob storage, OAuth providers, webhook outbound. Each is a `Gateway` interface in the composition root.
3. **Time:** `Clock` interface (`now()`, `addMs(date, ms)`).
4. **Randomness:** `IdGenerator` interface (`token(n)`, `uuid()`).
5. **I/O:** logging, audit events, observability.
6. **Permissions:** `assertCan(actorId, perm, scope)` — every mutation gates on this.

The inventory becomes the deps interface of every service factory. Tests inject fakes; production injects real adapters.

## Recipe 6 — Wave sizing

| Agent count per wave | When to use |
|---|---|
| 4 | Early phases (E1-E3): foundations being built; few independent slices yet. Underutilization is OK; coordination cost is high. |
| **8** | Default for implementation phases. Empirical sweet spot — collision density manageable, parallelism real. |
| 12+ | Avoid. Cross-cutting collision density on shared files (composition/types.ts, _handlers.ts, _shared.ts) climbs faster than Pattern 9 convergence. Wave time goes up, not down. |
| 16+ (audit-driven bulk fix) | Only when subagents are doing **non-overlapping** work AND each has a tight §2 (Pattern 10). Tested in OpenProject UX-test wave: 5 audit + 8 fix + 1 fixture-fix. |

## Recipe 7 — Logbook entry shape

After every wave, write a logbook entry with:

```markdown
# Logbook NNNN — Phase X: <wave name>

**Timestamp:** <ISO date + time + tz>
**Wave:** <list of specs landed>

## What was specced + landed
| Spec | Scope | Files | Tests added |
|---|---|---|---|
| F<NN> | <one-line> | <count> | +<N> |

## Phase drift
- Drift events: <count>.
- Watchdog timeouts: <count>.
- Sibling cleanups: <count>.
- Pattern 9 incidents: <list of cross-cutting flags raised + resolved>.

## Cumulative numbers
| Metric | Pre-phase | **Post-phase** | Δ |
|---|---|---|---|
| TS files | | | |
| LOC | | | |
| Tests | | | |
| API routes | | | |
| Page routes | | | |
| Specs written | | | |
| Implementation agents lifetime | | | |
| Chat-only drifts lifetime | | | |

## What's now genuinely deferred (next phase candidates)
- ...

## Patterns reinforced or extended
- ...
```

The "what's deferred" section is what feeds the next wave's specs. The cumulative numbers are what the handbook cites.

## Recipe 8 — User-correction protocol

When the user corrects the orchestrator's approach:
1. **Stop dispatching** until the correction is integrated.
2. **Document the correction** as a one-line entry at the top of the logbook (becomes part of the empirical record).
3. **Translate the correction into a rule** that goes into CONVENTIONS.md or the spec template.
4. **Audit existing code/specs** for violations and fix them.
5. **Resume.**

OpenProject port had 5 user corrections. All 5 became patterns or anti-patterns in the handbook. The cost of NOT doing this protocol: same correction comes back later, more expensive each time.
