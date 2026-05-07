import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type VendorCandidate = {
  defaultPayableAccountId?: string;
  iban?: string;
  id: string;
  name: string;
  taxId?: string;
  vatId?: string;
};

export type ReceiptVendorEvidence = {
  companyId: string;
  receiptId: string;
  vendorName: string;
  defaultPayableAccountId?: string;
  iban?: string;
  taxId?: string;
  vatId?: string;
  requestedBy?: string;
};

export type ExistingReceiptFingerprint = {
  amount: number;
  currency: string;
  id: string;
  receiptDate?: string;
  sha256?: string;
  vendorInvoiceNumber?: string;
  vendorName: string;
};

export type ReceiptDuplicateEvidence = ExistingReceiptFingerprint & {
  companyId: string;
  requestedBy?: string;
};

export type CreateVendorFromReceiptPayload = {
  defaultPayableAccountId?: string;
  iban?: string;
  receiptId: string;
  taxId?: string;
  vatId?: string;
  vendorName: string;
};

export type MarkDuplicateReceiptPayload = {
  duplicateOfReceiptId: string;
  receiptId: string;
  reason: string;
};

export function findVendorCandidates(input: ReceiptVendorEvidence, vendors: VendorCandidate[]) {
  const needleName = normalize(input.vendorName);
  return vendors
    .map((vendor) => ({
      matchReasons: [
        vendor.iban && input.iban && vendor.iban.replace(/\s+/g, "") === input.iban.replace(/\s+/g, "") ? "iban" : undefined,
        vendor.vatId && input.vatId && normalize(vendor.vatId) === normalize(input.vatId) ? "vat_id" : undefined,
        vendor.taxId && input.taxId && normalize(vendor.taxId) === normalize(input.taxId) ? "tax_id" : undefined,
        normalize(vendor.name) === needleName ? "name_exact" : undefined,
        normalize(vendor.name).includes(needleName) || needleName.includes(normalize(vendor.name)) ? "name_contains" : undefined
      ].filter(Boolean) as string[],
      vendor
    }))
    .filter((candidate) => candidate.matchReasons.length > 0)
    .sort((left, right) => scoreVendor(right.matchReasons) - scoreVendor(left.matchReasons));
}

export function prepareCreateVendorFromReceiptCommand(input: ReceiptVendorEvidence): AccountingCommand<CreateVendorFromReceiptPayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      defaultPayableAccountId: input.defaultPayableAccountId,
      iban: input.iban,
      receiptId: input.receiptId,
      taxId: input.taxId,
      vatId: input.vatId,
      vendorName: input.vendorName
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "CreateVendorFromReceipt"
  });
}

export function findDuplicateReceipts(input: ReceiptDuplicateEvidence, existingReceipts: ExistingReceiptFingerprint[]) {
  return existingReceipts
    .filter((receipt) => receipt.id !== input.id)
    .map((receipt) => {
      const reasons = [
        input.sha256 && receipt.sha256 === input.sha256 ? "file_hash" : undefined,
        input.vendorInvoiceNumber && receipt.vendorInvoiceNumber === input.vendorInvoiceNumber ? "vendor_invoice_number" : undefined,
        normalize(receipt.vendorName) === normalize(input.vendorName) ? "vendor_name" : undefined,
        receipt.amount === input.amount && receipt.currency === input.currency ? "amount" : undefined,
        receipt.receiptDate && input.receiptDate && receipt.receiptDate === input.receiptDate ? "receipt_date" : undefined
      ].filter(Boolean) as string[];
      return { receipt, reasons };
    })
    .filter((candidate) => candidate.reasons.includes("file_hash") || candidate.reasons.includes("vendor_invoice_number") && candidate.reasons.includes("amount"))
    .sort((left, right) => right.reasons.length - left.reasons.length);
}

export function prepareMarkDuplicateReceiptCommand(input: {
  companyId: string;
  duplicateOfReceiptId: string;
  receiptId: string;
  reason?: string;
  requestedBy?: string;
}): AccountingCommand<MarkDuplicateReceiptPayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      duplicateOfReceiptId: input.duplicateOfReceiptId,
      receiptId: input.receiptId,
      reason: input.reason ?? "duplicate_receipt_detected"
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "MarkDuplicateReceipt"
  });
}

function normalize(value = "") {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "");
}

function scoreVendor(reasons: string[]) {
  return reasons.reduce((score, reason) => score + (reason === "iban" ? 5 : reason === "vat_id" ? 4 : reason === "tax_id" ? 3 : reason === "name_exact" ? 2 : 1), 0);
}
