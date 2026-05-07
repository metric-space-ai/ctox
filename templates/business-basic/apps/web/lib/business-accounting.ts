import {
  bankMatchConfidence,
  buildAssetAcquisitionJournalDraft,
  buildAssetDepreciationJournalDraft,
  buildAssetDisposalJournalDraft,
  buildDatevExtfCsv,
  buildBankMatchJournalDraft,
  buildCancellationCreditNote,
  buildCreditNoteJournalDraft,
  buildDunningFeeJournalDraft,
  buildEmployeeExpenseJournalDraft,
  buildPartialCreditNote,
  buildStraightLineDepreciationSchedule,
  buildInvoiceDocument as buildAccountingInvoiceDocument,
  buildInvoiceJournalDraft,
  buildReceiptJournalDraft,
  buildSupplierDiscountJournalDraft,
  buildSupplierPaymentJournalDraft,
  buildZugferdXml,
  checkPurchaseOrderReceiptMatch,
  createAccountingAuditEvent,
  createAccountingCommand,
  createAccountingProposal,
  createBusinessOutboxEvent,
  findDuplicateReceipts,
  findVendorCandidates,
  LedgerPosting,
  prepareCreateVendorFromReceiptCommand,
  prepareCreditNoteCommand,
  prepareDunningFeeCommand,
  prepareQuoteCommand,
  prepareQuoteConversionCommand,
  prepareAcceptBankMatchCommand,
  prepareMarkDuplicateReceiptCommand,
  preparePaymentRunCommand,
  preparePostReceiptCommand,
  preparePurchaseOrderMatchCommand,
  prepareReceiptClarificationCommand,
  prepareResolveReceiptVarianceCommand,
  prepareReviewReceiptExtractionCommand,
  prepareSendInvoiceCommand,
  prepareSubmitEmployeeExpenseCommand,
  prepareSupplierDiscountCommand,
  prepareSupplierPaymentCommand,
  quoteToInvoiceLike,
  reviewReceiptOcr,
  selectPaymentRunCandidates,
  validateQuoteForSend,
  validateInvoiceForSend,
  type BusinessQuoteLike,
  type DatevExtfLine,
  type InvoiceContext
} from "@ctox-business/accounting";
import { buildDatevLines } from "./accounting-runtime";
import type {
  BusinessBankTransaction,
  BusinessBookkeepingExport,
  BusinessBundle,
  BusinessCustomer,
  BusinessFixedAsset,
  BusinessInvoice,
  BusinessProduct,
  BusinessReceipt,
  SupportedLocale
} from "./business-seed";

const BUSINESS_COMPANY_ID = "business-basic-company";

export function buildInvoiceAccountingContext(
  data: Pick<BusinessBundle, "customers" | "products">,
  invoice: BusinessInvoice,
  locale: SupportedLocale = "de"
): InvoiceContext {
  return {
    companyId: BUSINESS_COMPANY_ID,
    companyName: "Metric Space UG (haftungsbeschränkt)",
    customer: data.customers.find((customer) => customer.id === invoice.customerId),
    defaultReceivableAccountId: "acc-ar",
    defaultRevenueAccountId: "acc-revenue-saas",
    defaultTaxAccountId: "acc-vat-output",
    issuerAddressLines: ["Metric Space UG (haftungsbeschraenkt)", "Brunnenstr. 7", "10119 Berlin", "Deutschland"],
    issuerTaxId: "37/123/45678",
    issuerVatId: "DE123456789",
    locale,
    products: data.products,
    requestedBy: "business-runtime"
  };
}

export function prepareExistingInvoiceForAccounting({
  data,
  invoice,
  locale = "de"
}: {
  data: Pick<BusinessBundle, "customers" | "products">;
  invoice: BusinessInvoice;
  locale?: SupportedLocale;
}) {
  const context = buildInvoiceAccountingContext(data, invoice, locale);
  const validation = validateInvoiceForSend(invoice, context);
  const command = prepareSendInvoiceCommand(invoice, context);
  const document = buildAccountingInvoiceDocument(invoice, context);
  const zugferdXml = buildZugferdXml(invoice, context);
  const journalDraft = validation.errors.length ? null : buildInvoiceJournalDraft(invoice, context);
  const invoiceProjection = {
    balanceDueMinor: toMinor(invoice.balanceDue ?? invoice.total),
    companyId: context.companyId,
    currency: invoice.currency,
    customerExternalId: invoice.customerId,
    dueDate: invoice.dueDate,
    externalId: invoice.id,
    issueDate: invoice.issueDate,
    lines: invoice.lines.map((line, index) => {
      const product = data.products.find((item) => item.id === line.productId);
      const lineNet = line.quantity * line.unitPrice;
      const taxAmount = lineNet * (line.taxRate / 100);
      return {
        description: localize(product?.description ?? product?.name ?? line.productId, locale),
        lineNetMinor: toMinor(lineNet),
        lineNo: index + 1,
        lineTotalMinor: toMinor(lineNet + taxAmount),
        productExternalId: line.productId,
        quantity: line.quantity,
        revenueAccountExternalId: product ? revenueAccountExternalId(product.revenueAccount) : context.defaultRevenueAccountId,
        taxAmountMinor: toMinor(taxAmount),
        taxRate: line.taxRate,
        unitPriceMinor: toMinor(line.unitPrice)
      };
    }),
    netAmountMinor: toMinor(invoice.netAmount ?? invoice.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0)),
    number: invoice.number,
    pdfBlobRef: `invoice-pdf:${invoice.id}`,
    postedJournalEntryExternalId: journalDraft ? journalExternalId(journalDraft) : null,
    sentAt: requestTimestampFromInvoice(invoice),
    serviceDate: invoice.serviceDate ?? null,
    status: invoice.status === "Draft" ? "prepared" : invoice.status.toLowerCase().replace(/\s+/g, "_"),
    taxAmountMinor: toMinor(invoice.taxAmount),
    totalAmountMinor: toMinor(invoice.total),
    zugferdXml
  };
  const proposal = createAccountingProposal({
    companyId: context.companyId,
    confidence: validation.errors.length ? 0.4 : validation.warnings.length ? 0.82 : 0.98,
    createdByAgent: "invoice-checker",
    evidence: {
      errors: validation.errors,
      warnings: validation.warnings,
      invoiceNumber: invoice.number,
      customerId: invoice.customerId
    },
    kind: "invoice_check",
    proposedCommand: command,
    refId: invoice.id,
    refType: "invoice"
  });
  const audit = createAccountingAuditEvent({
    action: "invoice.prepare_send",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, validation },
    companyId: context.companyId,
    refId: invoice.id,
    refType: "invoice"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: context.companyId,
    id: `outbox-business.invoice.prepare_send-${invoice.id}`,
    payload: {
      command,
      invoiceId: invoice.id,
      proposalId: proposal.id,
      validation
    },
    topic: "business.invoice.prepare_send"
  });

  return {
    audit,
    command,
    context,
    document,
    invoiceProjection,
    journalDraft,
    outbox,
    proposal,
    validation,
    zugferdXml
  };
}

export type ExistingInvoiceAccountingPreview = ReturnType<typeof prepareExistingInvoiceForAccounting>;

export function prepareQuoteForAccounting({
  data,
  locale = "de",
  payload
}: {
  data: Pick<BusinessBundle, "customers" | "products">;
  locale?: SupportedLocale;
  payload?: Record<string, unknown>;
}) {
  const quote = quoteFromPayload(payload, data);
  const context = buildInvoiceAccountingContext(data, quoteToInvoiceLike(quote, {
    dueDate: quote.validUntil,
    invoiceId: `invoice-preview-${quote.id}`,
    invoiceNumber: quote.number
  }) as BusinessInvoice, locale);
  const validation = validateQuoteForSend(quote, context);
  const command = prepareQuoteCommand(quote, context);
  const quoteProjection = {
    companyId: context.companyId,
    currency: quote.currency,
    customerExternalId: quote.customerId,
    externalId: quote.id,
    issueDate: quote.issueDate,
    lines: quote.lines.map((line, index) => {
      const product = data.products.find((item) => item.id === line.productId);
      const lineNet = line.quantity * line.unitPrice;
      const taxAmount = lineNet * (line.taxRate / 100);
      return {
        description: localize(product?.description ?? product?.name ?? line.productId, locale),
        lineNetMinor: toMinor(lineNet),
        lineNo: index + 1,
        lineTotalMinor: toMinor(lineNet + taxAmount),
        productExternalId: line.productId,
        quantity: line.quantity,
        revenueAccountExternalId: product ? revenueAccountExternalId(product.revenueAccount) : context.defaultRevenueAccountId,
        taxAmountMinor: toMinor(taxAmount),
        taxRate: line.taxRate,
        unitPriceMinor: toMinor(line.unitPrice)
      };
    }),
    netAmountMinor: toMinor(quote.netAmount ?? quote.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0)),
    number: quote.number,
    status: quote.status.toLowerCase().replace(/\s+/g, "_"),
    taxAmountMinor: toMinor(quote.taxAmount),
    totalAmountMinor: toMinor(quote.total),
    validUntil: quote.validUntil
  };
  const proposal = createAccountingProposal({
    companyId: context.companyId,
    confidence: validation.errors.length ? 0.44 : validation.warnings.length ? 0.82 : 0.96,
    createdByAgent: "invoice-checker",
    evidence: { errors: validation.errors, quoteNumber: quote.number, warnings: validation.warnings },
    kind: extendedProposalKind("quote_prepare"),
    proposedCommand: command,
    refId: quote.id,
    refType: "quote"
  });
  const audit = createAccountingAuditEvent({
    action: "quote.prepare_send",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, quoteProjection, validation },
    companyId: context.companyId,
    refId: quote.id,
    refType: "quote"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: context.companyId,
    id: `outbox-business.quote.prepare_send-${quote.id}`,
    payload: { command, proposalId: proposal.id, quoteProjection, validation },
    topic: "business.quote.prepare_send"
  });

  return { audit, command, journalDraft: null, outbox, proposal, quoteProjection, validation };
}

export function prepareQuoteConversionForAccounting({
  data,
  locale = "de",
  payload
}: {
  data: Pick<BusinessBundle, "customers" | "products">;
  locale?: SupportedLocale;
  payload?: Record<string, unknown>;
}) {
  const quote = quoteFromPayload(payload, data, "Accepted");
  const invoice = quoteToInvoiceLike(quote, {
    dueDate: readString(payload, "dueDate") ?? addDays(readString(payload, "issueDate") ?? todayIsoDate(), 14),
    invoiceId: readString(payload, "invoiceId") ?? `inv-from-${quote.id}`,
    invoiceNumber: readString(payload, "invoiceNumber") ?? `RE-${todayIsoDate().slice(0, 4)}-${quote.number.replace(/\D+/g, "").slice(-4) || "0001"}`,
    issueDate: readString(payload, "issueDate") ?? todayIsoDate(),
    serviceDate: readString(payload, "serviceDate") ?? readString(payload, "issueDate") ?? todayIsoDate()
  });
  const context = buildInvoiceAccountingContext(data, invoice as BusinessInvoice, locale);
  const validation = validateInvoiceForSend(invoice, context);
  const command = prepareQuoteConversionCommand(quote, invoice, context);
  const journalDraft = validation.errors.length ? null : buildInvoiceJournalDraft(invoice, context);
  const proposal = createAccountingProposal({
    companyId: context.companyId,
    confidence: validation.errors.length ? 0.42 : validation.warnings.length ? 0.82 : 0.97,
    createdByAgent: "invoice-checker",
    evidence: { errors: validation.errors, invoiceNumber: invoice.number, quoteNumber: quote.number, warnings: validation.warnings },
    kind: extendedProposalKind("quote_to_invoice"),
    proposedCommand: command,
    refId: quote.id,
    refType: "quote"
  });
  const audit = createAccountingAuditEvent({
    action: "quote.prepare_convert_to_invoice",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, invoice, journalDraft, validation },
    companyId: context.companyId,
    refId: quote.id,
    refType: "quote"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: context.companyId,
    id: `outbox-business.quote.prepare_convert-${quote.id}`,
    payload: { command, invoiceDraft: invoice, journalDraft, proposalId: proposal.id, validation },
    topic: "business.quote.prepare_convert_to_invoice"
  });

  return { audit, command, invoiceDraft: invoice, journalDraft, outbox, proposal, validation };
}

export function prepareInvoiceCreditNoteForAccounting({
  data,
  invoice,
  locale = "de",
  payload,
  type
}: {
  data: Pick<BusinessBundle, "customers" | "products">;
  invoice: BusinessInvoice;
  locale?: SupportedLocale;
  payload?: Record<string, unknown>;
  type: "cancellation" | "partial";
}) {
  const context = buildInvoiceAccountingContext(data, invoice, locale);
  const creditNote = type === "cancellation"
    ? buildCancellationCreditNote({
      id: readString(payload, "creditNoteId") ?? `credit-${invoice.id}`,
      issueDate: readString(payload, "issueDate") ?? todayIsoDate(),
      number: readString(payload, "creditNoteNumber") ?? `GS-${todayIsoDate().slice(0, 4)}-${invoice.number.replace(/\D+/g, "").slice(-4) || "0001"}`,
      originalInvoice: invoice,
      reason: readString(payload, "reason") ?? "Storno"
    })
    : buildPartialCreditNote({
      currency: invoice.currency,
      id: readString(payload, "creditNoteId") ?? `credit-${invoice.id}-${toMinor(readNumber(payload, "netAmount") ?? 0)}`,
      issueDate: readString(payload, "issueDate") ?? todayIsoDate(),
      line: {
        productId: readString(payload, "productId") ?? invoice.lines[0]?.productId ?? data.products[0]?.id ?? "manual-credit",
        quantity: readNumber(payload, "quantity") ?? 1,
        taxRate: readNumber(payload, "taxRate") ?? invoice.lines[0]?.taxRate ?? 19,
        unitPrice: readNumber(payload, "unitPrice") ?? readNumber(payload, "netAmount") ?? 0
      },
      netAmount: readNumber(payload, "netAmount") ?? readNumber(payload, "unitPrice"),
      number: readString(payload, "creditNoteNumber") ?? `GS-${todayIsoDate().slice(0, 4)}-${invoice.number.replace(/\D+/g, "").slice(-4) || "0001"}-T`,
      originalInvoiceId: invoice.id,
      reason: readString(payload, "reason") ?? "Teilkorrektur"
    });
  const command = prepareCreditNoteCommand(creditNote, invoice, context);
  const validation = { errors: [] as string[], warnings: [] as string[] };
  let journalDraft = null;
  try {
    journalDraft = buildCreditNoteJournalDraft(creditNote, invoice, context);
  } catch (error) {
    validation.errors.push(error instanceof Error ? error.message : "credit_note_validation_failed");
  }
  const proposal = createAccountingProposal({
    companyId: context.companyId,
    confidence: validation.errors.length ? 0.38 : type === "cancellation" ? 0.9 : 0.84,
    createdByAgent: "invoice-checker",
    evidence: { creditNoteNumber: creditNote.number, errors: validation.errors, originalInvoiceNumber: invoice.number, reason: creditNote.reason, type },
    kind: extendedProposalKind(type === "cancellation" ? "invoice_cancellation_credit_note" : "invoice_partial_credit_note"),
    proposedCommand: command,
    refId: creditNote.id,
    refType: "credit_note"
  });
  const audit = createAccountingAuditEvent({
    action: type === "cancellation" ? "invoice.prepare_cancellation_credit_note" : "invoice.prepare_partial_credit_note",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, creditNote, journalDraft, validation },
    companyId: context.companyId,
    refId: creditNote.id,
    refType: "credit_note"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: context.companyId,
    id: `outbox-business.credit_note.prepare-${creditNote.id}`,
    payload: { command, creditNoteProjection: creditNote, journalDraft, proposalId: proposal.id, validation },
    topic: type === "cancellation" ? "business.invoice.prepare_cancellation_credit_note" : "business.invoice.prepare_partial_credit_note"
  });

  return { audit, command, creditNoteProjection: creditNote, journalDraft, outbox, proposal, validation };
}

export function prepareDunningFeeForAccounting({
  data,
  invoice,
  locale = "de",
  payload
}: {
  data: Pick<BusinessBundle, "customers" | "products">;
  invoice: BusinessInvoice;
  locale?: SupportedLocale;
  payload?: Record<string, unknown>;
}) {
  const context = buildInvoiceAccountingContext(data, invoice, locale);
  const level = clampDunningLevel(readNumber(payload, "level") ?? ((invoice.reminderLevel ?? 0) + 1));
  const feeAmount = readNumber(payload, "feeAmount") ?? (level === 1 ? 0 : level === 2 ? 12 : 25);
  const command = prepareDunningFeeCommand(invoice, context, { feeAmount, level });
  const journalDraft = buildDunningFeeJournalDraft(invoice, context, {
    feeAmount,
    feeRevenueAccountId: readString(payload, "feeRevenueAccountId") ?? "acc-dunning-fees",
    issueDate: readString(payload, "issueDate") ?? todayIsoDate(),
    level,
    taxRate: (readNumber(payload, "taxRate") ?? 19) as 0 | 7 | 19
  });
  const validation = { errors: invoice.balanceDue === 0 ? ["invoice_has_no_open_balance"] : [], warnings: feeAmount === 0 ? ["dunning_without_fee"] : [] };
  const proposal = createAccountingProposal({
    companyId: context.companyId,
    confidence: validation.errors.length ? 0.35 : journalDraft ? 0.88 : 0.78,
    createdByAgent: "dunning-assistant",
    evidence: { feeAmount, invoiceNumber: invoice.number, level, openBalance: invoice.balanceDue ?? invoice.total },
    kind: "dunning_run",
    proposedCommand: command,
    refId: invoice.id,
    refType: "invoice"
  });
  const audit = createAccountingAuditEvent({
    action: "dunning.prepare_fee",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft, validation },
    companyId: context.companyId,
    refId: invoice.id,
    refType: "invoice"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: context.companyId,
    id: `outbox-business.dunning.prepare_fee-${invoice.id}-${level}`,
    payload: { command, journalDraft, proposalId: proposal.id, validation },
    topic: "business.dunning.prepare_fee"
  });

  return { audit, command, journalDraft, outbox, proposal, validation };
}

export function prepareReceiptForAccounting({
  receipt
}: {
  receipt: BusinessReceipt;
}) {
  const command = preparePostReceiptCommand(receipt, BUSINESS_COMPANY_ID);
  const journalDraft = buildReceiptJournalDraft(receipt, BUSINESS_COMPANY_ID);
  const receiptProjection = {
    companyId: BUSINESS_COMPANY_ID,
    currency: receipt.currency,
    dueDate: receipt.dueDate,
    expenseAccountExternalId: receipt.expenseAccountId,
    externalId: receipt.id,
    extractedJson: receipt.extractedFields,
    files: [{
      blobRef: `receipt-file:${receipt.id}`,
      mime: "application/pdf",
      originalFilename: receipt.attachmentName,
      sha256: stablePseudoSha256(`${receipt.id}:${receipt.attachmentName}`)
    }],
    lines: [{
      description: receipt.documentType,
      expenseAccountExternalId: receipt.expenseAccountId,
      lineNo: 1,
      netAmountMinor: toMinor(receipt.netAmount),
      taxAmountMinor: toMinor(receipt.taxAmount),
      taxCode: receipt.taxCode,
      totalAmountMinor: toMinor(receipt.total)
    }],
    netAmountMinor: toMinor(receipt.netAmount),
    number: receipt.number,
    payableAccountExternalId: receipt.payableAccountId,
    postedAt: receipt.status === "Posted" || receipt.status === "Paid" ? new Date(`${receipt.receiptDate}T00:00:00.000Z`) : null,
    postedJournalEntryExternalId: journalExternalId(journalDraft),
    receiptDate: receipt.receiptDate,
    reviewedAt: new Date(`${receipt.receiptDate}T00:00:00.000Z`),
    status: receipt.status.toLowerCase().replace(/\s+/g, "_"),
    taxAmountMinor: toMinor(receipt.taxAmount),
    taxCode: receipt.taxCode,
    totalAmountMinor: toMinor(receipt.total),
    vendorExternalId: vendorExternalId(receipt.vendorName),
    vendorInvoiceNumber: receipt.number
  };
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: receipt.status === "Needs review" ? 0.78 : 0.9,
    createdByAgent: "receipt-extractor",
    evidence: {
      extractedFields: receipt.extractedFields,
      source: receipt.source,
      vendorName: receipt.vendorName
    },
    kind: "receipt_extraction",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.prepare_post",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft, receiptProjection },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.receipt.prepare_post-${receipt.id}`,
    payload: { command, journalDraft, proposalId: proposal.id, receiptProjection },
    topic: "business.receipt.prepare_post"
  });

  return { audit, command, journalDraft, outbox, proposal, receiptProjection, validation: { errors: [], warnings: [] } };
}

export function prepareReceiptOcrReviewForAccounting({
  receipt
}: {
  receipt: BusinessReceipt;
}) {
  const review = reviewReceiptOcr({
    companyId: BUSINESS_COMPANY_ID,
    fields: receipt.extractedFields,
    receiptId: receipt.id,
    totalAmount: receipt.total
  });
  const command = review.status === "reviewed"
    ? prepareReviewReceiptExtractionCommand({
      companyId: BUSINESS_COMPANY_ID,
      fields: receipt.extractedFields,
      receiptId: receipt.id,
      totalAmount: receipt.total
    })
    : prepareReceiptClarificationCommand({
      companyId: BUSINESS_COMPANY_ID,
      fields: receipt.extractedFields,
      receiptId: receipt.id,
      totalAmount: receipt.total
    });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: review.confidence,
    createdByAgent: "receipt-ocr-reviewer",
    evidence: { extractedFields: receipt.extractedFields, review },
    kind: review.status === "reviewed" ? "receipt_extraction" : "receipt_clarification",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.review_ocr",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, review },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.receipt.review_ocr-${receipt.id}`,
    payload: { command, proposalId: proposal.id, review },
    topic: "business.receipt.review_ocr"
  });

  return { audit, command, outbox, proposal, review, validation: { errors: review.errors, warnings: review.warnings } };
}

export type BusinessVendorCandidate = {
  defaultPayableAccountId?: string;
  iban?: string;
  id: string;
  name: string;
  taxId?: string;
  vatId?: string;
};

export function prepareVendorFromReceiptForAccounting({
  existingVendors = [],
  receipt,
  vendorEvidence = {}
}: {
  existingVendors?: BusinessVendorCandidate[];
  receipt: BusinessReceipt;
  vendorEvidence?: { iban?: string; taxId?: string; vatId?: string };
}) {
  const evidence = {
    companyId: BUSINESS_COMPANY_ID,
    defaultPayableAccountId: receipt.payableAccountId,
    iban: vendorEvidence.iban,
    receiptId: receipt.id,
    taxId: vendorEvidence.taxId,
    vatId: vendorEvidence.vatId,
    vendorName: receipt.vendorName
  };
  const candidates = findVendorCandidates(evidence, existingVendors);
  const command = prepareCreateVendorFromReceiptCommand(evidence);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: candidates.length ? 0.46 : 0.88,
    createdByAgent: "vendor-masterdata-agent",
    evidence: { candidates, receiptId: receipt.id, vendorEvidence: evidence },
    kind: "vendor_creation",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "vendor.prepare_from_receipt",
    actorId: "business-runtime",
    actorType: "system",
    after: { candidates, command },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.vendor.prepare_from_receipt-${receipt.id}`,
    payload: { candidates, command, proposalId: proposal.id },
    topic: "business.vendor.prepare_from_receipt"
  });

  return { audit, candidates, command, outbox, proposal, validation: { errors: [], warnings: candidates.length ? ["possible_vendor_duplicate"] : [] } };
}

export function prepareReceiptDuplicateCheckForAccounting({
  existingReceipts,
  receipt
}: {
  existingReceipts: BusinessReceipt[];
  receipt: BusinessReceipt;
}) {
  const receiptFingerprint = receiptToDuplicateFingerprint(receipt);
  const duplicates = findDuplicateReceipts({
    ...receiptFingerprint,
    companyId: BUSINESS_COMPANY_ID
  }, existingReceipts.map(receiptToDuplicateFingerprint));
  const command = duplicates[0]
    ? prepareMarkDuplicateReceiptCommand({
      companyId: BUSINESS_COMPANY_ID,
      duplicateOfReceiptId: duplicates[0].receipt.id,
      receiptId: receipt.id,
      reason: duplicates[0].reasons.join(",")
    })
    : prepareReviewReceiptExtractionCommand({
      companyId: BUSINESS_COMPANY_ID,
      fields: receipt.extractedFields,
      receiptId: receipt.id,
      totalAmount: receipt.total
    });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: duplicates.length ? 0.96 : 0.55,
    createdByAgent: "receipt-duplicate-checker",
    evidence: { duplicates, receiptFingerprint },
    kind: duplicates.length ? "receipt_duplicate" : "receipt_extraction",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.check_duplicate",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, duplicates },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.receipt.check_duplicate-${receipt.id}`,
    payload: { command, duplicates, proposalId: proposal.id },
    topic: "business.receipt.check_duplicate"
  });

  return { audit, command, duplicates, outbox, proposal, validation: { errors: [], warnings: duplicates.length ? ["duplicate_receipt_candidate"] : [] } };
}

export function prepareReceiptPurchaseOrderCheckForAccounting({
  input
}: {
  input: {
    receiptId: string;
    purchaseOrderId: string;
    orderedQuantity: number;
    receivedQuantity: number;
    invoicedQuantity: number;
    orderedUnitPrice: number;
    invoicedUnitPrice: number;
    taxAmount?: number;
  };
}) {
  const checkInput = { ...input, companyId: BUSINESS_COMPANY_ID };
  const match = checkPurchaseOrderReceiptMatch(checkInput);
  const command = preparePurchaseOrderMatchCommand(checkInput);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: match.status === "matched" ? 0.94 : 0.68,
    createdByAgent: "purchase-order-matcher",
    evidence: { input, match },
    kind: "purchase_order_match",
    proposedCommand: command,
    refId: input.receiptId,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.check_purchase_order",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, match },
    companyId: BUSINESS_COMPANY_ID,
    refId: input.receiptId,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.receipt.check_purchase_order-${input.receiptId}`,
    payload: { command, match, proposalId: proposal.id },
    topic: "business.receipt.check_purchase_order"
  });

  return { audit, command, match, outbox, proposal, validation: { errors: [], warnings: match.warnings } };
}

export function prepareReceiptVarianceResolutionForAccounting({
  action,
  purchaseOrderId,
  reason,
  receiptId,
  totalVariance
}: {
  action: "accept_difference" | "request_credit_note" | "request_supplier_clarification";
  purchaseOrderId: string;
  reason: string;
  receiptId: string;
  totalVariance: number;
}) {
  const command = prepareResolveReceiptVarianceCommand({
    action,
    companyId: BUSINESS_COMPANY_ID,
    purchaseOrderId,
    reason,
    receiptId,
    totalVariance
  });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: action === "accept_difference" ? 0.62 : 0.84,
    createdByAgent: "receipt-variance-agent",
    evidence: { action, purchaseOrderId, reason, totalVariance },
    kind: "receipt_variance",
    proposedCommand: command,
    refId: receiptId,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.resolve_variance",
    actorId: "business-runtime",
    actorType: "system",
    after: { command },
    companyId: BUSINESS_COMPANY_ID,
    refId: receiptId,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.receipt.resolve_variance-${receiptId}`,
    payload: { command, proposalId: proposal.id },
    topic: "business.receipt.resolve_variance"
  });

  return { audit, command, outbox, proposal, validation: { errors: [], warnings: action === "accept_difference" ? ["variance_accepted_requires_approval"] : [] } };
}

export function prepareEmployeeExpenseForAccounting({
  employeeName,
  employeePayableAccountId = "acc-employee-payables",
  projectId,
  receipt
}: {
  employeeName: string;
  employeePayableAccountId?: string;
  projectId?: string;
  receipt: BusinessReceipt;
}) {
  const input = {
    companyId: BUSINESS_COMPANY_ID,
    currency: receipt.currency,
    employeeName,
    employeePayableAccountId,
    expenseAccountId: receipt.expenseAccountId,
    expenseDate: receipt.receiptDate,
    grossAmount: receipt.total,
    id: receipt.id,
    netAmount: receipt.netAmount,
    projectId,
    taxAmount: receipt.taxAmount,
    taxCode: receipt.taxCode
  };
  const command = prepareSubmitEmployeeExpenseCommand(input);
  const journalDraft = buildEmployeeExpenseJournalDraft(input);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: receipt.extractedFields.some((field) => field.confidence < 0.7) ? 0.7 : 0.9,
    createdByAgent: "employee-expense-agent",
    evidence: { employeeName, projectId, receiptId: receipt.id },
    kind: "employee_expense",
    proposedCommand: command,
    refId: receipt.id,
    refType: "employee_expense"
  });
  const audit = createAccountingAuditEvent({
    action: "expense.prepare_employee_expense",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "employee_expense"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.expense.prepare_employee_expense-${receipt.id}`,
    payload: { command, journalDraft, proposalId: proposal.id },
    topic: "business.expense.prepare_employee_expense"
  });

  return { audit, command, journalDraft, outbox, proposal, validation: { errors: [], warnings: [] } };
}

export function prepareReceiptCapitalizationForAccounting({
  receipt
}: {
  receipt: BusinessReceipt;
}) {
  const assetId = `asset-${receipt.id}`;
  const asset = {
    accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
    acquisitionAccountId: receipt.payableAccountId,
    acquisitionCost: receipt.netAmount,
    acquisitionDate: receipt.receiptDate,
    assetAccountId: "acc-fixed-assets",
    currency: receipt.currency,
    depreciationExpenseAccountId: "acc-depreciation",
    id: assetId,
    name: `${receipt.vendorName} ${receipt.number}`,
    receiptId: receipt.id,
    salvageValue: 1,
    usefulLifeMonths: 60
  } satisfies Parameters<typeof buildAssetAcquisitionJournalDraft>[0]["asset"];
  const command = createAccountingCommand({
    companyId: BUSINESS_COMPANY_ID,
    payload: {
      assetId,
      receiptId: receipt.id,
      receiptNumber: receipt.number,
      vendorName: receipt.vendorName
    },
    refId: receipt.id,
    refType: "receipt",
    requestedBy: "business-runtime",
    type: "CapitalizeReceipt"
  });
  const journalDraft = buildAssetAcquisitionJournalDraft({
    asset,
    companyId: BUSINESS_COMPANY_ID,
    inputVatAccountId: receipt.taxAmount > 0 ? "acc-vat-input" : undefined,
    inputVatAmount: receipt.taxAmount,
    payableAccountId: receipt.payableAccountId
  });
  const receiptProjection = {
    companyId: BUSINESS_COMPANY_ID,
    currency: receipt.currency,
    dueDate: receipt.dueDate,
    expenseAccountExternalId: "acc-fixed-assets",
    externalId: receipt.id,
    extractedJson: receipt.extractedFields,
    files: [{
      blobRef: `receipt-file:${receipt.id}`,
      mime: "application/pdf",
      originalFilename: receipt.attachmentName,
      sha256: stablePseudoSha256(`${receipt.id}:${receipt.attachmentName}`)
    }],
    lines: [{
      description: `Fixed asset activation ${asset.name}`,
      expenseAccountExternalId: "acc-fixed-assets",
      lineNo: 1,
      netAmountMinor: toMinor(receipt.netAmount),
      taxAmountMinor: toMinor(receipt.taxAmount),
      taxCode: receipt.taxCode,
      totalAmountMinor: toMinor(receipt.total)
    }],
    netAmountMinor: toMinor(receipt.netAmount),
    number: receipt.number,
    payableAccountExternalId: receipt.payableAccountId,
    postedAt: new Date(`${receipt.receiptDate}T00:00:00.000Z`),
    postedJournalEntryExternalId: journalExternalId(journalDraft),
    receiptDate: receipt.receiptDate,
    reviewedAt: new Date(`${receipt.receiptDate}T00:00:00.000Z`),
    status: "posted",
    taxAmountMinor: toMinor(receipt.taxAmount),
    taxCode: receipt.taxCode,
    totalAmountMinor: toMinor(receipt.total),
    vendorExternalId: vendorExternalId(receipt.vendorName),
    vendorInvoiceNumber: receipt.number
  };
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: receipt.netAmount >= 250 ? 0.86 : 0.62,
    createdByAgent: "asset-accountant",
    evidence: {
      assetAccountId: asset.assetAccountId,
      netAmount: receipt.netAmount,
      receiptId: receipt.id,
      vendorName: receipt.vendorName
    },
    kind: "asset_activation",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "asset.prepare_capitalization",
    actorId: "business-runtime",
    actorType: "system",
    after: { asset, command, journalDraft, receiptProjection },
    companyId: BUSINESS_COMPANY_ID,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.asset.prepare_capitalization-${receipt.id}`,
    payload: { asset, command, journalDraft, proposalId: proposal.id, receiptProjection },
    topic: "business.asset.prepare_capitalization"
  });

  return {
    audit,
    assetProjection: asset,
    command,
    journalDraft,
    outbox,
    proposal,
    receiptProjection,
    validation: { errors: [], warnings: receipt.netAmount < 250 ? ["asset_capitalization_threshold_review"] : [] }
  };
}

export function prepareAssetDepreciationForAccounting({
  asset,
  fiscalYear = 2026
}: {
  asset: BusinessFixedAsset;
  fiscalYear?: number;
}) {
  const fixedAsset = {
    accumulatedDepreciationAccountId: asset.accumulatedDepreciationAccountId,
    acquisitionAccountId: asset.assetAccountId,
    acquisitionCost: asset.acquisitionCost,
    acquisitionDate: asset.acquisitionDate,
    assetAccountId: asset.assetAccountId,
    currency: asset.currency,
    depreciationExpenseAccountId: asset.depreciationExpenseAccountId,
    id: asset.id,
    name: asset.name,
    receiptId: asset.receiptId,
    salvageValue: asset.salvageValue,
    usefulLifeMonths: asset.usefulLifeMonths
  } satisfies Parameters<typeof buildAssetDepreciationJournalDraft>[0]["asset"];
  const dueLines = buildStraightLineDepreciationSchedule(fixedAsset).filter((line) => line.fiscalYear === fiscalYear);
  const annualAmount = dueLines.reduce((sum, line) => sum + line.amount, 0);
  const journalDraft = newAnnualDepreciationDraft(fixedAsset, fiscalYear, dueLines);
  const command = createAccountingCommand({
    companyId: BUSINESS_COMPANY_ID,
    payload: {
      amountMinor: toMinor(annualAmount),
      assetId: asset.id,
      fiscalYear
    },
    refId: `${asset.id}-${fiscalYear}`,
    refType: "asset",
    requestedBy: "business-runtime",
    type: "PostDepreciation"
  });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: dueLines.length ? 0.9 : 0.45,
    createdByAgent: "asset-accountant",
    evidence: {
      accountCredit: asset.accumulatedDepreciationAccountId,
      accountDebit: asset.depreciationExpenseAccountId,
      fiscalYear,
      lineCount: dueLines.length
    },
    kind: "asset_depreciation",
    proposedCommand: command,
    refId: asset.id,
    refType: "asset"
  });
  const audit = createAccountingAuditEvent({
    action: "asset.prepare_depreciation",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, dueLines, journalDraft },
    companyId: BUSINESS_COMPANY_ID,
    refId: asset.id,
    refType: "asset"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.asset.prepare_depreciation-${asset.id}-${fiscalYear}`,
    payload: { command, journalDraft, proposalId: proposal.id },
    topic: "business.asset.prepare_depreciation"
  });

  return {
    audit,
    command,
    journalDraft,
    outbox,
    proposal,
    validation: { errors: dueLines.length ? [] : ["asset_no_depreciation_due"], warnings: [] }
  };
}

export function prepareAssetDisposalForAccounting({
  asset,
  disposalDate = "2026-12-31",
  proceeds = 0
}: {
  asset: BusinessFixedAsset & { accumulatedDepreciation?: number; bookValue?: number };
  disposalDate?: string;
  proceeds?: number;
}) {
  const fixedAsset = {
    accumulatedDepreciationAccountId: asset.accumulatedDepreciationAccountId,
    acquisitionAccountId: asset.assetAccountId,
    acquisitionCost: asset.acquisitionCost,
    acquisitionDate: asset.acquisitionDate,
    assetAccountId: asset.assetAccountId,
    currency: asset.currency,
    depreciationExpenseAccountId: asset.depreciationExpenseAccountId,
    id: asset.id,
    name: asset.name,
    receiptId: asset.receiptId,
    salvageValue: asset.salvageValue,
    usefulLifeMonths: asset.usefulLifeMonths
  } satisfies Parameters<typeof buildAssetDisposalJournalDraft>[0]["asset"];
  const accumulatedDepreciation = asset.accumulatedDepreciation ?? Math.max(0, asset.acquisitionCost - (asset.bookValue ?? asset.acquisitionCost));
  const journalDraft = buildAssetDisposalJournalDraft({
    accumulatedDepreciation,
    asset: fixedAsset,
    companyId: BUSINESS_COMPANY_ID,
    disposalDate,
    gainAccountId: "acc-revenue-saas",
    lossAccountId: "acc-depreciation",
    proceeds,
    proceedsAccountId: "acc-bank"
  });
  const command = createAccountingCommand({
    companyId: BUSINESS_COMPANY_ID,
    payload: {
      accumulatedDepreciationMinor: toMinor(accumulatedDepreciation),
      assetId: asset.id,
      disposalDate,
      proceedsMinor: toMinor(proceeds)
    },
    refId: asset.id,
    refType: "asset",
    requestedBy: "business-runtime",
    type: "DisposeAsset"
  });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: asset.status === "Active" ? 0.84 : 0.52,
    createdByAgent: "asset-accountant",
    evidence: {
      accumulatedDepreciation,
      assetAccountId: asset.assetAccountId,
      bookValue: asset.bookValue ?? asset.acquisitionCost - accumulatedDepreciation,
      disposalDate,
      proceeds
    },
    kind: "asset_disposal",
    proposedCommand: command,
    refId: asset.id,
    refType: "asset"
  });
  const audit = createAccountingAuditEvent({
    action: "asset.prepare_disposal",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft },
    companyId: BUSINESS_COMPANY_ID,
    refId: asset.id,
    refType: "asset"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.asset.prepare_disposal-${asset.id}`,
    payload: { command, journalDraft, proposalId: proposal.id },
    topic: "business.asset.prepare_disposal"
  });

  return {
    audit,
    command,
    journalDraft,
    outbox,
    proposal,
    validation: { errors: asset.status === "Disposed" ? ["asset_already_disposed"] : [], warnings: proceeds === 0 ? ["asset_disposal_without_proceeds"] : [] }
  };
}

export function prepareBankMatchForAccounting({
  transaction
}: {
  transaction: BusinessBankTransaction;
}) {
  const command = prepareAcceptBankMatchCommand(transaction, BUSINESS_COMPANY_ID);
  const confidence = bankMatchConfidence(transaction);
  const journalDraft = buildBankMatchJournalDraft(transaction, {
    accountsPayableAccountId: "acc-ap",
    accountsReceivableAccountId: "acc-ar",
    bankAccountId: "acc-bank",
    bankFeeAccountId: "acc-fees",
    companyId: BUSINESS_COMPANY_ID
  });
  const paymentProjection = {
    allocation: transaction.matchedRecordId ? {
      amountMinor: toMinor(Math.abs(transaction.amount)),
      invoiceExternalId: transaction.matchType === "invoice" ? transaction.matchedRecordId : null,
      receiptExternalId: transaction.matchType === "receipt" ? transaction.matchedRecordId : null
    } : undefined,
    allocations: transaction.matchedRecordId ? [{
      amountMinor: toMinor(Math.abs(transaction.amount)),
      invoiceExternalId: transaction.matchType === "invoice" ? transaction.matchedRecordId : null,
      receiptExternalId: transaction.matchType === "receipt" ? transaction.matchedRecordId : null
    }] : [],
    amountMinor: toMinor(Math.abs(transaction.amount)),
    bankAccountExternalId: "acc-bank",
    bankStatementLineExternalId: transaction.id,
    companyId: BUSINESS_COMPANY_ID,
    currency: transaction.currency,
    externalId: `pay-${transaction.id}`,
    kind: transaction.amount >= 0 ? "incoming" : "outgoing",
    partyExternalId: transaction.matchedRecordId ?? vendorExternalId(transaction.counterparty),
    paymentDate: transaction.bookingDate,
    postedJournalEntryExternalId: journalExternalId(journalDraft)
  };
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence,
    createdByAgent: "bank-reconciler",
    evidence: {
      counterparty: transaction.counterparty,
      matchedRecordId: transaction.matchedRecordId,
      purpose: transaction.purpose,
      status: transaction.status
    },
    kind: "bank_match",
    proposedCommand: command,
    refId: transaction.id,
    refType: "bank_transaction",
    status: confidence >= 0.99 ? "auto_applied" : "open"
  });
  const audit = createAccountingAuditEvent({
    action: "bank_match.prepare_accept",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, confidence, journalDraft, paymentProjection },
    companyId: BUSINESS_COMPANY_ID,
    refId: transaction.id,
    refType: "bank_transaction"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.bank_match.prepare_accept-${transaction.id}`,
    payload: { command, journalDraft, paymentProjection, proposalId: proposal.id },
    topic: "business.bank_match.prepare_accept"
  });

  return { audit, command, journalDraft, outbox, paymentProjection, proposal, validation: { errors: [], warnings: confidence < 0.8 ? ["manual_review_recommended"] : [] } };
}

export function prepareSupplierPaymentForAccounting({
  allocations,
  bankAccountId = "acc-bank",
  id,
  payableAccountId = "acc-ap",
  paymentDate,
  vendorName
}: {
  allocations: Array<{ amount: number; receiptId: string }>;
  bankAccountId?: string;
  id: string;
  payableAccountId?: string;
  paymentDate: string;
  vendorName: string;
}) {
  const input = {
    allocations,
    bankAccountId,
    companyId: BUSINESS_COMPANY_ID,
    currency: "EUR",
    id,
    payableAccountId,
    paymentDate,
    vendorName
  };
  const command = prepareSupplierPaymentCommand(input);
  const journalDraft = buildSupplierPaymentJournalDraft(input);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: allocations.length > 1 ? 0.88 : 0.92,
    createdByAgent: "payables-reconciler",
    evidence: { allocations, vendorName },
    kind: "payables_payment",
    proposedCommand: command,
    refId: id,
    refType: "payment"
  });
  const audit = createAccountingAuditEvent({
    action: "payables.prepare_supplier_payment",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft },
    companyId: BUSINESS_COMPANY_ID,
    refId: id,
    refType: "payment"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.payables.prepare_supplier_payment-${id}`,
    payload: { command, journalDraft, proposalId: proposal.id },
    topic: "business.payables.prepare_supplier_payment"
  });

  return { audit, command, journalDraft, outbox, proposal, validation: { errors: [], warnings: [] } };
}

export function prepareSupplierDiscountForAccounting({
  allocations,
  bankAccountId = "acc-bank",
  discountAccountId = "acc-purchase-discounts",
  discountGrossAmount,
  discountNetAmount,
  id,
  inputVatAccountId = "acc-vat-input",
  inputVatCorrectionAmount,
  paidAmount,
  payableAccountId = "acc-ap",
  paymentDate,
  vendorName
}: {
  allocations: Array<{ amount: number; receiptId: string }>;
  bankAccountId?: string;
  discountAccountId?: string;
  discountGrossAmount: number;
  discountNetAmount: number;
  id: string;
  inputVatAccountId?: string;
  inputVatCorrectionAmount: number;
  paidAmount: number;
  payableAccountId?: string;
  paymentDate: string;
  vendorName: string;
}) {
  const input = {
    allocations,
    bankAccountId,
    companyId: BUSINESS_COMPANY_ID,
    currency: "EUR",
    discountAccountId,
    discountGrossAmount,
    discountNetAmount,
    id,
    inputVatAccountId,
    inputVatCorrectionAmount,
    paidAmount,
    payableAccountId,
    paymentDate,
    vendorName
  };
  const command = prepareSupplierDiscountCommand(input);
  const journalDraft = buildSupplierDiscountJournalDraft(input);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: 0.86,
    createdByAgent: "payables-discount-agent",
    evidence: { allocations, discountGrossAmount, paidAmount, vendorName },
    kind: "supplier_discount",
    proposedCommand: command,
    refId: id,
    refType: "payment"
  });
  const audit = createAccountingAuditEvent({
    action: "payables.prepare_supplier_discount",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, journalDraft },
    companyId: BUSINESS_COMPANY_ID,
    refId: id,
    refType: "payment"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.payables.prepare_supplier_discount-${id}`,
    payload: { command, journalDraft, proposalId: proposal.id },
    topic: "business.payables.prepare_supplier_discount"
  });

  return { audit, command, journalDraft, outbox, proposal, validation: { errors: [], warnings: [] } };
}

export function preparePaymentRunForAccounting({
  candidates,
  dueBy,
  id
}: {
  candidates: Array<{ amount: number; blocked?: boolean; currency: string; dueDate: string; receiptId: string; vendorName: string }>;
  dueBy: string;
  id: string;
}) {
  const input = { candidates, companyId: BUSINESS_COMPANY_ID, dueBy, id };
  const selected = selectPaymentRunCandidates(input);
  const command = preparePaymentRunCommand(input);
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: selected.length ? 0.84 : 0.5,
    createdByAgent: "payables-run-agent",
    evidence: { blockedCount: candidates.filter((candidate) => candidate.blocked).length, dueBy, selected },
    kind: "payables_payment_run",
    proposedCommand: command,
    refId: id,
    refType: "payment_run"
  });
  const audit = createAccountingAuditEvent({
    action: "payables.prepare_payment_run",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, selected },
    companyId: BUSINESS_COMPANY_ID,
    refId: id,
    refType: "payment_run"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.payables.prepare_payment_run-${id}`,
    payload: { command, proposalId: proposal.id, selected },
    topic: "business.payables.prepare_payment_run"
  });

  return { audit, command, outbox, proposal, selected, validation: { errors: [], warnings: selected.length ? [] : ["no_payables_due"] } };
}

export function prepareDatevExportForAccounting({
  data,
  exportBatch
}: {
  data: BusinessBundle;
  exportBatch: BusinessBookkeepingExport;
}) {
  const command = {
    companyId: BUSINESS_COMPANY_ID,
    idempotencyKey: `${BUSINESS_COMPANY_ID}:ExportDatev:bookkeeping:${exportBatch.id}`,
    payload: { exportId: exportBatch.id, period: exportBatch.period, system: exportBatch.system },
    refId: exportBatch.id,
    refType: "bookkeeping_export",
    requestedAt: new Date().toISOString(),
    requestedBy: "business-runtime",
    type: "ExportDatev" as const
  };
  const exportLines = buildDatevLines(data, exportBatch.id);
  const sourceRows = exportLines.length ? exportLines : buildDatevLines(data);
  const sourceLines: DatevExtfLine[] = sourceRows.map((line) => ({
    accountCode: line.account.code,
    amount: line.amount,
    contraAccountCode: line.contraAccount?.code,
    currency: line.account.currency,
    date: line.entry.postingDate,
    documentNumber: line.entry.number,
    side: line.side === "H" ? "H" : "S",
    taxCode: line.taxCode,
    text: typeof line.entry.narration === "string" ? line.entry.narration : line.entry.narration.de
  }));
  const csv = buildDatevExtfCsv(sourceLines, {
    accountLength: 4,
    clientNumber: "67890",
    consultantNumber: "12345",
    fiscalYearStart: "20260101"
  });
  const proposal = createAccountingProposal({
    companyId: BUSINESS_COMPANY_ID,
    confidence: exportBatch.status === "Needs review" ? 0.72 : 0.94,
    createdByAgent: "datev-exporter",
    evidence: { lineCount: sourceLines.length, period: exportBatch.period, system: exportBatch.system },
    kind: "datev_export",
    proposedCommand: command,
    refId: exportBatch.id,
    refType: "bookkeeping_export"
  });
  const audit = createAccountingAuditEvent({
    action: "datev.prepare_export",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, lineCount: sourceLines.length },
    companyId: BUSINESS_COMPANY_ID,
    refId: exportBatch.id,
    refType: "bookkeeping_export"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: BUSINESS_COMPANY_ID,
    id: `outbox-business.datev.prepare_export-${exportBatch.id}`,
    payload: { command, proposalId: proposal.id },
    topic: "business.datev.prepare_export"
  });
  const datevExport = {
    companyId: BUSINESS_COMPANY_ID,
    csvBlobRef: `datev-export:${exportBatch.id}`,
    csvSha256: stablePseudoSha256(csv),
    externalId: exportBatch.id,
    exportedAt: exportBatch.status === "Exported" ? new Date(exportBatch.generatedAt) : null,
    exportedBy: exportBatch.status === "Exported" ? exportBatch.reviewer : null,
    lineCount: sourceLines.length,
    netAmountMinor: toMinor(exportBatch.netAmount),
    payload: { filename: `${exportBatch.period}-datev.csv`, proposalId: proposal.id },
    period: exportBatch.period,
    sourceProposalExternalId: proposal.id,
    status: exportBatch.status === "Exported" ? "exported" : "prepared",
    system: exportBatch.system,
    taxAmountMinor: toMinor(exportBatch.taxAmount)
  };

  return { audit, command, csv, datevExport, outbox, proposal, validation: { errors: [], warnings: [] } };
}

export function invoiceAccountingParties(customer: BusinessCustomer | undefined, products: BusinessProduct[]) {
  return {
    customer,
    products
  };
}

function newAnnualDepreciationDraft(
  asset: Parameters<typeof buildAssetDepreciationJournalDraft>[0]["asset"],
  fiscalYear: number,
  lines: ReturnType<typeof buildStraightLineDepreciationSchedule>
) {
  if (!lines.length) {
    return buildAssetDepreciationJournalDraft({
      asset,
      companyId: BUSINESS_COMPANY_ID,
      line: {
        accumulatedDepreciation: 0,
        amount: 0,
        bookValue: asset.acquisitionCost,
        fiscalYear,
        postingDate: `${fiscalYear}-12-31`
      }
    });
  }

  const amount = lines.reduce((sum, line) => sum + line.amount, 0);
  return new LedgerPosting(BUSINESS_COMPANY_ID, "asset", `${asset.id}-${fiscalYear}`, `${fiscalYear}-12-31`, asset.currency)
    .debit(asset.depreciationExpenseAccountId, amount)
    .credit(asset.accumulatedDepreciationAccountId, amount)
    .toJournalDraft("depreciation", `Annual depreciation ${fiscalYear} ${asset.name}.`);
}

function toMinor(amount: number) {
  return Math.round((amount + Number.EPSILON) * 100);
}

function localize(value: string | { de: string; en: string }, locale: SupportedLocale) {
  return typeof value === "string" ? value : value[locale];
}

function revenueAccountExternalId(revenueAccount: string) {
  const code = revenueAccount.trim().split(/\s+/)[0];
  if (code === "8337") return "acc-revenue-implementation";
  if (code === "8338") return "acc-revenue-research";
  if (code === "8401") return "acc-revenue-support";
  return "acc-revenue-saas";
}

function journalExternalId(journalDraft: ReturnType<typeof buildInvoiceJournalDraft>) {
  return `je-${journalDraft.type}-${journalDraft.refType}-${journalDraft.refId}`;
}

function requestTimestampFromInvoice(invoice: BusinessInvoice) {
  return invoice.status === "Draft" ? null : new Date(`${invoice.issueDate}T00:00:00.000Z`);
}

function quoteFromPayload(
  payload: Record<string, unknown> | undefined,
  data: Pick<BusinessBundle, "customers" | "products">,
  status: BusinessQuoteLike["status"] = "Draft"
): BusinessQuoteLike {
  const product = data.products.find((item) => item.id === readString(payload, "productId")) ?? data.products[0];
  const fallbackLine = {
    productId: product?.id ?? "manual-service",
    quantity: readNumber(payload, "quantity") ?? 1,
    taxRate: readNumber(payload, "taxRate") ?? product?.taxRate ?? 19,
    unitPrice: readNumber(payload, "unitPrice") ?? readNumber(payload, "netAmount") ?? product?.price ?? 0
  };
  const lines = readInvoiceLines(payload) ?? [fallbackLine];
  const netAmount = readNumber(payload, "netAmount") ?? lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);
  const taxAmount = readNumber(payload, "taxAmount") ?? lines.reduce((sum, line) => sum + line.quantity * line.unitPrice * (line.taxRate / 100), 0);
  return {
    currency: (readString(payload, "currency") === "USD" ? "USD" : "EUR"),
    customerId: readString(payload, "customerId") ?? data.customers[0]?.id ?? "customer-draft",
    id: readString(payload, "quoteId") ?? readString(payload, "id") ?? `quote-${crypto.randomUUID()}`,
    issueDate: readString(payload, "issueDate") ?? todayIsoDate(),
    lines,
    netAmount,
    notes: readString(payload, "notes") ?? "",
    number: readString(payload, "quoteNumber") ?? readString(payload, "number") ?? `ANG-${todayIsoDate().slice(0, 4)}-${Math.floor(Math.random() * 9000 + 1000)}`,
    status: readString(payload, "status") ?? status,
    taxAmount,
    total: readNumber(payload, "total") ?? round(netAmount + taxAmount),
    validUntil: readString(payload, "validUntil") ?? addDays(readString(payload, "issueDate") ?? todayIsoDate(), 14)
  };
}

function readInvoiceLines(payload: Record<string, unknown> | undefined) {
  const value = payload?.lines;
  if (!Array.isArray(value)) return null;
  const lines = value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    const productId = readString(record, "productId");
    const quantity = readNumber(record, "quantity");
    const unitPrice = readNumber(record, "unitPrice");
    const taxRate = readNumber(record, "taxRate");
    if (!productId || !quantity || unitPrice === undefined || taxRate === undefined) return [];
    return [{ productId, quantity, taxRate, unitPrice }];
  });
  return lines.length ? lines : null;
}

function readString(payload: Record<string, unknown> | undefined, key: string) {
  const value = payload?.[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function readNumber(payload: Record<string, unknown> | undefined, key: string) {
  const value = payload?.[key];
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() && Number.isFinite(Number(value))) return Number(value);
  return undefined;
}

function todayIsoDate() {
  return new Date().toISOString().slice(0, 10);
}

function addDays(value: string, days: number) {
  const date = new Date(`${value}T00:00:00.000Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

function clampDunningLevel(value: number): 1 | 2 | 3 {
  if (value <= 1) return 1;
  if (value >= 3) return 3;
  return 2;
}

function extendedProposalKind(kind: string) {
  return kind as Parameters<typeof createAccountingProposal>[0]["kind"];
}

function vendorExternalId(vendorName: string) {
  const normalized = vendorName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return normalized.startsWith("vendor-") ? normalized.replace(/^(vendor-)+/, "vendor-") : `vendor-${normalized}`;
}

function receiptToDuplicateFingerprint(receipt: BusinessReceipt) {
  return {
    amount: receipt.total,
    currency: receipt.currency,
    id: receipt.id,
    receiptDate: receipt.receiptDate,
    sha256: stablePseudoSha256(`${receipt.id}:${receipt.attachmentName}`),
    vendorInvoiceNumber: receipt.number,
    vendorName: receipt.vendorName
  };
}

function stablePseudoSha256(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = Math.imul(31, hash) + value.charCodeAt(index) | 0;
  }
  return `sha256-demo-${Math.abs(hash).toString(16).padStart(8, "0")}`;
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
