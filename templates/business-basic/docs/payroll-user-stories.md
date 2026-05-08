# Payroll User Stories (Manual + CTOX, 50 paired)

Module: `payroll` · Label: `Lohnabrechnung` · RFC: [`rfcs/0006_business-basic-payroll.md`](../../../rfcs/0006_business-basic-payroll.md)

Each story below uses the Skill-prescribed structure literally:

```
US-<n> Manual
As <actor>, when <trigger>, I <operate central object> so that <business result>.
UI path:
1. route:
2. select:
3. action:
4. result:
Done when:
- UI:
- DB:
- event/audit:

US-<n> CTOX
From <right-click/drawer/selected object>, ask CTOX to <prepare/validate/execute>.
Context payload:
- module:
- submodule:
- recordType:
- recordId:
- selectedFields:
- allowedAction:
Done when:
- CTOX result:
- persisted state:
- approval/recovery:
```

## 01–05 Setup / Master Data

### US-01 Manual
As a payroll operator, when I open `/app/operations/payroll` for the first time, I see a seeded period, three components, one structure, and two assignments so that I can verify the workspace is correctly bootstrapped.
UI path:
1. route: `/app/operations/payroll`
2. select: nothing
3. action: read left intake panel
4. result: periods, components, assignments visible
Done when:
- UI: left zone lists `2026-04-01 – 2026-04-30`, `Grundgehalt`, `Sozialversicherung AN`, `Lohnsteuer AN`, two assignments
- DB: `getPayrollSnapshot()` returns the seed snapshot
- event/audit: none

### US-01 CTOX
From the workbench header, ask CTOX to `validate-setup` for the current company.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: (none — workspace-level)
- selectedFields: { companyId }
- allowedAction: explain
Done when:
- CTOX result: list of seeded master rows plus warnings if employees lack assignments
- persisted state: no mutation
- approval/recovery: not required

### US-02 Manual
As an operator, when I expand `Komponente anlegen` and submit a new component `Sachbezug` (earning, taxable, depends-on-payment-days, account `6024`, formula `fix(50)`), the row appears in the master list so that the next run can use it.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → `Komponente anlegen`
3. action: fill code/label/type/account/formula and submit
4. result: component listed
Done when:
- UI: component listed under Komponenten
- DB: `payroll_component` rows include `Sachbezug`
- event/audit: `payroll_component` event with command `create_component`

### US-02 CTOX
From the right-click menu of an existing component, ask CTOX to `extend-formula` to derive a new component from the selected one.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_component
- recordId: pc-base
- selectedFields: { code: "base", formulaKind: "fix", formulaAmount: 4000 }
- allowedAction: extend-formula
Done when:
- CTOX result: queue task with proposed component definition
- persisted state: no mutation until operator confirms in drawer
- approval/recovery: operator confirms in inspector

### US-03 Manual
As an operator, when I assign structure `ps-default` to employee `emp-clara` from `2026-04-15` with base salary `3200 EUR`, the assignment appears in the intake panel so that the next run includes Clara.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → `Zuweisung anlegen`
3. action: pick employee/structure, enter base/currency/from-date, submit
4. result: assignment row visible
Done when:
- UI: row listed under Strukturzuweisungen
- DB: `payroll_structure_assignment` includes the row
- event/audit: `payroll_run` event with command `create_structure_assignment`

### US-03 CTOX
From the assignments panel, ask CTOX to `propose-assignments` for any active employee without one.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure
- recordId: ps-default
- selectedFields: { unassignedEmployees: [...] }
- allowedAction: recompute
Done when:
- CTOX result: list of unassigned employees with structure suggestions
- persisted state: no mutation until operator confirms one (then Manual US-03 path runs)
- approval/recovery: operator confirms in drawer

### US-04 Manual
As an operator, when I press `Sperren` next to a period, the period is locked and badge shows `gesperrt` so that no new run can target it.
UI path:
1. route: `/app/operations/payroll`
2. select: intake period row
3. action: click `Sperren`
4. result: badge flips to `gesperrt`
Done when:
- UI: locked badge visible
- DB: `payroll_period.locked = true`
- event/audit: `payroll_period` event command `lock_period`

### US-04 CTOX
From the right-click menu on a period row, ask CTOX to `explain` the impact of locking.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_period
- recordId: period_2026_04
- selectedFields: { locked: false, runs: [...] }
- allowedAction: explain
Done when:
- CTOX result: summary of current runs and slip states inside the period
- persisted state: no mutation
- approval/recovery: not required

### US-05 Manual
As an operator, when I expand `Periode anlegen` and submit a new monthly period for the next month, it appears in the period list so that I can run payroll once the current period closes.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → `Periode anlegen`
3. action: fill start/end/frequency, submit
4. result: new period row visible
Done when:
- UI: row listed
- DB: `payroll_period` includes the row, locked = false
- event/audit: `payroll_period` event command `create_period`

### US-05 CTOX
From the period section, ask CTOX to `prepare-period` for the next month based on the current frequency.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_period
- recordId: (latest)
- selectedFields: { frequency: "monthly", lastEnd: "2026-04-30" }
- allowedAction: recompute
Done when:
- CTOX result: proposed period boundaries (start/end/frequency)
- persisted state: no mutation until operator confirms
- approval/recovery: operator confirms in drawer

## 06–10 Intake / Source Work

### US-06 Manual
As an operator, when I select a Draft slip and add a `Zusatzposten` of 250 EUR for component `pc-base`, the slip recomputes and the line reflects the additional so that one-off bonuses appear in the run.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row → inspector → `Zusatzposten anlegen`
3. action: fill component/amount/note, submit (auto-triggers `recompute_run`)
4. result: line amount increases by 250
Done when:
- UI: line shows new amount, gross/net update
- DB: `payroll_additional` row exists; slip line `amount` includes additional
- event/audit: `payroll_additional` event command `create_additional`

### US-06 CTOX
From the slip detail, ask CTOX to `propose-additional` for unbilled bonus from a Workforce variance.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { employeeId, periodId, variance }
- allowedAction: recompute
Done when:
- CTOX result: draft additional payload
- persisted state: created only after operator accepts
- approval/recovery: operator confirms in drawer

### US-07 Manual
As an operator, when I press `Löschen` on a `Zusatzposten` before posting, the row is removed and the next recompute drops the amount from the slip line.
UI path:
1. route: `/app/operations/payroll`
2. select: inspector → additional row
3. action: click `Löschen`
4. result: row gone
Done when:
- UI: row removed
- DB: `payroll_additional` no longer present
- event/audit: `payroll_additional` event command `delete_additional`

### US-07 CTOX
From the additional row, ask CTOX to `explain` why the additional did not appear in the slip (period mismatch, component disabled, etc.).
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_additional
- recordId: <addId>
- selectedFields: { employeeId, periodId, componentId }
- allowedAction: explain
Done when:
- CTOX result: textual explanation grounded in current snapshot
- persisted state: no mutation
- approval/recovery: not required

### US-08 Manual
As an operator, when an approved Workforce time entry has been prepared as a payroll candidate (via `prepare_payroll_candidate` on the workforce side), the next payroll run automatically produces a `workforce_hours` slip line with the correct amount, so that hours flow into wages without a separate import step.
UI path:
1. route: `/app/operations/payroll`
2. select: run row → click `Run abschicken`
3. action: open the slip that belongs to the employee whose Workforce candidate was prepared
4. result: inspector shows a `Freigegebene Workforce-Stunden` line with `hours × hourlyRate` as amount
Done when:
- UI: slip line `workforce_hours` is visible with amount > 0
- DB: payroll snapshot's `payslip.lines` includes the line; `additionalsWithWorkforce` reads `workforce.payrollCandidates` for `(employeeId, periodId)` and folds them in via `pc-workforce-hours`
- event/audit: run audit `Queued → Submitted` is recorded; the workforce candidate keeps `status=prepared` and is now reflected as a payroll line

### US-08 CTOX
Ask CTOX to `reconcile` Workforce hours vs. payroll lines for an employee/period.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { employeeId, periodStart, periodEnd, workforceHours, payrollHours }
- allowedAction: reconcile
Done when:
- CTOX result: diff workforce vs payroll
- persisted state: no mutation
- approval/recovery: not required

### US-09 Manual
As an operator, when I click `Deaktivieren` on a component, the component is hidden from new runs and existing draft slips drop that line on recompute.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → component row
3. action: click `Deaktivieren`
4. result: badge `deaktiviert`
Done when:
- UI: component marked deaktiviert
- DB: `payroll_component.disabled = true`
- event/audit: `payroll_component` event command `update_component`

### US-09 CTOX
From the component row, ask CTOX to `explain` the downstream effect of disabling.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_component
- recordId: <componentId>
- selectedFields: { code, type, structuresReferenced }
- allowedAction: explain
Done when:
- CTOX result: list of structures and slips affected
- persisted state: no mutation
- approval/recovery: not required

### US-10 Manual
As an operator, when I edit the base salary on an assignment from 4000 to 4500 inline on the assignment row, the next `recompute_run` uses the new value so the slip's base line updates.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → Strukturzuweisungen row
3. action: change base in the inline numeric input and blur (auto-dispatches `update_structure_assignment`)
4. result: assignment row shows new base; recompute renders updated slip line
Done when:
- UI: numeric input persists the new value; slip line `base` reflects 4500 (`pc-base` is `percent_of(base_salary, 100)`)
- DB: `payroll_structure_assignment.baseSalary = 4500`
- event/audit: `payroll_run` event command `update_structure_assignment` recorded
- proof: `pnpm test:payroll` step 17 (recompute reflects new base)

### US-10 CTOX
From the assignment row, ask CTOX to `explain` the gross/net delta after the base change.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure_assignment
- recordId: <assignmentId>
- selectedFields: { oldBase, newBase }
- allowedAction: explain
Done when:
- CTOX result: simulated delta per slip without posting
- persisted state: no mutation
- approval/recovery: not required

## 11–20 Core Workflow

### US-11 Manual
As an operator, when I click `Run abschicken` for a Draft run, the run transitions to Submitted and one slip per assigned employee materializes so that I can review them.
UI path:
1. route: `/app/operations/payroll`
2. select: run row in center zone
3. action: click `Run abschicken`
4. result: run state Submitted; slip rows visible
Done when:
- UI: run badge `Submitted`; slip table populated
- DB: `payroll_payslip` rows for each employee with positive gross/net/three lines
- event/audit: run audit `Draft → Queued → Submitted`

### US-11 CTOX
From the run row, ask CTOX to `explain` the run plan before queueing.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { periodId, frequency, employeeIds }
- allowedAction: explain
Done when:
- CTOX result: list of employees + expected gross/deduction
- persisted state: no mutation
- approval/recovery: not required

### US-12 Manual
As an operator, when I click `Zur Prüfung` on a Draft slip, it transitions to Review so that I can post it next.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row
3. action: click `Zur Prüfung`
4. result: slip badge `Review`
Done when:
- UI: badge changed
- DB: `payroll_payslip.status = "Review"`
- event/audit: audit `Draft → Review`

### US-12 CTOX
Ask CTOX to `review` the slip and surface anomalies (missing additional, structure mismatch).
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { employeeId, gross, net, lines }
- allowedAction: review
Done when:
- CTOX result: structured anomaly list
- persisted state: no mutation
- approval/recovery: operator confirms or recomputes

### US-13 Manual
As an operator, when I click `Buchen` on a Review slip, a balanced journal entry is created and the slip moves to Posted.
UI path:
1. route: `/app/operations/payroll`
2. select: Review slip row
3. action: click `Buchen`
4. result: badge `Posted`; journal id shown in inspector
Done when:
- UI: badge `Posted`; journal entry id rendered
- DB: `postedJournals` contains balanced draft (debit total === credit total); slip `journalEntryId` set
- event/audit: audit `Review → Posted`

### US-13 CTOX
Ask CTOX to `post` the slip after manual review.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { gross, net, journalLines }
- allowedAction: post
Done when:
- CTOX result: returns journal id
- persisted state: same as Manual
- approval/recovery: operator confirms in drawer

### US-14 Manual
As an operator, when I cancel a Draft slip, no journal is created and the slip is terminal.
UI path:
1. route: `/app/operations/payroll`
2. select: Draft slip row
3. action: click `Stornieren`
4. result: badge `Cancelled`
Done when:
- UI: badge changed
- DB: slip status `Cancelled`; no journal entry
- event/audit: audit `Draft → Cancelled`

### US-14 CTOX
Ask CTOX to `cancel` a Draft slip with an explanation.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { status, reason }
- allowedAction: cancel
Done when:
- CTOX result: confirmation
- persisted state: cancellation
- approval/recovery: not required

### US-15 Manual
As an operator, when I cancel a Posted slip, a reversing journal is generated automatically.
UI path:
1. route: `/app/operations/payroll`
2. select: Posted slip row
3. action: click `Stornieren`
4. result: badge `Cancelled`; reversal journal entry id rendered
Done when:
- UI: reversal journal id visible in inspector
- DB: `postedJournals` includes reversal entry whose lines invert original debit/credit
- event/audit: audit `Posted → Cancelled`

### US-15 CTOX
Ask CTOX to `cancel` a Posted slip with a recorded reason.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { status: "Posted", reason }
- allowedAction: cancel
Done when:
- CTOX result: confirmation incl. reversal id
- persisted state: cancellation + reversal journal
- approval/recovery: operator confirms in drawer

### US-16 Manual
As an operator, when I attempt to create a second non-Cancelled run for the same period+frequency, the API rejects with `run_already_exists_for_period`.
UI path:
1. route: `/app/operations/payroll`
2. select: intake new-run form
3. action: submit duplicate
4. result: error toast
Done when:
- UI: error toast visible
- DB: no new run created
- event/audit: none

### US-16 CTOX
Ask CTOX to `explain` why a duplicate run is rejected.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <existingRunId>
- selectedFields: { periodId, status }
- allowedAction: explain
Done when:
- CTOX result: explanation referencing existing run
- persisted state: no mutation
- approval/recovery: not required

### US-17 Manual
As an operator, when I attempt to create a run for a locked period, the API rejects with `period_locked`.
UI path:
1. route: `/app/operations/payroll`
2. select: intake new-run form
3. action: submit against a locked period
4. result: error toast
Done when:
- UI: error toast visible
- DB: no new run
- event/audit: none

### US-17 CTOX
Ask CTOX to `explain` the lock state and propose unlocking.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_period
- recordId: <periodId>
- selectedFields: { locked: true }
- allowedAction: explain
Done when:
- CTOX result: explanation
- persisted state: no mutation
- approval/recovery: not required

### US-18 Manual
As an operator, when I click `Slips neu berechnen` on a Submitted run after a component change, draft slip lines reflect the new amounts.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `Slips neu berechnen`
4. result: slip lines updated
Done when:
- UI: slip totals updated
- DB: line amounts recomputed; status unchanged
- event/audit: `payroll_run` event command `recompute_run`

### US-18 CTOX
Ask CTOX to `recompute` a slip after a component change.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { changedComponentId }
- allowedAction: recompute
Done when:
- CTOX result: confirms new values
- persisted state: snapshot updated
- approval/recovery: not required

### US-19 Manual
As an operator, when I edit a Draft/Review slip line via the inline numeric input in the inspector, the override dispatches `update_payslip_line` and the slip totals re-derive.
UI path:
1. route: `/app/operations/payroll`
2. select: Draft or Review slip → inspector line
3. action: edit numeric input and blur
4. result: line amount + slip totals refresh; status unchanged
Done when:
- UI: input renders only when status ∈ {Draft, Review}; on blur the change is dispatched
- DB: `payroll_payslip_line.amount` and slip totals updated; runtime guards reject same call on Posted/Cancelled slip with `payslip_immutable`
- event/audit: `payroll_payslip` event command `update_payslip_line`
- proof: `pnpm test:payroll` step 18 (override + totals recompute)

### US-19 CTOX
Ask CTOX to `explain` the override impact on net pay before saving.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip_line
- recordId: <lineId>
- selectedFields: { oldAmount, newAmount }
- allowedAction: explain
Done when:
- CTOX result: predicted net change
- persisted state: no mutation
- approval/recovery: operator confirms in drawer

### US-20 Manual
As an operator, when I attempt to update a line on a Posted slip, the API rejects with `payslip_immutable`.
UI path:
1. route: `/app/operations/payroll`
2. select: Posted slip → inspector → line
3. action: edit
4. result: error toast
Done when:
- UI: input disabled or error toast
- DB: no mutation
- event/audit: none

### US-20 CTOX
Ask CTOX to `explain` why a Posted slip cannot be edited and propose a `payroll_additional` in a later period instead.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { status: "Posted" }
- allowedAction: explain
Done when:
- CTOX result: rationale + correction-path proposal
- persisted state: no mutation
- approval/recovery: not required

## 21–25 Move / Transition / Drag-Drop

### US-21 Manual
As an operator, when I press `Zurück zu Entwurf` on a Review or Withheld slip, `mark_payslip_draft` runs and the slip becomes editable again.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row in state Review or Withheld
3. action: click `Zurück zu Entwurf`
4. result: badge `Draft`; line override input becomes editable again
Done when:
- UI: button enabled only for Review/Withheld
- DB: slip status `Draft`
- event/audit: audit `Review → Draft` (or `Withheld → Draft`)
- proof: `pnpm test:payroll` step 19 (Draft → Review → Draft round-trip)

### US-21 CTOX
Ask CTOX to `recompute` the slip after moving back to Draft.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { status }
- allowedAction: recompute
Done when:
- CTOX result: snapshot diff
- persisted state: line recompute
- approval/recovery: not required

### US-22 Manual
As an operator, when I press the run‑header `Alle zur Prüfung`, `bulk_mark_review` flips every Draft slip in the selected run to Review.
UI path:
1. route: `/app/operations/payroll`
2. select: run row (sets `selectedRunId`)
3. action: click `Alle zur Prüfung` in the run header
4. result: every Draft slip badge `Review`
Done when:
- UI: button visible only when a run is selected
- DB: each affected slip transitions Draft → Review
- event/audit: per-slip audit row + one `payroll_run` event with `Bulk-Prüfung: <count>`
- proof: `pnpm test:payroll` step 20 (every slip in run ends Review)

### US-22 CTOX
Ask CTOX to `review` all slips in a run and pre-flag anomalies.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { slipIds }
- allowedAction: review
Done when:
- CTOX result: per-slip anomalies
- persisted state: no mutation
- approval/recovery: operator chooses to apply

### US-23 Manual
As an operator, when I press the run‑header `Alle buchen`, `bulk_post_run` posts every Review slip independently; per-slip failures (negative net, validation) leave the failing slip in Review with an error note while sibling slips proceed.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `Alle buchen`
4. result: each successful slip transitions Review → Posted; each failure stays Review and gets a `payslip.notes` annotation
Done when:
- UI: header `Alle buchen` button dispatches the bulk command
- DB: postedJournals appended for each successfully posted slip; balanced (debit total === credit total)
- event/audit: `payroll_run` event `Bulk-Buchung: <posted> gebucht, <failed> blockiert`; per-slip `Review → Posted` audit rows
- proof: `pnpm test:payroll` step 20 (postedSlips.length ≥ 2 for the run)

### US-23 CTOX
Ask CTOX to `post` all Review slips with one confirmation.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { reviewSlipIds }
- allowedAction: post
Done when:
- CTOX result: per-slip success/failure list
- persisted state: per-slip post
- approval/recovery: operator confirms once

### US-24 Manual
As an operator, when I withhold a slip from posting and later release it, the slip transitions Withheld → Review and is included in the next bulk post.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row
3. action: click `Zurückstellen`, later `Zur Prüfung`
4. result: badge `Withheld` then `Review`
Done when:
- UI: badge changes both ways
- DB: status transitions persist
- event/audit: audit Draft|Review → Withheld → Review

### US-24 CTOX
Ask CTOX to `withhold` a slip with a recorded reason.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { reason, status }
- allowedAction: withhold
Done when:
- CTOX result: confirmation
- persisted state: status flipped
- approval/recovery: operator chooses release path

### US-25 Manual
As an operator, when I cancel an entire run, every non-Posted child slip is also cancelled in the same audit pass; Posted slips are untouched.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `Run abbrechen`
4. result: run badge `Cancelled`; cascade to non-Posted slips
Done when:
- UI: badges updated
- DB: child non-Posted slips become Cancelled
- event/audit: per-slip cascade audit rows

### US-25 CTOX
Ask CTOX to `cancel` a run and summarise impact on Posted vs. unposted slips.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { postedCount, otherCount }
- allowedAction: cancel
Done when:
- CTOX result: cascade summary
- persisted state: cascade
- approval/recovery: operator confirms

## 26–30 Edit / Rename / Duplicate / Archive / Delete

### US-26 Manual
As an operator, when I edit the `payroll_structure` label inline on its row and blur, `update_structure` runs and only the label changes; `componentIds` and assignments stay attached.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → Strukturen row
3. action: change inline `<input defaultValue={label}>` and blur
4. result: row shows new label
Done when:
- UI: input blur triggers dispatch
- DB: `payroll_structure.label` changed; `componentIds` unchanged
- event/audit: structure event command `update_structure`

### US-26 CTOX
Ask CTOX to `rename` a structure with consistent naming across the company.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure
- recordId: <structureId>
- selectedFields: { oldLabel }
- allowedAction: update
Done when:
- CTOX result: proposed label
- persisted state: applied after operator confirms
- approval/recovery: operator confirms

### US-27 Manual
As an operator, when I click `Duplizieren` on a structure row, `duplicate_structure` produces a new structure with copied `componentIds` and a ` -kopie` suffix.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → Strukturen row
3. action: click `Duplizieren`
4. result: new structure row visible with copied components
Done when:
- UI: new row appears beneath the original
- DB: new `payroll_structure` with new id; original untouched
- event/audit: `payroll_run` event command `duplicate_structure`
- proof: `pnpm test:payroll` step 21 (clone produced with same `componentIds` join)

### US-27 CTOX
Ask CTOX to `duplicate` a structure with a custom rate scaling.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure
- recordId: <sourceId>
- selectedFields: { componentIds, scaling }
- allowedAction: duplicate
Done when:
- CTOX result: new structure id
- persisted state: created
- approval/recovery: operator confirms

### US-28 Manual
As an operator, when I disable a component, existing draft slips can recompute and skip it; downstream totals recompute.
UI path: see US-09 + US-18
Done when:
- UI: line removed from slip after recompute
- DB: line absent from recomputed slip
- event/audit: recompute audit per slip

### US-28 CTOX
Ask CTOX to `recompute` all draft slips after archiving a component.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { affectedComponentId }
- allowedAction: recompute
Done when:
- CTOX result: per-slip diff
- persisted state: recompute applied
- approval/recovery: not required

### US-29 Manual
As an operator, when I delete a `payroll_additional` row, the next recompute drops the amount.
UI path: see US-07
Done when:
- UI: row removed; recomputed slip line drops amount
- DB: `payroll_additional` removed
- event/audit: `payroll_additional` event command `delete_additional`

### US-29 CTOX
Ask CTOX to `explain` which slips would be affected before deleting.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_additional
- recordId: <addId>
- selectedFields: { employeeId, periodId, componentId, amount }
- allowedAction: explain
Done when:
- CTOX result: list of affected slips
- persisted state: no mutation
- approval/recovery: operator confirms before delete

### US-30 Manual
As an operator, when I press `Löschen` on a component referenced by an active `payroll_structure`, `delete_component` rejects with `component_in_use`; the row stays and a confirm dialog warns me before the call.
UI path:
1. route: `/app/operations/payroll`
2. select: intake → Komponenten row
3. action: click `Löschen` and confirm
4. result: error toast (`component_in_use`); component row unchanged
Done when:
- UI: confirm prompt then error toast
- DB: component still present; no mutation
- event/audit: none (rejection)
- proof: `pnpm test:payroll` step 22 (delete on `pc-base` blocked)
- event/audit: none

### US-30 CTOX
Ask CTOX to `explain` what blocks deletion and how to detach.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_component
- recordId: <componentId>
- selectedFields: { referencingStructures }
- allowedAction: explain
Done when:
- CTOX result: list of blockers + detach steps
- persisted state: no mutation
- approval/recovery: operator picks detach path

## 31–35 Right-Click Actions

### US-31 Manual
As an operator, when I right-click a `payroll_payslip` row, the context menu exposes Buchen, Stornieren, Markiere zur Prüfung, Zurückstellen, Prompt CTOX.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row
3. action: right-click
4. result: context menu opens
Done when:
- UI: menu visible with the named entries
- DB: row exposes `data-context-record-type=payroll_payslip` and id
- event/audit: none until an action is chosen

### US-31 CTOX
From the menu, ask CTOX to `post` the slip; operator confirms.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { gross, net }
- allowedAction: post
Done when:
- CTOX result: confirmation
- persisted state: post applied
- approval/recovery: operator confirms

### US-32 Manual
As an operator, when I right-click a `payroll_run` row, the context menu exposes Slips neu generieren, Slips neu berechnen, Run absenden, Run abbrechen, Prompt CTOX.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: right-click
4. result: menu opens
Done when:
- UI: menu visible
- DB: row exposes `data-context-record-type=payroll_run`
- event/audit: none until action

### US-32 CTOX
From the menu, ask CTOX to `recompute` the run and report total diff.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { totals }
- allowedAction: recompute
Done when:
- CTOX result: diff report
- persisted state: recompute applied
- approval/recovery: operator confirms

### US-33 Manual
As an operator, when I right-click a `payroll_payslip_line` row, the context menu exposes Komponente anzeigen, Wert überschreiben, Prompt CTOX.
UI path:
1. route: `/app/operations/payroll`
2. select: line row in inspector
3. action: right-click
4. result: menu opens
Done when:
- UI: menu visible
- DB: row exposes `data-context-record-type=payroll_payslip_line`
- event/audit: none

### US-33 CTOX
From the menu, ask CTOX to `explain` how the line was computed (formula trace).
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip_line
- recordId: <lineId>
- selectedFields: { componentCode, formulaKind, amount }
- allowedAction: explain
Done when:
- CTOX result: formula trace
- persisted state: no mutation
- approval/recovery: not required

### US-34 Manual
As an operator, when I right-click a `payroll_structure_assignment`, the context menu exposes Beenden, Duplizieren, Prompt CTOX.
UI path:
1. route: `/app/operations/payroll`
2. select: assignment row
3. action: right-click
4. result: menu opens
Done when:
- UI: menu visible
- DB: row exposes `data-context-record-type=payroll_structure_assignment`
- event/audit: none

### US-34 CTOX
From the menu, ask CTOX to `propose-assignments` for re-pointing the employee to a new structure.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure_assignment
- recordId: <assignmentId>
- selectedFields: { employeeId, structureId }
- allowedAction: update
Done when:
- CTOX result: alternative structures
- persisted state: created after confirm
- approval/recovery: operator confirms

### US-35 Manual
As an operator, when I right-click a `payroll_component`, the context menu exposes Bearbeiten, Deaktivieren, Prompt CTOX.
UI path:
1. route: `/app/operations/payroll`
2. select: component row
3. action: right-click
4. result: menu opens
Done when:
- UI: menu visible
- DB: row exposes `data-context-record-type=payroll_component`
- event/audit: none

### US-35 CTOX
From the menu, ask CTOX to `extend-formula` for a country-specific deduction variant.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_component
- recordId: <componentId>
- selectedFields: { code, formulaKind, formulaExpression }
- allowedAction: extend-formula
Done when:
- CTOX result: proposed component
- persisted state: created after confirm
- approval/recovery: operator confirms

## 36–40 CTOX-Assisted Actions

### US-36 Manual
As an operator, when I trigger a CTOX-driven `review` from a Draft slip, the resulting queue task carries the full slip context payload.
UI path:
1. route: `/app/operations/payroll`
2. select: Draft slip
3. action: Prompt CTOX → review
4. result: queue task created
Done when:
- UI: queue task confirmation
- DB: queue task with payload `module=operations, submodule=payroll, recordType=payroll_payslip, recordId=<id>, label=Lohnabrechnung <Name>`
- event/audit: payload emitted by runtime on the originating mutation

### US-36 CTOX
CTOX returns a structured review with anomalies + recommended action.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { gross, net, lines, status }
- allowedAction: review
Done when:
- CTOX result: anomalies + recommended next action
- persisted state: applied only after operator confirms
- approval/recovery: one-click apply

### US-37 Manual
As an operator, when I ask CTOX to `reconcile` a slip against bookkeeping, the response shows debit/credit totals match gross/net and links to ledger entries.
UI path:
1. route: `/app/operations/payroll`
2. select: Posted slip
3. action: Prompt CTOX → reconcile
4. result: reconciliation summary
Done when:
- UI: summary rendered
- DB: read-only against `postedJournals`
- event/audit: none

### US-37 CTOX
Same. Output is structured JSON consumed by the inspector.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { journalEntryId, gross, net, totalDeduction }
- allowedAction: reconcile
Done when:
- CTOX result: balanced/unbalanced flag + line breakdown
- persisted state: no mutation
- approval/recovery: not required

### US-38 Manual
As an operator, when I press `Prompt CTOX` next to the Zusatzposten‑Form, `propose_additional_via_ctox` runs and emits a `queueProposal=` event note carrying the slip and proposed payload — without creating a `payroll_additional`.
UI path:
1. route: `/app/operations/payroll`
2. select: slip → inspector → `Zusatzposten anlegen` → fill form → click `Prompt CTOX`
3. action: dispatch `propose_additional_via_ctox` instead of `create_additional`
4. result: event log shows `queueProposal=…`; no `payroll_additional` row yet
Done when:
- UI: button is next to `Anlegen`; only the proposal path is taken on click
- DB: no `payroll_additional` mutation
- event/audit: `payroll_additional` event whose `message` ends with `queueProposal=…`
- proof: `pnpm test:payroll` step 23 (event note contains `queueProposal=`)

### US-38 CTOX
CTOX inserts the proposed additional only after operator confirmation.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { proposedComponent, amount, note }
- allowedAction: recompute
Done when:
- CTOX result: queue task created
- persisted state: created after operator confirms
- approval/recovery: operator confirms

### US-39 Manual
As an operator, when I open the inspector's `Periodenvergleich (letzte 6)` panel for a selected slip, the workbench fetches `GET /api/operations/payroll?view=comparison&employeeId=<id>&periods=6` and renders the rows + Δ Brutto column for that employee.
UI path:
1. route: `/app/operations/payroll`
2. select: slip in inspector
3. action: open `Periodenvergleich (letzte 6)` details element
4. result: table renders rows for last N posted slips of the employee
Done when:
- UI: comparison panel renders without page reload (client fetch)
- DB: read-only against snapshot's `payroll_payslip` rows where `status === Posted`
- event/audit: none (read-only)
- proof: `pnpm test:payroll` step 25 (`comparison.rows.length >= 1`)

### US-39 CTOX
CTOX response cites slip ids and amounts; UI surfaces them as deep-links.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { employeeId, periodIds }
- allowedAction: explain
Done when:
- CTOX result: cited slips + deltas
- persisted state: no mutation
- approval/recovery: not required

### US-40 Manual
As an operator, when I press `DE‑Pack installieren` in the run header, `install_country_pack { country: "DE" }` imports `@ctox-business/payroll-de` and adds the German 2026 components plus the `pde-default` structure to the snapshot. Re-installing is a no-op (idempotent).
UI path:
1. route: `/app/operations/payroll`
2. select: any
3. action: click `DE‑Pack installieren`
4. result: new components and structure visible in intake
Done when:
- UI: `payroll_component` rows for KV/RV/AV/PV AN, Lohnsteuer, Soli appear; `pde-default` structure appears
- DB: each component / structure id added once; second install reports 0 added
- event/audit: `payroll_component` event with `Country pack DE: …`
- proof: `pnpm test:payroll-de-unit` (gross 4000 EUR → net 2398.40); `pnpm test:payroll` step 24 (`pde-default` structure present, components grew)

### US-40 CTOX
Operator can preview the proposal and apply selected components.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_structure
- recordId: <structureId>
- selectedFields: { country: "DE" }
- allowedAction: extend-formula
Done when:
- CTOX result: proposal
- persisted state: applied selectively
- approval/recovery: operator confirms each

## 41–44 Blocker / Exception Recovery

### US-41 Manual
As an operator, when a run fails because an employee lacks an active assignment in the period, the run flips to Failed and the error names the employee.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `Run abschicken`
4. result: run state Failed; error message visible
Done when:
- UI: run badge `Failed`; error message displayed
- DB: `payroll_run.error` populated; per-slip generation skipped for the failing employee
- event/audit: run audit `Queued → Failed`

### US-41 CTOX
Ask CTOX to `propose-assignments` for the named employee.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { error, missingEmployeeId }
- allowedAction: recompute
Done when:
- CTOX result: assignment proposal
- persisted state: created after confirm
- approval/recovery: operator confirms

### US-42 Manual
As an operator, when I fix the missing assignment and re-queue, the run flips back to Submitted and the missing slip is created.
UI path:
1. route: `/app/operations/payroll`
2. select: Failed run
3. action: re-queue (`queue_run` accepts `Failed`)
4. result: run Submitted; missing slip materialized
Done when:
- UI: run badge updated; new slip visible
- DB: slip created
- event/audit: run audit `Failed → Queued → Submitted`

### US-42 CTOX
Ask CTOX to `recompute` after the fix; confirms slip generated.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { newEmployeeIds }
- allowedAction: recompute
Done when:
- CTOX result: confirmation
- persisted state: applied
- approval/recovery: not required

### US-43 Manual
As an operator, when a slip has a negative net (line override + heavy deductions), the post button is disabled and a warning banner appears.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row
3. action: try Buchen (button disabled)
4. result: warning banner in inspector
Done when:
- UI: button disabled, banner visible
- DB: runtime guard returns `negative_net_pay_blocks_post` if force-attempted
- event/audit: none

### US-43 CTOX
Ask CTOX to `explain` which line caused the negative net.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { net, lines }
- allowedAction: explain
Done when:
- CTOX result: identifies the heaviest deduction line
- persisted state: no mutation
- approval/recovery: not required

### US-44 Manual
As an operator, when the journal builder rejects an unbalanced draft (synthetic test), the slip stays Review with a recorded error.
UI path: not user-facing; engine path
Done when:
- UI: error rendered as `payroll_posting_unbalanced`
- DB: no `postedJournals` entry
- event/audit: error logged

### US-44 CTOX
Ask CTOX to `explain` the imbalance and propose a fix.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { debitTotal, creditTotal }
- allowedAction: explain
Done when:
- CTOX result: imbalance source
- persisted state: no mutation
- approval/recovery: operator fixes inputs

## 45–47 Cross-Module Handoff

### US-45 Manual
As an operator, after posting, I can see the journal entry surfaced in `business/ledger` (read-only) and in DATEV export.
UI path:
1. route: `/app/business/ledger`
2. select: ledger row
3. action: read
4. result: payroll JE listed
Done when:
- UI: JE visible in ledger view
- DB: bookkeeping export contains JE rows
- event/audit: none beyond original payroll post

### US-45 CTOX
Ask CTOX to `reconcile` posted payroll JEs with bookkeeping totals for the period.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { jeIds }
- allowedAction: reconcile
Done when:
- CTOX result: balance check
- persisted state: no mutation
- approval/recovery: not required

### US-46 Manual
As an operator, when a structure includes the `pc-workforce-hours` component (auto-ensured by `normalizeSnapshot`), `queue_run` and `recompute_run` pull `workforce.payrollCandidates` for `periodId` via `additionalsWithWorkforce` and fold them into the slip lines, so that approved hours flow into wages without manual import.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `Run abschicken` (or `Slips neu berechnen` when candidates change)
4. result: each slip whose employee has a prepared candidate shows a `workforce_hours` line whose amount equals `hours × hourlyRate`
Done when:
- UI: `workforce_hours` line visible per affected slip
- DB: `payslip.lines` reflect candidate contribution; engine sums `pc-workforce-hours` (formulaAmount = 0) plus the synthetic additional from `wf_<candidateId>`
- event/audit: run audit `Queued → Submitted` recorded

### US-46 CTOX
Ask CTOX to `reconcile` Workforce hours vs slip amounts.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { workforceHours, payrollHours }
- allowedAction: reconcile
Done when:
- CTOX result: diff
- persisted state: no mutation
- approval/recovery: operator picks reconciliation strategy

### US-47 Manual
As an operator, when I trigger a SEPA proposal from a Posted run, a payment proposal is drafted with slip id, employee IBAN, and net amount.
UI path:
1. route: `/app/operations/payroll`
2. select: run row
3. action: click `SEPA-Vorschlag`
4. result: proposal preview rendered
Done when:
- UI: proposal preview
- DB: `payment_proposal` row per slip
- event/audit: payments event command `create_payment_proposal`

### US-47 CTOX
Ask CTOX to `prepare` a payment batch for posted slips in a period.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { postedSlipCount }
- allowedAction: post
Done when:
- CTOX result: batch summary
- persisted state: drafts created after confirm
- approval/recovery: operator confirms

## 48–50 Report / Export / Audit / Regression

### US-48 Manual
As an operator, when I open a slip's inspector, the audit trail shows Draft → Review → Posted with actor and timestamp.
UI path:
1. route: `/app/operations/payroll`
2. select: slip row
3. action: read inspector audit list
4. result: ordered list
Done when:
- UI: audit list rendered
- DB: `audit` array populated
- event/audit: every state transition recorded

### US-48 CTOX
Ask CTOX to `explain` why a slip was Withheld using audit history.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_payslip
- recordId: <slipId>
- selectedFields: { auditTrail }
- allowedAction: explain
Done when:
- CTOX result: explanation grounded in audit
- persisted state: no mutation
- approval/recovery: not required

### US-49 Manual
As an operator, when I click `CSV‑Export` in the run header, `GET /api/operations/payroll?view=export&periodId=<id>` streams a CSV file with columns `employee_id,employee_name,gross,deductions,net,journal_id,status`.
UI path:
1. route: `/app/operations/payroll`
2. select: run row (sets `selectedRunId` whose period is used)
3. action: click `CSV‑Export` anchor (triggers browser download)
4. result: file `payroll-<periodId>.csv` saved
Done when:
- UI: anchor `<a href=…>` with `download` attribute; visible only when a run is selected
- DB: read-only against `payroll_payslip` rows of the period
- event/audit: none (read-only export)
- proof: `pnpm test:payroll` step 26 (header + ≥3 lines)

### US-49 CTOX
Ask CTOX to `explain` outliers in the report (largest deltas vs. previous period).
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { csvSummary }
- allowedAction: explain
Done when:
- CTOX result: outlier list
- persisted state: no mutation
- approval/recovery: not required

### US-50 Manual
As an operator, after any large change (component added, run re-run), I re-execute US-01, US-02, US-11, US-31, US-36 to confirm no regression.
UI path: smoke + browser proof
Done when:
- UI: all five core stories still pass
- DB: no spurious state transitions
- event/audit: regression report attached to release notes

### US-50 CTOX
Ask CTOX to run a regression check using stored prior expectations.
Context payload:
- module: operations
- submodule: payroll
- recordType: payroll_run
- recordId: <runId>
- selectedFields: { expectedTotals }
- allowedAction: recompute
Done when:
- CTOX result: pass/fail per regression marker
- persisted state: no mutation unless operator chooses to apply a fix
- approval/recovery: operator decides

## Re-test list after large changes

```
US-01 setup
US-02 create master data (component)
US-11 first core workflow (run + slip)
US-31 first right-click action
US-36 first CTOX-assisted action
```

Each invocation of `pnpm --filter @ctox-business/web test:payroll` and `pnpm --filter @ctox-business/web test:payroll-browser` exercises this regression set automatically.
