import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { evaluateComponent, evaluateExpression, FormulaError, type FormulaContext } from "../formula";
import type { PayrollComponent } from "../types";

const baseCtx: FormulaContext = {
  baseSalary: 4000,
  paymentDays: 30,
  lwpDays: 0,
  absentDays: 0,
  workingDays: 30,
  components: { base: 4000 }
};

const fixtureComponent: PayrollComponent = {
  id: "c1",
  code: "base",
  label: "Grundgehalt",
  type: "earning",
  taxable: true,
  dependsOnPaymentDays: false,
  accountId: "6020",
  formulaKind: "fix",
  formulaAmount: 4000,
  sequence: 10,
  disabled: false
};

describe("formula DSL", () => {
  it("computes fix amount", () => {
    assert.equal(evaluateComponent(fixtureComponent, baseCtx), 4000);
  });

  it("prorates fix amount when dependsOnPaymentDays", () => {
    const halfMonth = { ...baseCtx, paymentDays: 15 };
    const c = { ...fixtureComponent, dependsOnPaymentDays: true };
    assert.equal(evaluateComponent(c, halfMonth), 2000);
  });

  it("computes percent_of base_salary", () => {
    const c: PayrollComponent = {
      ...fixtureComponent,
      id: "c2",
      code: "social_employee",
      type: "deduction",
      formulaKind: "percent_of",
      formulaBase: "base_salary",
      formulaPercent: 9.3,
      formulaAmount: undefined
    };
    assert.equal(evaluateComponent(c, baseCtx), 372);
  });

  it("computes percent_of another component", () => {
    const c: PayrollComponent = {
      ...fixtureComponent,
      id: "c3",
      code: "tax_employee",
      type: "deduction",
      formulaKind: "percent_of",
      formulaBase: "base",
      formulaPercent: 18,
      formulaAmount: undefined
    };
    assert.equal(evaluateComponent(c, baseCtx), 720);
  });

  it("evaluates arithmetic expression with components and variables", () => {
    const c: PayrollComponent = {
      ...fixtureComponent,
      id: "c4",
      code: "performance_bonus",
      formulaKind: "formula",
      formulaExpression: "base * 0.05 + 100",
      formulaAmount: undefined
    };
    assert.equal(evaluateComponent(c, baseCtx), 300);
  });

  it("respects operator precedence and parentheses", () => {
    assert.equal(evaluateExpression("2 + 3 * 4", baseCtx), 14);
    assert.equal(evaluateExpression("(2 + 3) * 4", baseCtx), 20);
    assert.equal(evaluateExpression("-base_salary + base_salary", baseCtx), 0);
  });

  it("rejects unknown identifiers", () => {
    assert.throws(() => evaluateExpression("foo + 1", baseCtx), FormulaError);
  });

  it("rejects forbidden tokens", () => {
    assert.throws(() => evaluateExpression("base_salary; drop table", baseCtx), FormulaError);
    assert.throws(() => evaluateExpression("base_salary[0]", baseCtx), FormulaError);
    assert.throws(() => evaluateExpression('"injection"', baseCtx), FormulaError);
  });

  it("guards division by zero", () => {
    assert.throws(() => evaluateExpression("base_salary / 0", baseCtx), FormulaError);
  });
});
