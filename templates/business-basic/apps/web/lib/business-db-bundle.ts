import { loadAccountingBusinessRows } from "@ctox-business/db/accounting";
import type {
  BusinessAccount,
  BusinessBankTransaction,
  BusinessBundle,
  BusinessCustomer,
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
    const hasAccountingRows = rows.accounts.length || rows.invoices.length || rows.receipts.length || rows.journalEntries.length;
    if (!hasAccountingRows) return seed;

    return {
      ...seed,
      accounts: rows.accounts.length ? rows.accounts.map((account) => ({
        accountType: accountType(account.accountType),
        code: account.code,
        currency: currency(account.currency),
        id: account.externalId,
        isPosting: account.isGroup !== 1,
        name: account.name,
        rootType: rootType(account.rootType)
      })) : seed.accounts,
      bankTransactions: rows.bankStatementLines.length || rows.payments.length
        ? mergeBankTransactions(seed.bankTransactions, rows.bankStatementLines, rows.payments, rows.parties)
        : seed.bankTransactions,
      customers: rows.parties.some((party) => party.kind === "customer") ? rows.parties
        .filter((party) => party.kind === "customer")
        .map((party) => customerFromParty(party, seed)) : seed.customers,
      invoices: rows.invoices.length ? rows.invoices.map((invoice) => {
        const lines = rows.invoiceLines.filter((line) => line.invoiceExternalId === invoice.externalId);
        return {
          balanceDue: invoice.balanceDueMinor / 100,
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
          serviceDate: invoice.serviceDate ?? undefined,
          status: invoiceStatus(invoice.status),
          taxAmount: invoice.taxAmountMinor / 100,
          total: invoice.totalAmountMinor / 100,
          id: invoice.externalId
        };
      }) : seed.invoices,
      journalEntries: rows.journalEntries.length ? rows.journalEntries.map((entry) => ({
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
      })) : seed.journalEntries,
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
  parties: Awaited<ReturnType<typeof loadAccountingBusinessRows>>["parties"]
): BusinessBankTransaction[] {
  const byId = new Map<string, BusinessBankTransaction>();
  for (const line of bankStatementLines) {
    byId.set(line.externalId, {
      amount: line.amountMinor / 100,
      bookingDate: line.bookingDate,
      confidence: line.matchStatus === "matched" ? 1 : line.matchStatus === "suggested" ? 0.9 : 0.45,
      counterparty: line.remitterName ?? "-",
      currency: currency(line.currency),
      id: line.externalId,
      matchedRecordId: line.matchedJournalEntryExternalId ?? undefined,
      matchType: line.matchedJournalEntryExternalId ? "invoice" : undefined,
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
  if (value === "bank" || value === "receivable" || value === "payable" || value === "tax" || value === "income" || value === "expense" || value === "equity") return value;
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
  if (value === "invoice" || value === "payment" || value === "receipt" || value === "bank_transaction" || value === "manual") return value;
  return "manual";
}

function journalType(value: string): BusinessJournalEntry["type"] {
  if (value === "invoice" || value === "payment" || value === "receipt" || value === "manual" || value === "fx" || value === "reverse") return value;
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
