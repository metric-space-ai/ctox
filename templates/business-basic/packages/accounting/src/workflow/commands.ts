export type AccountingCommandType =
  | "AcceptBankMatch"
  | "ExportDatev"
  | "IngestReceipt"
  | "ImportBankStatement"
  | "PostReceipt"
  | "RunDunning"
  | "SendInvoice";

export type AccountingCommand<TPayload extends Record<string, unknown> = Record<string, unknown>> = {
  companyId: string;
  idempotencyKey: string;
  refId: string;
  refType: string;
  requestedAt: string;
  requestedBy: string;
  type: AccountingCommandType;
  payload: TPayload;
};

export function createAccountingCommand<TPayload extends Record<string, unknown>>(
  input: Omit<AccountingCommand<TPayload>, "idempotencyKey" | "requestedAt"> & {
    idempotencyKey?: string;
    requestedAt?: string;
  }
): AccountingCommand<TPayload> {
  return {
    ...input,
    idempotencyKey: input.idempotencyKey ?? `${input.companyId}:${input.type}:${input.refType}:${input.refId}`,
    requestedAt: input.requestedAt ?? new Date().toISOString()
  };
}
