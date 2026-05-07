export type BankImportFormat = "camt053" | "csv" | "mt940";

export type BankStatementLine = {
  amount: number;
  bookingDate: string;
  currency: string;
  endToEndRef?: string;
  lineNo: number;
  purpose?: string;
  remitterIban?: string;
  remitterName?: string;
  valueDate?: string;
};

export type BankStatement = {
  closingBalance?: number;
  currency: string;
  endDate?: string;
  format: BankImportFormat;
  lines: BankStatementLine[];
  openingBalance?: number;
  sourceFilename?: string;
  startDate?: string;
};
