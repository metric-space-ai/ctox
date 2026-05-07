import type {
  BusinessAccount,
  BusinessBankTransaction,
  BusinessBundle,
  BusinessFixedAsset,
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
  fixedAssetNetBookValue: number;
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

export type ProfitAndLossRuntimeReport = {
  expense: number;
  income: number;
  netIncome: number;
  rows: TrialBalanceRow[];
};

export type BusinessAnalysisRuntimeReport = {
  depreciation: number;
  directCosts: number;
  ebit: number;
  grossProfit: number;
  operatingExpenses: number;
  personnelCosts: number;
  revenue: number;
  rows: Array<TrialBalanceRow & { bwaGroup: "depreciation" | "direct_costs" | "operating_expenses" | "personnel_costs" | "revenue" }>;
};

export type BalanceSheetRuntimeReport = {
  assets: number;
  balanced: boolean;
  difference: number;
  equity: number;
  liabilities: number;
  retainedEarnings: number;
  rows: TrialBalanceRow[];
};

export type VatStatementBox = {
  amount: number;
  amountKind: "base" | "settlement" | "tax";
  code: "66" | "81" | "83" | "86" | "KU" | "RC";
  label: string;
  source: string;
  taxRate?: number;
};

export type VatStatementRuntimeReport = {
  boxes: VatStatementBox[];
  inputVat: number;
  netPosition: number;
  outputVat: number;
  payable: number;
  refundable: number;
  rows: Array<TrialBalanceRow & { vatKind: "input" | "other" | "output" }>;
};

export type FiscalPeriodRuntimeState = {
  closedCount: number;
  currentPeriod?: BusinessBundle["fiscalPeriods"][number];
  lastClosedPeriod?: BusinessBundle["fiscalPeriods"][number];
  nextClosablePeriod?: BusinessBundle["fiscalPeriods"][number];
  openCount: number;
  periodCount: number;
};

export type FixedAssetRegisterRow = BusinessFixedAsset & {
  accumulatedDepreciation: number;
  bookValue: number;
  currentYearDepreciation: number;
  schedule: Array<{
    accumulatedDepreciation: number;
    amount: number;
    bookValue: number;
    fiscalYear: number;
    posted: boolean;
  }>;
};

export type ReconciliationRow = BusinessBankTransaction & {
  matchedLabel: string;
  nextAction: string;
};

export function buildLedgerRows(data: BusinessBundle): LedgerRow[] {
  return postedJournalEntries(data)
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
  const entries = postedJournalEntries(data);
  const rows = data.accounts.map((account) => {
    const accountLines = entries.flatMap((entry) => entry.lines).filter((line) => line.accountId === account.id);
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

export function buildProfitAndLoss(data: BusinessBundle): ProfitAndLossRuntimeReport {
  const rows = buildTrialBalance(data).filter((row) => row.account.rootType === "income" || row.account.rootType === "expense");
  const income = roundMoney(rows.filter((row) => row.account.rootType === "income").reduce((sum, row) => sum + row.balance, 0));
  const expense = roundMoney(rows.filter((row) => row.account.rootType === "expense").reduce((sum, row) => sum + row.balance, 0));
  return {
    expense,
    income,
    netIncome: roundMoney(income - expense),
    rows
  };
}

export function buildBusinessAnalysis(data: BusinessBundle): BusinessAnalysisRuntimeReport {
  const rows = buildTrialBalance(data)
    .filter((row) => row.account.rootType === "income" || row.account.rootType === "expense")
    .map((row) => ({
      ...row,
      bwaGroup: bwaGroup(row.account)
    }));
  const amountFor = (group: BusinessAnalysisRuntimeReport["rows"][number]["bwaGroup"]) => roundMoney(rows
    .filter((row) => row.bwaGroup === group)
    .reduce((sum, row) => sum + row.balance, 0));
  const revenue = amountFor("revenue");
  const directCosts = amountFor("direct_costs");
  const personnelCosts = amountFor("personnel_costs");
  const operatingExpenses = amountFor("operating_expenses");
  const depreciation = amountFor("depreciation");
  const grossProfit = roundMoney(revenue - directCosts);
  return {
    depreciation,
    directCosts,
    ebit: roundMoney(grossProfit - personnelCosts - operatingExpenses - depreciation),
    grossProfit,
    operatingExpenses,
    personnelCosts,
    revenue,
    rows
  };
}

export function buildBalanceSheet(data: BusinessBundle): BalanceSheetRuntimeReport {
  const rows = buildTrialBalance(data).filter((row) => row.account.rootType === "asset" || row.account.rootType === "liability" || row.account.rootType === "equity");
  const pnl = buildProfitAndLoss(data);
  const assets = roundMoney(rows.filter((row) => row.account.rootType === "asset").reduce((sum, row) => sum + row.balance, 0));
  const liabilities = roundMoney(rows.filter((row) => row.account.rootType === "liability").reduce((sum, row) => sum + row.balance, 0));
  const equity = roundMoney(rows.filter((row) => row.account.rootType === "equity").reduce((sum, row) => sum + row.balance, 0) + pnl.netIncome);
  const difference = roundMoney(assets - liabilities - equity);
  return {
    assets,
    balanced: Math.abs(difference) < 0.01,
    difference,
    equity,
    liabilities,
    retainedEarnings: pnl.netIncome,
    rows
  };
}

export function buildVatStatement(data: BusinessBundle): VatStatementRuntimeReport {
  const rows = buildTrialBalance(data)
    .filter((row) => row.account.accountType === "tax")
    .map((row) => ({
      ...row,
      vatKind: vatKind(row.account)
    }));
  const inputVat = roundMoney(Math.abs(rows.filter((row) => row.vatKind === "input").reduce((sum, row) => sum + row.balance, 0)));
  const outputVat = roundMoney(rows.filter((row) => row.vatKind === "output").reduce((sum, row) => sum + row.balance, 0));
  const netPosition = roundMoney(outputVat - inputVat);
  return {
    boxes: buildVatStatementBoxes(data, { inputVat, netPosition, outputVat }),
    inputVat,
    netPosition,
    outputVat,
    payable: Math.max(0, netPosition),
    refundable: Math.max(0, roundMoney(-netPosition)),
    rows
  };
}

function buildVatStatementBoxes(data: BusinessBundle, input: { inputVat: number; netPosition: number; outputVat: number }): VatStatementBox[] {
  return [
    {
      amount: taxableInvoiceBaseByRate(data, 19),
      amountKind: "base",
      code: "81",
      label: "Steuerpflichtige Umsaetze 19%",
      source: "invoice_lines_tax_rate_19",
      taxRate: 19
    },
    {
      amount: taxableInvoiceBaseByRate(data, 7),
      amountKind: "base",
      code: "86",
      label: "Steuerpflichtige Umsaetze 7%",
      source: "invoice_lines_tax_rate_7",
      taxRate: 7
    },
    {
      amount: taxableInvoiceBaseByRate(data, 0),
      amountKind: "base",
      code: "RC",
      label: "Reverse-Charge / nicht steuerbare Umsaetze",
      source: "invoice_lines_tax_rate_0"
    },
    {
      amount: input.inputVat,
      amountKind: "tax",
      code: "66",
      label: "Abziehbare Vorsteuer",
      source: "tax_account_1576"
    },
    {
      amount: input.netPosition,
      amountKind: "settlement",
      code: "83",
      label: "Verbleibende Vorauszahlung / Ueberschuss",
      source: "tax_accounts_1776_minus_1576"
    }
  ];
}

export function buildFiscalPeriodState(data: BusinessBundle, today = new Date().toISOString().slice(0, 10)): FiscalPeriodRuntimeState {
  const periods = data.fiscalPeriods.filter((period) => isMonthlyFiscalPeriod(period.id));
  const closedPeriods = periods.filter((period) => period.status === "closed");
  const openPeriods = periods.filter((period) => period.status === "open");
  return {
    closedCount: closedPeriods.length,
    currentPeriod: periods.find((period) => period.startDate <= today && period.endDate >= today),
    lastClosedPeriod: closedPeriods.sort((left, right) => right.endDate.localeCompare(left.endDate))[0],
    nextClosablePeriod: openPeriods
      .filter((period) => period.endDate < today)
      .sort((left, right) => left.endDate.localeCompare(right.endDate))[0],
    openCount: openPeriods.length,
    periodCount: periods.length
  };
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
    fixedAssetNetBookValue: roundMoney(balanceFor((account) => account.accountType === "fixed_asset" || account.accountType === "accumulated_depreciation")),
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

export function buildFixedAssetRegister(data: BusinessBundle): FixedAssetRegisterRow[] {
  return data.fixedAssets.map((asset) => {
    const assetEntries = postedJournalEntries(data).filter((entry) => entry.refType === "asset" && journalEntryBelongsToAsset(entry.refId, asset.id));
    const assetAccountBalance = assetEntries
      .flatMap((entry) => entry.lines)
      .filter((line) => line.accountId === asset.assetAccountId)
      .reduce((sum, line) => sum + line.debit - line.credit, 0);
    const accumulatedDepreciation = assetEntries
      .flatMap((entry) => entry.lines)
      .filter((line) => line.accountId === asset.accumulatedDepreciationAccountId)
      .reduce((sum, line) => sum + line.credit - line.debit, 0);
    const currentYearDepreciation = postedJournalEntries(data)
      .filter((entry) => entry.refType === "asset" && entry.refId === asset.id && entry.type === "depreciation" && entry.postingDate.startsWith("2026-"))
      .flatMap((entry) => entry.lines)
      .filter((line) => line.accountId === asset.depreciationExpenseAccountId)
      .reduce((sum, line) => sum + line.debit - line.credit, 0);
    const disposed = asset.status === "Disposed" || assetEntries.some((entry) => entry.type === "manual"
      && entry.lines.some((line) => line.accountId === asset.assetAccountId && line.credit >= asset.acquisitionCost));
    const bookValue = disposed ? 0 : assetEntries.length ? assetAccountBalance - accumulatedDepreciation : asset.acquisitionCost - accumulatedDepreciation;
    const annual = straightLineAnnualSchedule(asset, accumulatedDepreciation);
    return {
      ...asset,
      accumulatedDepreciation: roundMoney(accumulatedDepreciation),
      bookValue: roundMoney(bookValue),
      currentYearDepreciation: roundMoney(currentYearDepreciation),
      schedule: annual,
      status: disposed ? "Disposed" : asset.status
    };
  }).sort((left, right) => left.name.localeCompare(right.name));
}

function journalEntryBelongsToAsset(refId: string, assetId: string) {
  return refId === assetId
    || refId === `${assetId}-disposal`
    || (refId.startsWith(`${assetId}-`) && /^\d{4}$/.test(refId.slice(assetId.length + 1)));
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
  return postedJournalEntries(data)
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

function vatKind(account: BusinessAccount): "input" | "other" | "output" {
  if (account.rootType === "asset") return "input";
  if (account.rootType === "liability") return "output";
  return "other";
}

function bwaGroup(account: BusinessAccount): BusinessAnalysisRuntimeReport["rows"][number]["bwaGroup"] {
  if (account.rootType === "income") return "revenue";
  if (account.accountType === "depreciation" || account.code.startsWith("48")) return "depreciation";
  if (/^(41|42|60|61)/.test(account.code)) return "personnel_costs";
  return "operating_expenses";
}

function postedJournalEntries(data: BusinessBundle) {
  return data.journalEntries.filter((entry) => entry.status === "Posted");
}

function accountPostedBalance(data: BusinessBundle, accountId: string, normalSide: "credit" | "debit") {
  return postedJournalEntries(data)
    .flatMap((entry) => entry.lines)
    .filter((line) => line.accountId === accountId)
    .reduce((sum, line) => sum + (normalSide === "credit" ? line.credit - line.debit : line.debit - line.credit), 0);
}

function straightLineAnnualSchedule(asset: BusinessFixedAsset, postedAccumulated: number) {
  const years = Math.ceil(asset.usefulLifeMonths / 12);
  const depreciable = Math.max(0, asset.acquisitionCost - asset.salvageValue);
  const annual = roundMoney(depreciable / years);
  let accumulated = 0;
  return Array.from({ length: years }, (_, index) => {
    const fiscalYear = Number(asset.acquisitionDate.slice(0, 4)) + index;
    const isLast = index === years - 1;
    const amount = isLast ? roundMoney(depreciable - accumulated) : annual;
    accumulated = roundMoney(accumulated + amount);
    return {
      accumulatedDepreciation: accumulated,
      amount,
      bookValue: roundMoney(asset.acquisitionCost - accumulated),
      fiscalYear,
      posted: postedAccumulated + 0.01 >= accumulated
    };
  });
}

function partyLabel(data: BusinessBundle, partyId?: string) {
  if (!partyId) return "-";
  return data.customers.find((customer) => customer.id === partyId)?.name ?? partyId;
}

function referenceLabel(data: BusinessBundle, refType?: string, refId?: string) {
  if (!refId) return "-";
  if (refType === "invoice") return data.invoices.find((invoice) => invoice.id === refId)?.number ?? refId;
  if (refType === "receipt") return data.receipts.find((receipt) => receipt.id === refId)?.number ?? refId;
  if (refType === "asset") return data.fixedAssets.find((asset) => asset.id === refId)?.name ?? refId;
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

function taxableInvoiceBaseByRate(data: BusinessBundle, taxRate: number) {
  return roundMoney(data.invoices
    .filter((invoice) => invoice.status !== "Draft" && invoice.currency === "EUR")
    .flatMap((invoice) => invoice.lines)
    .filter((line) => line.taxRate === taxRate)
    .reduce((sum, line) => sum + line.quantity * line.unitPrice, 0));
}

function isMonthlyFiscalPeriod(periodId: string) {
  return /^fy-\d{4}-\d{2}$/.test(periodId);
}

function roundMoney(amount: number) {
  return Math.round((amount + Number.EPSILON) * 100) / 100;
}
