export type FiscalPeriod = {
  closedAt?: string;
  endDate: string;
  id: string;
  startDate: string;
  status: "closed" | "open";
};

export type PeriodCloseChecklistInput = {
  datevExported?: boolean;
  entries?: Array<{
    lines: Array<{ credit: { minor: number }; debit: { minor: number } }>;
    postingDate: string;
    refId: string;
    type: string;
  }>;
  openDraftCount?: number;
  period: FiscalPeriod;
  unmatchedBankLineCount?: number;
  unpostedReceiptCount?: number;
  vatStatementReviewed?: boolean;
};

export type PeriodCloseChecklist = {
  blockers: string[];
  entryCount: number;
  period: FiscalPeriod;
  ready: boolean;
  status: "blocked" | "closed" | "ready";
  warnings: string[];
};

export function assertPeriodOpen(periods: FiscalPeriod[], postingDate: string) {
  const date = postingDate.slice(0, 10);
  const matchingPeriods = periods.filter((item) => item.startDate <= date && item.endDate >= date);
  if (!matchingPeriods.length) throw new Error("fiscal_period_missing");
  if (matchingPeriods.some((item) => item.status === "closed")) throw new Error("fiscal_period_closed");
  return matchingPeriods
    .sort((left, right) => periodLengthDays(left) - periodLengthDays(right))[0]!;
}

export function closeFiscalPeriod(period: FiscalPeriod, closedAt = new Date().toISOString()): FiscalPeriod {
  if (period.status === "closed") return period;
  return { ...period, closedAt, status: "closed" };
}

export function buildPeriodCloseChecklist(input: PeriodCloseChecklistInput): PeriodCloseChecklist {
  const blockers: string[] = [];
  const warnings: string[] = [];
  const entries = (input.entries ?? []).filter((entry) => input.period.startDate <= entry.postingDate.slice(0, 10) && input.period.endDate >= entry.postingDate.slice(0, 10));

  if (input.period.status === "closed") {
    return {
      blockers,
      entryCount: entries.length,
      period: input.period,
      ready: true,
      status: "closed",
      warnings
    };
  }

  for (const entry of entries) {
    const debit = entry.lines.reduce((sum, line) => sum + line.debit.minor, 0);
    const credit = entry.lines.reduce((sum, line) => sum + line.credit.minor, 0);
    if (debit !== credit) blockers.push(`unbalanced_journal:${entry.type}:${entry.refId}`);
  }

  if ((input.openDraftCount ?? 0) > 0) blockers.push("open_drafts_in_period");
  if ((input.unpostedReceiptCount ?? 0) > 0) blockers.push("unposted_receipts_in_period");
  if ((input.unmatchedBankLineCount ?? 0) > 0) blockers.push("unmatched_bank_lines_in_period");
  if (!input.vatStatementReviewed) blockers.push("vat_statement_not_reviewed");
  if (!input.datevExported) warnings.push("datev_export_missing");
  if (!entries.length) warnings.push("period_has_no_journal_entries");

  return {
    blockers,
    entryCount: entries.length,
    period: input.period,
    ready: blockers.length === 0,
    status: blockers.length === 0 ? "ready" : "blocked",
    warnings
  };
}

function periodLengthDays(period: FiscalPeriod) {
  const start = new Date(`${period.startDate}T00:00:00.000Z`).getTime();
  const end = new Date(`${period.endDate}T00:00:00.000Z`).getTime();
  return Math.round((end - start) / 86_400_000);
}
