import { evaluateComponent, type FormulaContext } from "./formula";
import type {
  PayrollAdditional,
  PayrollComponent,
  PayrollEmployee,
  PayrollPayslip,
  PayrollPayslipLine,
  PayrollPeriod,
  PayrollStructure,
  PayrollStructureAssignment
} from "./types";

export type ComputePayslipInput = {
  run: { id: string; periodId: string; postingDate: string };
  period: PayrollPeriod;
  employee: PayrollEmployee;
  assignment: PayrollStructureAssignment;
  structure: PayrollStructure;
  components: PayrollComponent[];
  additionals: PayrollAdditional[];
  paymentDays?: number;
  lwpDays?: number;
  absentDays?: number;
  workingDays?: number;
};

export function computePayslip(input: ComputePayslipInput): PayrollPayslip {
  const components = orderedComponents(input.structure, input.components);
  const baseSalary = input.assignment.baseSalary;
  const workingDays = input.workingDays ?? defaultWorkingDays(input.period);
  const paymentDays = clamp(input.paymentDays ?? workingDays, 0, workingDays);
  const lwpDays = Math.max(0, input.lwpDays ?? 0);
  const absentDays = Math.max(0, input.absentDays ?? 0);

  const ctx: FormulaContext = {
    baseSalary,
    paymentDays,
    lwpDays,
    absentDays,
    workingDays,
    components: {}
  };

  const lines: PayrollPayslipLine[] = [];
  for (const component of components) {
    if (component.disabled) continue;
    const computed = evaluateComponent(component, ctx);
    const additional = additionalsFor(input.additionals, input.employee.id, input.period.id, component.id);
    const amount = round2(computed + additional);
    ctx.components[component.code] = amount;
    lines.push({
      id: `${input.run.id}_${input.employee.id}_${component.code}`,
      componentId: component.id,
      componentCode: component.code,
      componentLabel: component.label,
      sequence: component.sequence,
      type: component.type,
      qty: 1,
      rate: amount,
      amount
    });
  }

  const grossPay = round2(sumByType(lines, "earning"));
  const totalDeduction = round2(sumByType(lines, "deduction"));
  const netPay = round2(grossPay - totalDeduction);

  return {
    id: `${input.run.id}_${input.employee.id}`,
    runId: input.run.id,
    employeeId: input.employee.id,
    employeeName: input.employee.displayName,
    assignmentId: input.assignment.id,
    periodId: input.period.id,
    startDate: input.period.startDate,
    endDate: input.period.endDate,
    paymentDays,
    lwpDays,
    absentDays,
    currency: input.assignment.currency,
    grossPay,
    totalDeduction,
    netPay,
    status: "Draft",
    lines
  };
}

function orderedComponents(structure: PayrollStructure, all: PayrollComponent[]): PayrollComponent[] {
  const set = new Set(structure.componentIds);
  return all
    .filter((component) => set.has(component.id))
    .sort((a, b) => {
      if (a.sequence !== b.sequence) return a.sequence - b.sequence;
      if (a.type === b.type) return a.code.localeCompare(b.code);
      return a.type === "earning" ? -1 : 1;
    });
}

function defaultWorkingDays(period: PayrollPeriod): number {
  const start = Date.parse(period.startDate);
  const end = Date.parse(period.endDate);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return 0;
  const days = Math.round((end - start) / (24 * 3600 * 1000)) + 1;
  return days;
}

function additionalsFor(additionals: PayrollAdditional[], employeeId: string, periodId: string, componentId: string): number {
  return additionals
    .filter((a) => a.employeeId === employeeId && a.periodId === periodId && a.componentId === componentId)
    .reduce((acc, a) => acc + a.amount, 0);
}

function sumByType(lines: PayrollPayslipLine[], type: "earning" | "deduction"): number {
  return lines.filter((line) => line.type === type).reduce((acc, line) => acc + line.amount, 0);
}

function clamp(value: number, min: number, max: number) {
  if (Number.isNaN(value)) return min;
  return Math.min(Math.max(value, min), max);
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}
