import { LedgerPosting, type JournalDraft } from "../posting/service";
import { germanInputVatAccountId } from "../tax";
import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type EmployeeExpenseInput = {
  companyId: string;
  currency: string;
  employeeName: string;
  employeePayableAccountId: string;
  expenseAccountId: string;
  expenseDate: string;
  grossAmount: number;
  id: string;
  netAmount: number;
  projectId?: string;
  requestedBy?: string;
  taxAmount: number;
  taxCode?: "DE_19_INPUT" | "DE_7_INPUT" | "DE_0" | string;
};

export type SubmitEmployeeExpensePayload = {
  employeeName: string;
  grossAmount: number;
  projectId?: string;
  receiptId: string;
};

export function prepareSubmitEmployeeExpenseCommand(input: EmployeeExpenseInput): AccountingCommand<SubmitEmployeeExpensePayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      employeeName: input.employeeName,
      grossAmount: input.grossAmount,
      projectId: input.projectId,
      receiptId: input.id
    },
    refId: input.id,
    refType: "employee_expense",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "SubmitEmployeeExpense"
  });
}

export function buildEmployeeExpenseJournalDraft(input: EmployeeExpenseInput): JournalDraft {
  if (input.grossAmount <= 0) throw new Error("employee_expense_amount_must_be_positive");
  const taxCode = input.taxCode ?? "DE_19_INPUT";
  const posting = new LedgerPosting(input.companyId, "employee_expense", input.id, input.expenseDate, input.currency);
  posting.debit(input.expenseAccountId, input.netAmount, undefined, { projectId: input.projectId, taxCode });
  if (input.taxAmount > 0) posting.debit(germanInputVatAccountId(taxCode), input.taxAmount, undefined, { projectId: input.projectId, taxCode });
  posting.credit(input.employeePayableAccountId, input.grossAmount, input.employeeName, { projectId: input.projectId });
  return posting.toJournalDraft("receipt", `Employee expense ${input.id} submitted by ${input.employeeName}.`);
}
