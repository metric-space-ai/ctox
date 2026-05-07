import { buildPeriodCloseChecklist, closeFiscalPeriod, createAccountingAuditEvent, createBusinessOutboxEvent, moneyFromMajor } from "@ctox-business/accounting";
import { closeAccountingFiscalPeriod, saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { buildFiscalPeriodState } from "@/lib/accounting-runtime";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { getBusinessBundle } from "@/lib/business-seed";

export async function POST(request: Request) {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const body = await parseJsonBody(request);
  const requestedPeriodId = typeof body?.periodId === "string" ? body.periodId : null;
  const periodToClose = requestedPeriodId
    ? data.fiscalPeriods.find((item) => item.id === requestedPeriodId)
    : buildFiscalPeriodState(data).nextClosablePeriod;

  if (!periodToClose) {
    return NextResponse.json({
      error: "no_closable_period",
      fiscalPeriods: buildFiscalPeriodState(data),
      persisted: false,
      reason: "No open fiscal period ending before today was found."
    }, { status: 409 });
  }

  const checklist = buildPeriodCloseChecklist({
    datevExported: data.bookkeeping.some((item) => item.status === "Exported" && item.period.includes(periodToClose.startDate.slice(0, 7))),
    entries: data.journalEntries
      .filter((entry) => entry.status === "Posted")
      .map((entry) => ({
        lines: entry.lines.map((line) => ({
          credit: moneyFromMajor(line.credit, "EUR"),
          debit: moneyFromMajor(line.debit, "EUR")
        })),
        postingDate: entry.postingDate,
        refId: entry.refId,
        type: entry.type
      })),
    openDraftCount: data.invoices.filter((invoice) => invoice.status === "Draft" && invoice.issueDate >= periodToClose.startDate && invoice.issueDate <= periodToClose.endDate).length,
    period: periodToClose,
    unmatchedBankLineCount: data.bankTransactions.filter((line) => line.status === "Unmatched" && line.bookingDate >= periodToClose.startDate && line.bookingDate <= periodToClose.endDate).length,
    unpostedReceiptCount: data.receipts.filter((receipt) => !["Paid", "Posted", "Rejected"].includes(receipt.status) && receipt.receiptDate >= periodToClose.startDate && receipt.receiptDate <= periodToClose.endDate).length,
    vatStatementReviewed: body?.vatStatementReviewed === true || data.reports.some((report) => report.id.includes("vat") && report.status === "Current")
  });

  if (body?.strict === true && !checklist.ready) {
    return NextResponse.json({
      checklist,
      error: "period_close_blocked",
      fiscalPeriods: buildFiscalPeriodState(data),
      persisted: false
    }, { status: 409 });
  }

  const period = {
    ...periodToClose,
    ...closeFiscalPeriod(periodToClose)
  };
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
      checklist,
      fiscalPeriods: buildFiscalPeriodState({
        ...data,
        fiscalPeriods: data.fiscalPeriods.map((item) => item.id === period.id ? period : item)
      }),
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
  return NextResponse.json({ checklist, period: closedPeriod, persisted: true, workflow });
}

async function parseJsonBody(request: Request) {
  try {
    return await request.json() as { periodId?: unknown; strict?: unknown; vatStatementReviewed?: unknown };
  } catch {
    return null;
  }
}
