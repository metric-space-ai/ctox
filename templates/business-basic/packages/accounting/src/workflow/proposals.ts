import type { AccountingCommand } from "./commands";

export type AccountingProposalStatus = "accepted" | "auto_applied" | "open" | "rejected" | "superseded";

export type AccountingProposal = {
  companyId: string;
  confidence: number;
  createdAt: string;
  createdByAgent: string;
  evidence: Record<string, unknown>;
  id: string;
  kind: "asset_activation" | "asset_depreciation" | "asset_disposal" | "bank_match" | "datev_export" | "dunning_run" | "invoice_check" | "receipt_extraction" | "receipt_ingest";
  proposedCommand: AccountingCommand;
  refId: string;
  refType: string;
  status: AccountingProposalStatus;
};

export function createAccountingProposal(input: Omit<AccountingProposal, "createdAt" | "id" | "status"> & {
  createdAt?: string;
  id?: string;
  status?: AccountingProposalStatus;
}): AccountingProposal {
  return {
    ...input,
    createdAt: input.createdAt ?? new Date().toISOString(),
    id: input.id ?? `proposal-${input.kind}-${input.refId}`,
    status: input.status ?? "open"
  };
}
