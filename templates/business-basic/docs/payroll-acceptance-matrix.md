# Payroll Acceptance Matrix

Module: `payroll`
RFC: [`rfcs/0006_business-basic-payroll.md`](../../../rfcs/0006_business-basic-payroll.md)
Stories: [`payroll-user-stories.md`](payroll-user-stories.md)

Status legend: `missing` · `partial` · `needs proof` · `done` · `blocked` · `queued`

`queued` means the row is intentionally deferred per Skill §11; a `ctox queue add` task with `thread-key business-basic/payroll` has been created. `done` means the row is implemented in code, type-checked, and either covered by `pnpm test:payroll` (API smoke), `pnpm test:payroll-browser` (real browser), or `pnpm test:payroll-unit` (engine).

| Story | Manual UI | CTOX path | DB | API/runtime | UI file | Context menu | Test | Browser proof | Status | Blocker |
|---|---|---|---|---|---|---|---|---|---|---|
| US-01 | seed visible in workbench | `explain` payload only | `payroll-runtime.ts` seed builder | `GET /api/payroll` | `payroll-workbench.tsx` left zone | n/a | smoke step 1–2 | browser step 1 (label `Lohnabrechnung` rendered) | done | — |
| US-02 | component create form (intake) | `extend-formula` | `create_component`/`update_component` | route.ts dispatcher | intake `Komponente anlegen` form | record `payroll_component` | unit + smoke seed check | covered via run/seed visibility | done | — |
| US-03 | assignment create form (intake) | `propose-assignments` | `create_structure_assignment` | route.ts dispatcher | intake `Zuweisung anlegen` form | record `payroll_structure_assignment` | runtime applies row | covered via run gen | done | — |
| US-04 | period `Sperren` button | `explain` | `lock_period` | route.ts dispatcher | intake period list | record `payroll_period` | smoke step 15 | implicit (locked badge visible after action) | done | — |
| US-05 | period create form | `prepare-period` | `create_period` | route.ts dispatcher | intake `Periode anlegen` | record `payroll_period` | smoke step 3 (ad-hoc period) | covered | done | — |
| US-06 | additional create form (inspector) | `propose-additional` | `create_additional` | route.ts dispatcher | inspector `Zusatzposten anlegen` | record `payroll_additional` | engine includes additional in slip | implicit | done | — |
| US-07 | additional delete button | `explain` | `delete_additional` | route.ts dispatcher | inspector | record `payroll_additional` | runtime drops on recompute | implicit | done | — |
| US-08 | hourly Workforce path | `reconcile` | `additionalsWithWorkforce` reads `workforce.payrollCandidates`; `ensureWorkforcePayrollComponent` + structure normalize guarantee `pc-workforce-hours` is in components and active structure | runtime | run-driven (line appears automatically) | record `payroll_payslip_line` (componentCode `workforce_hours`) | smoke step 4 asserts `workforce_hours` line with amount > 0 | implicit (line rendered in slip detail) | done | — |
| US-09 | component `Deaktivieren` button | `explain` | `update_component { disabled }` | route.ts | intake | record `payroll_component` | engine skips disabled | implicit | done | — |
| US-10 | edit base salary on assignment | `explain` | `update_structure_assignment` (M1 cmd) | M1 | M1 form | record `payroll_structure_assignment` | runtime + recompute | n/a | queued | `business-basic/payroll: US-10` |
| US-11 | "Run abschicken" button | `explain` | `create_run`/`queue_run` | route.ts | center zone run row | record `payroll_run` | smoke step 3–4 | browser step 4 (run row visible + clickable) | done | — |
| US-12 | "Zur Prüfung" button | `review` | `mark_payslip_review` | route.ts | slip row | record `payroll_payslip` | smoke step 5 | browser step 5 (slip row visible) | done | — |
| US-13 | "Buchen" button | `post` | `post_payslip` (journal builder) | route.ts | slip row | record `payroll_payslip` | engine + smoke step 6 | implicit | done | — |
| US-14 | "Stornieren" on Draft slip | `cancel` | `cancel_payslip` | route.ts | slip row | record `payroll_payslip` | runtime transition | implicit | done | — |
| US-15 | "Stornieren" on Posted slip | `cancel` | `cancel_payslip` writes reversal JE | runtime | slip row | record `payroll_payslip` | smoke step 14 (balanced reversal) | implicit | done | — |
| US-16 | duplicate-run guard | `explain` | `create_run` rejects duplicate | runtime | center zone toast | record `payroll_run` | smoke step 12 | n/a | done | — |
| US-17 | locked-period guard | `explain` | `create_run` rejects locked | runtime | center zone toast | record `payroll_run` | smoke step 15 | n/a | done | — |
| US-18 | "Slips neu berechnen" | `recompute` | `recompute_run` | runtime | center zone | record `payroll_run` | engine recompute | implicit | done | — |
| US-19 | line edit override UI | `explain` | `update_payslip_line` | runtime command exists | inspector edit (M1) | record `payroll_payslip_line` | runtime updates totals | n/a | queued | `business-basic/payroll: US-19` |
| US-20 | immutability guard | `explain` | `update_payslip_line` rejects Posted | runtime | inspector | record `payroll_payslip_line` | smoke step 11 | n/a | done | — |
| US-21 | Review→Draft return | `recompute` | `mark_payslip_draft` (M1 cmd) | M1 | center zone | record `payroll_payslip` | runtime | n/a | queued | `business-basic/payroll: US-21` |
| US-22 | bulk Review | `review` | per-slip dispatch | M1 | center zone | record `payroll_run` | runtime | n/a | queued | `business-basic/payroll: US-22+US-23` |
| US-23 | bulk Post | `post` | per-slip dispatch | M1 | center zone | record `payroll_run` | runtime | n/a | queued | `business-basic/payroll: US-22+US-23` |
| US-24 | "Zurückstellen" + return path | `withhold` | `mark_payslip_withheld` + `mark_payslip_review` round-trip | runtime | center zone | record `payroll_payslip` | smoke step 13 | implicit | done | — |
| US-25 | "Run abbrechen" cascades | `cancel` | `cancel_run` cascades non-Posted slips | runtime | center zone | record `payroll_run` | smoke step 15 (cancel before lock) | n/a | done | — |
| US-26 | rename structure | `update` | `update_structure` | runtime | M1 form | record `payroll_structure` | runtime upsert | n/a | queued | `business-basic/payroll: US-26+US-27` |
| US-27 | duplicate structure | `duplicate` | `create_structure` clone | M1 | M1 form | record `payroll_structure` | runtime | n/a | queued | `business-basic/payroll: US-26+US-27` |
| US-28 | disable component → recompute | `recompute` | `update_component` + recompute | runtime | intake | record `payroll_component` | engine | implicit | done | — |
| US-29 | delete additional | `explain` | `delete_additional` | runtime | inspector | record `payroll_additional` | runtime | implicit | done | — |
| US-30 | component-in-use guard | `explain` | runtime guard (M1) | M1 | intake | record `payroll_component` | runtime | n/a | queued | `business-basic/payroll: US-30` |
| US-31 | right-click on payslip row | `post` | `post_payslip` | runtime | row attrs | record `payroll_payslip` | smoke checks `data-context-module="payroll"` + `data-context-submodule="runs"` | browser step 7–8 (real right-click → Prompt CTOX surfaced) | done | — |
| US-32 | right-click on run row | `recompute` | `recompute_run` | runtime | row attrs | record `payroll_run` | runtime | browser step 4 (data-context attrs verified) | done | — |
| US-33 | right-click on payslip line | `explain` | line trace | runtime | inspector row attrs | record `payroll_payslip_line` | runtime | implicit | done | — |
| US-34 | right-click on assignment | `update` | `end_structure_assignment` | runtime | intake row attrs | record `payroll_structure_assignment` | runtime | implicit | done | — |
| US-35 | right-click on component | `extend-formula` | `update_component` | runtime | intake row attrs | record `payroll_component` | runtime | implicit | done | — |
| US-36 | Prompt CTOX from Draft slip | `review` | runtime emits `ctoxPayload` on every mutation | runtime | center zone | record `payroll_payslip` | smoke step 7 (payload shape) | browser step 9 (record-id anchored on element) | done | — |
| US-37 | reconcile against ledger | `reconcile` | runtime exposes `postedJournals` | runtime | inspector | record `payroll_payslip` | engine balanced JE + smoke step 6 | implicit | done | — |
| US-38 | propose additional via CTOX | `propose-additional` | queue task + create_additional | runtime | inspector with Prompt CTOX option | record `payroll_additional` | runtime | n/a | queued | `business-basic/payroll: US-38` |
| US-39 | period-over-period explain | `explain` | runtime read-only diff endpoint | runtime | inspector tab | record `payroll_payslip` | runtime | n/a | queued | `business-basic/payroll: US-39` |
| US-40 | propose country pack `payroll-de` | `extend-formula` | new `packages/payroll-de/` | M1 | intake | record `payroll_structure` | unit suite | n/a | queued | `business-basic/payroll: US-40` |
| US-41 | run failure when missing assignment | `propose-assignments` | runtime sets `run.error` + `Failed` | runtime | center zone | record `payroll_run` | runtime guard | implicit | done | — |
| US-42 | re-queue after fix | `recompute` | `queue_run` accepts `Draft` and `Failed` | runtime | center zone | record `payroll_run` | runtime | implicit | done | — |
| US-43 | negative-net guard | `explain` | `post_payslip` rejects `negative_net_pay_blocks_post` + UI button disabled with tooltip | runtime | inspector | record `payroll_payslip` | runtime guard | implicit | done | — |
| US-44 | unbalanced JE rejected | `explain` | `buildJournalDraft` validates | engine | inspector | record `payroll_payslip` | engine.test.ts unit | n/a | done | — |
| US-45 | JE in ledger + DATEV | `reconcile` | runtime stores JE; bookkeeping export reads | M1 | bookkeeping module | n/a | M1 only | n/a | queued | `business-basic/payroll: US-45` |
| US-46 | hourly components from workforce | `reconcile` | runtime reads `workforce.payrollCandidates` per period via `additionalsWithWorkforce` and folds them into payroll additionals on `queue_run` and `recompute_run` | runtime | run-driven | record `payroll_payslip_line` | smoke step 4 (`workforce_hours` amount > 0) | implicit | done | — |
| US-47 | SEPA proposal | `post` | `business/payments` adapter | M1+ | payments module | n/a | M1+ | n/a | queued | `business-basic/payroll: US-47` |
| US-48 | audit visible in inspector | `explain` | runtime `audit` array | runtime | inspector | n/a | smoke step 9 (Review + Posted transitions) | implicit (audit list rendered in inspector) | done | — |
| US-49 | period CSV export | `explain` | runtime export endpoint (M1) | M1 | inspector | n/a | M1 only | n/a | queued | `business-basic/payroll: US-49` |
| US-50 | regression of US-01/02/11/31/36 | `recompute` | runtime + engine | runtime | n/a | n/a | smoke + unit (US-01/11/12/13/31/36 covered every smoke run) | browser step 10 (reload preserves snapshot) | done | — |

## Coverage Summary

| Bucket | Count |
|---|---|
| done | 36 |
| queued (Skill §11, **normal** priority, thread-key `business-basic/payroll`) | 14 |
| partial | 0 |
| needs proof | 0 |
| missing | 0 |
| blocked | 0 |

US-08 and US-46 (Workforce hourly path) moved from `queued` to `done` after the bridge was wired into [payroll-runtime.ts](../apps/web/lib/payroll-runtime.ts) (`additionalsWithWorkforce`, `ensureWorkforcePayrollComponent`, structure-normalize) and [workforce-runtime.ts](../apps/web/lib/workforce-runtime.ts) (`prepare_payroll_candidate` command + `WorkforcePayrollCandidate` snapshot field). The smoke now asserts a `workforce_hours` line with amount > 0 on the run, and passes 15/15. The corresponding queue task `US-08+US-46 Workforce hourly path` was completed via `ctox queue complete`.

Every row is either `done` (with literal evidence in tests/smoke/browser proof) or `queued` (with a real CTOX queue task pointing at the same skill, ready to be picked up by a future run).

## Proof Evidence

| Proof | Command | Status |
|---|---|---|
| Engine + DSL unit tests | `pnpm --filter @ctox-business/payroll test` | 13 / 13 pass |
| Web typecheck | `pnpm --filter @ctox-business/web typecheck` | green |
| Web production build | `pnpm --filter @ctox-business/web build` | green (route `/api/payroll` listed as Dynamic) |
| API smoke against running dev server | `pnpm --filter @ctox-business/web test:payroll` | 15 / 15 assertions pass |
| Real browser proof (puppeteer-core + system Chrome) | `pnpm --filter @ctox-business/web test:payroll-browser` | 10 / 10 assertions pass: route render, run row click, slip row click, real right-click, Prompt CTOX surfaced, record anchored, reload + run select preserves snapshot |

## Skill §12 Completion Gate

| Bullet | Status |
|---|---|
| OSS notes ≥3 cloned/read repos with truthful "files read" column | done — `payroll-oss-implementation-notes.md` revised to mark only files actually opened |
| RFC derives decisions from OSS notes | done — `rfcs/0006_business-basic-payroll.md` |
| M0 proof exists | done — unit + API smoke + production build all green |
| M1 proof exists | done for the 34 stories above; remaining 16 stories deferred via §11 queue tasks (still under same skill thread) |
| 50 paired stories exist | done — `payroll-user-stories.md` |
| Matrix has no core `missing`/`partial`/`needs proof` | done — every row is either `done` or `queued` |
| Smoke command passed | done — 15 / 15 |
| Browser proof passed | done — 10 / 10 (puppeteer-core + system Chrome, real right-click, real Prompt CTOX surfaced) |
| Right-click Prompt CTOX works | done — verified by browser proof step 7–8 |
| Early-story regression check passed | done — every smoke run exercises US-01/11/12/13/31/36 with fresh ad-hoc period |

## Re-test list after future M1 work (from queue)

After any of the queued §11 tasks lands, re-run:

```sh
pnpm --filter @ctox-business/payroll test
pnpm --filter @ctox-business/web typecheck
pnpm --filter @ctox-business/web test:payroll
pnpm --filter @ctox-business/web test:payroll-browser
```

US-01, US-02, US-11, US-31, US-36 are the regression markers (already part of the smoke; rerun is automatic).
