import {
  bankMatchConfidence,
  buildDatevExtfCsv,
  buildBankMatchJournalDraft,
  buildInvoiceDocument as buildAccountingInvoiceDocument,
  buildInvoiceJournalDraft,
  buildReceiptJournalDraft,
  buildZugferdXml,
  createAccountingAuditEvent,
  createAccountingProposal,
  createBusinessOutboxEvent,
  prepareAcceptBankMatchCommand,
  preparePostReceiptCommand,
  prepareSendInvoiceCommand,
  validateInvoiceForSend,
  type DatevExtfLine,
  type InvoiceContext
} from "@ctox-business/accounting";
import { buildDatevLines } from "./accounting-runtime";
import type {
  BusinessBankTransaction,
  BusinessBookkeepingExport,
  BusinessBundle,
  BusinessCustomer,
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
    payload: { command, journalDraft, proposalId: proposal.id, receiptProjection },
    topic: "business.receipt.prepare_post"
  });

  return { audit, command, journalDraft, outbox, proposal, receiptProjection, validation: { errors: [], warnings: [] } };
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
    payload: { command, journalDraft, paymentProjection, proposalId: proposal.id },
    topic: "business.bank_match.prepare_accept"
  });

  return { audit, command, journalDraft, outbox, paymentProjection, proposal, validation: { errors: [], warnings: confidence < 0.8 ? ["manual_review_recommended"] : [] } };
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
  const sourceLines: DatevExtfLine[] = buildDatevLines(data, exportBatch.id).map((line) => ({
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
    payload: { command, proposalId: proposal.id },
    topic: "business.datev.prepare_export"
  });

  return { audit, command, csv, outbox, proposal, validation: { errors: [], warnings: [] } };
}

export function invoiceAccountingParties(customer: BusinessCustomer | undefined, products: BusinessProduct[]) {
  return {
    customer,
    products
  };
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

function vendorExternalId(vendorName: string) {
  return `vendor-${vendorName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}`;
}

function stablePseudoSha256(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = Math.imul(31, hash) + value.charCodeAt(index) | 0;
  }
  return `sha256-demo-${Math.abs(hash).toString(16).padStart(8, "0")}`;
}
