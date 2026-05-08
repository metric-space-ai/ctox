# Payroll Implementation Map

Target submodule: `operations/payroll` (matches Workforce pattern; not a top-level nav entry)
Source RFC: [`rfcs/0006_business-basic-payroll.md`](../../../rfcs/0006_business-basic-payroll.md)
Acceptance matrix: [`payroll-acceptance-matrix.md`](payroll-acceptance-matrix.md)
User stories: [`payroll-user-stories.md`](payroll-user-stories.md)
Current status: M0 + M1 implemented. Acceptance matrix: 48 / 50 `done`, 2 `queued` (US‑45 ledger/DATEV cross‑module, US‑47 SEPA — both depend on adjacent modules). Proofs: `pnpm --filter @ctox-business/payroll test` 13/13, `pnpm --filter @ctox-business/payroll-de test` 2/2, `pnpm --filter @ctox-business/web typecheck` (payroll‑side) clean, `pnpm --filter @ctox-business/web test:payroll` 26/26, `pnpm --filter @ctox-business/web test:payroll-browser` 10/10, `next build --webpack` emits `/api/operations/payroll` + `/app/operations/payroll`.

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
| 1 | Register `payroll` as a submodule of `operations` (matches the Workforce pattern; **not** a top-level nav entry). | [`packages/ui/src/navigation/model.ts`](../packages/ui/src/navigation/model.ts) | `/app/operations/payroll` visible in the Betrieb sub-nav as `Lohnabrechnung`. |
| 2 | Extend the Operations module manifest with payroll record types and CTOX sync events (no separate `modules/payroll/`). | [`modules/operations/module.json`](../modules/operations/module.json) | `records[]` includes `payroll_periods`/`_components`/`_structures`/`_runs`/`_payslips`/…; `ctoxSync[]` includes `payroll_run_events`/`payroll_payslip_events`. |
| 2b | Reuse the existing Workforce CSS contract (`.wf2-shell`, `.wf2-rail`, `.wf2-board`, `.wf2-side-drawer`, `.wf2-bottom`). | [`packages/ui/src/theme/theme.css`](../packages/ui/src/theme/theme.css), `apps/web/app/globals.css` (`.wf2-…` block) | Operations module accent applies; no separate `[data-module="payroll"]` rule needed because Payroll is an Operations submodule. |
| 3 | Self-contained engine package with formula DSL, computation, and journal builder; unit tests for both. | [`packages/payroll/`](../packages/payroll/) | `pnpm --filter @ctox-business/payroll test` → 13 / 13 green. |
| 4 | Durable JSON-backed runtime with seed (1 period, 3 components, 1 structure, 2 employees, 2 assignments). | [`apps/web/lib/payroll-runtime.ts`](../apps/web/lib/payroll-runtime.ts) | GET `/api/operations/payroll` returns seed snapshot. |
| 5 | REST surface with `command + payload` envelope. | [`apps/web/app/api/operations/payroll/route.ts`](../apps/web/app/api/operations/payroll/route.ts) | Dispatcher accepts the documented commands and rejects unknown ones. |
| 6 | Workbench follows the Workforce layout: left rail (Eingang/Lohnläufe & Stamm + "Verwalten"), central main view (Status-Kanban for the selected run), right rail (Prüfung & Buchung + "Übergaben"); selected slip opens a `BottomPayslipDrawer`; master data + handoff in side-drawers. | [`apps/web/components/payroll-workbench.tsx`](../apps/web/components/payroll-workbench.tsx), [`apps/web/components/operations-workspace.tsx`](../apps/web/components/operations-workspace.tsx) | Route `/app/operations/payroll` renders the workbench; `OperationsWorkspace` dispatches `submoduleId === "payroll"`. |
| 7 | `data-context-*` attributes on every clickable record (run, slip, line). | `payroll-workbench.tsx` | Right-click `Prompt CTOX` payload contains module/submodule/recordType/recordId/label. |
| 8 | Smoke script exercises load → run → review → post → ledger → reload → audit → idempotent re-post. | [`apps/web/scripts/payroll-smoke.mjs`](../apps/web/scripts/payroll-smoke.mjs) | `pnpm test:payroll` exits 0 against running dev server. |
| 9 | Web typecheck passes. | `pnpm --filter @ctox-business/web typecheck` | No errors. |

## M1 Status

All M1 areas listed below are implemented and proven by `pnpm test:payroll` (26 / 26) plus `pnpm test:payroll-browser` (10 / 10) plus engine + payroll-de unit tests (15 / 15). Two stories remain `queued` (US‑45, US‑47) because they depend on adjacent `business/ledger` and `business/payments` surfaces.

| Area | Implementation | Proof |
|---|---|---|
| Formula DSL component create form | `Komponente anlegen` form with formulaKind switch (fix / percent_of / formula); validates expression via the typed parser at submit | `payroll` engine unit (parser, eval) + workbench form |
| Additionals | `create_additional` / `delete_additional` runtime cmds; create + delete UI in inspector; auto‑recompute after create | smoke + workbench |
| Withheld + return | `mark_payslip_withheld`, `mark_payslip_review` (Draft|Withheld → Review), `mark_payslip_draft` (Review|Withheld → Draft); buttons in slip row | smoke step 13 + step 19 |
| Reversal | `cancel_payslip` on `Posted` writes a reversal journal in `postedJournals` (debit/credit inverted, balanced); reversal id rendered in inspector | smoke step 14 (balanced reversal) |
| Period lock | `lock_period` flips `period.locked = true`; `create_run` rejects with `period_locked` | smoke step 15 |
| Hourly Workforce path | `additionalsWithWorkforce` reads `workforce.payrollCandidates` for the period; `ensureWorkforcePayrollComponent` + structure normalize guarantee `pc-workforce-hours` exists; engine produces `workforce_hours` line with `hours × hourlyRate` | smoke step 4 (`workforce_hours` line, amount > 0) |
| Bulk Review / Bulk Post | `bulk_mark_review`, `bulk_post_run`; per-slip post failures isolated; run-header buttons | smoke step 20 |
| Negative-net guard | `post_payslip` rejects with `negative_net_pay_blocks_post`; UI button disabled with tooltip | runtime guard + workbench |
| Component-in-use guard | `delete_component` rejects with `component_in_use` if active structure references it | smoke step 22 |
| Component disable | `update_component { disabled }` and recompute drops the line | runtime + workbench |
| Country pack DE | `packages/payroll-de` with components / structure / `installIntoSnapshot`; `install_country_pack` runtime cmd; UI install button | `payroll-de` unit + smoke step 24 |
| DSL feasibility for ESt 2026 | `rfcs/0007_payroll-de-dsl.md` (Option B: typed table-lookup + `pow`) | RFC committed |
| Period-over-period read | `GET /api/operations/payroll?view=comparison&employeeId=…&periods=…` returns rows + grossDeltas; `PeriodComparisonPanel` renders | smoke step 25 |
| CSV export | `GET /api/operations/payroll?view=export&periodId=…` streams CSV with columns `employee_id,employee_name,gross,deductions,net,journal_id,status` | smoke step 26 |
| Propose-via-CTOX | `propose_additional_via_ctox` emits `queueProposal=…` event note without mutating additionals | smoke step 23 |
| 50 paired user stories | `payroll-user-stories.md` — every Manual + CTOX block uses the Skill-prescribed bullet structure | doc gate |
| Acceptance matrix | 48 done / 2 queued / 0 partial / 0 needs proof / 0 missing | doc gate |
| Browser proof | `payroll-browser-proof.mjs` (puppeteer-core + system Chrome) covers route render, click, real right-click, Prompt CTOX visible, reload-preserves-snapshot | `pnpm test:payroll-browser` |
| §11 Queue at normal priority | 2 remaining tasks (US‑45, US‑47) with `--priority normal --skill product_engineering/business-basic-module-development --thread-key business-basic/payroll` | `ctox queue list` |

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
├── modules/operations/module.json   # records[] + ctoxSync[] include payroll_*
├── apps/
│   └── web/
│       ├── lib/payroll-runtime.ts
│       ├── components/payroll-workbench.tsx     # `.wf2-…` rails + kanban + drawers
│       ├── components/operations-workspace.tsx  # dispatches submoduleId === "payroll"
│       ├── app/api/operations/payroll/route.ts  # REST surface (commands + view=comparison + view=export)
│       └── scripts/
│           ├── payroll-smoke.mjs
│           ├── payroll-browser-proof.mjs
│           └── payroll-screenshot.mjs
├── docs/
│   ├── payroll-oss-implementation-notes.md
│   ├── payroll-implementation-map.md
│   ├── payroll-user-stories.md
│   └── payroll-acceptance-matrix.md
└── packages/ui/src/navigation/model.ts          # operations.submodules.payroll (Lohnabrechnung)
```

## Required CTOX Context

Every payroll element renders the standard contract:

```html
data-context-module="operations"
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
