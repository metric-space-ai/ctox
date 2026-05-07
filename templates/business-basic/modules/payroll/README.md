# Payroll Module (`/app/payroll`)

Top-level Business Basic module. Owns the periodic computation of employee wages and the journal-entry handoff to the existing accounting layer.

## Submodules

- `runs` — Lohnläufe (default workbench: run table + slip table + slip inspector + audit)
- `payslips` — Lohnzettel (slip-focused view)
- `master` — Stamm (Perioden, Komponenten, Strukturen, Zuweisungen)
- `additionals` — Zusatzposten (one-off bonuses, arrears, deductions per period)
- `audit` — Audit-Verlauf

## Owned Resources

- `payroll_period` — frequency-bound time window with optional lock
- `payroll_component` — earning/deduction with formula DSL (fix / percent_of / formula) and GL account
- `payroll_structure` — bundle of components per frequency/currency
- `payroll_structure_assignment` — employee → structure with from/to dates and base salary
- `payroll_run` — periodic batch generating one slip per assigned employee
- `payroll_payslip` (+ lines) — central object; once Posted, immutable
- `payroll_additional` — one-off bonus / arrear / deduction picked up by next run
- `payroll_audit` — append-only state-transition log

## Cross-Module Handoffs

- `business/ledger` — every Posted slip writes a balanced journal entry
- `business/bookkeeping` — DATEV export reads the resulting journal entries unchanged
- `business/payments` — net pay drives a SEPA payment proposal (queued, M1+)
- `operations/workforce` — approved time entries feed hourly components (queued, M1+)
- `ctox/sync` — every state transition emits a sync event with deep-link

## Code Layout

```
templates/business-basic/
├── modules/payroll/
│   ├── module.json                # this manifest
│   └── README.md                  # this file
├── packages/payroll/              # bare-engine: types, formula DSL, computation, posting builder
├── apps/web/
│   ├── lib/payroll-runtime.ts     # durable JSON-backed snapshot + command dispatcher
│   ├── components/
│   │   ├── payroll-workspace.tsx  # PayrollWorkspace + PayrollPanel (route-level entry)
│   │   └── payroll-workbench.tsx  # 3-zone work surface (intake / center / inspector)
│   ├── app/api/payroll/           # REST routes (collection, runs, payslips, components, structures, periods, additionals)
│   ├── app/payroll/page.tsx       # redirects to default submodule
│   └── scripts/
│       ├── payroll-smoke.mjs       # end-to-end API smoke (15 assertions)
│       └── payroll-browser-proof.mjs  # puppeteer-core + system Chrome real browser test
└── docs/
    ├── payroll-oss-implementation-notes.md
    ├── payroll-implementation-map.md
    ├── payroll-user-stories.md
    └── payroll-acceptance-matrix.md
```

## Source RFC

[`rfcs/0006_business-basic-payroll.md`](../../../../rfcs/0006_business-basic-payroll.md)

## Skill

`product_engineering/business-basic-module-development`
