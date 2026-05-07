import { decideAccountingProposal } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";

type DecisionRequest = {
  decision?: "accept" | "reject" | "supersede";
  proposedCommand?: Record<string, unknown>;
  resultingJournalEntryId?: string | null;
};

const statusByDecision = {
  accept: "accepted",
  reject: "rejected",
  supersede: "superseded"
} as const;

export async function POST(
  request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params;
  const body = await request.json().catch(() => ({})) as DecisionRequest;
  const decision = body.decision === "reject" || body.decision === "supersede" ? body.decision : "accept";
  const status = statusByDecision[decision];
  const resultingJournalEntryId = body.resultingJournalEntryId ?? resultingJournalEntryIdForCommand(body.proposedCommand) ?? null;

  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      persisted: false,
      proposal: {
        externalId: id,
        resultingJournalEntryId,
        status
      },
      reason: "DATABASE_URL not configured"
    });
  }

  try {
    const proposal = await decideAccountingProposal({
      actorId: "business-user",
      externalId: id,
      resultingJournalEntryId,
      status
    });
    return NextResponse.json({ persisted: true, proposal });
  } catch (error) {
    return NextResponse.json({
      error: error instanceof Error ? error.message : String(error)
    }, { status: 404 });
  }
}

function resultingJournalEntryIdForCommand(command: Record<string, unknown> | undefined) {
  const type = command?.type;
  const refType = typeof command?.refType === "string" ? command.refType : null;
  const refId = typeof command?.refId === "string" ? command.refId : null;

  if (!refType || !refId) return null;
  if (type === "SendInvoice") return `je-invoice-${refType}-${refId}`;
  if (type === "PostReceipt") return `je-receipt-${refType}-${refId}`;
  if (type === "AcceptBankMatch") return `je-payment-${refType}-${refId}`;
  if (type === "RunDunning") return `dunning-run-${refId}`;
  if (type === "ExportDatev") return `datev-export-${refId}`;
  if (type === "ImportBankStatement") return `bank-statement-${refId}`;
  if (type === "IngestReceipt") return `receipt-ingest-${refId}`;
  return null;
}
