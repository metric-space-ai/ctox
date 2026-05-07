# Payroll Implementation Map

Target module: `payroll`
Source RFC: [`rfcs/0006_business-basic-payroll.md`](../../../rfcs/0006_business-basic-payroll.md)
Acceptance matrix: [`payroll-acceptance-matrix.md`](payroll-acceptance-matrix.md)
User stories: [`payroll-user-stories.md`](payroll-user-stories.md)
Current status: M0 implemented (engine + runtime + API + UI dispatch + smoke); package unit tests green.

## Ownership

| Object | Owner | Notes |
|---|---|---|
| `payroll_period` | Operations Payroll | Period master; locked flag blocks new runs. |
| `payroll_component` | Operations Payroll | Earning/deduction with formula (fix / percent_of / formula DSL) and GL account. |
| `payroll_structure` | Operations Payroll | Bundles components per frequency/currency. |
| `payroll_structure_assignment` | Operations Payroll | Employee → structure with from/to dates and base salary. |
| `payroll_run` | Operations Payroll | Periodic batch generating one slip per assigned employee. |
| `payroll_payslip` (+ lines) | Operations Payroll | Central UI object. Posted slip is immutable. |
| `payroll_additional` | Operations Payroll | One-off bonus/arrear/deduction picked up by next run. |
| `accounting_account` (read) | Accounting | Components reference GL accounts; payroll never owns them. |
| Journal entry on post | Accounting (via posting) | Created through the existing `packages/accounting` posting pattern. |
| Workforce time entry (M1+) | Operations Workforce | Read-only source for hourly components. |

## M0 Status

| Step | Required behavior | Files touched | Proof |
|---|---|---|---|
| 1 | Add `payroll` as TOP-LEVEL module to navigation registry plus `BusinessModuleId` type. | [`packages/ui/src/navigation/model.ts`](../packages/ui/src/navigation/model.ts) | Visible in nav/deeplink: `/app/payroll/runs`. |
| 2 | Create `modules/payroll/module.json` and `modules/payroll/README.md` per Skill §5. | [`modules/payroll/module.json`](../modules/payroll/module.json), [`modules/payroll/README.md`](../modules/payroll/README.md) | Module manifest exists at top level alongside Sales/Marketing/Operations/Business/CTOX. |
| 2b | Add `[data-module="payroll"]` accent token to global theme. | [`packages/ui/src/theme/theme.css`](../packages/ui/src/theme/theme.css) | Module-scoped color variable applied. |
| 3 | Self-contained engine package with formula DSL, computation, and journal builder; unit tests for both. | [`packages/payroll/`](../packages/payroll/) | `pnpm --filter @ctox-business/payroll test` → 13 / 13 green. |
| 4 | Durable JSON-backed runtime with seed (1 period, 3 components, 1 structure, 2 employees, 2 assignments). | [`apps/web/lib/payroll-runtime.ts`](../apps/web/lib/payroll-runtime.ts) | GET `/api/payroll` returns seed snapshot. |
| 5 | REST surface with `command + payload` envelope. | [`apps/web/app/api/payroll/route.ts`](../apps/web/app/api/payroll/route.ts) | Dispatcher accepts the documented commands and rejects unknown ones. |
| 6 | Workbench renders left intake, center run + slip tables, right inspector with line breakdown and audit. | [`apps/web/components/payroll-workbench.tsx`](../apps/web/components/payroll-workbench.tsx), [`apps/web/components/payroll-workspace.tsx`](../apps/web/components/payroll-workspace.tsx), [`apps/web/app/app/[module]/[submodule]/page.tsx`](../apps/web/app/app/%5Bmodule%5D/%5Bsubmodule%5D/page.tsx) | Route `/app/payroll/runs` renders the workbench; `[module]/[submodule]/page.tsx` dispatches `isPayroll`. |
| 7 | `data-context-*` attributes on every clickable record (run, slip, line). | `payroll-workbench.tsx` | Right-click `Prompt CTOX` payload contains module/submodule/recordType/recordId/label. |
| 8 | Smoke script exercises load → run → review → post → ledger → reload → audit → idempotent re-post. | [`apps/web/scripts/payroll-smoke.mjs`](../apps/web/scripts/payroll-smoke.mjs) | `pnpm test:payroll` exits 0 against running dev server. |
| 9 | Web typecheck passes. | `pnpm --filter @ctox-business/web typecheck` | No errors. |

## M1 Plan

| Area | Required M1 behavior |
|---|---|
| Formula DSL | UI editor for component formula with live preview; reject illegal tokens with inline error. |
| Additionals | Create/edit/delete `payroll_additional` per employee/period/component; surface in slip lines. |
| Withheld | Slip context menu can move `Draft`/`Review` to `Withheld` and back; `Withheld` slips do not post on bulk run. |
| Reversal | `Cancel` on a `Posted` slip writes a reversing journal entry (already wired to `postedJournals`); UI surface. |
| Period lock | Locked period blocks new runs and edits to existing slip lines. |
| Hourly components | Pull approved `workforce_time_entry` rows for a period and feed them into formulas via `worked_hours` variable. |
| DATEV export | Confirm posted payroll JEs surface in DATEV export with the right account codes. |
| Failure recovery | A run with at least one failed slip flips the run to `Failed`; recompute can recover individual slips. |
| 50 paired user stories | All required, manual + CTOX block per story, in `payroll-user-stories.md`. |
| Acceptance matrix | No core `missing` / `partial` / `needs proof` rows. |
| Browser proof | Click + right-click + drawer + post + reload via `ctox-browser-automation`. |

## File Layout (current)

```
templates/business-basic/
├── packages/
│   └── payroll/
│       ├── package.json
│       ├── tsconfig.json
│       └── src/
│           ├── index.ts
│           ├── types.ts
│           ├── formula.ts        # tokenizer + recursive descent parser, no eval
│           ├── engine.ts         # computePayslip (lines, gross/deduction/net)
│           ├── posting.ts        # buildJournalDraft (balanced JE)
│           └── tests/
│               ├── formula.test.ts
│               └── engine.test.ts
├── modules/
│   └── payroll/
│       ├── module.json           # top-level module manifest
│       └── README.md
├── apps/
│   └── web/
│       ├── lib/payroll-runtime.ts
│       ├── components/payroll-workspace.tsx     # PayrollWorkspace + PayrollPanel route entry
│       ├── components/payroll-workbench.tsx     # 3-zone work surface
│       ├── app/payroll/page.tsx                 # redirects /app/payroll → /app/payroll/runs
│       ├── app/api/payroll/route.ts             # REST surface
│       └── scripts/
│           ├── payroll-smoke.mjs
│           └── payroll-browser-proof.mjs
├── docs/
│   ├── payroll-oss-implementation-notes.md
│   ├── payroll-implementation-map.md
│   ├── payroll-user-stories.md
│   └── payroll-acceptance-matrix.md
└── packages/ui/src/navigation/model.ts          # payroll added as 6th top-level module
```

## Required CTOX Context

Every payroll element renders the standard contract:

```html
data-context-module="payroll"
data-context-submodule="<runs|payslips|master|additionals|audit>"
data-context-record-type="payroll_run | payroll_payslip | payroll_payslip_line | payroll_period | payroll_component | payroll_structure_assignment | payroll_additional"
data-context-record-id="<id>"
data-context-label="<human label>"
data-context-skill="product_engineering/business-basic-module-development"
```

Prompt CTOX payload schema is documented in the RFC and exercised by the smoke script (`ctoxPayload.module === "payroll" && ctoxPayload.submodule === "runs"`).

## Required Proof Commands

```sh
pnpm --filter @ctox-business/payroll test         # engine + DSL unit tests
pnpm --filter @ctox-business/payroll typecheck    # package strictness
pnpm --filter @ctox-business/web typecheck        # web app type integrity
pnpm --filter @ctox-business/web exec node apps/web/scripts/payroll-smoke.mjs
```

Browser proof against running dev server (M1 gate, not M0):

```sh
CTOX_BUSINESS_BASE_URL=http://localhost:3001 pnpm test:payroll
```
