import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";
import { LedgerPosting, type JournalDraft } from "../posting/service";

export type BankTransactionLike = {
  amount: number;
  bookingDate: string;
  counterparty: string;
  currency: string;
  id: string;
  matchType?: "fee" | "invoice" | "manual" | "receipt";
  matchedRecordId?: string;
  purpose: string;
  status: string;
};

export type AcceptBankMatchPayload = {
  amount: number;
  bankTransactionId: string;
  matchedRecordId?: string;
  matchType?: string;
};

export function prepareAcceptBankMatchCommand(
  transaction: BankTransactionLike,
  companyId: string,
  requestedBy = "business-runtime"
): AccountingCommand<AcceptBankMatchPayload> {
  return createAccountingCommand({
    companyId,
    payload: {
      amount: transaction.amount,
      bankTransactionId: transaction.id,
      matchedRecordId: transaction.matchedRecordId,
      matchType: transaction.matchType
    },
    refId: transaction.id,
    refType: "bank_transaction",
    requestedBy,
    type: "AcceptBankMatch"
  });
}

export function bankMatchConfidence(transaction: BankTransactionLike) {
  if (transaction.status === "Matched") return 1;
  if (transaction.status === "Suggested" && transaction.matchedRecordId) return 0.92;
  if (transaction.matchType === "fee") return 0.74;
  return 0.45;
}

export type PaymentJournalContext = {
  accountsPayableAccountId: string;
  accountsReceivableAccountId: string;
  bankAccountId: string;
  bankFeeAccountId: string;
  companyId: string;
};

export function buildBankMatchJournalDraft(transaction: BankTransactionLike, context: PaymentJournalContext): JournalDraft {
  if (transaction.amount === 0) throw new Error("bank_transaction_amount_must_not_be_zero");

  const amount = Math.abs(transaction.amount);
  const posting = new LedgerPosting(context.companyId, "bank_transaction", transaction.id, transaction.bookingDate, transaction.currency);

  if (transaction.matchType === "invoice" || transaction.amount > 0) {
    posting.debit(context.bankAccountId, amount);
    posting.credit(context.accountsReceivableAccountId, amount, transaction.matchedRecordId);
    return posting.toJournalDraft("payment", `Accepted incoming bank match ${transaction.id}.`);
  }

  if (transaction.matchType === "receipt") {
    posting.debit(context.accountsPayableAccountId, amount, transaction.matchedRecordId);
    posting.credit(context.bankAccountId, amount);
    return posting.toJournalDraft("payment", `Accepted outgoing bank match ${transaction.id}.`);
  }

  posting.debit(context.bankFeeAccountId, amount);
  posting.credit(context.bankAccountId, amount);
  return posting.toJournalDraft("payment", `Booked bank fee or manual debit ${transaction.id}.`);
}
