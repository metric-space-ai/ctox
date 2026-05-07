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

export type BalanceSheetReport = {
  assets: number;
  equity: number;
  liabilities: number;
  rows: TrialBalanceRow[];
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

export function buildBalanceSheet(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): BalanceSheetReport {
  const rows = buildTrialBalanceFromEntries(input).filter((row) => row.account.rootType === "asset" || row.account.rootType === "liability" || row.account.rootType === "equity");
  const assets = round(rows.filter((row) => row.account.rootType === "asset").reduce((sum, row) => sum + row.balance, 0));
  const liabilities = round(rows.filter((row) => row.account.rootType === "liability").reduce((sum, row) => sum + row.balance, 0));
  const equity = round(rows.filter((row) => row.account.rootType === "equity").reduce((sum, row) => sum + row.balance, 0));
  return {
    assets,
    equity,
    liabilities,
    rows
  };
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
