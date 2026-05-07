import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { computePayslip } from "../engine";
import { buildJournalDraft } from "../posting";
import type {
  PayrollAdditional,
  PayrollComponent,
  PayrollEmployee,
  PayrollPeriod,
  PayrollStructure,
  PayrollStructureAssignment
} from "../types";

const period: PayrollPeriod = {
  id: "p1",
  companyId: "co",
  frequency: "monthly",
  startDate: "2026-04-01",
  endDate: "2026-04-30",
  locked: false,
  createdAt: "2026-04-01T00:00:00.000Z"
};

const employee: PayrollEmployee = { id: "emp-1", displayName: "Anna Müller" };
const assignment: PayrollStructureAssignment = {
  id: "a1",
  employeeId: "emp-1",
  structureId: "s1",
  baseSalary: 4000,
  currency: "EUR",
  fromDate: "2026-01-01",
  createdAt: "2026-01-01T00:00:00.000Z",
  createdBy: "operator"
};
const components: PayrollComponent[] = [
  {
    id: "c-base",
    code: "base",
    label: "Grundgehalt",
    type: "earning",
    taxable: true,
    dependsOnPaymentDays: true,
    accountId: "6020",
    formulaKind: "fix",
    formulaAmount: 4000,
    sequence: 10,
    disabled: false
  },
  {
    id: "c-social",
    code: "social_employee",
    label: "Sozialversicherung AN",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1742",
    formulaKind: "percent_of",
    formulaBase: "base",
    formulaPercent: 20,
    sequence: 20,
    disabled: false
  },
  {
    id: "c-tax",
    code: "tax_employee",
    label: "Lohnsteuer AN",
    type: "deduction",
    taxable: false,
    dependsOnPaymentDays: false,
    accountId: "1741",
    formulaKind: "percent_of",
    formulaBase: "base",
    formulaPercent: 18,
    sequence: 30,
    disabled: false
  }
];

const structure: PayrollStructure = {
  id: "s1",
  companyId: "co",
  label: "Standard Monat",
  frequency: "monthly",
  currency: "EUR",
  isActive: true,
  modeOfPayment: "bank",
  componentIds: ["c-base", "c-social", "c-tax"]
};

describe("payroll engine", () => {
  it("computes a full slip with deductions", () => {
    const slip = computePayslip({
      run: { id: "r1", periodId: "p1", postingDate: "2026-04-30" },
      period,
      employee,
      assignment,
      structure,
      components,
      additionals: []
    });
    assert.equal(slip.lines.length, 3);
    assert.equal(slip.lines[0].amount, 4000);
    assert.equal(slip.lines[1].amount, 800);
    assert.equal(slip.lines[2].amount, 720);
    assert.equal(slip.grossPay, 4000);
    assert.equal(slip.totalDeduction, 1520);
    assert.equal(slip.netPay, 2480);
    assert.equal(slip.status, "Draft");
  });

  it("prorates earning when dependsOnPaymentDays and lwp", () => {
    const slip = computePayslip({
      run: { id: "r2", periodId: "p1", postingDate: "2026-04-30" },
      period,
      employee,
      assignment,
      structure,
      components,
      additionals: [],
      paymentDays: 15,
      workingDays: 30
    });
    assert.equal(slip.lines[0].amount, 2000);
  });

  it("includes additionals and tags them under the right component", () => {
    const additionals: PayrollAdditional[] = [
      { id: "ad1", employeeId: "emp-1", periodId: "p1", componentId: "c-base", amount: 500, note: "Bonus" }
    ];
    const slip = computePayslip({
      run: { id: "r3", periodId: "p1", postingDate: "2026-04-30" },
      period,
      employee,
      assignment,
      structure,
      components,
      additionals
    });
    assert.equal(slip.lines.find((l) => l.componentCode === "base")?.amount, 4500);
    assert.equal(slip.grossPay, 4500);
  });

  it("builds a balanced journal draft", () => {
    const slip = computePayslip({
      run: { id: "r4", periodId: "p1", postingDate: "2026-04-30" },
      period,
      employee,
      assignment,
      structure,
      components,
      additionals: []
    });
    const draft = buildJournalDraft({
      payslip: slip,
      components,
      payableAccountId: "1755",
      postingDate: "2026-04-30"
    });
    const debit = draft.lines.reduce((acc, l) => acc + l.debit, 0);
    const credit = draft.lines.reduce((acc, l) => acc + l.credit, 0);
    assert.equal(Math.round(debit * 100), Math.round(credit * 100));
    assert.equal(draft.lines.find((l) => l.accountId === "1755")?.credit, 2480);
  });
});
