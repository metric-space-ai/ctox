import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";
import type { BankImportFormat } from "./types";

export type ImportBankStatementCommandPayload = {
  format: BankImportFormat;
  sourceFilename: string;
  sourceSha256: string;
};

export function prepareImportBankStatementCommand(input: {
  companyId: string;
  format: BankImportFormat;
  requestedBy?: string;
  sourceFilename: string;
  sourceSha256: string;
}): AccountingCommand<ImportBankStatementCommandPayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      format: input.format,
      sourceFilename: input.sourceFilename,
      sourceSha256: input.sourceSha256
    },
    refId: input.sourceSha256,
    refType: "bank_statement",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "ImportBankStatement"
  });
}
