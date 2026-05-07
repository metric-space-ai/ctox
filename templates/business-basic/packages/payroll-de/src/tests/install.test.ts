import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { computePayslip } from "@ctox-business/payroll/engine";
import type {
  PayrollComponent,
  PayrollEmployee,
  PayrollPeriod,
  PayrollStructure,
  PayrollStructureAssignment
} from "@ctox-business/payroll/types";
import {
  PAYROLL_DE_VERSION,
  installIntoSnapshot,
  payrollDeComponents,
  payrollDeStructures
} from "../index";

describe("payroll-de country pack", () => {
  it("installs components and structures idempotently", () => {
    const snapshot = { components: [] as PayrollComponent[], structures: [] as PayrollStructure[] };
    const first = installIntoSnapshot(snapshot, { actor: "test", at: "2026-04-01T00:00:00Z" });
    assert.equal(first.componentsAdded, payrollDeComponents.length);
    assert.equal(first.structuresAdded, payrollDeStructures.length);
    assert.equal(first.version, PAYROLL_DE_VERSION);
    const second = installIntoSnapshot(snapshot, { actor: "test", at: "2026-04-01T00:00:00Z" });
    assert.equal(second.componentsAdded, 0);
    assert.equal(second.structuresAdded, 0);
  });

  it("computes a 4000 EUR base into a plausible 2026 net (DE simplified)", () => {
    const components: PayrollComponent[] = payrollDeComponents.map((component) =>
      component.code === "base" ? { ...component, formulaAmount: 4000 } : { ...component, disabled: false }
    );
    const structure = payrollDeStructures[0];
    const period: PayrollPeriod = {
      id: "p",
      companyId: "co",
      frequency: "monthly",
      startDate: "2026-04-01",
      endDate: "2026-04-30",
      locked: false,
      createdAt: "2026-04-01T00:00:00Z"
    };
    const employee: PayrollEmployee = { id: "emp", displayName: "Tester" };
    const assignment: PayrollStructureAssignment = {
      id: "a",
      employeeId: "emp",
      structureId: structure.id,
      baseSalary: 4000,
      currency: "EUR",
      fromDate: "2026-01-01",
      createdAt: "2026-01-01T00:00:00Z",
      createdBy: "test"
    };
    const slip = computePayslip({
      run: { id: "r", periodId: "p", postingDate: "2026-04-30" },
      period,
      employee,
      assignment,
      structure: { ...structure, componentIds: structure.componentIds.filter((id) => !["pde-overtime"].includes(id)) },
      components,
      additionals: []
    });
    assert.equal(slip.grossPay, 4000);
    // Sum of deductions for 4000 EUR base @ DE 2026 simplified percentages:
    // KV 8.15 %, RV 9.3 %, AV 1.3 %, PV 2.3 %, LSt 18 %, Soli 5.5 % of LSt
    // = 326 + 372 + 52 + 92 + 720 + 39.6 = 1601.6
    assert.ok(Math.abs(slip.totalDeduction - 1601.6) < 0.01, `deductions ${slip.totalDeduction} not ~1601.60`);
    assert.ok(Math.abs(slip.netPay - 2398.4) < 0.01, `net ${slip.netPay} not ~2398.40`);
  });
});
