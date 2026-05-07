import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type DunningInvoice = {
  balanceDue: number;
  customerId: string;
  dueDate: string;
  id: string;
  number: string;
  reminderLevel?: 0 | 1 | 2 | 3;
  status: string;
};

export type DunningRule = {
  daysOverdue: number;
  feeAmount: number;
  level: 1 | 2 | 3;
};

export type DunningProposal = {
  command: AccountingCommand<{
    feeAmount: number;
    invoiceId: string;
    invoiceNumber: string;
    level: 1 | 2 | 3;
  }>;
  daysOverdue: number;
};

const defaultRules: DunningRule[] = [
  { daysOverdue: 3, feeAmount: 0, level: 1 },
  { daysOverdue: 14, feeAmount: 5, level: 2 },
  { daysOverdue: 30, feeAmount: 10, level: 3 }
];

export function buildDunningProposals(input: {
  asOf: string;
  companyId: string;
  invoices: DunningInvoice[];
  requestedBy?: string;
  rules?: DunningRule[];
}): DunningProposal[] {
  const asOf = new Date(`${input.asOf}T00:00:00.000Z`);
  const rules = [...(input.rules ?? defaultRules)].sort((left, right) => right.daysOverdue - left.daysOverdue);

  return input.invoices.flatMap((invoice) => {
    if (invoice.balanceDue <= 0 || invoice.status === "Paid" || invoice.status === "Draft") return [];
    const due = new Date(`${invoice.dueDate}T00:00:00.000Z`);
    const daysOverdue = Math.floor((asOf.getTime() - due.getTime()) / 86_400_000);
    const rule = rules.find((item) => daysOverdue >= item.daysOverdue && (invoice.reminderLevel ?? 0) < item.level);
    if (!rule) return [];
    return [{
      command: createAccountingCommand({
        companyId: input.companyId,
        payload: {
          feeAmount: rule.feeAmount,
          invoiceId: invoice.id,
          invoiceNumber: invoice.number,
          level: rule.level
        },
        refId: invoice.id,
        refType: "invoice",
        requestedBy: input.requestedBy ?? "dunning-assistant",
        type: "RunDunning"
      }),
      daysOverdue
    }];
  });
}
