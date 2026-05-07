import { NextResponse } from "next/server";
import { buildOpenItems as buildOpenItemsFromDrafts, moneyFromMajor } from "@ctox-business/accounting";
import { buildAccountingSnapshot, buildBalanceSheet, buildBusinessAnalysis, buildDatevLines, buildFiscalPeriodState, buildFixedAssetRegister, buildLedgerRows, buildProfitAndLoss, buildTrialBalance, buildVatStatement } from "@/lib/accounting-runtime";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";

export async function GET() {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  return NextResponse.json({
    balanceSheet: buildBalanceSheet(data),
    businessAnalysis: buildBusinessAnalysis(data),
    datevExports: data.bookkeeping,
    datevPreview: buildDatevLines(data).slice(0, 50),
    dunningRuns: data.invoices
      .filter((invoice) => (invoice.reminderLevel ?? 0) > 0)
      .map((invoice) => ({
        collectionStatus: invoice.collectionStatus,
        dueDate: invoice.dueDate,
        invoiceId: invoice.id,
        invoiceNumber: invoice.number,
        level: invoice.reminderLevel,
        reminderDueDate: invoice.reminderDueDate
      })),
    fiscalPeriods: buildFiscalPeriodState(data),
    fixedAssets: buildFixedAssetRegister(data),
    ledger: buildLedgerRows(data),
    openItems: buildOpenItemsFromDrafts({
      accounts: data.accounts.map((account) => ({
        ...account,
        accountType: account.accountType === "equity" ? "temporary" : account.accountType
      })),
      asOf: new Date().toISOString().slice(0, 10),
      dueDatesByRef: Object.fromEntries([
        ...data.invoices.map((invoice) => [invoice.id, invoice.dueDate] as const),
        ...data.receipts.map((receipt) => [receipt.id, receipt.dueDate ?? receipt.receiptDate] as const)
      ]),
      entries: data.journalEntries
        .filter((entry) => entry.status === "Posted")
        .map((entry) => ({
          companyId: "business-basic-company",
          currency: "EUR",
          lines: entry.lines.map((line) => ({
            accountId: line.accountId,
            credit: moneyFromMajor(line.credit, "EUR"),
            debit: moneyFromMajor(line.debit, "EUR"),
            partyId: line.partyId,
            taxCode: line.taxCode
          })),
          narration: typeof entry.narration === "string" ? entry.narration : entry.narration.en,
          postingDate: entry.postingDate,
          refId: entry.refId,
          refType: entry.refType,
          type: entry.type
        }))
    }),
    profitAndLoss: buildProfitAndLoss(data),
    snapshot: buildAccountingSnapshot(data),
    trialBalance: buildTrialBalance(data),
    vatStatement: buildVatStatement(data)
  });
}
