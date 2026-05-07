import type { BankStatement } from "./types";

export function parseMt940(input: string, sourceFilename?: string): BankStatement {
  const statementLines = input.split(/\r?\n/);
  const lines = statementLines
    .map((line, index) => parseTransactionLine(line, index + 1))
    .filter((line) => line !== null);

  return {
    currency: lines[0]?.currency ?? "EUR",
    format: "mt940",
    lines,
    sourceFilename,
    startDate: lines[0]?.bookingDate,
    endDate: lines.at(-1)?.bookingDate
  };
}

function parseTransactionLine(line: string, lineNo: number) {
  const match = /^:61:(\d{6})(\d{4})?([CD])([A-Z])?([\d,]+)(?:N\w{3})?(.*)$/.exec(line.trim());
  if (!match) return null;
  const [, booking, value, direction, , amountRaw, tail] = match;
  return {
    amount: (direction === "D" ? -1 : 1) * Number.parseFloat(amountRaw.replace(",", ".")),
    bookingDate: mt940Date(booking),
    currency: "EUR",
    endToEndRef: tail?.trim() || undefined,
    lineNo,
    purpose: tail?.trim() || undefined,
    valueDate: value ? mt940Date(`${booking.slice(0, 2)}${value}`) : undefined
  };
}

function mt940Date(value: string) {
  const year = Number.parseInt(value.slice(0, 2), 10);
  const month = value.slice(2, 4);
  const day = value.slice(4, 6);
  return `${year >= 70 ? "19" : "20"}${String(year).padStart(2, "0")}-${month}-${day}`;
}
