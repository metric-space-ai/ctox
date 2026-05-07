import {
  bankMatchConfidence,
  buildAssetAcquisitionJournalDraft,
  buildAssetDepreciationJournalDraft,
  buildAssetDisposalJournalDraft,
  buildDatevExtfCsv,
  buildBankMatchJournalDraft,
  buildStraightLineDepreciationSchedule,
  buildInvoiceDocument as buildAccountingInvoiceDocument,
  buildInvoiceJournalDraft,
  buildReceiptJournalDraft,
  buildZugferdXml,
  createAccountingAuditEvent,
  createAccountingCommand,
  createAccountingProposal,
  createBusinessOutboxEvent,
  LedgerPosting,
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
    refId: `${asset.id}-disposal`,
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
