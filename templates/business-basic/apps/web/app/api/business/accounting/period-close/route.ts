import { closeFiscalPeriod, createAccountingAuditEvent, createBusinessOutboxEvent } from "@ctox-business/accounting";
import { closeAccountingFiscalPeriod, saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";

export async function POST() {
  const period = closeFiscalPeriod({
    endDate: "2026-04-30",
    id: "fy-2026-04",
    startDate: "2026-04-01",
    status: "open"
  });
  const audit = createAccountingAuditEvent({
    action: "period.close.prepare",
    actorId: "business-runtime",
    actorType: "system",
    after: period,
    companyId: "business-basic-company",
    refId: period.id,
    refType: "fiscal_period"
  });
  const outbox = createBusinessOutboxEvent({
    companyId: "business-basic-company",
    id: `outbox-business.period.close-${period.id}`,
    payload: { period },
    topic: "business.period.close"
  });
  const workflow = { audit, outbox };

  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      period,
      persisted: false,
      reason: "DATABASE_URL not configured",
      workflow
    });
  }

  const closedPeriod = await closeAccountingFiscalPeriod({ externalId: period.id });
  if (!closedPeriod) {
    return NextResponse.json({
      error: "fiscal_period_not_found",
      period,
      persisted: false,
      reason: `Run accounting setup before closing ${period.id}.`,
      workflow
    }, { status: 404 });
  }
  await saveAccountingWorkflowSnapshot(workflow);
  return NextResponse.json({ period: closedPeriod, persisted: true, workflow });
}
