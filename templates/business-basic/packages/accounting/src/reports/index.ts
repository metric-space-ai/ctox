import type { ChartAccount } from "../chart";
import { moneyToMajor } from "../money";
import type { JournalDraft } from "../posting/service";

export type TrialBalanceSourceLine = {
  accountCode: string;
  accountName: string;
  credit: number;
  debit: number;
};

export function summarizeTrialBalance(lines: TrialBalanceSourceLine[]) {
  return {
    credit: round(lines.reduce((sum, line) => sum + line.credit, 0)),
    debit: round(lines.reduce((sum, line) => sum + line.debit, 0)),
    rows: lines
  };
}

export type GeneralLedgerRow = {
  accountId: string;
  credit: number;
  debit: number;
  journalRef: string;
  partyId?: string;
  postingDate: string;
  refId: string;
  refType: string;
  runningBalance: number;
};

export type TrialBalanceRow = {
  account: ChartAccount;
  balance: number;
  credit: number;
  debit: number;
};

export type ProfitAndLossReport = {
  expense: number;
  income: number;
  netIncome: number;
  rows: TrialBalanceRow[];
};

export type BusinessAnalysisReport = {
  depreciation: number;
  directCosts: number;
  ebit: number;
  grossProfit: number;
  operatingExpenses: number;
  personnelCosts: number;
  revenue: number;
  rows: Array<TrialBalanceRow & { bwaGroup: "depreciation" | "direct_costs" | "operating_expenses" | "personnel_costs" | "revenue" }>;
};

export type BalanceSheetReport = {
  assets: number;
  equity: number;
  retainedEarnings: number;
  liabilities: number;
  balanced: boolean;
  difference: number;
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

export type VatStatementReport = {
  boxes: VatStatementBox[];
  inputVat: number;
  netPosition: number;
  outputVat: number;
  payable: number;
  refundable: number;
  rows: Array<TrialBalanceRow & { vatKind: "input" | "other" | "output" }>;
};

export type OpenItemStatus = "open" | "paid" | "partial";

export type OpenItemRow = {
  account: ChartAccount;
  ageDays?: number;
  dueDate?: string;
  kind: "payable" | "receivable";
  originalAmount: number;
  outstandingAmount: number;
  paidAmount: number;
  partyId?: string;
  postingDate: string;
  refId: string;
  refType: string;
  status: OpenItemStatus;
};

export type OpenItemsReport = {
  asOf: string;
  buckets: {
    current: number;
    overdue1To30: number;
    overdue31To60: number;
    overdue61To90: number;
    overdueOver90: number;
  };
  openPayables: number;
  openReceivables: number;
  rows: OpenItemRow[];
};

export function buildGeneralLedger(input: { accountId: string; entries: JournalDraft[] }): GeneralLedgerRow[] {
  let runningBalance = 0;
  return input.entries
    .flatMap((entry) => entry.lines.map((line) => ({ entry, line })))
    .filter(({ line }) => line.accountId === input.accountId)
    .sort((left, right) => `${left.entry.postingDate}:${left.entry.refId}`.localeCompare(`${right.entry.postingDate}:${right.entry.refId}`))
    .map(({ entry, line }) => {
      const debit = moneyToMajor(line.debit);
      const credit = moneyToMajor(line.credit);
      runningBalance = round(runningBalance + debit - credit);
      return {
        accountId: line.accountId,
        credit,
        debit,
        journalRef: `${entry.type}:${entry.refType}:${entry.refId}`,
        partyId: line.partyId,
        postingDate: entry.postingDate,
        refId: entry.refId,
        refType: entry.refType,
        runningBalance
      };
    });
}

export function buildOpenItems(input: {
  accounts: ChartAccount[];
  asOf?: string;
  dueDatesByRef?: Record<string, string | undefined>;
  entries: JournalDraft[];
  includePaid?: boolean;
}): OpenItemsReport {
  const asOf = input.asOf ?? new Date().toISOString().slice(0, 10);
  const accountById = new Map(input.accounts.map((account) => [account.id, account]));
  const sourceRows: OpenItemRow[] = [];
  const settlementLines: Array<{ accountId: string; amount: number; kind: OpenItemRow["kind"]; partyId?: string; refId: string }> = [];

  for (const entry of input.entries) {
    for (const line of entry.lines) {
      const account = accountById.get(line.accountId);
      if (!account || (account.accountType !== "receivable" && account.accountType !== "payable")) continue;

      const debit = moneyToMajor(line.debit);
      const credit = moneyToMajor(line.credit);
      const kind = account.accountType === "receivable" ? "receivable" : "payable";
      const signed = kind === "receivable" ? debit - credit : credit - debit;
      if (signed === 0) continue;

      if ((entry.type === "invoice" && kind === "receivable" && signed > 0)
        || (entry.type === "receipt" && kind === "payable" && signed > 0)
        || (entry.type === "manual" && signed > 0)) {
        const dueDate = input.dueDatesByRef?.[entry.refId];
        sourceRows.push({
          account,
          ageDays: dueDate ? daysBetween(dueDate, asOf) : undefined,
          dueDate,
          kind,
          originalAmount: round(signed),
          outstandingAmount: round(signed),
          paidAmount: 0,
          partyId: line.partyId,
          postingDate: entry.postingDate,
          refId: entry.refId,
          refType: entry.refType,
          status: "open"
        });
      } else if (signed < 0) {
        settlementLines.push({
          accountId: account.id,
          amount: round(Math.abs(signed)),
          kind,
          partyId: line.partyId,
          refId: entry.refId
        });
      }
    }
  }

  for (const settlement of settlementLines) {
    let remaining = settlement.amount;
    const candidates = sourceRows
      .filter((row) => row.kind === settlement.kind
        && row.account.id === settlement.accountId
        && row.outstandingAmount > 0
        && (!settlement.partyId || settlement.partyId === row.refId || settlement.partyId === row.partyId))
      .sort((left, right) => left.postingDate.localeCompare(right.postingDate));

    for (const row of candidates) {
      if (remaining <= 0) break;
      const applied = Math.min(row.outstandingAmount, remaining);
      row.paidAmount = round(row.paidAmount + applied);
      row.outstandingAmount = round(row.outstandingAmount - applied);
      remaining = round(remaining - applied);
    }
  }

  for (const row of sourceRows) {
    row.status = row.outstandingAmount <= 0 ? "paid" : row.paidAmount > 0 ? "partial" : "open";
  }

  const rows = sourceRows
    .filter((row) => input.includePaid || row.status !== "paid")
    .sort((left, right) => `${left.dueDate ?? left.postingDate}:${left.refId}`.localeCompare(`${right.dueDate ?? right.postingDate}:${right.refId}`));

  return {
    asOf,
    buckets: buildAgingBuckets(rows),
    openPayables: round(rows.filter((row) => row.kind === "payable").reduce((sum, row) => sum + row.outstandingAmount, 0)),
    openReceivables: round(rows.filter((row) => row.kind === "receivable").reduce((sum, row) => sum + row.outstandingAmount, 0)),
    rows
  };
}

export function buildTrialBalanceFromEntries(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): TrialBalanceRow[] {
  return input.accounts.map((account) => {
    const lines = input.entries.flatMap((entry) => entry.lines).filter((line) => line.accountId === account.id);
    const debit = round(lines.reduce((sum, line) => sum + moneyToMajor(line.debit), 0));
    const credit = round(lines.reduce((sum, line) => sum + moneyToMajor(line.credit), 0));
    const debitNormal = account.rootType === "asset" || account.rootType === "expense";
    return {
      account,
      balance: round(debitNormal ? debit - credit : credit - debit),
      credit,
      debit
    };
  }).filter((row) => row.debit !== 0 || row.credit !== 0);
}

function buildAgingBuckets(rows: OpenItemRow[]): OpenItemsReport["buckets"] {
  const buckets: OpenItemsReport["buckets"] = {
    current: 0,
    overdue1To30: 0,
    overdue31To60: 0,
    overdue61To90: 0,
    overdueOver90: 0
  };

  for (const row of rows) {
    const age = row.ageDays ?? 0;
    if (age <= 0) buckets.current = round(buckets.current + row.outstandingAmount);
    else if (age <= 30) buckets.overdue1To30 = round(buckets.overdue1To30 + row.outstandingAmount);
    else if (age <= 60) buckets.overdue31To60 = round(buckets.overdue31To60 + row.outstandingAmount);
    else if (age <= 90) buckets.overdue61To90 = round(buckets.overdue61To90 + row.outstandingAmount);
    else buckets.overdueOver90 = round(buckets.overdueOver90 + row.outstandingAmount);
  }

  return buckets;
}

function daysBetween(startDate: string, endDate: string) {
  const start = new Date(`${startDate.slice(0, 10)}T00:00:00.000Z`).getTime();
  const end = new Date(`${endDate.slice(0, 10)}T00:00:00.000Z`).getTime();
  return Math.floor((end - start) / 86_400_000);
}

export function buildProfitAndLoss(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): ProfitAndLossReport {
  const rows = buildTrialBalanceFromEntries(input).filter((row) => row.account.rootType === "income" || row.account.rootType === "expense");
  const income = round(rows.filter((row) => row.account.rootType === "income").reduce((sum, row) => sum + row.balance, 0));
  const expense = round(rows.filter((row) => row.account.rootType === "expense").reduce((sum, row) => sum + row.balance, 0));
  return {
    expense,
    income,
    netIncome: round(income - expense),
    rows
  };
}

export function buildBusinessAnalysis(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): BusinessAnalysisReport {
  const rows = buildTrialBalanceFromEntries(input)
    .filter((row) => row.account.rootType === "income" || row.account.rootType === "expense")
    .map((row) => ({
      ...row,
      bwaGroup: bwaGroup(row.account)
    }));
  const amountFor = (group: BusinessAnalysisReport["rows"][number]["bwaGroup"]) => round(rows
    .filter((row) => row.bwaGroup === group)
    .reduce((sum, row) => sum + row.balance, 0));
  const revenue = amountFor("revenue");
  const directCosts = amountFor("direct_costs");
  const personnelCosts = amountFor("personnel_costs");
  const operatingExpenses = amountFor("operating_expenses");
  const depreciation = amountFor("depreciation");
  const grossProfit = round(revenue - directCosts);
  return {
    depreciation,
    directCosts,
    ebit: round(grossProfit - personnelCosts - operatingExpenses - depreciation),
    grossProfit,
    operatingExpenses,
    personnelCosts,
    revenue,
    rows
  };
}

export function buildBalanceSheet(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): BalanceSheetReport {
  const rows = buildTrialBalanceFromEntries(input).filter((row) => row.account.rootType === "asset" || row.account.rootType === "liability" || row.account.rootType === "equity");
  const pnl = buildProfitAndLoss(input);
  const assets = round(rows.filter((row) => row.account.rootType === "asset").reduce((sum, row) => sum + row.balance, 0));
  const liabilities = round(rows.filter((row) => row.account.rootType === "liability").reduce((sum, row) => sum + row.balance, 0));
  const equity = round(rows.filter((row) => row.account.rootType === "equity").reduce((sum, row) => sum + row.balance, 0) + pnl.netIncome);
  const difference = round(assets - liabilities - equity);
  return {
    assets,
    balanced: Math.abs(difference) < 0.01,
    difference,
    equity,
    retainedEarnings: pnl.netIncome,
    liabilities,
    rows
  };
}

export function buildVatStatement(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): VatStatementReport {
  const rows = buildTrialBalanceFromEntries(input)
    .filter((row) => row.account.accountType === "tax")
    .map((row) => ({
      ...row,
      vatKind: vatKind(row.account)
    }));
  const inputVat = round(Math.abs(rows.filter((row) => row.vatKind === "input").reduce((sum, row) => sum + row.balance, 0)));
  const outputVat = round(rows.filter((row) => row.vatKind === "output").reduce((sum, row) => sum + row.balance, 0));
  const netPosition = round(outputVat - inputVat);
  return {
    boxes: buildVatStatementBoxes({ accounts: input.accounts, entries: input.entries, inputVat, netPosition, outputVat }),
    inputVat,
    netPosition,
    outputVat,
    payable: Math.max(0, netPosition),
    refundable: Math.max(0, round(-netPosition)),
    rows
  };
}

function buildVatStatementBoxes(input: { accounts: ChartAccount[]; entries: JournalDraft[]; inputVat: number; netPosition: number; outputVat: number }): VatStatementBox[] {
  const taxAccountIds = new Set(input.accounts.filter((account) => account.accountType === "tax").map((account) => account.id));
  const outputTaxByCode = taxLineAmountsByCode(input.entries, taxAccountIds, "credit");
  const taxableBase19 = taxableBaseForOutputCode(input.entries, taxAccountIds, "DE_19_OUTPUT", 19, outputTaxByCode.get("DE_19_OUTPUT") ?? input.outputVat);
  const taxableBase7 = taxableBaseForOutputCode(input.entries, taxAccountIds, "DE_7_OUTPUT", 7, outputTaxByCode.get("DE_7_OUTPUT") ?? 0);
  const reverseChargeBase = taxableBaseForTaxCode(input.entries, taxAccountIds, "DE_RC");
  const kleinunternehmerBase = taxableBaseForTaxCode(input.entries, taxAccountIds, "DE_KU");
  return [
    {
      amount: taxableBase19,
      amountKind: "base",
      code: "81",
      label: "Steuerpflichtige Umsaetze 19%",
      source: outputTaxByCode.has("DE_19_OUTPUT") ? "invoice_revenue_lines_de_19_output" : "derived_from_output_vat_19",
      taxRate: 19
    },
    {
      amount: taxableBase7,
      amountKind: "base",
      code: "86",
      label: "Steuerpflichtige Umsaetze 7%",
      source: outputTaxByCode.has("DE_7_OUTPUT") ? "invoice_revenue_lines_de_7_output" : "derived_from_output_vat_7",
      taxRate: 7
    },
    {
      amount: reverseChargeBase,
      amountKind: "base",
      code: "RC",
      label: "Reverse-Charge / nicht steuerbare Umsaetze",
      source: "journal_lines_de_rc"
    },
    {
      amount: kleinunternehmerBase,
      amountKind: "base",
      code: "KU",
      label: "Kleinunternehmer-Umsaetze",
      source: "journal_lines_de_ku"
    },
    {
      amount: input.inputVat,
      amountKind: "tax",
      code: "66",
      label: "Abziehbare Vorsteuer",
      source: "input_vat_tax_accounts"
    },
    {
      amount: input.netPosition,
      amountKind: "settlement",
      code: "83",
      label: "Verbleibende Vorauszahlung / Ueberschuss",
      source: "output_vat_minus_input_vat"
    }
  ];
}

function taxableBaseForOutputCode(entries: JournalDraft[], taxAccountIds: Set<string>, taxCode: string, taxRate: number, fallbackTaxAmount: number) {
  const lineBase = taxableBaseForTaxCode(entries, taxAccountIds, taxCode);
  if (lineBase > 0) return lineBase;
  return fallbackTaxAmount === 0 ? 0 : round(fallbackTaxAmount / (taxRate / 100));
}

function taxableBaseForTaxCode(entries: JournalDraft[], taxAccountIds: Set<string>, taxCode: string) {
  return round(entries
    .flatMap((entry) => entry.lines)
    .filter((line) => line.taxCode === taxCode && !taxAccountIds.has(line.accountId) && moneyToMajor(line.credit) > 0)
    .reduce((sum, line) => sum + moneyToMajor(line.credit), 0));
}

function taxLineAmountsByCode(entries: JournalDraft[], taxAccountIds: Set<string>, side: "credit" | "debit") {
  const amounts = new Map<string, number>();
  for (const line of entries.flatMap((entry) => entry.lines)) {
    if (!line.taxCode) continue;
    if (!taxAccountIds.has(line.accountId)) continue;
    const amount = moneyToMajor(side === "credit" ? line.credit : line.debit);
    if (amount <= 0) continue;
    amounts.set(line.taxCode, round((amounts.get(line.taxCode) ?? 0) + amount));
  }
  return amounts;
}

function vatKind(account: ChartAccount): "input" | "other" | "output" {
  if (account.rootType === "asset") return "input";
  if (account.rootType === "liability") return "output";
  return "other";
}

function bwaGroup(account: ChartAccount): BusinessAnalysisReport["rows"][number]["bwaGroup"] {
  if (account.rootType === "income") return "revenue";
  if (account.accountType === "cogs") return "direct_costs";
  if (account.accountType === "depreciation" || account.code.startsWith("48")) return "depreciation";
  if (/^(41|42|60|61)/.test(account.code)) return "personnel_costs";
  return "operating_expenses";
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
