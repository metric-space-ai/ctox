import { moneyFromMajor } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";
import { germanInputVatAccountId } from "../tax";
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
  taxCode?: "DE_19_INPUT" | "DE_7_INPUT" | "DE_0" | "RC" | string;
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
  const taxCode = receipt.taxCode ?? "DE_19_INPUT";
  posting.debit(receipt.expenseAccountId, moneyFromMajor(receipt.netAmount, receipt.currency), undefined, { taxCode });
  if (receipt.taxAmount > 0) posting.debit(germanInputVatAccountId(taxCode), moneyFromMajor(receipt.taxAmount, receipt.currency), undefined, { taxCode });
  posting.credit(receipt.payableAccountId, moneyFromMajor(receipt.total, receipt.currency));
  return posting.toJournalDraft("receipt", `Posted inbound receipt ${receipt.number} from ${receipt.vendorName}.`);
}
