# RFC 0007: Payroll-DE DSL Extension Feasibility

Status: Open — feasibility audit, no implementation
Target: `packages/payroll/src/formula.ts` typed DSL
Trigger: `templates/business-basic/packages/payroll-de` country pack ships a flat-bracket Lohnsteuer (`percent_of base 18 %`) as a placeholder; the real 2026 ESt-Tarif is a piecewise polynomial that the current DSL cannot express.
Created: 2026-05-07

## Current DSL surface

The typed expression DSL in [packages/payroll/src/formula.ts](../templates/business-basic/packages/payroll/src/formula.ts) accepts:

- `fix(<amount>)`
- `percent_of(<base-name>, <pct>)`
- `formula(<expr>)` where `<expr>` is `+ - * /`, parentheses, unary minus, numeric literals, and identifiers (other component codes plus reserved `base_salary`, `payment_days`, `lwp_days`, `absent_days`, `working_days`).

What is missing for German Lohnsteuer 2026:

1. **Conditional / piecewise expressions.** The ESt-Tarif §32a EStG defines four brackets:
   - 0 €–12,096 €: 0 %
   - 12,097 €–17,443 €: progressive `(932.30·y + 1,400)·y`
   - 17,444 €–68,480 €: progressive `(176.64·z + 2,397)·z + 1,015.13`
   - 68,481 €–277,825 €: linear `0.42·zvE − 10,911.92`
   - above: linear `0.45·zvE − 19,256.67`
   The current DSL has no `if(...)` / `case(...)` / range condition.
2. **Power / polynomial operator.** `y²` and `z²` are required for the second and third brackets. Current DSL has no `^` or `pow()`.
3. **Annual vs monthly resolution.** Lohnsteuer is computed on annualized taxable income, then divided by months. The DSL has no implicit annualization.
4. **Lookup tables.** Solidaritätszuschlag has its own free-amount band (0 € below 18,130 € jährlich, gleitender Übergang up to 33,400 €). Tax classes (Steuerklasse I–VI), Kinderfreibeträge, Kirchensteuer (8 % BY/BW, 9 % rest) — all need tabular lookups.

## Decision

Three options were considered:

### A. Extend the DSL with `if`, `pow`, and runtime constants

Add to grammar:
```
factor := … | "if" "(" expr "," expr "," expr ")" | "pow" "(" expr "," expr ")"
```

Reserved variables would gain `annual_base = base_salary * 12 / period_months`.

Pros: stays in pure typed DSL; no host code; auditable.
Cons: brackets like `(932.30·y + 1400)·y` with `y = (zvE − 12096)/10000` are still tedious. Polynomial degree stays at 2, so this is feasible. Tax-class tables would need a separate lookup mechanism.

### B. Add a typed table lookup primitive

Add to grammar:
```
factor := … | "lookup" "(" identifier "," expr ")"
```

Lookups resolve against a finite, typed table registered in the country pack:
```ts
{
  name: "de_lst_2026",
  rows: [
    { upTo: 12096, fn: (zvE) => 0 },
    { upTo: 17443, fn: (zvE) => /* ... */ },
    …
  ]
}
```

Pros: cleanly separates math (in pack code) from formula reference. A component formula becomes `lookup(de_lst_2026, base_salary * 12) / 12`.
Cons: re-introduces host-side functions (the `fn` per row). Mitigation: each row is itself a small DSL expression, so the table is data, not code.

### C. Punt to a different country-pack format that does not use the formula DSL

`packages/payroll-de` could ship a `compute(payslip, ctx) → lines` function called by an explicit hook in `payroll-runtime.ts`, bypassing the formula DSL for tax components. Other earnings/deductions still use the DSL.

Pros: zero DSL change; arbitrary expressivity.
Cons: breaks the `Reject Python-eval rule formulas` decision in [`rfcs/0006_business-basic-payroll.md`](0006_business-basic-payroll.md). Country packs become opaque code, harder to audit, harder to debug for non-developers.

## Recommendation

**Option B** — typed table lookup primitive — is the recommended path:

1. It preserves the no-host-eval invariant that motivated the typed DSL in the first place.
2. The 2026 ESt-Tarif fits in 5 rows; the country pack registers the table as plain data (with each row's polynomial expressed in the existing DSL grammar).
3. Solidaritätszuschlag and Kirchensteuer have similar table shapes; same primitive serves all three.
4. Polynomial degree-2 needs are met by adding `pow(x, 2)` once, no further DSL extension needed for 2026 brackets.

## Pre-implementation checklist for a future CTOX run

- Add `pow(<expr>, <integer>)` and `lookup(<table>, <expr>)` factors to `formula.ts` parser.
- Add table registry: `packages/payroll/src/tables.ts` that the country pack populates.
- Extend `FormulaContext` with `tables: Map<string, LookupTable>`.
- Unit tests:
  - `pow(2, 3) === 8`
  - `lookup(de_lst_2026, 50000)` matches reference value from BMF Lohnsteuertabelle
  - Boundary tests at each bracket transition
- Replace `payroll-de`'s placeholder Lohnsteuer (current `percent_of base 18 %`) with the real bracket lookup.
- Replace placeholder Soli with the real `lookup(de_soli_2026, lohnsteuer * 12) / 12`.
- Acceptance: gross 4,000 € / month / Steuerklasse I produces net within ±2 € of the official BMF Lohn- und Einkommensteuerrechner 2026.

## Out of scope for this RFC

- Steuerklasse selection (ELStAM integration) — operator-side master data, not DSL.
- Kinderfreibeträge — table addition once Steuerklasse master is in place.
- AAG (Aufwendungsausgleichsgesetz) U1/U2 — separate employer-side filing, not part of Lohnabrechnung core.
- Lohnsteueranmeldung (XML to ELStAM) — country-export module, separate RFC.

## Decision required from operator

Approve Option B, schedule the DSL extension before re-attempting the payroll-de country pack at production fidelity. Until then, payroll-de's flat-bracket Lohnsteuer is a documented placeholder.
