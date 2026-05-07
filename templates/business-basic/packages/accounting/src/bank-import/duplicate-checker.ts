import type { BankStatementLine } from "./types";

export type DuplicateCheckResult = {
  duplicateOf?: BankStatementLine;
  fingerprint: string;
  line: BankStatementLine;
};

export function fingerprintBankLine(line: BankStatementLine) {
  return [
    line.bookingDate,
    line.amount.toFixed(2),
    line.endToEndRef || `${line.remitterIban ?? ""}:${line.purpose ?? ""}`
  ].join("|").toLowerCase();
}

export function findDuplicateBankLines(lines: BankStatementLine[], existing: BankStatementLine[] = []): DuplicateCheckResult[] {
  const seen = new Map(existing.map((line) => [fingerprintBankLine(line), line]));
  return lines.map((line) => {
    const fingerprint = fingerprintBankLine(line);
    const duplicateOf = seen.get(fingerprint);
    if (!duplicateOf) seen.set(fingerprint, line);
    return { duplicateOf, fingerprint, line };
  });
}
