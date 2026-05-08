# Business Basic Payroll - OSS Implementation Notes

Module candidate: `payroll`
German UI label: `Lohnabrechnung`
Scope tested: salary structure setup, periodic payroll run, per‑employee payslip generation, posting to ledger, payslip lifecycle and approvals.

## Local CTOX Baseline

| Area | Evidence read | Finding | Decision |
|---|---|---|---|
| Workforce boundary | `rfcs/0005_business-basic-workforce.md` lines 40-50 and 440-460 | Workforce explicitly lists `payroll calculation`, `external payroll export`, and `Payroll/payslip states` as out of scope and deferred. | Payroll is its own module. It does not live inside Workforce, but consumes Workforce time and Employee master data. |
| Navigation | `templates/business-basic/packages/ui/src/navigation/model.ts` | `operations` already hosts `workforce` (`Einsatzplanung`). No `payroll`. | Add `payroll` as the next operations submodule. Same human-management cluster as workforce; ledger handoff goes to `business/ledger` and bookkeeping export goes to `business/bookkeeping`. |
| Accounting handoff | `templates/business-basic/packages/accounting/src/{posting,export,workflow}` | `accounting` already owns journal posting, period close, and DATEV export. | Payroll posts a journal entry per payroll run via the existing accounting posting surface. It does not own its own ledger. |
| Bookkeeping export | `templates/business-basic/packages/accounting/src/export` | DATEV export is the bookkeeping handoff today. | Payroll registers payroll-line journal entries as standard journal entries; DATEV export picks them up automatically. A separate Lohnsteueranmeldung/SV export is M1+, not M0. |
| Workforce billable handoff | `rfcs/0005_business-basic-workforce.md` Out of scope | Approved billable time goes to `business/invoices`. | Approved working/overtime time goes to `payroll` for cost-side wage calculation. The same `workforce_time_entry` source therefore feeds two handoffs: invoice (billable) and payroll (cost). |

## OSS Repositories Read

| Repo | Files read | Objects | Tables/schema | States | Transitions | APIs/services | UI pattern | Tests | Business Basic decision |
|---|---|---|---|---|---|---|---|---|---|
| `frappe/hrms` | Opened with content extraction: `hrms/payroll/doctype/salary_slip/salary_slip.json` (status options + first ~50 field names), `hrms/payroll/doctype/payroll_entry/payroll_entry.json` (status options + first ~45 field names), `hrms/payroll/doctype/salary_structure/salary_structure.json` (full field list), `hrms/payroll/doctype/salary_component/salary_component.json` (first 40 field names), `hrms/payroll/doctype/salary_slip/salary_slip.py` (method signatures via rg). Directory-listed only (not opened): `additional_salary`, `arrear`, `payroll_period`, `salary_withholding`, `income_tax_slab`, `salary_slip_timesheet`, `salary_structure_assignment`, `gratuity*`, test files. | Read directly: `Salary Component`, `Salary Structure`, `Payroll Entry`, `Salary Slip`. Inferred from directory listing only: `Salary Structure Assignment`, `Payroll Period`, `Additional Salary`, `Arrear`, `Salary Withholding`, `Income Tax Slab`, `Salary Slip Timesheet`. | Salary Component fields incl. `formula`, `condition`, `amount_based_on_formula`, `amount`, `is_tax_applicable`, `depends_on_payment_days`, `accounts`. Salary Structure fields incl. `payroll_frequency`, `mode_of_payment`, `currency`, `earnings`, `deductions`, `account`, `payment_account`. Payroll Entry fields incl. `posting_date`, `payroll_frequency`, `start_date`, `end_date`, `employees`, `payroll_payable_account`, `status`, `error_message`. Salary Slip fields incl. `start_date`, `end_date`, `payment_days`, `gross_pay`, `total_deduction`, `net_pay`, `journal_entry`, `payroll_entry`, `salary_structure`, `timesheets`, `earnings`, `deductions`. | Salary Slip status (read directly): `Draft`, `Submitted`, `Cancelled`, `Withheld`. Payroll Entry status (read directly): `Draft`, `Submitted`, `Cancelled`, `Queued`, `Failed`. | From `salary_slip.py` rg method list: `validate`, `on_submit`, `on_cancel`, `set_status`, `calculate_net_pay`, `calculate_component_amounts`, `compute_taxable_earnings_for_year`, `calculate_variable_tax`, `compute_year_to_date`, `validate_dates`, `calculate_lwp_or_ppl_based_on_leave_application`. | Inferred from method names + DocType convention only — Python document methods on Frappe DocType, sandboxed Python eval for component formulas. Not directly read. | Inferred from directory listing of `payroll_dashboard`, `dashboard_chart`, `number_card`, `print_format` — not directly read. | Existence confirmed (`hrms/payroll/doctype/salary_slip/test_salary_slip.py`, `payroll_entry/test_payroll_entry.py`); test bodies not read. | Adopt Component → Structure → Assignment → Run → Payslip object chain. Use the same `Draft/Submitted/Cancelled` slip workflow plus a `Withheld` hold state. Use `Draft/Queued/Submitted/Failed/Cancelled` for the batch run. Reject component-formula Python eval as the M0/M1 path; use a small typed expression DSL instead. |
| `OCA/payroll` | Opened with content extraction: `payroll/models/hr_payslip.py` (state field, `action_payslip_done`, `action_payslip_cancel`, `compute_sheet` references via rg + sed), `payroll/models/hr_payslip_run.py` (state field), `payroll/models/hr_salary_rule.py` (`condition_select`, `condition_range_*`, `amount_select`, `amount_fix`, `amount_percentage`, `amount_percentage_base`, `amount_python_compute` defaults via sed). Directory-listed only (not opened): `hr_payslip_line.py`, `hr_payslip_input.py`, `hr_payslip_worked_days.py`, `hr_payroll_structure.py`, `hr_contract.py`, `hr_contribution_register.py`, `payroll_account/models/*`, all `payroll/tests/*`. | Read directly: `hr.payslip`, `hr.payslip.run`, `hr.salary.rule`. Inferred from imports/file names only: `hr.payslip.line`, `hr.payslip.input`, `hr.payslip.worked_days`, `hr.payroll.structure`, `hr.contract`, `hr.contribution.register`, `hr.salary.rule.category`. | Payslip period fields `date_from`/`date_to` and `state` confirmed in source. Rule `condition_select` options `none/range/python` and `amount_select` options `percentage/fix/code` confirmed in source plus `amount_python_compute` Python source default. Structure tree, contract wage linkage inferred from imports only. | Payslip state (read directly): `draft`, `verify`, `done`, `cancel`. Payslip Run state declared via `state = fields.Selection(...)` confirmed but option list not extracted. | From source: `compute_sheet` (re-evaluates lines), `action_payslip_done`, `action_payslip_cancel`. Inferred only: `refund_sheet`, batch run computes children. | Inferred from Odoo conventions only — `payroll_account` extension creates account moves on payslip done; not directly read. | Not read; UI patterns inferred from Odoo conventions. | Test files listed (`test_payslip_flow`, `test_hr_salary_rule`, `test_hr_payslip_change_state`, `test_hr_payroll_cancel`, `test_hr_payslip_worked_days`); bodies not read. | Adopt rule split into `condition` and `amount`. Replace the Python-eval rule with a typed DSL (`fix`, `percent_of`, `formula(<typed-expression>)`). Adopt the four-state slip lifecycle as `Draft → Verify → Done → Cancel` (mapped to `Draft → Review → Posted → Cancelled`). Use a separate `payroll_run` state machine for the batch. |
| `orangehrm/orangehrm` | Opened with content extraction: `src/plugins/orangehrmAdminPlugin/entity/PayGrade.php` (head 60), `src/plugins/orangehrmPimPlugin/entity/EmployeeSalary.php` (head 80). Directory-listed only (not opened): `PayGradeCurrency.php`, `PayGradeDao.php`, `PayGradeAPI.php`, `PayPeriod.php`, `EmployeeSalaryDao.php`, `EmployeeSalaryComponentAPI.php`, `SavePayGradeController.php`, `PayGradeService.php`, all DAO/Service/API tests, fixture YAMLs. | Read directly: `PayGrade` (with `name`, `payGradeCurrencies` collection), `EmployeeSalary` (with `Employee`, `PayGrade`, `CurrencyType`, `amount`, `PayPeriod` joins). Inferred from file names only: `PayGradeCurrency`, `PayPeriod`, `CurrencyType`, `Employee`. | From source: `ohrm_pay_grade` table, `hs_hr_emp_basicsalary` table with `emp_number`, `sal_grd_code`, `currency_id`, `ebsal_basic_salary`. Other table names (`ohrm_pay_grade_currency`, `ohrm_pay_period`) inferred from entity names but column lists not read. | Not read. Asserting "no payroll-run state in OSS" is an inference from the absence of a payslip entity in the file listing, not from positive evidence. | CRUD only; this is an inference from the entity-only files I opened. | Doctrine ORM annotations confirmed in opened entities. Symfony controller / DAO / REST API are inferences from PSR-style filenames only — files not opened. | Inferred only. | Test files listed via `find` (DAO/API/Service/fixture); bodies not read. | Adopt the `PayGrade` master-data table (min/max bracket per currency) and the `EmployeeSalary` per-employee fixed-wage record as a baseline. Confirms that a real OSS HR product can ship without an OSS payroll engine — Business Basic must therefore implement the engine in `packages/payroll` and not assume a vendor library. |

## Cross-Implementation Evidence

The shared payroll model across the three repos:

- **Master data**: paygrade/structure with components/rules; per-employee assignment with from-date and currency.
- **Source data**: worked days, leave-without-pay, absent days, additional salary, arrears; in mature systems also tax slabs, withholding, contracts.
- **Run**: a periodic batch (`payroll_entry` / `hr.payslip.run`) selects employees for a payroll period and frequency, generates one slip per employee, and tracks `Queued/Submitted/Failed` separately from each slip.
- **Slip**: per-employee document with earnings/deductions detail lines, gross, deductions, net; states Draft → (Verify/Review) → Submitted/Done → optional Cancelled/Withheld; immutable once posted.
- **Posting**: on submit, slip creates a journal entry against the payroll-payable account using each component's account mapping. Mature stacks (frappe/hrms, OCA payroll_account) have a clear "post to ledger" hook that Business Basic must mirror.
- **Time linkage**: timesheet rows can drive earnings (frappe `salary_slip_timesheet`), or the slip can stand alone with worked_days only (OCA worked_days_line_ids).
- **Country tax**: large area outside the engine itself; tax tables, slabs, exemptions and statutory contributions are pluggable in frappe (`income_tax_slab`, `employee_tax_exemption_*`) and country-localized in OCA (separate `l10n_*` modules).

## Business Basic Decisions

1. **Module placement**: create `payroll` (German label `Lohnabrechnung`). Same human-management cluster as `operations/workforce`. Posting handoff goes to `business/ledger`; bookkeeping export reuses `business/bookkeeping`.

2. **Central object**: `payroll_payslip`. The payslip is the operator's working unit (one per employee per period). The work surface selects payslips and opens detail in a bottom drawer, mirroring the workforce assignment pattern.

3. **Owned tables (logical)**:
   - `payroll_period` (frequency, start/end, locked flag)
   - `payroll_component` (id, label, type=earning|deduction, taxable, depends_on_payment_days, account_id, formula_kind, formula)
   - `payroll_structure` (id, label, currency, frequency, component refs with order)
   - `payroll_structure_assignment` (employee_id, structure_id, base_salary, currency, from_date, to_date)
   - `payroll_run` (period_id, frequency, status, employee filter, totals, error)
   - `payroll_payslip` (run_id, employee_id, period, payment_days, lwp_days, absent_days, gross, total_deduction, net, currency, status, journal_entry_id, posted_at)
   - `payroll_payslip_line` (payslip_id, component_id, sequence, type, qty, rate, amount)
   - `payroll_additional` (employee_id, period_id, component_id, amount) — bonuses/arrears/one-off

4. **Read-only adjacent data**: `employee` (via Workforce/master data), `workforce_time_entry` and `workforce_assignment` for time-driven components, `accounting_account` for component → GL account mapping.

5. **Outbound handoffs**:
   - `business/ledger` — journal entry created on payslip post (gross/deduction/net split per component → GL account).
   - `business/bookkeeping` — DATEV export picks the journal entries up via the existing accounting export surface.
   - `business/payments` — net pay creates a bank-payment proposal (M1+).
   - `operations/workforce` — payslip references the source `workforce_time_entry` rows (read-only attachment).

6. **State models**:
   - `payroll_run`: `Draft → Queued → Running → Submitted` with branch states `Failed` (recoverable) and `Cancelled`.
   - `payroll_payslip`: `Draft → Review → Posted → Cancelled`. Optional hold state `Withheld` for individual slips that must not post with the rest of the run.
   - Once `Posted`, a payslip and its lines are immutable. Corrections happen via a new payslip in a later run (frappe `Salary Withholding`/`Arrear` pattern), not in‑place edits.

7. **Component formula language**: reject Python‑eval (frappe and OCA both expose this and it is unsafe). Use a typed DSL with three forms only:
   - `fix(<amount>)` — fixed amount
   - `percent_of(<base-component-or-base-salary>, <pct>)` — percentage of a named base
   - `formula(<expression-over-named-components>)` — small grammar with `+ - * /`, parentheses, numeric literals, and identifiers that resolve to other components or to whitelisted variables (`base_salary`, `payment_days`, `lwp_days`).
   This matches the OCA `fix/percentage/code` split but replaces `code` with a parser + evaluator that has no host access.

8. **Run drives slip generation**: `POST /api/operations/payroll/runs` queues a run; the runtime materializes one `payroll_payslip` per selected employee and computes lines. Slip rows persist and are individually editable in `Draft`/`Review`. Posting is per-run (atomic per slip; failures move the slip to `Failed`, not the whole run).

9. **Time → wage path**: M0 pulls only `payment_days` and `lwp_days` from a fixed monthly period; M1 ties to approved `workforce_time_entry` for hourly components (mirrors frappe `salary_slip_timesheet`).

10. **Country/tax**: M0 ships a generic earning/deduction model with fixed‑amount and percentage components (e.g. employer pension contribution at X%). German Lohnsteuer/SV brackets, AAG, and Lohnsteueranmeldung exports are M1+ as a country pack `payroll-de`, not core M0.

11. **Audit and idempotency**: every state transition is recorded in an audit table with actor/timestamp/from/to. `payroll_run` is keyed by (company, period_id, frequency); duplicate run for the same key is rejected unless previous run is `Cancelled`. Submitted slip → posted journal is idempotent on (slip_id).

## Rejected OSS Patterns

- **Python‑eval rule formulas** (frappe `salary_component.condition/formula`, OCA `amount_python_compute`). Replaced by a parser-backed typed DSL.
- **OrangeHRM "OSS edition has no real payroll" pattern**. Pay grade alone is insufficient — Business Basic must own the engine.
- **frappe per-component country localization in core** (`taxable_salary_slab`, `employee_tax_exemption_*`). M0 stays country-agnostic; localization goes to a country pack.
- **OCA `state = verify` as a separate "approver review" state per slip without an explicit approver.** Business Basic uses `Review` with a recorded approver actor and timestamp, mirroring workforce approval.

## Deferred OSS Patterns

- Gratuity, retention bonus, salary withholding, employee benefit applications/claims (frappe). Useful eventually; not M0/M1.
- Leave-encashment math, full attendance integration, full LWP-from-leave-application calc.
- Bank file generation (SEPA Lohnzahlung). Goes through `business/payments` once that module exists.
- Full multi-currency: M0/M1 single base currency per company; multi-currency assignments come later.
- Cross-period correction with arrear journal lines (frappe `arrear`).

## Open Questions For RFC

- Should the canonical period be calendar month only, or also bi-weekly/weekly? (Workforce timesheets are weekly; bookkeeping is monthly.) → RFC must pick monthly for M0 and leave frequency configurable in the structure for M1.
- Is the employee master data owned by `operations/workforce` or a future `operations/employees`? Workforce currently holds person/availability; Payroll needs employer data, contract type, tax id, bank account. → RFC must declare a thin `payroll_employee_profile` extension owned by Payroll, while Workforce keeps the operational person record.
- Posting target: do payroll journal entries hit a single `payroll_payable` account or split across wage/tax/employer-share accounts? → RFC must require per-component account mapping, with `payroll_payable_account` as the contra used when the slip posts.
