import { asc, desc, eq, sql } from "drizzle-orm";
import { createBusinessDb } from "./client";
import {
  accountingAccounts,
  accountingBankStatementLines,
  accountingBankStatements,
  accountingDatevExports,
  accountingDunningRuns,
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
  allocations?: Array<{
    amountMinor: number;
    invoiceExternalId?: string | null;
    receiptExternalId?: string | null;
  }>;
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

export type AccountingBankStatementProjection = {
  accountExternalId: string;
  closingBalanceMinor?: number;
  companyId: string;
  currency: string;
  endDate?: string | null;
  externalId: string;
  format: string;
  importedBy?: string | null;
  lines: Array<{
    amountMinor: number;
    bookingDate: string;
    currency: string;
    duplicateOfLineExternalId?: string | null;
    endToEndRef?: string | null;
    externalId: string;
    lineNo: number;
    matchStatus?: string;
    matchedJournalEntryExternalId?: string | null;
    purpose?: string | null;
    remitterIban?: string | null;
    remitterName?: string | null;
    valueDate?: string | null;
  }>;
  openingBalanceMinor?: number;
  sourceFilename: string;
  sourceSha256: string;
  startDate?: string | null;
};

export type AccountingDatevExportProjection = {
  companyId: string;
  csvBlobRef?: string | null;
  csvSha256?: string | null;
  externalId: string;
  exportedAt?: Date | null;
  exportedBy?: string | null;
  lineCount: number;
  netAmountMinor: number;
  payload?: unknown;
  period: string;
  sourceProposalExternalId?: string | null;
  status: string;
  system: string;
  taxAmountMinor: number;
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
  bankStatement?: AccountingBankStatementProjection;
  datevExport?: AccountingDatevExportProjection;
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
        set: {
          ...values,
          status: sql`case when ${businessAccountingProposals.status} in ('accepted', 'rejected', 'superseded') then ${businessAccountingProposals.status} else excluded.status end`
        }
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
        set: {
          ...values,
          attempts: sql`case when ${businessOutboxEvents.status} = 'delivered' then ${businessOutboxEvents.attempts} else excluded.attempts end`,
          payloadJson: sql`case when ${businessOutboxEvents.status} = 'delivered' then ${businessOutboxEvents.payloadJson} else excluded.payload_json end`,
          status: sql`case when ${businessOutboxEvents.status} = 'delivered' then ${businessOutboxEvents.status} else excluded.status end`
        }
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

    if (snapshot.bankStatement) {
      await upsertAccountingBankStatement(tx, snapshot.bankStatement);
    }

    if (snapshot.datevExport) {
      await upsertAccountingDatevExport(tx, snapshot.datevExport);
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
  return createBusinessDb(databaseUrl)
    .select()
    .from(businessAccountingProposals)
    .orderBy(desc(businessAccountingProposals.updatedAt))
    .limit(20);
}

export async function listBusinessOutboxEvents(databaseUrl?: string) {
  return createBusinessDb(databaseUrl)
    .select()
    .from(businessOutboxEvents)
    .orderBy(desc(businessOutboxEvents.updatedAt))
    .limit(20);
}

export async function listAccountingAuditEvents(databaseUrl?: string) {
  return createBusinessDb(databaseUrl)
    .select()
    .from(businessAccountingAuditEvents)
    .orderBy(desc(businessAccountingAuditEvents.createdAt))
    .limit(20);
}

export async function loadAccountingBusinessRows(databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  const [
    accounts,
    bankStatementLines,
    datevExports,
    dunningRuns,
    fiscalPeriods,
    invoiceLines,
    invoices,
    journalEntries,
    journalEntryLines,
    parties,
    payments,
    receiptFiles,
    receiptLines,
    receipts
  ] = await Promise.all([
    db.select().from(accountingAccounts).orderBy(asc(accountingAccounts.code)),
    db.select().from(accountingBankStatementLines).orderBy(desc(accountingBankStatementLines.bookingDate), asc(accountingBankStatementLines.lineNo)),
    db.select().from(accountingDatevExports).orderBy(desc(accountingDatevExports.updatedAt)),
    db.select().from(accountingDunningRuns).orderBy(desc(accountingDunningRuns.deliveredAt), desc(accountingDunningRuns.createdAt)),
    db.select().from(accountingFiscalPeriods).orderBy(asc(accountingFiscalPeriods.startDate), asc(accountingFiscalPeriods.endDate)),
    db.select().from(accountingInvoiceLines).orderBy(asc(accountingInvoiceLines.invoiceExternalId), asc(accountingInvoiceLines.lineNo)),
    db.select().from(accountingInvoices).orderBy(desc(accountingInvoices.issueDate)),
    db.select().from(accountingJournalEntries).orderBy(desc(accountingJournalEntries.postingDate), desc(accountingJournalEntries.createdAt)),
    db.select().from(accountingJournalEntryLines).orderBy(asc(accountingJournalEntryLines.journalEntryExternalId), asc(accountingJournalEntryLines.lineNo)),
    db.select().from(accountingParties).orderBy(asc(accountingParties.name)),
    db.select().from(accountingPayments).orderBy(desc(accountingPayments.paymentDate)),
    db.select().from(accountingReceiptFiles).orderBy(asc(accountingReceiptFiles.uploadedAt)),
    db.select().from(accountingReceiptLines).orderBy(asc(accountingReceiptLines.receiptExternalId), asc(accountingReceiptLines.lineNo)),
    db.select().from(accountingReceipts).orderBy(desc(accountingReceipts.receiptDate))
  ]);

  return {
    accounts,
    bankStatementLines,
    datevExports,
    dunningRuns,
    fiscalPeriods,
    invoiceLines,
    invoices,
    journalEntries,
    journalEntryLines,
    parties,
    payments,
    receiptFiles,
    receiptLines,
    receipts
  };
}

export async function decideAccountingProposal(input: {
  actorId: string;
  externalId: string;
  resultingJournalEntryId?: string | null;
  status: "accepted" | "rejected" | "superseded";
}, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  const now = new Date();

  return await db.transaction(async (tx) => {
    const [proposal] = await tx.select()
      .from(businessAccountingProposals)
      .where(eq(businessAccountingProposals.externalId, input.externalId))
      .limit(1);

    if (!proposal) {
      throw new Error(`accounting proposal not found: ${input.externalId}`);
    }
    const command = parseJsonRecord(proposal.proposedCommandJson);
    const proposedJournalEntryId = input.resultingJournalEntryId
      ?? proposal.resultingJournalEntryId
      ?? resultingJournalEntryIdForCommand(command);
    const resultingJournalEntryId = requiresJournalEntry(command) ? proposedJournalEntryId : null;
    if (input.status === "accepted" && resultingJournalEntryId) {
      await assertJournalEntryAcceptable(tx, proposal.companyId, resultingJournalEntryId, command);
    }
    const appliedSideEffects = input.status === "accepted"
      ? await applyAcceptedProposal(tx, proposal, command, resultingJournalEntryId, now, input.actorId)
      : [];

    const [updated] = await tx.update(businessAccountingProposals)
      .set({
        decidedAt: now,
        decidedBy: input.actorId,
        resultingJournalEntryId,
        status: input.status,
        updatedAt: now
      })
      .where(eq(businessAccountingProposals.externalId, input.externalId))
      .returning();

    await tx.insert(businessAccountingAuditEvents).values({
      action: `proposal.${input.status}`,
      actorId: input.actorId,
      actorType: "user",
      afterJson: JSON.stringify({
        appliedSideEffects,
        resultingJournalEntryId,
        status: input.status
      }),
      beforeJson: JSON.stringify({
        status: proposal.status
      }),
      companyId: proposal.companyId,
      refId: proposal.refId,
      refType: proposal.refType
    });

    return updated;
  });
}

async function assertJournalEntryAcceptable(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  companyId: string,
  journalEntryExternalId: string,
  command: Record<string, unknown> | null
) {
  const [entry] = await tx.select({ postingDate: accountingJournalEntries.postingDate })
    .from(accountingJournalEntries)
    .where(eq(accountingJournalEntries.externalId, journalEntryExternalId))
    .limit(1);
  if (!entry) {
    if (requiresJournalEntry(command)) throw new Error("journal_entry_missing_for_proposal");
    return;
  }

  const periods = await tx.select({
    endDate: accountingFiscalPeriods.endDate,
    startDate: accountingFiscalPeriods.startDate,
    status: accountingFiscalPeriods.status
  })
    .from(accountingFiscalPeriods)
    .where(eq(accountingFiscalPeriods.companyId, companyId));
  const matchingPeriods = periods.filter((period) => period.startDate <= entry.postingDate && period.endDate >= entry.postingDate);
  if (matchingPeriods.some((period) => period.status === "closed")) {
    throw new Error("fiscal_period_closed");
  }
}

function requiresJournalEntry(command: Record<string, unknown> | null) {
  return command?.type === "AcceptBankMatch"
    || command?.type === "CapitalizeReceipt"
    || command?.type === "DisposeAsset"
    || command?.type === "PostDepreciation"
    || command?.type === "PostReceipt"
    || command?.type === "SendInvoice";
}

async function applyAcceptedProposal(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  proposal: typeof businessAccountingProposals.$inferSelect,
  command: Record<string, unknown> | null,
  resultingJournalEntryId: string | null,
  now: Date,
  actorId: string
) {
  const applied: string[] = [];
  const type = command?.type;

  if (resultingJournalEntryId && type === "SendInvoice") {
    await tx.update(accountingInvoices)
      .set({
        postedJournalEntryExternalId: resultingJournalEntryId,
        sentAt: now,
        status: "sent",
        updatedAt: now
      })
      .where(eq(accountingInvoices.externalId, proposal.refId));
    applied.push("invoice.sent");
  }

  if (resultingJournalEntryId && type === "PostReceipt") {
    await tx.update(accountingReceipts)
      .set({
        postedAt: now,
        postedJournalEntryExternalId: resultingJournalEntryId,
        status: "posted",
        updatedAt: now
      })
      .where(eq(accountingReceipts.externalId, proposal.refId));
    applied.push("receipt.posted");
  }

  if (resultingJournalEntryId && type === "CapitalizeReceipt") {
    await tx.update(accountingReceipts)
      .set({
        postedAt: now,
        postedJournalEntryExternalId: resultingJournalEntryId,
        status: "posted",
        updatedAt: now
      })
      .where(eq(accountingReceipts.externalId, proposal.refId));
    applied.push("receipt.capitalized");
  }

  if (resultingJournalEntryId && type === "PostDepreciation") {
    applied.push("asset.depreciation_posted");
  }

  if (resultingJournalEntryId && type === "DisposeAsset") {
    applied.push("asset.disposed");
  }

  if (type === "IngestReceipt") {
    const payload = parseCommandPayload(command);
    const receiptId = typeof payload?.receiptId === "string" ? payload.receiptId : proposal.refId;
    const updated = await tx.update(accountingReceipts)
      .set({
        status: "extracted",
        updatedAt: now
      })
      .where(sql`${accountingReceipts.externalId} = ${receiptId} and ${accountingReceipts.status} not in ('paid', 'posted', 'rejected')`)
      .returning({ externalId: accountingReceipts.externalId });
    if (updated.length) applied.push("receipt.extracted");
  }

  if (type === "RunDunning") {
    await upsertAcceptedDunningRun(tx, proposal, command, now, actorId);
    applied.push("dunning.accepted");
  }

  if (type === "ExportDatev") {
    await upsertAcceptedDatevExport(tx, proposal, command, now, actorId);
    applied.push("datev_export.accepted");
  }

  if (resultingJournalEntryId && type === "AcceptBankMatch") {
    const paymentEffect = await applyAcceptedBankPayment(tx, command, resultingJournalEntryId, now);
    await tx.update(accountingPayments)
      .set({
        postedJournalEntryExternalId: resultingJournalEntryId,
        updatedAt: now
      })
      .where(sql`${accountingPayments.bankStatementLineExternalId} = ${proposal.refId} or ${accountingPayments.externalId} = ${`pay-${proposal.refId}`}`);
    await tx.update(accountingBankStatementLines)
      .set({
        matchedJournalEntryExternalId: resultingJournalEntryId,
        matchStatus: "matched"
      })
      .where(sql`${accountingBankStatementLines.externalId} = ${proposal.refId} or ${accountingBankStatementLines.externalId} = ${proposal.refId.replace(/^bank-line-/, "bank-statement-line-")}`);
    applied.push("bank_match.accepted");
    applied.push(...paymentEffect);
  }

  await tx.update(businessOutboxEvents)
    .set({
      deliveredAt: now,
      status: "delivered",
      updatedAt: now
    })
    .where(sql`${businessOutboxEvents.payloadJson}::jsonb ->> 'proposalId' = ${proposal.externalId}`);
  applied.push("outbox.delivered");

  return applied;
}

async function applyAcceptedBankPayment(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  command: Record<string, unknown> | null,
  resultingJournalEntryId: string,
  now: Date
) {
  const effects: string[] = [];
  const payload = parseCommandPayload(command);
  const amountMinor = Math.round(Math.abs(Number(payload?.amount ?? 0)) * 100);
  const matchedRecordId = typeof payload?.matchedRecordId === "string" ? payload.matchedRecordId : null;
  const matchType = typeof payload?.matchType === "string" ? payload.matchType : null;
  if (!matchedRecordId || amountMinor <= 0) return effects;

  if (matchType === "invoice") {
    const [invoice] = await tx.select({
      balanceDueMinor: accountingInvoices.balanceDueMinor,
      totalAmountMinor: accountingInvoices.totalAmountMinor
    })
      .from(accountingInvoices)
      .where(eq(accountingInvoices.externalId, matchedRecordId))
      .limit(1);
    if (invoice) {
      const nextBalance = Math.max(0, invoice.balanceDueMinor - amountMinor);
      await tx.update(accountingInvoices)
        .set({
          balanceDueMinor: nextBalance,
          status: nextBalance === 0 ? "paid" : "partially_paid",
          updatedAt: now
        })
        .where(eq(accountingInvoices.externalId, matchedRecordId));
      effects.push(nextBalance === 0 ? "invoice.paid" : "invoice.partially_paid");
    }
  }

  if (matchType === "receipt") {
    const [receipt] = await tx.select({
      totalAmountMinor: accountingReceipts.totalAmountMinor
    })
      .from(accountingReceipts)
      .where(eq(accountingReceipts.externalId, matchedRecordId))
      .limit(1);
    if (receipt) {
      await tx.update(accountingReceipts)
        .set({
          postedJournalEntryExternalId: resultingJournalEntryId,
          status: "paid",
          updatedAt: now
        })
        .where(eq(accountingReceipts.externalId, matchedRecordId));
      effects.push("receipt.paid");
    }
  }

  return effects;
}

async function upsertAcceptedDatevExport(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  proposal: typeof businessAccountingProposals.$inferSelect,
  command: Record<string, unknown> | null,
  now: Date,
  actorId: string
) {
  const payload = parseCommandPayload(command);
  const evidence = parseJsonRecord(proposal.evidenceJson) ?? {};
  const exportId = typeof payload?.exportId === "string" ? payload.exportId : proposal.refId;
  const period = typeof payload?.period === "string" ? payload.period : "unknown";
  const system = typeof payload?.system === "string" ? payload.system : "DATEV";
  const lineCount = Number(evidence.lineCount ?? 0);
  const [existing] = await tx.select({
    csvBlobRef: accountingDatevExports.csvBlobRef,
    csvSha256: accountingDatevExports.csvSha256,
    netAmountMinor: accountingDatevExports.netAmountMinor,
    taxAmountMinor: accountingDatevExports.taxAmountMinor
  })
    .from(accountingDatevExports)
    .where(eq(accountingDatevExports.externalId, exportId))
    .limit(1);

  await upsertAccountingDatevExport(tx, {
    companyId: proposal.companyId,
    csvBlobRef: existing?.csvBlobRef ?? null,
    csvSha256: existing?.csvSha256 ?? null,
    exportedAt: now,
    exportedBy: actorId,
    externalId: exportId,
    lineCount,
    netAmountMinor: existing?.netAmountMinor ?? 0,
    payload: { command, evidence },
    period,
    sourceProposalExternalId: proposal.externalId,
    status: "exported",
    system,
    taxAmountMinor: existing?.taxAmountMinor ?? 0
  });
}

async function upsertAcceptedDunningRun(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  proposal: typeof businessAccountingProposals.$inferSelect,
  command: Record<string, unknown> | null,
  now: Date,
  actorId: string
) {
  const payload = parseCommandPayload(command);
  const evidence = parseJsonRecord(proposal.evidenceJson) ?? {};
  const invoiceId = typeof payload?.invoiceId === "string" ? payload.invoiceId : proposal.refId;
  const invoiceNumber = typeof payload?.invoiceNumber === "string" ? payload.invoiceNumber : invoiceId;
  const level = normalizePositiveInt(payload?.level);
  const daysOverdue = normalizePositiveInt(evidence.daysOverdue);
  const feeAmountMinor = Math.round(Number(payload?.feeAmount ?? 0) * 100);
  const externalId = `dunning-${invoiceId}-level-${level}`;

  await tx.insert(accountingDunningRuns).values({
    companyId: proposal.companyId,
    createdBy: actorId,
    daysOverdue,
    deliveredAt: now,
    externalId,
    feeAmountMinor,
    invoiceExternalId: invoiceId,
    invoiceNumber,
    level,
    payloadJson: JSON.stringify({ command, evidence }),
    sourceProposalExternalId: proposal.externalId,
    status: "delivered",
    updatedAt: now
  }).onConflictDoUpdate({
    target: accountingDunningRuns.externalId,
    set: {
      daysOverdue,
      deliveredAt: sql`coalesce(${accountingDunningRuns.deliveredAt}, excluded.delivered_at)`,
      feeAmountMinor,
      payloadJson: JSON.stringify({ command, evidence }),
      sourceProposalExternalId: proposal.externalId,
      status: "delivered",
      updatedAt: now
    }
  });
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
  const [period] = await db.update(accountingFiscalPeriods)
    .set({
      closedAt: input.closedAt ?? new Date(),
      status: input.status ?? "closed",
      updatedAt: new Date()
    })
    .where(eq(accountingFiscalPeriods.externalId, input.externalId))
    .returning();
  return period ?? null;
}

async function upsertAccountingDatevExport(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  exportBatch: AccountingDatevExportProjection
) {
  const values = {
    companyId: exportBatch.companyId,
    csvBlobRef: exportBatch.csvBlobRef ?? null,
    csvSha256: exportBatch.csvSha256 ?? null,
    exportedAt: exportBatch.exportedAt ?? null,
    exportedBy: exportBatch.exportedBy ?? null,
    externalId: exportBatch.externalId,
    lineCount: exportBatch.lineCount,
    netAmountMinor: exportBatch.netAmountMinor,
    payloadJson: JSON.stringify(exportBatch.payload ?? {}),
    period: exportBatch.period,
    sourceProposalExternalId: exportBatch.sourceProposalExternalId ?? null,
    status: exportBatch.status,
    system: exportBatch.system,
    taxAmountMinor: exportBatch.taxAmountMinor,
    updatedAt: new Date()
  };

  await tx.insert(accountingDatevExports).values(values).onConflictDoUpdate({
    target: accountingDatevExports.externalId,
    set: {
      ...values,
      csvBlobRef: sql`coalesce(excluded.csv_blob_ref, ${accountingDatevExports.csvBlobRef})`,
      csvSha256: sql`coalesce(excluded.csv_sha256, ${accountingDatevExports.csvSha256})`,
      exportedAt: sql`coalesce(${accountingDatevExports.exportedAt}, excluded.exported_at)`,
      exportedBy: sql`coalesce(${accountingDatevExports.exportedBy}, excluded.exported_by)`,
      status: sql`case when ${accountingDatevExports.status} = 'exported' then ${accountingDatevExports.status} else excluded.status end`
    }
  });
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
    set: {
      ...values,
      balanceDueMinor: sql`case when ${accountingInvoices.status} in ('paid', 'partially_paid') then ${accountingInvoices.balanceDueMinor} else excluded.balance_due_minor end`,
      postedJournalEntryExternalId: sql`coalesce(${accountingInvoices.postedJournalEntryExternalId}, excluded.posted_journal_entry_external_id)`,
      sentAt: sql`coalesce(${accountingInvoices.sentAt}, excluded.sent_at)`,
      status: sql`case when ${accountingInvoices.status} in ('paid', 'partially_paid') then ${accountingInvoices.status} else excluded.status end`
    }
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
    set: {
      ...values,
      postedAt: sql`coalesce(${accountingReceipts.postedAt}, excluded.posted_at)`,
      postedJournalEntryExternalId: sql`coalesce(${accountingReceipts.postedJournalEntryExternalId}, excluded.posted_journal_entry_external_id)`,
      status: sql`case when ${accountingReceipts.status} in ('paid', 'posted') then ${accountingReceipts.status} else excluded.status end`
    }
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
    set: {
      ...values,
      postedJournalEntryExternalId: sql`coalesce(${accountingPayments.postedJournalEntryExternalId}, excluded.posted_journal_entry_external_id)`
    }
  });

  await tx.delete(accountingPaymentAllocations).where(eq(accountingPaymentAllocations.paymentExternalId, payment.externalId));
  const allocations = payment.allocations ?? (payment.allocation ? [payment.allocation] : []);
  if (allocations.length) {
    await tx.insert(accountingPaymentAllocations).values(allocations.map((allocation) => ({
      amountMinor: allocation.amountMinor,
      invoiceExternalId: allocation.invoiceExternalId ?? null,
      paymentExternalId: payment.externalId,
      receiptExternalId: allocation.receiptExternalId ?? null
    })));
  }
}

async function upsertAccountingBankStatement(
  tx: Parameters<Parameters<ReturnType<typeof createBusinessDb>["transaction"]>[0]>[0],
  statement: AccountingBankStatementProjection
) {
  const values = {
    accountExternalId: statement.accountExternalId,
    closingBalanceMinor: statement.closingBalanceMinor ?? 0,
    companyId: statement.companyId,
    endDate: statement.endDate ?? null,
    externalId: statement.externalId,
    format: statement.format,
    importedBy: statement.importedBy ?? null,
    openingBalanceMinor: statement.openingBalanceMinor ?? 0,
    sourceFilename: statement.sourceFilename,
    sourceSha256: statement.sourceSha256,
    startDate: statement.startDate ?? null
  };

  await tx.insert(accountingBankStatements).values(values).onConflictDoUpdate({
    target: accountingBankStatements.externalId,
    set: values
  });

  for (const line of statement.lines) {
    const values = {
      amountMinor: line.amountMinor,
      bookingDate: line.bookingDate,
      currency: line.currency,
      duplicateOfLineExternalId: line.duplicateOfLineExternalId ?? null,
      endToEndRef: line.endToEndRef ?? null,
      externalId: line.externalId,
      lineNo: line.lineNo,
      matchStatus: line.matchStatus ?? "unmatched",
      matchedJournalEntryExternalId: line.matchedJournalEntryExternalId ?? null,
      purpose: line.purpose ?? null,
      remitterIban: line.remitterIban ?? null,
      remitterName: line.remitterName ?? null,
      statementExternalId: statement.externalId,
      valueDate: line.valueDate ?? null
    };

    await tx.insert(accountingBankStatementLines).values(values).onConflictDoUpdate({
      target: accountingBankStatementLines.externalId,
      set: {
        ...values,
        matchStatus: sql`case when ${accountingBankStatementLines.matchStatus} = 'matched' then ${accountingBankStatementLines.matchStatus} else excluded.match_status end`,
        matchedJournalEntryExternalId: sql`coalesce(${accountingBankStatementLines.matchedJournalEntryExternalId}, excluded.matched_journal_entry_external_id)`
      }
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

function parseJsonRecord(value: string) {
  try {
    const parsed = JSON.parse(value) as unknown;
    return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

function parseCommandPayload(command: Record<string, unknown> | null) {
  const payload = command?.payload;
  return payload && typeof payload === "object" ? payload as Record<string, unknown> : null;
}

function normalizePositiveInt(value: unknown) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? Math.round(parsed) : 0;
}

function resultingJournalEntryIdForCommand(command: Record<string, unknown> | null) {
  const type = command?.type;
  const refType = typeof command?.refType === "string" ? command.refType : null;
  const refId = typeof command?.refId === "string" ? command.refId : null;

  if (!refType || !refId) return null;
  if (type === "SendInvoice") return `je-invoice-${refType}-${refId}`;
  if (type === "PostReceipt") return `je-receipt-${refType}-${refId}`;
  if (type === "CapitalizeReceipt") return `je-manual-asset-asset-${refId}`;
  if (type === "DisposeAsset") return `je-manual-asset-${refId}`;
  if (type === "PostDepreciation") return `je-depreciation-asset-${refId}`;
  if (type === "AcceptBankMatch") return `je-payment-${refType}-${refId}`;
  return null;
}
