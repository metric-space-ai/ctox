import { closeFiscalPeriod } from "@ctox-business/accounting";
import { closeAccountingFiscalPeriod } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";

export async function POST() {
  const period = closeFiscalPeriod({
    endDate: "2026-04-30",
    id: "fy-2026-04",
    startDate: "2026-04-01",
    status: "open"
  });

  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      period,
      persisted: false,
      reason: "DATABASE_URL not configured"
    });
  }

  await closeAccountingFiscalPeriod({ externalId: period.id });
  return NextResponse.json({ period, persisted: true });
}
