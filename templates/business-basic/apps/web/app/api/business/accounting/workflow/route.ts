import { listAccountingAuditEvents, listAccountingProposals, listBusinessOutboxEvents } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import {
  prepareBankMatchForAccounting,
  prepareDatevExportForAccounting,
  prepareExistingInvoiceForAccounting,
  prepareReceiptForAccounting
} from "@/lib/business-accounting";

export async function GET() {
  if (!process.env.DATABASE_URL) {
    const demo = await buildDemoWorkflow();
    return NextResponse.json({
      audit: demo.audit,
      outbox: demo.outbox,
      persistence: "disabled",
      proposals: demo.proposals,
      reason: "DATABASE_URL not configured",
      source: "demo"
    });
  }

  try {
    const [proposals, outbox, audit] = await Promise.all([
      listAccountingProposals(),
      listBusinessOutboxEvents(),
      listAccountingAuditEvents()
    ]);

    return NextResponse.json({
      audit,
      outbox,
      persistence: "enabled",
      proposals,
      source: "database"
    });
  } catch (error) {
    return NextResponse.json({
      audit: [],
      error: error instanceof Error ? error.message : String(error),
      outbox: [],
      persistence: "error",
      proposals: []
    }, { status: 500 });
  }
}

async function buildDemoWorkflow() {
  const data = await getBusinessBundle();
  const invoice = data.invoices[0];
  const receipt = data.receipts.find((item) => item.status === "Needs review" || item.status === "Inbox") ?? data.receipts[0];
  const bankTransaction = data.bankTransactions.find((item) => item.status === "Suggested") ?? data.bankTransactions[0];
  const exportBatch = data.bookkeeping[0];
  const entries = [
    invoice ? prepareExistingInvoiceForAccounting({ data, invoice }) : undefined,
    receipt ? prepareReceiptForAccounting({ receipt }) : undefined,
    bankTransaction ? prepareBankMatchForAccounting({ transaction: bankTransaction }) : undefined,
    exportBatch ? prepareDatevExportForAccounting({ data, exportBatch }) : undefined
  ].filter((entry): entry is NonNullable<typeof entry> => Boolean(entry));

  return {
    audit: entries.map((entry) => entry.audit),
    outbox: entries.map((entry) => entry.outbox),
    proposals: entries.map((entry) => entry.proposal)
  };
}
