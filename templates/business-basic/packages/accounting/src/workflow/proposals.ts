import type { AccountingCommand } from "./commands";

export type AccountingProposalStatus = "accepted" | "auto_applied" | "open" | "rejected" | "superseded";

export type AccountingProposal = {
  companyId: string;
  confidence: number;
  createdAt: string;
  createdByAgent: string;
  evidence: Record<string, unknown>;
  id: string;
  kind:
    | "asset_activation"
    | "asset_depreciation"
    | "asset_disposal"
    | "bank_match"
    | "business_analysis"
    | "chart_setup"
    | "cost_center_assignment"
    | "customer_masterdata"
    | "datev_export"
    | "dunning_run"
    | "employee_expense"
    | "gobd_reversal"
    | "invoice_check"
    | "invoice_cancellation_credit_note"
    | "invoice_partial_credit_note"
    | "loan_drawdown"
    | "loan_installment"
    | "manual_journal"
    | "month_close"
    | "open_items_review"
    | "payables_payment"
    | "payables_payment_run"
    | "product_account_assignment"
    | "profit_and_loss_analysis"
    | "purchase_order_match"
    | "quote_prepare"
    | "quote_to_invoice"
    | "recurring_posting"
    | "receipt_clarification"
    | "receipt_duplicate"
    | "receipt_extraction"
    | "receipt_ingest"
    | "receipt_variance"
    | "report_balance_sheet"
    | "reverse_charge_receipt"
    | "story_workflow"
    | "supplier_discount"
    | "tax_advisor_handoff"
    | "travel_expense_report"
    | "vat_return"
    | "vendor_creation";
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
