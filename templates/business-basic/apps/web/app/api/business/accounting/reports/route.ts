import { NextResponse } from "next/server";
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
    profitAndLoss: buildProfitAndLoss(data),
    snapshot: buildAccountingSnapshot(data),
    trialBalance: buildTrialBalance(data),
    vatStatement: buildVatStatement(data)
  });
}
