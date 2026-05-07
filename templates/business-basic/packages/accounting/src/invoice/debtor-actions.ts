import { moneyFromMajor } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";
import { resolveGermanTaxRate } from "../tax";
import { createAccountingCommand, type AccountingCommand, type AccountingCommandType } from "../workflow/commands";
import type { BusinessInvoiceLike, BusinessInvoiceLineLike, InvoiceContext, InvoiceValidationResult, LocalizedValue } from "./types";
import { assertInvoiceCanSend, validateInvoiceForSend } from "./validate";

export type BusinessQuoteLike = {
  id: string;
  closingText?: LocalizedValue;
  currency: BusinessInvoiceLike["currency"];
  customerId: string;
  issueDate: string;
  lines: BusinessInvoiceLineLike[];
  netAmount?: number;
  notes?: LocalizedValue;
  number: string;
  status: "Accepted" | "Draft" | "Expired" | "Sent" | string;
  taxAmount: number;
  total: number;
  validUntil: string;
};

export type BusinessCreditNoteLike = {
  id: string;
  currency: BusinessInvoiceLike["currency"];
  issueDate: string;
  lines: BusinessInvoiceLineLike[];
  netAmount?: number;
  number: string;
  originalInvoiceId: string;
  reason: string;
  taxAmount: number;
  total: number;
  type: "cancellation" | "partial";
};

export type QuoteCommandPayload = {
  customerId: string;
  quoteId: string;
  quoteNumber: string;
  validationWarnings: string[];
};

export type QuoteConversionCommandPayload = {
  invoiceId: string;
  invoiceNumber: string;
  quoteId: string;
  quoteNumber: string;
  validationWarnings: string[];
};

export type CreditNoteCommandPayload = {
  creditNoteId: string;
  creditNoteNumber: string;
  originalInvoiceId: string;
  originalInvoiceNumber: string;
  reason: string;
  type: BusinessCreditNoteLike["type"];
};

export type DunningFeeCommandPayload = {
  feeAmount: number;
  invoiceId: string;
  invoiceNumber: string;
  level: 1 | 2 | 3;
};

export function validateQuoteForSend(quote: BusinessQuoteLike, context: InvoiceContext): InvoiceValidationResult {
  const invoiceLike = quoteToInvoiceLike(quote, {
    dueDate: quote.validUntil,
    invoiceId: `invoice-preview-${quote.id}`,
    invoiceNumber: quote.number
  });
  const validation = validateInvoiceForSend(invoiceLike, context);
  return {
    errors: validation.errors.filter((error) => error !== "invoice_number_required"),
    warnings: validation.warnings
  };
}

export function prepareQuoteCommand(quote: BusinessQuoteLike, context: InvoiceContext): AccountingCommand<QuoteCommandPayload> {
  const validation = validateQuoteForSend(quote, context);
  return createAccountingCommand({
    companyId: context.companyId,
    payload: {
      customerId: quote.customerId,
      quoteId: quote.id,
      quoteNumber: quote.number,
      validationWarnings: validation.warnings
    },
    refId: quote.id,
    refType: "quote",
    requestedBy: context.requestedBy ?? "business-runtime",
    type: extendedCommandType("PrepareQuote")
  });
}

export function quoteToInvoiceLike(
  quote: BusinessQuoteLike,
  input: {
    dueDate: string;
    invoiceId: string;
    invoiceNumber: string;
    issueDate?: string;
    serviceDate?: string;
  }
): BusinessInvoiceLike {
  return {
    closingText: quote.closingText,
    currency: quote.currency,
    customerId: quote.customerId,
    dueDate: input.dueDate,
    id: input.invoiceId,
    issueDate: input.issueDate ?? quote.issueDate,
    lines: quote.lines.map((line) => ({ ...line })),
    netAmount: quote.netAmount,
    notes: quote.notes,
    number: input.invoiceNumber,
    serviceDate: input.serviceDate ?? input.issueDate ?? quote.issueDate,
    status: "Draft",
    taxAmount: quote.taxAmount,
    total: quote.total
  };
}

export function prepareQuoteConversionCommand(
  quote: BusinessQuoteLike,
  invoice: BusinessInvoiceLike,
  context: InvoiceContext
): AccountingCommand<QuoteConversionCommandPayload> {
  const validation = validateInvoiceForSend(invoice, context);
  return createAccountingCommand({
    companyId: context.companyId,
    payload: {
      invoiceId: invoice.id,
      invoiceNumber: invoice.number,
      quoteId: quote.id,
      quoteNumber: quote.number,
      validationWarnings: validation.warnings
    },
    refId: quote.id,
    refType: "quote",
    requestedBy: context.requestedBy ?? "business-runtime",
    type: extendedCommandType("ConvertQuoteToInvoice")
  });
}

export function prepareCreditNoteCommand(
  creditNote: BusinessCreditNoteLike,
  originalInvoice: BusinessInvoiceLike,
  context: InvoiceContext
): AccountingCommand<CreditNoteCommandPayload> {
  return createAccountingCommand({
    companyId: context.companyId,
    payload: {
      creditNoteId: creditNote.id,
      creditNoteNumber: creditNote.number,
      originalInvoiceId: originalInvoice.id,
      originalInvoiceNumber: originalInvoice.number,
      reason: creditNote.reason,
      type: creditNote.type
    },
    refId: creditNote.id,
    refType: "credit_note",
    requestedBy: context.requestedBy ?? "business-runtime",
    type: extendedCommandType(creditNote.type === "cancellation" ? "CreateCancellationCreditNote" : "CreatePartialCreditNote")
  });
}

export function buildCancellationCreditNote(input: {
  id: string;
  issueDate: string;
  number: string;
  originalInvoice: BusinessInvoiceLike;
  reason: string;
}): BusinessCreditNoteLike {
  return {
    currency: input.originalInvoice.currency,
    id: input.id,
    issueDate: input.issueDate,
    lines: input.originalInvoice.lines.map((line) => ({ ...line })),
    netAmount: input.originalInvoice.netAmount,
    number: input.number,
    originalInvoiceId: input.originalInvoice.id,
    reason: input.reason,
    taxAmount: input.originalInvoice.taxAmount,
    total: input.originalInvoice.total,
    type: "cancellation"
  };
}

export function buildPartialCreditNote(input: {
  currency: BusinessInvoiceLike["currency"];
  id: string;
  issueDate: string;
  line: BusinessInvoiceLineLike;
  netAmount?: number;
  number: string;
  originalInvoiceId: string;
  reason: string;
}): BusinessCreditNoteLike {
  const netAmount = input.netAmount ?? input.line.quantity * input.line.unitPrice;
  const taxAmount = round(netAmount * (input.line.taxRate / 100));
  return {
    currency: input.currency,
    id: input.id,
    issueDate: input.issueDate,
    lines: [{ ...input.line }],
    netAmount,
    number: input.number,
    originalInvoiceId: input.originalInvoiceId,
    reason: input.reason,
    taxAmount,
    total: round(netAmount + taxAmount),
    type: "partial"
  };
}

export function buildCreditNoteJournalDraft(
  creditNote: BusinessCreditNoteLike,
  originalInvoice: BusinessInvoiceLike,
  context: InvoiceContext
): JournalDraft {
  assertInvoiceCanSend(originalInvoice, context);
  validateCreditNote(creditNote, originalInvoice);

  const posting = new LedgerPosting(context.companyId, "credit_note", creditNote.id, creditNote.issueDate, creditNote.currency);
  const taxByCode = new Map<string, number>();

  for (const line of creditNote.lines) {
    const product = context.products.find((item) => item.id === line.productId);
    const lineNet = line.quantity * line.unitPrice;
    const tax = resolveGermanTaxRate({
      kleinunternehmer: context.kleinunternehmer || originalInvoice.kleinunternehmer,
      reverseCharge: originalInvoice.reverseCharge || line.reverseCharge,
      taxRate: line.taxRate
    });
    const taxCode = outputTaxCodeForTaxRate(tax.code);
    posting.debit(revenueAccountId(product?.revenueAccount, context), moneyFromMajor(lineNet, creditNote.currency), originalInvoice.customerId, { taxCode });
    if (line.taxRate > 0 && taxCode.endsWith("_OUTPUT")) {
      const accountId = tax.accountId ?? context.defaultTaxAccountId;
      taxByCode.set(`${taxCode}:${accountId}`, round(taxByCode.get(`${taxCode}:${accountId}`) ?? 0) + round(lineNet * (line.taxRate / 100)));
    }
  }

  for (const [key, amount] of taxByCode) {
    if (amount > 0) {
      const [taxCode, accountId] = key.split(":");
      posting.debit(accountId ?? context.defaultTaxAccountId, moneyFromMajor(round(amount), creditNote.currency), originalInvoice.customerId, { taxCode });
    }
  }

  posting.credit(context.defaultReceivableAccountId, moneyFromMajor(creditNote.total, creditNote.currency), originalInvoice.customerId);
  return posting.toJournalDraft("reverse", `${creditNote.type === "cancellation" ? "Canceled" : "Credited"} customer invoice ${originalInvoice.number} with ${creditNote.number}.`);
}

export function prepareDunningFeeCommand(
  invoice: BusinessInvoiceLike,
  context: InvoiceContext,
  input: { feeAmount: number; level: 1 | 2 | 3 }
): AccountingCommand<DunningFeeCommandPayload> {
  return createAccountingCommand({
    companyId: context.companyId,
    payload: {
      feeAmount: input.feeAmount,
      invoiceId: invoice.id,
      invoiceNumber: invoice.number,
      level: input.level
    },
    refId: invoice.id,
    refType: "invoice",
    requestedBy: context.requestedBy ?? "business-runtime",
    type: "RunDunning"
  });
}

export function buildDunningFeeJournalDraft(
  invoice: BusinessInvoiceLike,
  context: InvoiceContext,
  input: {
    feeAmount: number;
    feeRevenueAccountId?: string;
    issueDate: string;
    level: 1 | 2 | 3;
    taxRate?: 0 | 7 | 19;
  }
): JournalDraft | null {
  if (input.feeAmount <= 0) return null;
  assertInvoiceCanSend(invoice, context);

  const taxRate = input.taxRate ?? 19;
  const netAmount = taxRate > 0 ? round(input.feeAmount / (1 + taxRate / 100)) : input.feeAmount;
  const taxAmount = round(input.feeAmount - netAmount);
  const tax = resolveGermanTaxRate({ taxRate });
  const taxCode = outputTaxCodeForTaxRate(tax.code);
  const posting = new LedgerPosting(context.companyId, "invoice", `${invoice.id}-dunning-${input.level}`, input.issueDate, invoice.currency);

  posting.debit(context.defaultReceivableAccountId, moneyFromMajor(input.feeAmount, invoice.currency), invoice.customerId);
  posting.credit(input.feeRevenueAccountId ?? "acc-dunning-fees", moneyFromMajor(netAmount, invoice.currency), invoice.customerId, { taxCode });
  if (taxAmount > 0) {
    posting.credit(tax.accountId ?? context.defaultTaxAccountId, moneyFromMajor(taxAmount, invoice.currency), invoice.customerId, { taxCode });
  }

  return posting.toJournalDraft("invoice", `Posted dunning level ${input.level} fee for customer invoice ${invoice.number}.`);
}

function validateCreditNote(creditNote: BusinessCreditNoteLike, originalInvoice: BusinessInvoiceLike) {
  if (creditNote.originalInvoiceId !== originalInvoice.id) {
    throw new Error("credit_note_original_invoice_mismatch");
  }
  if (creditNote.total <= 0) {
    throw new Error("credit_note_total_must_be_positive");
  }
  if (creditNote.total - (originalInvoice.balanceDue ?? originalInvoice.total) > 0.01) {
    throw new Error("credit_note_exceeds_open_invoice_balance");
  }
  const expectedNet = round(creditNote.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0));
  const expectedTax = round(creditNote.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice * (line.taxRate / 100), 0));
  if (Math.abs(round(creditNote.netAmount ?? expectedNet) - expectedNet) > 0.01) {
    throw new Error("credit_note_net_amount_mismatch");
  }
  if (Math.abs(round(creditNote.taxAmount) - expectedTax) > 0.01) {
    throw new Error("credit_note_tax_amount_mismatch");
  }
  if (Math.abs(round(creditNote.total) - round(expectedNet + expectedTax)) > 0.01) {
    throw new Error("credit_note_total_amount_mismatch");
  }
}

function extendedCommandType(type: string) {
  return type as AccountingCommandType;
}

function outputTaxCodeForTaxRate(code: ReturnType<typeof resolveGermanTaxRate>["code"]) {
  if (code === "DE_19") return "DE_19_OUTPUT";
  if (code === "DE_7") return "DE_7_OUTPUT";
  return code;
}

function revenueAccountId(revenueAccount: string | undefined, context: InvoiceContext) {
  if (!revenueAccount) return context.defaultRevenueAccountId;
  const code = revenueAccount.trim().split(/\s+/)[0];
  const knownAccountId = knownRevenueAccountIds[code];
  if (knownAccountId) return knownAccountId;
  if (!code) return context.defaultRevenueAccountId;
  return `acc-${code}`;
}

const knownRevenueAccountIds: Record<string, string> = {
  "8337": "acc-revenue-implementation",
  "8338": "acc-revenue-research",
  "8400": "acc-revenue-saas",
  "8401": "acc-revenue-support"
};

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
