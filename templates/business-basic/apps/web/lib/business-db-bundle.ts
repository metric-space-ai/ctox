import { loadAccountingBusinessRows } from "@ctox-business/db/accounting";
import type {
  BusinessAccount,
  BusinessBankTransaction,
  BusinessBookkeepingExport,
  BusinessBundle,
  BusinessCustomer,
  BusinessFiscalPeriod,
  BusinessFixedAsset,
  BusinessInvoice,
  BusinessJournalEntry,
  BusinessProduct,
  BusinessReceipt
} from "./business-seed";
import { normalizeBusinessResource } from "./business-seed";

export async function getDatabaseBackedBusinessBundle(seed: BusinessBundle): Promise<BusinessBundle> {
  if (!process.env.DATABASE_URL) return seed;

  try {
    const rows = await loadAccountingBusinessRows();
    const hasAccountingRows = rows.accounts.length || rows.datevExports.length || rows.dunningRuns.length || rows.invoices.length || rows.receipts.length || rows.journalEntries.length;
    if (!hasAccountingRows) return seed;

    const dbAccounts: BusinessAccount[] = rows.accounts.map((account) => ({
      accountType: accountType(account.accountType),
      code: account.code,
      currency: currency(account.currency),
      id: account.externalId,
      isPosting: account.isGroup !== 1,
      name: account.name,
      rootType: rootType(account.rootType)
    }));
    const dbJournalEntries: BusinessJournalEntry[] = rows.journalEntries.map((entry) => ({
      id: entry.externalId,
      lines: rows.journalEntryLines
        .filter((line) => line.journalEntryExternalId === entry.externalId)
        .map((line) => ({
          accountId: line.accountExternalId,
          costCenter: line.costCenterExternalId ?? undefined,
          credit: line.creditMinor / 100,
          debit: line.debitMinor / 100,
          partyId: line.partyExternalId ?? undefined,
          projectId: line.projectExternalId ?? undefined
        })),
      narration: entry.narration ?? dbNote("Persisted journal entry", "Persistierter Buchungssatz"),
      number: entry.number,
      postedAt: entry.postedAt?.toISOString(),
      postingDate: entry.postingDate,
      refId: entry.refId,
      refType: journalRefType(entry.refType),
      status: entry.postedAt ? "Posted" : "Draft",
      type: journalType(entry.type)
    }));

    return {
      ...seed,
      accounts: rows.accounts.length ? mergeById(seed.accounts, dbAccounts) : seed.accounts,
      bankTransactions: rows.bankStatementLines.length || rows.payments.length
        ? mergeBankTransactions(seed.bankTransactions, rows.bankStatementLines, rows.payments, rows.parties, seed.invoices, rows.invoices)
        : seed.bankTransactions,
      bookkeeping: rows.datevExports.length ? mergeBookkeepingExports(seed.bookkeeping, rows.datevExports) : seed.bookkeeping,
      customers: rows.parties.some((party) => party.kind === "customer") ? rows.parties
        .filter((party) => party.kind === "customer")
        .map((party) => customerFromParty(party, seed)) : seed.customers,
      fiscalPeriods: rows.fiscalPeriods.length ? rows.fiscalPeriods.map(fiscalPeriodFromRow) : seed.fiscalPeriods,
      fixedAssets: mergeById(seed.fixedAssets, fixedAssetsFromJournals(seed.fixedAssets, dbJournalEntries, rows.receipts)),
      invoices: rows.invoices.length ? rows.invoices.map((invoice) => {
        const lines = rows.invoiceLines.filter((line) => line.invoiceExternalId === invoice.externalId);
        const dunning = latestDunningRun(rows.dunningRuns, invoice.externalId);
        return {
          balanceDue: invoice.balanceDueMinor / 100,
          collectionStatus: dunning ? collectionStatusFromDunningLevel(dunning.level) : undefined,
          customerId: invoice.customerExternalId,
          currency: currency(invoice.currency),
          dueDate: invoice.dueDate,
          issueDate: invoice.issueDate,
          lines: lines.length ? lines.map((line) => ({
            productId: line.productExternalId ?? "db-product",
            quantity: line.quantity,
            taxRate: line.taxRate,
            unitPrice: line.unitPriceMinor / 100
          })) : [{ productId: "db-product", quantity: 1, taxRate: 0, unitPrice: invoice.netAmountMinor / 100 }],
          netAmount: invoice.netAmountMinor / 100,
          notes: dbNote("Persisted accounting invoice", "Persistierte Accounting-Rechnung"),
          number: invoice.number,
          reminderDueDate: dunning?.deliveredAt?.toISOString().slice(0, 10),
          reminderLevel: dunning?.level as 0 | 1 | 2 | 3 | undefined,
          serviceDate: invoice.serviceDate ?? undefined,
          status: invoiceStatus(invoice.status),
          taxAmount: invoice.taxAmountMinor / 100,
          total: invoice.totalAmountMinor / 100,
          id: invoice.externalId
        };
      }) : seed.invoices,
      journalEntries: rows.journalEntries.length ? mergeById(seed.journalEntries, dbJournalEntries) : seed.journalEntries,
      receipts: rows.receipts.length ? rows.receipts.map((receipt) => {
        const files = rows.receiptFiles.filter((file) => file.receiptExternalId === receipt.externalId);
        const lines = rows.receiptLines.filter((line) => line.receiptExternalId === receipt.externalId);
        const firstLine = lines[0];
        return {
          attachmentName: files[0]?.originalFilename ?? `${receipt.number}.pdf`,
          currency: currency(receipt.currency),
          documentType: "Invoice",
          dueDate: receipt.dueDate ?? receipt.receiptDate,
          expenseAccountId: receipt.expenseAccountExternalId ?? firstLine?.expenseAccountExternalId ?? "acc-expense",
          extractedFields: receipt.extractedJson ? [{ confidence: 0.9, label: "Extracted", value: "available" }] : [],
          id: receipt.externalId,
          journalEntryId: receipt.postedJournalEntryExternalId ?? undefined,
          netAmount: receipt.netAmountMinor / 100,
          notes: dbNote("Persisted inbound receipt", "Persistierter Eingangsbeleg"),
          number: receipt.number,
          payableAccountId: receipt.payableAccountExternalId ?? "acc-ap",
          receiptDate: receipt.receiptDate,
          source: "Upload",
          status: receiptStatus(receipt.status),
          taxAmount: receipt.taxAmountMinor / 100,
          taxCode: taxCode(receipt.taxCode),
          total: receipt.totalAmountMinor / 100,
          vendorName: receipt.vendorExternalId ?? receipt.vendorInvoiceNumber ?? "Vendor"
        };
      }) : seed.receipts,
      products: mergeProducts(seed.products, rows.invoiceLines)
    };
  } catch {
    return seed;
  }
}

function mergeById<T extends { id: string }>(seedRows: T[], dbRows: T[]) {
  const byId = new Map(seedRows.map((row) => [row.id, row]));
  for (const row of dbRows) byId.set(row.id, row);
  return Array.from(byId.values());
}

function mergeBookkeepingExports(
  seedRows: BusinessBookkeepingExport[],
  dbRows: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["datevExports"]
) {
  const byId = new Map(seedRows.map((row) => [row.id, row]));
  for (const row of dbRows) {
    const existing = byId.get(row.externalId);
    byId.set(row.externalId, {
      context: existing?.context ?? dbNote("Persisted DATEV export batch", "Persistierter DATEV-Exportstapel"),
      dueDate: existing?.dueDate ?? row.period,
      generatedAt: row.exportedAt?.toISOString() ?? row.updatedAt.toISOString(),
      id: row.externalId,
      invoiceIds: existing?.invoiceIds ?? [],
      netAmount: row.netAmountMinor / 100,
      period: row.period,
      reviewer: row.exportedBy ?? existing?.reviewer ?? "datev-exporter",
      status: row.status === "exported" ? "Exported" : "Ready",
      system: row.system === "Lexoffice" ? "Lexoffice" : row.system === "CSV" ? "CSV" : "DATEV",
      taxAmount: row.taxAmountMinor / 100
    });
  }
  return Array.from(byId.values());
}

function latestDunningRun(
  runs: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["dunningRuns"],
  invoiceExternalId: string
) {
  return runs
    .filter((run) => run.invoiceExternalId === invoiceExternalId)
    .sort((left, right) => right.level - left.level || (right.deliveredAt?.getTime() ?? 0) - (left.deliveredAt?.getTime() ?? 0))[0];
}

function collectionStatusFromDunningLevel(level: number): BusinessInvoice["collectionStatus"] {
  if (level >= 3) return "Final notice";
  return "Reminder sent";
}

function fiscalPeriodFromRow(period: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["fiscalPeriods"][number]): BusinessFiscalPeriod {
  return {
    closedAt: period.closedAt?.toISOString(),
    companyId: period.companyId,
    endDate: period.endDate,
    id: period.externalId,
    startDate: period.startDate,
    status: period.status === "closed" ? "closed" : "open"
  };
}

function fixedAssetsFromJournals(
  seedAssets: BusinessFixedAsset[],
  journalEntries: BusinessJournalEntry[],
  receipts: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["receipts"]
): BusinessFixedAsset[] {
  const known = new Set(seedAssets.map((asset) => asset.id));
  const assets: BusinessFixedAsset[] = [];
  for (const entry of journalEntries.filter((item) => item.refType === "asset" && !known.has(item.refId))) {
    const assetLine = entry.lines.find((line) => line.accountId === "acc-fixed-assets" && line.debit > 0);
    if (!assetLine) continue;
    const receiptExternalId = entry.refId.startsWith("asset-") ? entry.refId.slice("asset-".length) : undefined;
    const receipt = receiptExternalId ? receipts.find((item) => item.externalId === receiptExternalId) : undefined;
    assets.push({
      accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
      acquisitionCost: assetLine.debit,
      acquisitionDate: entry.postingDate,
      acquisitionJournalEntryId: entry.id,
      assetAccountId: "acc-fixed-assets",
      category: "Aus Eingangsbeleg aktiviert",
      currency: "EUR",
      depreciationExpenseAccountId: "acc-depreciation",
      depreciationMethod: "Straight line",
      id: entry.refId,
      name: receipt?.vendorInvoiceNumber ? `Anlage ${receipt.vendorInvoiceNumber}` : `Anlage ${entry.refId}`,
      notes: dbNote("Capitalized from an inbound receipt workflow.", "Aus dem Eingangsbeleg-Workflow aktiviert."),
      receiptId: receiptExternalId,
      salvageValue: 1,
      status: "Active",
      supplier: receipt?.vendorExternalId ?? "Eingangsbeleg",
      usefulLifeMonths: 60
    });
  }
  return assets;
}

function customerFromParty(
  party: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["parties"][number],
  seed: BusinessBundle
): BusinessCustomer {
  const existing = seed.customers.find((customer) => customer.id === party.externalId);
  return existing ?? {
    arBalance: 0,
    billingEmail: "",
    country: "",
    id: party.externalId,
    lastInvoiceId: "",
    mrr: 0,
    name: party.name,
    notes: dbNote("Persisted accounting customer", "Persistierter Accounting-Kunde"),
    owner: "accounting",
    paymentTerms: "14 days net",
    segment: "Accounting",
    status: "Active",
    taxId: party.taxId ?? "",
  };
}

function mergeProducts(
  seedProducts: BusinessProduct[],
  invoiceLines: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["invoiceLines"]
) {
  const byId = new Map(seedProducts.map((product) => [product.id, product]));
  for (const line of invoiceLines) {
    const id = line.productExternalId ?? "db-product";
    if (byId.has(id)) continue;
    byId.set(id, {
      description: dbNote(line.description, line.description),
      id,
      margin: 0,
      name: line.description,
      price: line.unitPriceMinor / 100,
      revenueAccount: line.revenueAccountExternalId ?? "acc-revenue",
      sku: id,
      status: "Billable",
      taxRate: line.taxRate,
      type: "Service"
    });
  }
  return Array.from(byId.values());
}

function mergeBankTransactions(
  seedTransactions: BusinessBankTransaction[],
  bankStatementLines: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["bankStatementLines"],
  payments: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["payments"],
  parties: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["parties"],
  seedInvoices: BusinessInvoice[],
  dbInvoices: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["invoices"]
): BusinessBankTransaction[] {
  const byId = new Map<string, BusinessBankTransaction>();
  const invoiceRefs = [
    ...seedInvoices.map((invoice) => ({ id: invoice.id, number: invoice.number })),
    ...dbInvoices.map((invoice) => ({ id: invoice.externalId, number: invoice.number }))
  ];
  for (const line of bankStatementLines) {
    const suggestedInvoice = line.matchStatus === "suggested"
      ? invoiceRefs.find((invoice) => line.purpose?.includes(invoice.number))
      : undefined;
    byId.set(line.externalId, {
      amount: line.amountMinor / 100,
      bookingDate: line.bookingDate,
      confidence: line.matchStatus === "matched" ? 1 : line.matchStatus === "suggested" ? 0.9 : 0.45,
      counterparty: line.remitterName ?? "-",
      currency: currency(line.currency),
      id: line.externalId,
      matchedRecordId: line.matchedJournalEntryExternalId ?? suggestedInvoice?.id,
      matchType: line.matchedJournalEntryExternalId || suggestedInvoice ? "invoice" : undefined,
      purpose: line.purpose ?? "",
      status: bankStatus(line.matchStatus),
      valueDate: line.valueDate ?? line.bookingDate
    });
  }

  for (const payment of payments) {
    const id = payment.bankStatementLineExternalId ?? payment.externalId;
    if (byId.has(id)) continue;
    const party = parties.find((item) => item.externalId === payment.partyExternalId);
    byId.set(id, {
      amount: payment.kind === "outgoing" ? -payment.amountMinor / 100 : payment.amountMinor / 100,
      bookingDate: payment.paymentDate,
      confidence: payment.postedJournalEntryExternalId ? 1 : 0.75,
      counterparty: party?.name ?? payment.partyExternalId ?? "Accounting payment",
      currency: currency(payment.currency),
      id,
      matchedRecordId: payment.postedJournalEntryExternalId ?? undefined,
      matchType: payment.kind === "outgoing" ? "receipt" : "invoice",
      purpose: payment.kind === "outgoing" ? "Outgoing payment" : "Incoming payment",
      status: payment.postedJournalEntryExternalId ? "Matched" : "Suggested",
      valueDate: payment.paymentDate
    });
  }

  return byId.size ? Array.from(byId.values()) : seedTransactions;
}

export async function getDatabaseBackedBusinessResource(resource: string, seed: BusinessBundle) {
  const normalized = normalizeBusinessResource(resource);
  if (!normalized) return null;
  return (await getDatabaseBackedBusinessBundle(seed))[normalized];
}

function accountType(value: string): BusinessAccount["accountType"] {
  if (
    value === "accumulated_depreciation"
    || value === "bank"
    || value === "depreciation"
    || value === "fixed_asset"
    || value === "receivable"
    || value === "payable"
    || value === "tax"
    || value === "income"
    || value === "expense"
    || value === "equity"
  ) return value;
  return "expense";
}

function bankStatus(value: string): BusinessBankTransaction["status"] {
  if (value === "matched") return "Matched";
  if (value === "suggested") return "Suggested";
  if (value === "ignored") return "Ignored";
  return "Unmatched";
}

function currency(value: string) {
  return value === "USD" ? "USD" : "EUR";
}

function dbNote(en: string, de: string) {
  return { de, en };
}

function invoiceStatus(value: string): BusinessInvoice["status"] {
  if (value === "draft" || value === "prepared") return "Draft";
  if (value === "paid") return "Paid";
  if (value === "overdue") return "Overdue";
  if (value === "export_ready") return "Export ready";
  return "Sent";
}

function journalRefType(value: string): BusinessJournalEntry["refType"] {
  if (value === "asset" || value === "invoice" || value === "payment" || value === "receipt" || value === "bank_transaction" || value === "manual") return value;
  return "manual";
}

function journalType(value: string): BusinessJournalEntry["type"] {
  if (value === "depreciation" || value === "invoice" || value === "payment" || value === "receipt" || value === "manual" || value === "fx" || value === "reverse") return value;
  return "manual";
}

function receiptStatus(value: string): BusinessReceipt["status"] {
  if (value === "inbox" || value === "scanned") return "Inbox";
  if (value === "reviewed" || value === "extracted") return "Needs review";
  if (value === "paid") return "Paid";
  if (value === "rejected") return "Rejected";
  return "Posted";
}

function rootType(value: string): BusinessAccount["rootType"] {
  if (value === "asset" || value === "liability" || value === "equity" || value === "income" || value === "expense") return value;
  return "expense";
}

function taxCode(value: string | null): BusinessReceipt["taxCode"] {
  if (value === "DE_7_INPUT" || value === "DE_0" || value === "RC") return value;
  return "DE_19_INPUT";
}
