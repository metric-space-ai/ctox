export type AccountingAuditEvent = {
  action: string;
  actorId: string;
  actorType: "agent" | "system" | "user";
  after?: Record<string, unknown>;
  before?: Record<string, unknown>;
  companyId: string;
  createdAt: string;
  refId: string;
  refType: string;
};

export function createAccountingAuditEvent(input: Omit<AccountingAuditEvent, "createdAt"> & { createdAt?: string }): AccountingAuditEvent {
  return {
    ...input,
    createdAt: input.createdAt ?? new Date().toISOString()
  };
}
