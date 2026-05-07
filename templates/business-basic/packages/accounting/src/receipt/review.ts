import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type ExtractedReceiptField = {
  confidence: number;
  label: string;
  value: string;
};

export type ReceiptOcrReviewInput = {
  companyId: string;
  receiptId: string;
  fields: ExtractedReceiptField[];
  totalAmount?: number;
  requestedBy?: string;
};

export type ReceiptOcrReviewResult = {
  confidence: number;
  errors: string[];
  missingFields: string[];
  status: "reviewed" | "needs_clarification";
  warnings: string[];
};

export type ReviewReceiptExtractionPayload = {
  confidence: number;
  missingFields: string[];
  receiptId: string;
};

export type RequestReceiptClarificationPayload = {
  missingFields: string[];
  receiptId: string;
  reason: string;
};

const REQUIRED_RECEIPT_FIELDS = ["vendor", "invoice_number", "receipt_date", "total_amount"];

export function reviewReceiptOcr(input: ReceiptOcrReviewInput): ReceiptOcrReviewResult {
  const normalizedLabels = new Set(input.fields.map((field) => normalizeLabel(field.label)));
  const missingFields = REQUIRED_RECEIPT_FIELDS.filter((field) => !normalizedLabels.has(field));
  const lowConfidenceFields = input.fields.filter((field) => field.confidence < 0.7).map((field) => field.label);
  const averageConfidence = input.fields.length
    ? input.fields.reduce((sum, field) => sum + field.confidence, 0) / input.fields.length
    : 0;
  const errors = [
    ...missingFields.map((field) => `receipt_missing_${field}`),
    ...(averageConfidence < 0.55 ? ["receipt_ocr_confidence_too_low"] : [])
  ];
  const warnings = lowConfidenceFields.map((field) => `receipt_low_confidence_${normalizeLabel(field)}`);

  return {
    confidence: roundConfidence(averageConfidence),
    errors,
    missingFields,
    status: errors.length ? "needs_clarification" : "reviewed",
    warnings
  };
}

export function prepareReviewReceiptExtractionCommand(input: ReceiptOcrReviewInput): AccountingCommand<ReviewReceiptExtractionPayload> {
  const review = reviewReceiptOcr(input);
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      confidence: review.confidence,
      missingFields: review.missingFields,
      receiptId: input.receiptId
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "ReviewReceiptExtraction"
  });
}

export function prepareReceiptClarificationCommand(
  input: ReceiptOcrReviewInput,
  reason = "receipt_requires_manual_clarification"
): AccountingCommand<RequestReceiptClarificationPayload> {
  const review = reviewReceiptOcr(input);
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      missingFields: review.missingFields,
      receiptId: input.receiptId,
      reason
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "RequestReceiptClarification"
  });
}

function normalizeLabel(label: string) {
  const normalized = label.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
  if (["supplier", "vendor_name", "lieferant", "rechnungssteller"].includes(normalized)) return "vendor";
  if (["number", "invoice_no", "invoice_number", "rechnungsnummer", "belegnummer"].includes(normalized)) return "invoice_number";
  if (["date", "invoice_date", "receipt_date", "belegdatum", "rechnungsdatum"].includes(normalized)) return "receipt_date";
  if (["amount", "gross", "gross_amount", "total", "total_amount", "brutto", "betrag"].includes(normalized)) return "total_amount";
  return normalized;
}

function roundConfidence(value: number) {
  return Math.round(value * 100) / 100;
}
