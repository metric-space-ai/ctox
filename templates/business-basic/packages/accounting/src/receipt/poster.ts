import { moneyFromMajor } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";
import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type BusinessReceiptLike = {
  currency: string;
  expenseAccountId: string;
  id: string;
  netAmount: number;
  number: string;
  payableAccountId: string;
  receiptDate: string;
  status: string;
  taxAmount: number;
  total: number;
  vendorName: string;
};

export type PostReceiptCommandPayload = {
  receiptId: string;
  receiptNumber: string;
  vendorName: string;
};

export function preparePostReceiptCommand(
  receipt: BusinessReceiptLike,
  companyId: string,
  requestedBy = "business-runtime"
): AccountingCommand<PostReceiptCommandPayload> {
  return createAccountingCommand({
    companyId,
    payload: {
      receiptId: receipt.id,
      receiptNumber: receipt.number,
      vendorName: receipt.vendorName
    },
    refId: receipt.id,
    refType: "receipt",
    requestedBy,
    type: "PostReceipt"
  });
}

export function buildReceiptJournalDraft(receipt: BusinessReceiptLike, companyId: string): JournalDraft {
  if (receipt.total <= 0) throw new Error("receipt_total_must_be_positive");
  if (!receipt.expenseAccountId) throw new Error("receipt_expense_account_required");
  if (!receipt.payableAccountId) throw new Error("receipt_payable_account_required");

  const posting = new LedgerPosting(companyId, "receipt", receipt.id, receipt.receiptDate, receipt.currency);
  posting.debit(receipt.expenseAccountId, moneyFromMajor(receipt.netAmount, receipt.currency));
  if (receipt.taxAmount > 0) posting.debit("acc-vat-input", moneyFromMajor(receipt.taxAmount, receipt.currency), undefined, { taxCode: "DE_19_INPUT" });
  posting.credit(receipt.payableAccountId, moneyFromMajor(receipt.total, receipt.currency));
  return posting.toJournalDraft("receipt", `Posted inbound receipt ${receipt.number} from ${receipt.vendorName}.`);
}
