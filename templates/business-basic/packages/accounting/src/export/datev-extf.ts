import type { ChartAccount } from "../chart";
import { moneyToMajor } from "../money";
import type { JournalDraft } from "../posting/service";

export type DatevExtfLine = {
  accountCode: string;
  amount: number;
  contraAccountCode?: string;
  currency: string;
  date: string;
  documentNumber: string;
  side: "H" | "S";
  taxCode?: string;
  text: string;
};

export type DatevExtfExportBundle = {
  csv: string;
  lineCount: number;
  period: {
    endDate: string;
    startDate: string;
  };
  totals: {
    credit: number;
    debit: number;
  };
  validation: ReturnType<typeof validateDatevExtf>;
};

export type DatevExtfSettings = {
  accountLength: number;
  consultantNumber: string;
  clientNumber: string;
  fiscalYearStart: string;
};

export function buildDatevExtfCsv(lines: DatevExtfLine[], settings?: DatevExtfSettings) {
  const validation = validateDatevExtf(lines, settings);
  if (validation.errors.length) throw new Error(`datev_extf_validation_failed:${validation.errors.join(",")}`);
  const prelude = settings ? [
    ["EXTF", "700", "21", "Buchungsstapel", "13", settings.consultantNumber, settings.clientNumber, settings.fiscalYearStart, String(settings.accountLength)].map(csvCell).join(";")
  ] : [];
  const header = ["Umsatz", "Soll/Haben", "WKZ Umsatz", "Konto", "Gegenkonto", "Belegdatum", "Belegfeld 1", "Buchungstext", "Steuerschluessel"];
  const rows = lines.map((line) => [
    formatDatevAmount(line.amount),
    line.side,
    line.currency,
    line.accountCode,
    line.contraAccountCode ?? "",
    line.date,
    line.documentNumber,
    line.text,
    line.taxCode ?? ""
  ]);
  return [...prelude, ...[header, ...rows].map((row) => row.map(csvCell).join(";"))].join("\n");
}

export function validateDatevExtf(lines: DatevExtfLine[], settings?: DatevExtfSettings) {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!lines.length) errors.push("datev_lines_required");
  if (settings) {
    if (!/^\d+$/.test(settings.consultantNumber)) errors.push("datev_consultant_number_invalid");
    if (!/^\d+$/.test(settings.clientNumber)) errors.push("datev_client_number_invalid");
    if (!/^\d{8}$/.test(settings.fiscalYearStart)) errors.push("datev_fiscal_year_start_invalid");
    if (!Number.isInteger(settings.accountLength) || settings.accountLength < 4 || settings.accountLength > 8) errors.push("datev_account_length_invalid");
  } else {
    warnings.push("datev_settings_missing");
  }

  lines.forEach((line, index) => {
    const prefix = `datev_line_${index + 1}`;
    if (!(line.amount > 0)) errors.push(`${prefix}_amount_must_be_positive`);
    if (!/^\d{4,8}$/.test(line.accountCode)) errors.push(`${prefix}_account_code_invalid`);
    if (line.contraAccountCode && !/^\d{4,8}$/.test(line.contraAccountCode)) errors.push(`${prefix}_contra_account_code_invalid`);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(line.date)) errors.push(`${prefix}_date_invalid`);
    if (!line.documentNumber.trim()) errors.push(`${prefix}_document_number_required`);
    if (!line.text.trim()) warnings.push(`${prefix}_text_missing`);
    if (!["EUR", "USD"].includes(line.currency)) warnings.push(`${prefix}_currency_review_required`);
    if (settings?.accountLength && line.accountCode.length !== settings.accountLength) warnings.push(`${prefix}_account_length_mismatch`);
  });

  return { errors, warnings };
}

export function buildDatevExtfLinesFromJournalDrafts(input: { accounts: ChartAccount[]; entries: JournalDraft[] }): DatevExtfLine[] {
  return input.entries.flatMap((entry) => entry.lines.map((line) => {
    const account = findAccount(input.accounts, line.accountId);
    const contraLine = entry.lines.find((candidate) => candidate.accountId !== line.accountId);
    const contraAccount = contraLine ? findAccount(input.accounts, contraLine.accountId) : undefined;
    return {
      accountCode: account.code,
      amount: Math.max(moneyToMajor(line.debit), moneyToMajor(line.credit)),
      contraAccountCode: contraAccount?.code,
      currency: entry.currency,
      date: entry.postingDate,
      documentNumber: entry.refId,
      side: line.debit.minor > 0 ? "S" : "H",
      taxCode: line.taxCode,
      text: entry.narration ?? `${entry.type}:${entry.refType}:${entry.refId}`
    };
  }));
}

export function buildDatevExtfExportBundle(input: {
  accounts: ChartAccount[];
  entries: JournalDraft[];
  period: { endDate: string; startDate: string };
  settings?: DatevExtfSettings;
}): DatevExtfExportBundle {
  const entries = input.entries.filter((entry) => entry.postingDate >= input.period.startDate && entry.postingDate <= input.period.endDate);
  const lines = buildDatevExtfLinesFromJournalDrafts({ accounts: input.accounts, entries });
  const validation = validateDatevExtf(lines, input.settings);
  if (validation.errors.length) throw new Error(`datev_extf_validation_failed:${validation.errors.join(",")}`);
  return {
    csv: buildDatevExtfCsv(lines, input.settings),
    lineCount: lines.length,
    period: input.period,
    totals: {
      credit: round(lines.filter((line) => line.side === "H").reduce((sum, line) => sum + line.amount, 0)),
      debit: round(lines.filter((line) => line.side === "S").reduce((sum, line) => sum + line.amount, 0))
    },
    validation
  };
}

function findAccount(accounts: ChartAccount[], accountId: string) {
  const account = accounts.find((candidate) => candidate.id === accountId);
  if (!account) throw new Error(`datev_account_not_found:${accountId}`);
  return account;
}

function formatDatevAmount(amount: number) {
  return amount.toFixed(2).replace(".", ",");
}

function csvCell(value: string) {
  const escaped = value.replace(/"/g, "\"\"");
  return /[;"\n\r]/.test(escaped) ? `"${escaped}"` : escaped;
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
