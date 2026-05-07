import type { BankStatement } from "./types";

export type CsvBankImportOptions = {
  delimiter?: "," | ";";
  sourceFilename?: string;
};

export function parseBankCsv(input: string, options: CsvBankImportOptions = {}): BankStatement {
  const delimiter = options.delimiter ?? detectDelimiter(input);
  const rows = input.trim().split(/\r?\n/).filter(Boolean).map((line) => parseCsvLine(line, delimiter));
  if (rows.length < 2) throw new Error("bank_csv_requires_header_and_rows");

  const header = rows[0].map((cell) => normalizeHeader(cell));
  const lines = rows.slice(1).map((row, index) => ({
    amount: parseAmount(read(row, header, "amount")),
    bookingDate: read(row, header, "bookingdate"),
    currency: read(row, header, "currency") || "EUR",
    endToEndRef: read(row, header, "endtoendref") || undefined,
    lineNo: index + 1,
    purpose: read(row, header, "purpose") || undefined,
    remitterIban: read(row, header, "remitteriban") || undefined,
    remitterName: read(row, header, "remittername") || undefined,
    valueDate: read(row, header, "valuedate") || undefined
  }));

  return {
    currency: lines[0]?.currency ?? "EUR",
    format: "csv",
    lines,
    sourceFilename: options.sourceFilename,
    startDate: lines[0]?.bookingDate,
    endDate: lines.at(-1)?.bookingDate
  };
}

function detectDelimiter(input: string) {
  const firstLine = input.split(/\r?\n/, 1)[0] ?? "";
  return firstLine.includes(";") ? ";" : ",";
}

function parseCsvLine(line: string, delimiter: string) {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (char === "\"" && line[index + 1] === "\"") {
      current += "\"";
      index += 1;
    } else if (char === "\"") {
      quoted = !quoted;
    } else if (char === delimiter && !quoted) {
      cells.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current.trim());
  return cells;
}

function normalizeHeader(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function read(row: string[], header: string[], name: string) {
  return row[header.indexOf(name)] ?? "";
}

function parseAmount(value: string) {
  const normalized = value.replace(/\./g, "").replace(",", ".");
  const amount = Number.parseFloat(normalized);
  if (!Number.isFinite(amount)) throw new Error("bank_csv_amount_invalid");
  return amount;
}
