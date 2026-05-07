import {
  buildDunningProposals,
  createAccountingAuditEvent,
  createAccountingProposal,
  createBusinessOutboxEvent
} from "@ctox-business/accounting";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";

const companyId = "business-basic-company";
const asOf = "2026-05-07";

export async function POST() {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const proposals = buildDunningProposals({
    asOf,
    companyId,
    invoices: data.invoices.map((invoice) => ({
      balanceDue: invoice.balanceDue ?? invoice.total,
      customerId: invoice.customerId,
      dueDate: invoice.dueDate,
      id: invoice.id,
      number: invoice.number,
      reminderLevel: invoice.reminderLevel,
      status: invoice.status
    })),
    requestedBy: "dunning-assistant"
  });
  const scanWorkflow = {
    audit: createAccountingAuditEvent({
      action: "dunning.scan",
      actorId: "dunning-assistant",
      actorType: "agent",
      after: {
        asOf,
        proposalCount: proposals.length,
        scannedInvoiceCount: data.invoices.length
      },
      companyId,
      refId: `dunning-scan-${asOf}`,
      refType: "dunning_run"
    }),
    outbox: createBusinessOutboxEvent({
      companyId,
      id: `outbox-business.dunning.scan-${asOf}`,
      payload: {
        asOf,
        proposalCount: proposals.length,
        scannedInvoiceCount: data.invoices.length
      },
      topic: "business.dunning.scan"
    })
  };
  const workflow = [scanWorkflow, ...proposals.map((item) => {
    const proposal = createAccountingProposal({
      companyId,
      confidence: item.daysOverdue >= 14 ? 0.9 : 0.82,
      createdByAgent: "dunning-assistant",
      evidence: {
        daysOverdue: item.daysOverdue,
        feeAmount: item.command.payload.feeAmount,
        invoiceNumber: item.command.payload.invoiceNumber,
        level: item.command.payload.level
      },
      kind: "dunning_run",
      proposedCommand: item.command,
      refId: item.command.payload.invoiceId,
      refType: "invoice"
    });
    const audit = createAccountingAuditEvent({
      action: "dunning.prepare_run",
      actorId: "business-runtime",
      actorType: "system",
      after: { command: item.command, daysOverdue: item.daysOverdue },
      companyId,
      refId: item.command.payload.invoiceId,
      refType: "invoice"
    });
    const outbox = createBusinessOutboxEvent({
      companyId,
      id: `outbox-business.dunning.prepare_run-${item.command.payload.invoiceId}-${item.command.payload.level}`,
      payload: { command: item.command, daysOverdue: item.daysOverdue, proposalId: proposal.id },
      topic: "business.dunning.prepare_run"
    });

    return { audit, outbox, proposal };
  })];

  if (process.env.DATABASE_URL) {
    await Promise.all(workflow.map((snapshot) => saveAccountingWorkflowSnapshot(snapshot)));
  }

  return NextResponse.json({
    persisted: Boolean(process.env.DATABASE_URL),
    proposals,
    workflow
  });
}
