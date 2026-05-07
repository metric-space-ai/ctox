import { eq } from "drizzle-orm";
import { createBusinessDb } from "./client";
import {
  accountingAccounts,
  accountingFiscalPeriods,
  accountingInvoiceLines,
  accountingInvoices,
  accountingJournalEntries,
  accountingJournalEntryLines,
  accountingLedgerEntries,
  accountingParties,
  accountingPaymentAllocations,
  accountingPayments,
  accountingReceiptFiles,
  accountingReceiptLines,
  accountingReceipts,
  accountingTaxRates,
  businessAccountingAuditEvents,
  businessAccountingProposals,
  businessOutboxEvents
} from "./schema";

export type AccountingJournalDraft = {
  companyId: string;
  lines: Array<{
    accountId: string;
    costCenterId?: string;
    credit: { minor: number };
    debit: { minor: number };
    partyId?: string;
    projectId?: string;
  }>;
  narration?: string;
  postingDate: string;
  refId: string;
  refType: string;
  type: string;
};

export type AccountingInvoiceProjection = {
  balanceDueMinor: number;
  companyId: string;
  currency: string;
  customerExternalId: string;
  dueDate: string;
  externalId: string;
  issueDate: string;
  lines: Array<{
    description: string;
    lineNetMinor: number;
    lineNo: number;
    lineTotalMinor: number;
    productExternalId?: string;
    quantity: number;
    revenueAccountExternalId?: string;
    taxAmountMinor: number;
    taxRate: number;
    unitPriceMinor: number;
  }>;
  netAmountMinor: number;
  number: string;
  pdfBlobRef?: string | null;
  postedJournalEntryExternalId?: string | null;
  sentAt?: Date | null;
  serviceDate?: string | null;
  status: string;
  taxAmountMinor: number;
  totalAmountMinor: number;
  zugferdXml?: string | null;
};

export type AccountingReceiptProjection = {
  companyId: string;
  currency: string;
  dueDate?: string | null;
  expenseAccountExternalId?: string | null;
  externalId: string;
  extractedJson?: unknown;
  files?: Array<{
    blobRef: string;
    mime: string;
    originalFilename: string;
    sha256: string;
  }>;
  lines: Array<{
    description: string;
    expenseAccountExternalId: string;
    lineNo: number;
    netAmountMinor: number;
    taxAmountMinor: number;
    taxCode?: string | null;
    totalAmountMinor: number;
  }>;
  netAmountMinor: number;
  number: string;
  ocrText?: string | null;
  payableAccountExternalId?: string | null;
  postedAt?: Date | null;
  postedJournalEntryExternalId?: string | null;
  receiptDate: string;
  reviewedAt?: Date | null;
  status: string;
  taxAmountMinor: number;
  taxCode?: string | null;
  totalAmountMinor: number;
  vendorExternalId?: string | null;
  vendorInvoiceNumber?: string | null;
};

export type AccountingPaymentProjection = {
  allocation?: {
    amountMinor: number;
    invoiceExternalId?: string | null;
    receiptExternalId?: string | null;
  };
  amountMinor: number;
  bankAccountExternalId: string;
  bankStatementLineExternalId?: string | null;
  companyId: string;
  currency: string;
  externalId: string;
  kind: string;
  partyExternalId?: string | null;
  paymentDate: string;
  postedJournalEntryExternalId?: string | null;
};

export type AccountingWorkflowSnapshot = {
  audit?: {
    action: string;
    actorId: string;
    actorType: string;
    after?: unknown;
    before?: unknown;
    companyId: string;
    refId: string;
    refType: string;
  };
  invoice?: AccountingInvoiceProjection;
  journalDraft?: AccountingJournalDraft | null;
  outbox?: {
    companyId: string;
    id: string;
    payload: unknown;
    status: string;
    topic: string;
  };
  payment?: AccountingPaymentProjection;
  proposal?: {
    companyId: string;
    confidence: number;
    createdByAgent: string;
    evidence: unknown;
    id: string;
    kind: string;
    proposedCommand: unknown;
    refId: string;
    refType: string;
    status: string;
  };
  receipt?: AccountingReceiptProjection;
};

export type AccountingSetupSnapshot = {
  accounts: Array<{
    accountType: string;
    code: string;
    companyId: string;
    currency: string;
    externalId: string;
    isGroup?: boolean;
    name: string;
    parentId?: string;
    rootType: string;
  }>;
  fiscalPeriods: Array<{
    companyId: string;
    endDate: string;
    externalId: string;
    startDate: string;
    status: string;
  }>;
  parties: Array<{
    companyId: string;
    defaultPayableAccountId?: string;
    defaultReceivableAccountId?: string;
    externalId: string;
    kind: string;
    name: string;
    taxId?: string;
    vatId?: string;
  }>;
  taxRates: Array<{
    accountId?: string;
    code: string;
    companyId: string;
    externalId: string;
    rate: number;
    type: string;
  }>;
};

export async function saveAccountingWorkflowSnapshot(snapshot: AccountingWorkflowSnapshot, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);

  await db.transaction(async (tx) => {
    if (snapshot.proposal) {
      const proposal = snapshot.proposal;
      const values = {
        companyId: proposal.companyId,
        confidence: Math.round(proposal.confidence * 100),
        createdByAgent: proposal.createdByAgent,
        evidenceJson: JSON.stringify(proposal.evidence),
        externalId: proposal.id,
        kind: proposal.kind,
        proposedCommandJson: JSON.stringify(proposal.proposedCommand),
        refId: proposal.refId,
        refType: proposal.refType,
        status: proposal.status,
        updatedAt: new Date()
      };

      await tx.insert(businessAccountingProposals).values(values).onConflictDoUpdate({
        target: businessAccountingProposals.externalId,
        set: values
      });
    }

    if (snapshot.outbox) {
      const outbox = snapshot.outbox;
      const values = {
        attempts: 0,
        companyId: outbox.companyId,
        externalId: outbox.id,
        payloadJson: JSON.stringify(outbox.payload),
        status: outbox.status,
        topic: outbox.topic,
        updatedAt: new Date()
      };

      await tx.insert(businessOutboxEvents).values(values).onConflictDoUpdate({
        target: businessOutboxEvents.externalId,
        set: values
      });
    }

    if (snapshot.audit) {
      const audit = snapshot.audit;
      await tx.insert(businessAccountingAuditEvents).values({
        action: audit.action,
        actorId: audit.actorId,
        actorType: audit.actorType,
        afterJson: audit.after === undefined ? null : JSON.stringify(audit.after),
        beforeJson: audit.before === undefined ? null : JSON.stringify(audit.before),
        companyId: audit.companyId,
        refId: audit.refId,
        refType: audit.refType
      });
    }

    if (snapshot.invoice) {
      await upsertAccountingInvoice(tx, snapshot.invoice);
    }

    if (snapshot.receipt) {
      await upsertAccountingReceipt(tx, snapshot.receipt);
    }

    if (snapshot.payment) {
      await upsertAccountingPayment(tx, snapshot.payment);
    }

    if (snapshot.journalDraft) {
      await upsertJournalDraft(tx, snapshot.journalDraft);
    }
  });
}

export async function listAccountingProposals(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(businessAccountingProposals);
}

export async function listBusinessOutboxEvents(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(businessOutboxEvents);
}

export async function saveAccountingSetupSnapshot(snapshot: AccountingSetupSnapshot, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  await db.transaction(async (tx) => {
    for (const account of snapshot.accounts) {
      const values = {
        accountType: account.accountType,
        code: account.code,
        companyId: account.companyId,
        currency: account.currency,
        externalId: account.externalId,
        isGroup: account.isGroup ? 1 : 0,
        name: account.name,
        parentExternalId: account.parentId ?? null,
        rootType: account.rootType,
        updatedAt: new Date()
      };
      await tx.insert(accountingAccounts).values(values).onConflictDoUpdate({
        target: accountingAccounts.externalId,
        set: values
      });
    }

    for (const party of snapshot.parties) {
      const values = {
        companyId: party.companyId,
        defaultPayableAccountExternalId: party.defaultPayableAccountId ?? null,
        defaultReceivableAccountExternalId: party.defaultReceivableAccountId ?? null,
        externalId: party.externalId,
        kind: party.kind,
        name: party.name,
        taxId: party.taxId ?? null,
        updatedAt: new Date(),
        vatId: party.vatId ?? null
      };
      await tx.insert(accountingParties).values(values).onConflictDoUpdate({
        target: accountingParties.externalId,
        set: values
      });
    }

    for (const taxRate of snapshot.taxRates) {
      const values = {
        accountExternalId: taxRate.accountId ?? null,
        code: taxRate.code,
        companyId: taxRate.companyId,
        externalId: taxRate.externalId,
        rate: taxRate.rate,
        type: taxRate.type,
        updatedAt: new Date()
      };
      await tx.insert(accountingTaxRates).values(values).onConflictDoUpdate({
        target: accountingTaxRates.externalId,
        set: values
      });
    }

    for (const period of snapshot.fiscalPeriods) {
      const values = {
        companyId: period.companyId,
        endDate: period.endDate,
        externalId: period.externalId,
        startDate: period.startDate,
        status: period.status,
        updatedAt: new Date()
      };
      await tx.insert(accountingFiscalPeriods).values(values).onConflictDoUpdate({
        target: accountingFiscalPeriods.externalId,
        set: values
      });
    }
  });
}

export async function closeAccountingFiscalPeriod(input: {
  closedAt?: Date;
  externalId: string;
  status?: "closed";
}, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  await db.update(accountingFiscalPeriods)
    .set({
      closedAt: input.closedAt ?? new Date(),
      status: input.status ?? "closed",
      updatedAt: new Date()
    })
    .where(eq(accountingFiscalPeriods.externalId, input.externalId));
}

async function upsertAccountingInvoice(tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0], invoice: AccountingInvoiceProjection) {
  const values = {
    balanceDueMinor: invoice.balanceDueMinor,
    companyId: invoice.companyId,
    currency: invoice.currency,
    customerExternalId: invoice.customerExternalId,
    dueDate: invoice.dueDate,
    externalId: invoice.externalId,
    issueDate: invoice.issueDate,
    netAmountMinor: invoice.netAmountMinor,
    number: invoice.number,
    pdfBlobRef: invoice.pdfBlobRef ?? null,
    postedJournalEntryExternalId: invoice.postedJournalEntryExternalId ?? null,
    sentAt: invoice.sentAt ?? null,
    serviceDate: invoice.serviceDate ?? null,
    status: invoice.status,
    taxAmountMinor: invoice.taxAmountMinor,
    totalAmountMinor: invoice.totalAmountMinor,
    updatedAt: new Date(),
    zugferdXml: invoice.zugferdXml ?? null
  };

  await tx.insert(accountingInvoices).values(values).onConflictDoUpdate({
    target: accountingInvoices.externalId,
    set: values
  });

  await tx.delete(accountingInvoiceLines).where(eq(accountingInvoiceLines.invoiceExternalId, invoice.externalId));
  if (invoice.lines.length) {
    await tx.insert(accountingInvoiceLines).values(invoice.lines.map((line) => ({
      description: line.description,
      invoiceExternalId: invoice.externalId,
      lineNetMinor: line.lineNetMinor,
      lineNo: line.lineNo,
      lineTotalMinor: line.lineTotalMinor,
      productExternalId: line.productExternalId ?? null,
      quantity: line.quantity,
      revenueAccountExternalId: line.revenueAccountExternalId ?? null,
      taxAmountMinor: line.taxAmountMinor,
      taxRate: line.taxRate,
      unitPriceMinor: line.unitPriceMinor
    })));
  }
}

async function upsertAccountingReceipt(tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0], receipt: AccountingReceiptProjection) {
  const values = {
    companyId: receipt.companyId,
    currency: receipt.currency,
    dueDate: receipt.dueDate ?? null,
    expenseAccountExternalId: receipt.expenseAccountExternalId ?? null,
    externalId: receipt.externalId,
    extractedJson: receipt.extractedJson === undefined ? null : JSON.stringify(receipt.extractedJson),
    netAmountMinor: receipt.netAmountMinor,
    number: receipt.number,
    ocrText: receipt.ocrText ?? null,
    payableAccountExternalId: receipt.payableAccountExternalId ?? null,
    postedAt: receipt.postedAt ?? null,
    postedJournalEntryExternalId: receipt.postedJournalEntryExternalId ?? null,
    receiptDate: receipt.receiptDate,
    reviewedAt: receipt.reviewedAt ?? null,
    status: receipt.status,
    taxAmountMinor: receipt.taxAmountMinor,
    taxCode: receipt.taxCode ?? null,
    totalAmountMinor: receipt.totalAmountMinor,
    updatedAt: new Date(),
    vendorExternalId: receipt.vendorExternalId ?? null,
    vendorInvoiceNumber: receipt.vendorInvoiceNumber ?? null
  };

  await tx.insert(accountingReceipts).values(values).onConflictDoUpdate({
    target: accountingReceipts.externalId,
    set: values
  });

  await tx.delete(accountingReceiptLines).where(eq(accountingReceiptLines.receiptExternalId, receipt.externalId));
  if (receipt.lines.length) {
    await tx.insert(accountingReceiptLines).values(receipt.lines.map((line) => ({
      description: line.description,
      expenseAccountExternalId: line.expenseAccountExternalId,
      lineNo: line.lineNo,
      netAmountMinor: line.netAmountMinor,
      receiptExternalId: receipt.externalId,
      taxAmountMinor: line.taxAmountMinor,
      taxCode: line.taxCode ?? null,
      totalAmountMinor: line.totalAmountMinor
    })));
  }

  await tx.delete(accountingReceiptFiles).where(eq(accountingReceiptFiles.receiptExternalId, receipt.externalId));
  if (receipt.files?.length) {
    await tx.insert(accountingReceiptFiles).values(receipt.files.map((file) => ({
      blobRef: file.blobRef,
      mime: file.mime,
      originalFilename: file.originalFilename,
      receiptExternalId: receipt.externalId,
      sha256: file.sha256
    })));
  }
}

async function upsertAccountingPayment(tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0], payment: AccountingPaymentProjection) {
  const values = {
    amountMinor: payment.amountMinor,
    bankAccountExternalId: payment.bankAccountExternalId,
    bankStatementLineExternalId: payment.bankStatementLineExternalId ?? null,
    companyId: payment.companyId,
    currency: payment.currency,
    externalId: payment.externalId,
    kind: payment.kind,
    partyExternalId: payment.partyExternalId ?? null,
    paymentDate: payment.paymentDate,
    postedJournalEntryExternalId: payment.postedJournalEntryExternalId ?? null,
    updatedAt: new Date()
  };

  await tx.insert(accountingPayments).values(values).onConflictDoUpdate({
    target: accountingPayments.externalId,
    set: values
  });

  await tx.delete(accountingPaymentAllocations).where(eq(accountingPaymentAllocations.paymentExternalId, payment.externalId));
  if (payment.allocation) {
    await tx.insert(accountingPaymentAllocations).values({
      amountMinor: payment.allocation.amountMinor,
      invoiceExternalId: payment.allocation.invoiceExternalId ?? null,
      paymentExternalId: payment.externalId,
      receiptExternalId: payment.allocation.receiptExternalId ?? null
    });
  }
}

async function upsertJournalDraft(tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0], journal: AccountingJournalDraft) {
  const externalId = journalExternalId(journal);
  const now = new Date();
  const existing = await tx.select({ externalId: accountingJournalEntries.externalId })
    .from(accountingJournalEntries)
    .where(eq(accountingJournalEntries.externalId, externalId))
    .limit(1);
  if (existing.length) return;

  await tx.insert(accountingJournalEntries).values({
    companyId: journal.companyId,
    createdBy: "business-runtime",
    externalId,
    narration: journal.narration ?? null,
    number: journalNumber(journal),
    postedAt: now,
    postingDate: journal.postingDate,
    refId: journal.refId,
    refType: journal.refType,
    type: journal.type
  }).onConflictDoNothing();

  await tx.delete(accountingJournalEntryLines).where(eq(accountingJournalEntryLines.journalEntryExternalId, externalId));
  await tx.delete(accountingLedgerEntries).where(eq(accountingLedgerEntries.journalEntryExternalId, externalId));

  if (!journal.lines.length) return;

  await tx.insert(accountingJournalEntryLines).values(journal.lines.map((line, index) => ({
    accountExternalId: line.accountId,
    costCenterExternalId: line.costCenterId ?? null,
    creditMinor: line.credit.minor,
    debitMinor: line.debit.minor,
    journalEntryExternalId: externalId,
    lineNo: index + 1,
    partyExternalId: line.partyId ?? null,
    projectExternalId: line.projectId ?? null
  })));

  await tx.insert(accountingLedgerEntries).values(journal.lines.map((line, index) => ({
    accountExternalId: line.accountId,
    companyId: journal.companyId,
    creditMinor: line.credit.minor,
    debitMinor: line.debit.minor,
    externalId: `${externalId}-ledger-${index + 1}`,
    journalEntryExternalId: externalId,
    partyExternalId: line.partyId ?? null,
    postingDate: journal.postingDate,
    refId: journal.refId,
    refType: journal.refType
  })));
}

function journalExternalId(journal: AccountingJournalDraft) {
  return `je-${journal.type}-${journal.refType}-${journal.refId}`;
}

function journalNumber(journal: AccountingJournalDraft) {
  return `${journal.type.toUpperCase()}-${journal.refId}`;
}
