import type {
  BusinessAccount,
  BusinessBankTransaction,
  BusinessBundle,
  BusinessInvoice,
  BusinessJournalEntry,
  BusinessReceipt
} from "./business-seed";

export type LedgerRow = {
  id: string;
  account: BusinessAccount;
  credit: number;
  debit: number;
  entry: BusinessJournalEntry;
  partyLabel: string;
  refLabel: string;
  signedAmount: number;
};

export type TrialBalanceRow = {
  account: BusinessAccount;
  balance: number;
  credit: number;
  debit: number;
};

export type AccountingSnapshot = {
  bankBalance: number;
  expenseTotal: number;
  inputVat: number;
  openReceiptTotal: number;
  outputVat: number;
  payableBalance: number;
  receivableBalance: number;
  revenueTotal: number;
  taxableRevenue: number;
  vatPayable: number;
};

export type ReconciliationRow = BusinessBankTransaction & {
  matchedLabel: string;
  nextAction: string;
};

export function buildLedgerRows(data: BusinessBundle): LedgerRow[] {
  return data.journalEntries
    .flatMap((entry) => entry.lines.map((line, index) => {
      const account = accountById(data, line.accountId);
      return {
        id: `${entry.id}-${index}`,
        account,
        credit: line.credit,
        debit: line.debit,
        entry,
        partyLabel: partyLabel(data, line.partyId),
        refLabel: referenceLabel(data, entry.refType, entry.refId),
        signedAmount: signedAccountAmount(account, line.debit, line.credit)
      };
    }))
    .sort((left, right) => `${right.entry.postingDate}-${right.entry.number}`.localeCompare(`${left.entry.postingDate}-${left.entry.number}`));
}

export function buildTrialBalance(data: BusinessBundle): TrialBalanceRow[] {
  const rows = data.accounts.map((account) => {
    const accountLines = data.journalEntries.flatMap((entry) => entry.lines).filter((line) => line.accountId === account.id);
    const debit = roundMoney(accountLines.reduce((sum, line) => sum + line.debit, 0));
    const credit = roundMoney(accountLines.reduce((sum, line) => sum + line.credit, 0));
    return {
      account,
      balance: roundMoney(signedAccountAmount(account, debit, credit)),
      credit,
      debit
    };
  });

  return rows.filter((row) => row.debit !== 0 || row.credit !== 0).sort((left, right) => left.account.code.localeCompare(right.account.code));
}

export function buildAccountingSnapshot(data: BusinessBundle): AccountingSnapshot {
  const trialBalance = buildTrialBalance(data);
  const balanceFor = (predicate: (account: BusinessAccount) => boolean) => trialBalance
    .filter((row) => predicate(row.account))
    .reduce((sum, row) => sum + row.balance, 0);
  const outputVat = balanceFor((account) => account.id === "acc-vat-output");
  const inputVat = Math.abs(balanceFor((account) => account.id === "acc-vat-input"));

  return {
    bankBalance: roundMoney(balanceFor((account) => account.accountType === "bank")),
    expenseTotal: roundMoney(balanceFor((account) => account.rootType === "expense")),
    inputVat: roundMoney(inputVat),
    openReceiptTotal: roundMoney(data.receipts.filter((receipt) => receipt.status !== "Paid" && receipt.status !== "Rejected").reduce((sum, receipt) => sum + receipt.total, 0)),
    outputVat: roundMoney(outputVat),
    payableBalance: roundMoney(balanceFor((account) => account.accountType === "payable")),
    receivableBalance: roundMoney(balanceFor((account) => account.accountType === "receivable")),
    revenueTotal: roundMoney(balanceFor((account) => account.rootType === "income")),
    taxableRevenue: roundMoney(data.invoices.filter((invoice) => invoice.currency === "EUR").reduce((sum, invoice) => sum + invoiceNet(invoice), 0)),
    vatPayable: roundMoney(outputVat - inputVat)
  };
}

export function buildReconciliationRows(data: BusinessBundle): ReconciliationRow[] {
  return data.bankTransactions
    .map((transaction) => ({
      ...transaction,
      matchedLabel: transaction.matchedRecordId ? referenceLabel(data, transaction.matchType === "receipt" ? "receipt" : "invoice", transaction.matchedRecordId) : "-",
      nextAction: reconciliationAction(transaction)
    }))
    .sort((left, right) => right.bookingDate.localeCompare(left.bookingDate));
}

export function buildReceiptQueue(data: BusinessBundle) {
  return data.receipts
    .map((receipt) => ({
      ...receipt,
      expenseAccount: accountById(data, receipt.expenseAccountId),
      payableAccount: accountById(data, receipt.payableAccountId),
      bankTransaction: receipt.bankTransactionId ? data.bankTransactions.find((transaction) => transaction.id === receipt.bankTransactionId) : undefined
    }))
    .sort((left, right) => receiptPriority(left) - receiptPriority(right) || right.receiptDate.localeCompare(left.receiptDate));
}

export function buildDatevLines(data: BusinessBundle, exportId?: string) {
  return data.journalEntries
    .filter((entry) => !exportId || entry.exportId === exportId)
    .flatMap((entry) => entry.lines.map((line) => {
      const account = accountById(data, line.accountId);
      const contraLine = entry.lines.find((candidate) => candidate.accountId !== line.accountId && (candidate.credit > 0 || candidate.debit > 0));
      const contraAccount = contraLine ? accountById(data, contraLine.accountId) : undefined;
      return {
        account,
        amount: Math.max(line.debit, line.credit),
        contraAccount,
        entry,
        side: line.debit > 0 ? "S" : "H",
        taxCode: line.taxCode ?? account.taxCode ?? ""
      };
    }));
}

export function isBalanced(entry: BusinessJournalEntry) {
  const debit = entry.lines.reduce((sum, line) => sum + line.debit, 0);
  const credit = entry.lines.reduce((sum, line) => sum + line.credit, 0);
  return Math.abs(debit - credit) < 0.01;
}

export function accountById(data: BusinessBundle, accountId: string) {
  return data.accounts.find((account) => account.id === accountId) ?? data.accounts[0];
}

function signedAccountAmount(account: BusinessAccount, debit: number, credit: number) {
  const debitNormal = account.rootType === "asset" || account.rootType === "expense";
  return roundMoney(debitNormal ? debit - credit : credit - debit);
}

function partyLabel(data: BusinessBundle, partyId?: string) {
  if (!partyId) return "-";
  return data.customers.find((customer) => customer.id === partyId)?.name ?? partyId;
}

function referenceLabel(data: BusinessBundle, refType?: string, refId?: string) {
  if (!refId) return "-";
  if (refType === "invoice") return data.invoices.find((invoice) => invoice.id === refId)?.number ?? refId;
  if (refType === "receipt") return data.receipts.find((receipt) => receipt.id === refId)?.number ?? refId;
  if (refType === "bank_transaction") return data.bankTransactions.find((transaction) => transaction.id === refId)?.counterparty ?? refId;
  return refId;
}

function reconciliationAction(transaction: BusinessBankTransaction) {
  if (transaction.status === "Matched") return "Posted";
  if (transaction.status === "Suggested") return transaction.matchType === "fee" ? "Review fee account" : "Confirm match";
  if (transaction.status === "Ignored") return "No posting";
  return "Create receipt or manual posting";
}

function receiptPriority(receipt: BusinessReceipt) {
  if (receipt.status === "Needs review") return 0;
  if (receipt.status === "Inbox") return 1;
  if (receipt.status === "Posted") return 2;
  if (receipt.status === "Paid") return 3;
  return 4;
}

function invoiceNet(invoice: BusinessInvoice) {
  return invoice.netAmount ?? invoice.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);
}

function roundMoney(amount: number) {
  return Math.round((amount + Number.EPSILON) * 100) / 100;
}
