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

export type DatevExtfSettings = {
  accountLength: number;
  consultantNumber: string;
  clientNumber: string;
  fiscalYearStart: string;
};

export function buildDatevExtfCsv(lines: DatevExtfLine[], settings?: DatevExtfSettings) {
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

function formatDatevAmount(amount: number) {
  return amount.toFixed(2).replace(".", ",");
}

function csvCell(value: string) {
  const escaped = value.replace(/"/g, "\"\"");
  return /[;"\n\r]/.test(escaped) ? `"${escaped}"` : escaped;
}
