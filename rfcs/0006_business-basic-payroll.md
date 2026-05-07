# RFC 0006: Business Basic Payroll

Status: Research and RFC complete, M0 not started
Target module: `payroll`
German UI label: `Lohnabrechnung`
Created: 2026-05-07

## OSS Evidence Summary

Research notes: `templates/business-basic/docs/payroll-oss-implementation-notes.md`

Three real implementations were cloned and read under `runtime/research/business-basic-payroll/repos`:

| Repo | Evidence used |
|---|---|
| `frappe/hrms` | Salary Component, Salary Structure, Salary Structure Assignment, Payroll Entry, Salary Slip, Payroll Period, Additional Salary, Arrear, Salary Withholding, Income Tax Slab, Salary Slip Timesheet — full DocType schema, status fields, validate/on_submit/on_cancel transitions, calculate_net_pay/calculate_component_amounts methods, test files. |
| `OCA/payroll` (+ `payroll_account`) | hr.payslip / hr.payslip.run / hr.payslip.line / hr.payslip.input / hr.payslip.worked_days / hr.salary.rule / hr.payroll.structure / hr.contract — state machines, condition_select and amount_select rule modes, compute_sheet, action_payslip_done/cancel, journal posting via payroll_account, full test coverage of state transitions and rule evaluation. |
| `orangehrm/orangehrm` | PayGrade, PayGradeCurrency, EmployeeSalary, PayPeriod entities; Doctrine ORM schema and DAO/Service/API tests. Confirms that an OSS HR product can ship without an actual payroll engine — payslip generation is enterprise-only. |

The common implementation pattern across the three:

- master data: paygrade or salary structure with components/rules; per-employee assignment with from-date and currency
- source data: worked days, leave-without-pay, absent days, plus optional additional salary, arrears, tax slabs, withholding
- run: a periodic batch selects employees for a period and frequency, generates one slip per employee, and tracks Queued/Submitted/Failed independently from each slip's own lifecycle
- slip: per-employee document with earnings/deductions detail rows, gross/total_deduction/net, immutable once posted
- posting: on submit, slip creates a journal entry against the payroll-payable account using each component's account mapping
- time linkage: timesheet rows can drive earnings, or the slip can stand alone with worked_days only

## Business Basic Scope

Payroll computes wages for employees over a period and posts the resulting journal entry to the existing accounting ledger.

In scope:

- maintain salary components, salary structures, and per-employee structure assignments
- run payroll for a period and frequency, materializing one payslip per selected employee
- compute earnings and deductions per component, including fixed amounts, percentage of base, and small DSL formulas
- record gross / total deductions / net per slip
- review, edit, post, and cancel payslips
- post a journal entry per submitted payslip via the existing `accounting` posting surface
- expose payslip and run records as right-clickable, CTOX-promptable items
- expose approved time entries from `operations/workforce` as a read-only source for hourly components (M1)

Out of scope:

- Lohnsteueranmeldung, SV-Meldungen, AAG, ELStAM, U1/U2 country-specific German payroll filings (deferred to a `payroll-de` country pack)
- Bank file generation / SEPA payment runs (handled later via `business/payments`)
- Gratuity, retention bonus, salary withholding, employee benefit applications (frappe-style enterprise concerns)
- Multi-currency assignments per employee (single base currency per company in M0/M1)
- Cross-period arrear journal lines and historical re-run
- Leave-encashment math and full attendance integration
- Self-service payslip portal for employees
- Auto-scheduled monthly run; payroll runs are operator-triggered

## Central Object

The central object is `payroll_payslip`.

A payslip is one employee's wage statement for a single payroll period. It is owned by a `payroll_run`, links to a `payroll_structure_assignment`, and contains `payroll_payslip_line` rows for each computed component. The work surface selects payslips and opens detail in a bottom drawer, mirroring the workforce assignment pattern.

## Owned Tables

M0 may implement these as durable JSON/runtime storage first if that matches the existing Business Basic pattern; the API and object names must already match the later Postgres tables.

- `payroll_period` — `id`, `company_id`, `frequency` (`monthly`|`bi-weekly`|`weekly`), `start_date`, `end_date`, `locked`, `created_at`, `updated_at`. Operator generates periods up front; locking blocks new runs.
- `payroll_component` — `id`, `code`, `label`, `type` (`earning`|`deduction`), `taxable`, `depends_on_payment_days`, `account_id` (GL), `formula_kind` (`fix`|`percent_of`|`formula`), `formula_amount` (number, used for `fix`), `formula_base` (component code, used for `percent_of`), `formula_percent` (number, used for `percent_of`), `formula_expression` (string, used for `formula`), `sequence`, `disabled`.
- `payroll_structure` — `id`, `company_id`, `label`, `frequency`, `currency`, `is_active`, `mode_of_payment`, plus an ordered list of `payroll_component` references.
- `payroll_structure_assignment` — `id`, `employee_id`, `structure_id`, `base_salary`, `currency`, `from_date`, `to_date` (nullable), `created_at`, `created_by`. Only one active assignment per employee on any date.
- `payroll_run` — `id`, `company_id`, `period_id`, `frequency`, `status` (`Draft`|`Queued`|`Running`|`Submitted`|`Failed`|`Cancelled`), `selected_employee_ids`, `payable_account_id`, `posting_date`, `error`, `created_by`, `created_at`, `submitted_at`. Unique on (company_id, period_id, frequency) for non-cancelled runs.
- `payroll_payslip` — `id`, `run_id`, `employee_id`, `assignment_id`, `period_id`, `start_date`, `end_date`, `payment_days`, `lwp_days`, `absent_days`, `currency`, `gross_pay`, `total_deduction`, `net_pay`, `status` (`Draft`|`Review`|`Posted`|`Cancelled`|`Withheld`), `journal_entry_id` (nullable), `posted_at` (nullable), `posted_by` (nullable), `notes`. Unique on (run_id, employee_id).
- `payroll_payslip_line` — `id`, `payslip_id`, `component_id`, `sequence`, `type` (`earning`|`deduction`), `qty`, `rate`, `amount`. Frozen once the parent slip is `Posted`.
- `payroll_additional` — `id`, `employee_id`, `period_id`, `component_id`, `amount`, `note`, `applied_to_payslip_id` (nullable). One-off bonuses, deductions, and arrears that the run picks up for the period.
- `payroll_audit` — `id`, `entity_type`, `entity_id`, `from_status`, `to_status`, `actor`, `at`, `note`. Append-only; mirrors `accounting` audit pattern.

## Read-Only Adjacent Data

- Employee master data (currently surfaced through `operations/workforce` person records). Payroll reads name, employee number, bank account ref, tax id, employer-side fields. Payroll does not redefine employee identity.
- `workforce_time_entry` — approved billable/cost entries for a period. Used by hourly components in M1.
- `accounting_account` — chart of accounts. Components and the run's `payable_account_id` reference accounts but do not own them.
- `accounting_period` — fiscal period state. A payroll run may not post to a closed period (mirrors existing `period-close` route guard).

## Outbound Handoffs

- `business/ledger` — on `payroll_payslip` post, an accounting journal entry is created via the existing `packages/accounting` posting surface. One JE per slip; lines split per component → component.account_id; net contra → run.payable_account_id.
- `business/bookkeeping` — DATEV export consumes the resulting journal entries unchanged. No payroll-specific export in M0/M1.
- `business/payments` — net pay creates a bank-payment proposal. Deferred until `business/payments` exists as a real module.
- `operations/workforce` — payslip references source `workforce_time_entry` ids when the structure includes hourly components (M1).
- `ctox/sync` — every status transition emits a sync event with a deep link to the payslip or run, matching the existing `sync_event` pattern in `packages/db`.

## State Model

`payroll_run`:

```
Draft → Queued → Running → Submitted
                       ↘  Failed (recoverable; can re-run individual failed slips)
            ↘  Cancelled
```

- `Draft`: operator picks period, frequency, structure filter, employee filter, payable account.
- `Queued`: run accepted; runtime begins materializing slips.
- `Running`: slip generation in progress (advisory only; a single tick may transition straight from Queued to Submitted in M0 single-tenant runs).
- `Submitted`: all slips materialized; each is independently editable/postable. The run itself is then a stable container.
- `Failed`: at least one slip generation step failed. The run carries the error; failed slips can be re-run individually.
- `Cancelled`: terminal. All child slips become `Cancelled`. A new run can then be created for the same period.

`payroll_payslip`:

```
Draft → Review → Posted
        ↘  Withheld
        ↘  Cancelled
```

- `Draft`: just generated by a run; lines computed but no review.
- `Review`: an operator has reviewed the slip and marked it ready to post.
- `Posted`: journal entry exists in `business/ledger`; slip and lines are immutable.
- `Withheld`: slip is held back from this run's post (e.g. dispute, missing data). Stays in the run; can move back to `Review` in the same period or be `Cancelled`.
- `Cancelled`: terminal. If `Posted`, cancellation requires a reversing journal entry created via `accounting` (mirrors the existing journal cancel flow).

## Commands and Mutations

All commands route through the existing Business Basic CTOX bridge as queue-backed actions, mirroring how `operations` mutations work today.

- `POST /api/payroll/components` — create/update/disable a `payroll_component`.
- `POST /api/payroll/structures` — create/update a `payroll_structure` and its component refs.
- `POST /api/payroll/structure-assignments` — create/end a `payroll_structure_assignment` for an employee.
- `POST /api/payroll/periods` — create/lock a `payroll_period`.
- `POST /api/payroll/additionals` — create/update/delete a `payroll_additional` row.
- `POST /api/payroll/runs` — create a run in `Draft`, queue a `Draft` run, cancel a run.
- `POST /api/payroll/runs/:id/recompute` — recompute a `Failed` or `Submitted` run's slips that are still in `Draft`.
- `POST /api/payroll/payslips/:id` — edit `Draft`/`Review` slip, transition Draft↔Review, mark Withheld, post, cancel.
- `GET  /api/payroll` — list runs with status and totals.
- `GET  /api/payroll/runs/:id` — full run with slips and totals.
- `GET  /api/payroll/payslips/:id` — full slip with lines and source data.
- `GET  /api/payroll/components` — component master data.
- `GET  /api/payroll/structures` — structure master data with assignments.

Mutation actions allowed (the same five-verb pattern as Operations elsewhere):

```
create | update | delete | post | cancel
```

`post` is the explicit ledger handoff; `cancel` on a `Posted` slip requires a `reversal` reason and creates a reversing journal entry.

## Idempotency and Audit

- `payroll_run` is keyed by (company_id, period_id, frequency); creating a duplicate non-cancelled run for the same key is rejected.
- `payroll_payslip` is keyed by (run_id, employee_id); each slip is generated exactly once per run.
- Slip → journal posting is idempotent on (slip_id): a re-post on a `Posted` slip is a no-op and returns the existing `journal_entry_id`.
- Every state transition writes a row in `payroll_audit` with `actor`, `at`, `from_status`, `to_status`, optional `note`. The audit trail is what `Cancel` reversal entries reference.
- The runtime never edits `Posted` slip rows or lines. Corrections happen via `payroll_additional` in a later period (frappe Arrear pattern, retitled `Nachzahlung` in UI).

## API and Runtime Contract

- All POST routes accept the existing `{ action, payload }` envelope used by `operations` and queue the intent through CTOX.
- Server-side runtime under `templates/business-basic/apps/web/lib/payroll-runtime.ts` holds JSON-backed state for M0; the same shape moves to Postgres in `packages/db/src/schema.ts` when the schema lands.
- Computation lives in `packages/payroll/src` (a new package) with no UI imports. The Next.js API routes call into the package; the package returns plain DTOs.
- Component formulas are evaluated by a small parser in `packages/payroll/src/formula.ts`. Inputs are `base_salary`, `payment_days`, `lwp_days`, plus already-computed component codes. No host access. No `eval`. No `Function`.

## UI Work Surface

`/app/payroll` follows the standard four-zone Business OS layout:

- **Left intake/master** — accordion: Periods, Structures, Components, Assignments, Additionals.
- **Center workbench** — the run table for the current company:
  - row per `payroll_run` with period, frequency, employee count, totals, status badge
  - drill-in renders the slip table for a selected run: row per `payroll_payslip` with employee, structure, gross, deductions, net, status badge
  - explicit action buttons: `Run starten`, `Slips neu berechnen`, `Run absenden`, `Run abbrechen`
- **Right inspector** — selected payslip detail: header, computed earnings, computed deductions, totals, journal-entry link if posted, audit timeline.
- **Bottom drawer** — slip line editor for `Draft`/`Review` slips: inline-edit qty/rate/amount with formula evaluation preview.

Slip detail is reached only through drawer/inspector — no deep navigable record pages.

## Right-Click Actions

- on `payroll_run`: `Slips neu generieren`, `Slips neu berechnen`, `Run absenden`, `Run abbrechen`, `Prompt CTOX`.
- on `payroll_payslip`: `Bearbeiten`, `Markiere zur Prüfung`, `Markiere zurückgestellt`, `Buchen`, `Stornieren`, `Prompt CTOX`.
- on `payroll_payslip_line`: `Komponente anzeigen`, `Wert überschreiben`, `Prompt CTOX`.
- on `payroll_structure_assignment`: `Beenden`, `Duplizieren`, `Prompt CTOX`.
- on `payroll_component`: `Bearbeiten`, `Deaktivieren`, `Prompt CTOX`.

Every right-clickable element exposes `data-context-*` per the Business Basic contract.

## CTOX Prompt Payload

Every payroll element renders:

```html
data-context-module="operations"
data-context-submodule="payroll"
data-context-record-type="payroll_run | payroll_payslip | payroll_payslip_line | payroll_structure_assignment | payroll_component"
data-context-record-id="<id>"
data-context-label="<human label, e.g. 'Lohnlauf 2026-04 monatlich' or 'Lohnabrechnung Müller 2026-04'>"
data-context-skill="product_engineering/business-basic-module-development"
```

Prompt CTOX payload schema:

```json
{
  "prompt": "<allowed action> for <record label>",
  "action": "review | post | cancel | recompute | extend-formula | explain | reconcile",
  "items": [
    {
      "moduleId": "operations",
      "submoduleId": "payroll",
      "recordType": "payroll_payslip",
      "recordId": "<id>",
      "label": "<label>",
      "href": "/app/payroll?recordId=<id>&drawer=right",
      "skill": "product_engineering/business-basic-module-development"
    }
  ]
}
```

## M0 Scope

M0 is one end-to-end payroll workflow:

1. Operator creates a `payroll_period` (monthly, current month), one `payroll_structure` with three components (`base`, `social_employee`, `tax_employee`), and one `payroll_structure_assignment` for two seeded employees.
2. Operator creates a `payroll_run` for that period and queues it.
3. Runtime materializes two `payroll_payslip` rows in `Draft` with computed earnings and deductions.
4. Operator opens a slip in the bottom drawer, reviews, and transitions it to `Review`, then posts it.
5. Posting creates a journal entry through the existing `accounting` posting surface; the slip moves to `Posted` with `journal_entry_id` populated.
6. Reload preserves all state.
7. Right-click on the slip exposes `Buchen`, `Stornieren`, `Prompt CTOX`. The CTOX payload contains module, submodule, record type, id, label.
8. The operations smoke script `apps/web/scripts/payroll-smoke.mjs` exercises create-period → create-structure → create-assignment → run → review → post → ledger-readback.
9. Browser proof verifies route render, slip click, drawer open, right-click, post action, persistence on reload.

M0 does not require: time-entry-driven hourly components, multi-period runs, country tax tables, additional/arrear handling, withheld state, multi-currency.

## M1 Scope

M1 is the smallest usable payroll module:

- Component formula DSL with `fix`, `percent_of`, and `formula` working including precedence and component‑reference resolution.
- `payroll_additional` (one-off bonus/arrear/deduction) picked up by the next run.
- `Withheld` state available from the slip context menu and visible in the workbench.
- Hourly components: structure references a `workforce_activity_type`; run pulls approved `workforce_time_entry` rows for each employee in the period and folds them into the relevant component.
- `Cancelled` after `Posted` creates a reversing journal entry through `accounting` and updates `payroll_audit`.
- Period locking blocks new runs and edits to slips inside that period.
- Run failure path: a slip that fails computation (e.g. missing assignment) is marked `Failed` on the run aggregate; operator can fix the cause and recompute just that slip.
- Bookkeeping export verified: posted payroll journal entries appear in DATEV export with correct accounts.
- Re-test of US‑01/02/11/31/36 from the user-stories file.

## Rejected OSS Patterns

- **Python-eval rule formulas** (frappe `salary_component.condition/formula`, OCA `amount_python_compute`). Replaced by a parser-backed typed DSL with no host access.
- **Slip in-place edit after post** (frappe). Posted slips are immutable; corrections go via `payroll_additional` in a later period.
- **Verify-state without recorded approver** (OCA `state = verify`). Business Basic uses `Review` and records the approver actor and timestamp.
- **Pay-grade-only model** (OrangeHRM OSS edition). Insufficient — Business Basic owns components, structures, and the actual payslip computation.
- **Country tax in core** (frappe `taxable_salary_slab`, `employee_tax_exemption_*`). Country specifics belong in a country pack, not core.

## Deferred OSS Patterns

- Gratuity, retention bonus, salary withholding object, employee benefit applications/claims (frappe).
- Leave-encashment math, full LWP-from-leave-application, full attendance integration.
- Bank file generation (SEPA Lohnzahlung) — through `business/payments` once it exists.
- Multi-currency salary assignments per employee.
- Auto-scheduled monthly run; for now operator-triggered only.
- Self-service portal for employees viewing their own slips.
- Cross-period arrear journal entries beyond `payroll_additional`.

## Acceptance Evidence

Implementation may claim M0 only after:

- `/app/payroll` route renders in the browser.
- Period, structure, assignment, and run can be created from the UI and persist after reload.
- Run materializes one slip per assigned employee with computed `gross_pay`, `total_deduction`, `net_pay`.
- Slip post creates a journal entry in `business/ledger` and `payroll_payslip.journal_entry_id` is populated.
- Slip and run state transitions are recorded in `payroll_audit` with actor and timestamp.
- Right-click menu opens on run, slip, and slip line; menu includes `Prompt CTOX`.
- CTOX payload contains `operations`/`payroll`/record-type/id/label.
- `apps/web/scripts/payroll-smoke.mjs` exits 0.
- Browser proof verifies click, right-click, mutation, post, reload, ledger readback.
- M0 acceptance matrix entries (US-01, US-02, US-11, US-31, US-36) are `done` with file references.

Implementation may claim M1 only after:

- M0 evidence still passes after M1 changes (early-story regression check).
- Hourly component path picks up `workforce_time_entry` rows and reflects them in slip lines.
- `payroll_additional` flow tested end-to-end: create additional → run picks it up → slip line appears → post → JE includes it.
- `Withheld` and `Cancelled (after Posted)` flows tested with reversal JE.
- DATEV export contains payroll JEs with expected accounts.
- All 50 paired user stories exist in `templates/business-basic/docs/payroll-user-stories.md`.
- `templates/business-basic/docs/payroll-acceptance-matrix.md` has no core `missing` / `partial` / `needs proof` rows.

## Open Decisions

- **Employee identity ownership**: workforce currently holds the operational person record. Payroll needs additional employer-side fields (tax id, social-insurance number, contract type, bank account). Decision: introduce a thin `payroll_employee_profile` table owned by Payroll; the Workforce person record stays the canonical operational identity.
- **Period frequency**: M0 ships monthly only; the structure stores `frequency` so M1 can extend to bi-weekly/weekly without a schema break.
- **Posting account split**: per-component `account_id` is required; the run's `payable_account_id` is the contra. No single lump-sum payable account.
- **Approver model**: a single `Review`-then-`Post` actor is sufficient for M0; multi-stage approval is M1+.

## Implementation Map

To be created at `templates/business-basic/docs/payroll-implementation-map.md` once M0 begins, mirroring the workforce implementation map. Entries point at:

- `templates/business-basic/packages/ui/src/navigation/model.ts` — add `payroll`.
- `templates/business-basic/modules/operations/module.json` — extend `records`/`ctoxSync` with payroll record types.
- `templates/business-basic/packages/payroll/` — new package with formula DSL, component/structure/assignment, run engine, slip computation, posting hook into `accounting`.
- `templates/business-basic/apps/web/lib/payroll-runtime.ts` — durable runtime store for M0.
- `templates/business-basic/apps/web/app/api/payroll/...` — REST surface.
- `templates/business-basic/apps/web/components/payroll-*.tsx` — workbench, slip drawer, line editor, right-click menus.
- `templates/business-basic/apps/web/scripts/payroll-smoke.mjs` — end-to-end smoke.
