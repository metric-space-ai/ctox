import { LedgerPosting, type JournalDraft } from "../posting/service";
import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type PayableAllocation = {
  amount: number;
  receiptId: string;
};

export type SupplierPaymentInput = {
  bankAccountId: string;
  companyId: string;
  currency: string;
  id: string;
  payableAccountId: string;
  paymentDate: string;
  vendorId?: string;
  vendorName: string;
  allocations: PayableAllocation[];
  requestedBy?: string;
};

export type SupplierDiscountPaymentInput = SupplierPaymentInput & {
  discountAccountId: string;
  discountGrossAmount: number;
  discountNetAmount: number;
  inputVatAccountId: string;
  inputVatCorrectionAmount: number;
  paidAmount: number;
};

export type PayableRunCandidate = {
  amount: number;
  blocked?: boolean;
  currency: string;
  dueDate: string;
  receiptId: string;
  skontoDeadline?: string;
  vendorName: string;
};

export type PaymentRunInput = {
  companyId: string;
  dueBy: string;
  id: string;
  candidates: PayableRunCandidate[];
  requestedBy?: string;
};

export type PostSupplierPaymentPayload = {
  allocationCount: number;
  amount: number;
  paymentId: string;
  vendorName: string;
};

export type ApplySupplierDiscountPayload = PostSupplierPaymentPayload & {
  discountGrossAmount: number;
  discountNetAmount: number;
  inputVatCorrectionAmount: number;
};

export type PreparePaymentRunPayload = {
  dueBy: string;
  excludedCount: number;
  paymentRunId: string;
  selectedCount: number;
  totalAmount: number;
};

export function prepareSupplierPaymentCommand(input: SupplierPaymentInput): AccountingCommand<PostSupplierPaymentPayload> {
  const amount = allocationTotal(input.allocations);
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      allocationCount: input.allocations.length,
      amount,
      paymentId: input.id,
      vendorName: input.vendorName
    },
    refId: input.id,
    refType: "payment",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "PostSupplierPayment"
  });
}

export function buildSupplierPaymentJournalDraft(input: SupplierPaymentInput): JournalDraft {
  const amount = allocationTotal(input.allocations);
  if (amount <= 0) throw new Error("supplier_payment_amount_must_be_positive");
  const posting = new LedgerPosting(input.companyId, "payment", input.id, input.paymentDate, input.currency);
  for (const allocation of input.allocations) {
    posting.debit(input.payableAccountId, allocation.amount, allocation.receiptId);
  }
  posting.credit(input.bankAccountId, amount, input.vendorId ?? input.vendorName);
  return posting.toJournalDraft("payment", `Supplier payment ${input.id} for ${input.vendorName}.`);
}

export function prepareSupplierDiscountCommand(input: SupplierDiscountPaymentInput): AccountingCommand<ApplySupplierDiscountPayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      allocationCount: input.allocations.length,
      amount: input.paidAmount,
      discountGrossAmount: input.discountGrossAmount,
      discountNetAmount: input.discountNetAmount,
      inputVatCorrectionAmount: input.inputVatCorrectionAmount,
      paymentId: input.id,
      vendorName: input.vendorName
    },
    refId: input.id,
    refType: "payment",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "ApplySupplierDiscount"
  });
}

export function buildSupplierDiscountJournalDraft(input: SupplierDiscountPaymentInput): JournalDraft {
  const payableGross = allocationTotal(input.allocations);
  const creditedTotal = input.paidAmount + input.discountNetAmount + input.inputVatCorrectionAmount;
  if (Math.round((payableGross - creditedTotal) * 100) !== 0) throw new Error("supplier_discount_payment_must_clear_payable");
  const posting = new LedgerPosting(input.companyId, "payment", input.id, input.paymentDate, input.currency);
  for (const allocation of input.allocations) {
    posting.debit(input.payableAccountId, allocation.amount, allocation.receiptId);
  }
  posting.credit(input.bankAccountId, input.paidAmount, input.vendorId ?? input.vendorName);
  if (input.discountNetAmount > 0) posting.credit(input.discountAccountId, input.discountNetAmount, input.vendorId ?? input.vendorName);
  if (input.inputVatCorrectionAmount > 0) posting.credit(input.inputVatAccountId, input.inputVatCorrectionAmount, input.vendorId ?? input.vendorName);
  return posting.toJournalDraft("payment", `Supplier payment ${input.id} with discount for ${input.vendorName}.`);
}

export function preparePaymentRunCommand(input: PaymentRunInput): AccountingCommand<PreparePaymentRunPayload> {
  const selected = selectPaymentRunCandidates(input);
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      dueBy: input.dueBy,
      excludedCount: input.candidates.length - selected.length,
      paymentRunId: input.id,
      selectedCount: selected.length,
      totalAmount: allocationTotal(selected.map((candidate) => ({ amount: candidate.amount, receiptId: candidate.receiptId })))
    },
    refId: input.id,
    refType: "payment_run",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "PreparePaymentRun"
  });
}

export function selectPaymentRunCandidates(input: PaymentRunInput) {
  return input.candidates
    .filter((candidate) => !candidate.blocked && candidate.dueDate <= input.dueBy)
    .sort((left, right) => left.dueDate.localeCompare(right.dueDate) || left.vendorName.localeCompare(right.vendorName));
}

function allocationTotal(allocations: PayableAllocation[]) {
  return Math.round(allocations.reduce((sum, allocation) => sum + allocation.amount, 0) * 100) / 100;
}
