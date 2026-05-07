import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type ReceiptIngestInput = {
  companyId: string;
  file: {
    blobRef: string;
    mime: string;
    originalFilename: string;
    sha256: string;
  };
  receiptId: string;
  requestedBy?: string;
};

export function prepareReceiptIngestCommand(input: ReceiptIngestInput): AccountingCommand<{
  blobRef: string;
  mime: string;
  originalFilename: string;
  receiptId: string;
  sha256: string;
}> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      blobRef: input.file.blobRef,
      mime: input.file.mime,
      originalFilename: input.file.originalFilename,
      receiptId: input.receiptId,
      sha256: input.file.sha256
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "IngestReceipt"
  });
}
