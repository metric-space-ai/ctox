import { NextResponse } from "next/server";
import { buildAccountingSnapshot, buildDatevLines, buildLedgerRows, buildTrialBalance } from "@/lib/accounting-runtime";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";

export async function GET() {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  return NextResponse.json({
    datevPreview: buildDatevLines(data).slice(0, 50),
    ledger: buildLedgerRows(data),
    snapshot: buildAccountingSnapshot(data),
    trialBalance: buildTrialBalance(data)
  });
}
