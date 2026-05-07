import type { PayrollComponent, PayrollPayslip, PayrollPayslipLine } from "./types";

export type PayrollJournalLine = {
  accountId: string;
  debit: number;
  credit: number;
  componentCode?: string;
  partyId?: string;
  narration?: string;
};

export type PayrollJournalDraft = {
  refType: "payroll_payslip";
  refId: string;
  postingDate: string;
  currency: string;
  narration: string;
  lines: PayrollJournalLine[];
};

export type BuildJournalInput = {
  payslip: PayrollPayslip;
  components: PayrollComponent[];
  payableAccountId: string;
  postingDate: string;
};

/**
 * Build the journal draft for a payslip post.
 *
 * Convention:
 *   - earning components: debit component.accountId   (wage expense)
 *   - deduction components: credit component.accountId (statutory liabilities)
 *   - net pay: credit payableAccountId (employee payable)
 *
 * Net pay = gross - deductions.
 *
 * Total debit = gross. Total credit = deductions + net = gross. Always balanced.
 */
export function buildJournalDraft(input: BuildJournalInput): PayrollJournalDraft {
  const componentById = new Map(input.components.map((c) => [c.id, c]));
  const lines: PayrollJournalLine[] = [];

  for (const line of input.payslip.lines) {
    const component = componentById.get(line.componentId);
    if (!component) {
      throw new Error(`payroll_posting_unknown_component_${line.componentCode}`);
    }
    if (line.amount === 0) continue;
    if (line.type === "earning") {
      lines.push({
        accountId: component.accountId,
        debit: roundCents(line.amount),
        credit: 0,
        componentCode: line.componentCode,
        partyId: input.payslip.employeeId,
        narration: `${input.payslip.employeeName} ${line.componentLabel}`
      });
    } else {
      lines.push({
        accountId: component.accountId,
        debit: 0,
        credit: roundCents(line.amount),
        componentCode: line.componentCode,
        partyId: input.payslip.employeeId,
        narration: `${input.payslip.employeeName} ${line.componentLabel}`
      });
    }
  }

  if (input.payslip.netPay > 0) {
    lines.push({
      accountId: input.payableAccountId,
      debit: 0,
      credit: roundCents(input.payslip.netPay),
      partyId: input.payslip.employeeId,
      narration: `${input.payslip.employeeName} Nettoauszahlung`
    });
  }

  validateBalance(lines, input.payslip.id);

  return {
    refType: "payroll_payslip",
    refId: input.payslip.id,
    postingDate: input.postingDate,
    currency: input.payslip.currency,
    narration: `Lohnabrechnung ${input.payslip.employeeName} ${input.payslip.startDate}..${input.payslip.endDate}`,
    lines
  };
}

export function buildReversalDraft(original: PayrollJournalDraft, postingDate: string): PayrollJournalDraft {
  return {
    refType: "payroll_payslip",
    refId: `${original.refId}_reversal`,
    postingDate,
    currency: original.currency,
    narration: `Storno: ${original.narration}`,
    lines: original.lines.map((line) => ({
      accountId: line.accountId,
      debit: line.credit,
      credit: line.debit,
      componentCode: line.componentCode,
      partyId: line.partyId,
      narration: line.narration
    }))
  };
}

function roundCents(value: number): number {
  return Math.round(value * 100) / 100;
}

function validateBalance(lines: PayrollJournalLine[], slipId: string) {
  let debit = 0;
  let credit = 0;
  for (const line of lines) {
    debit += line.debit;
    credit += line.credit;
  }
  const debitC = Math.round(debit * 100);
  const creditC = Math.round(credit * 100);
  if (debitC !== creditC) {
    throw new Error(`payroll_posting_unbalanced ${slipId}: debit=${debit} credit=${credit}`);
  }
}

export function summariseJournal(lines: PayrollPayslipLine[]) {
  return {
    earnings: lines.filter((l) => l.type === "earning").reduce((acc, l) => acc + l.amount, 0),
    deductions: lines.filter((l) => l.type === "deduction").reduce((acc, l) => acc + l.amount, 0)
  };
}
