import {
  createAccountingAuditEvent,
  createAccountingProposal,
  createBusinessOutboxEvent,
  prepareReceiptIngestCommand
} from "@ctox-business/accounting";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";

const companyId = "business-basic-company";

export async function POST(
  request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params;
  const body = await request.json().catch(() => ({})) as {
    blobRef?: string;
    mime?: string;
    originalFilename?: string;
    sha256?: string;
    sourceText?: string;
  };
  const data = await getBusinessBundle();
  const receipt = data.receipts.find((item) => item.id === id);

  if (!receipt) {
    return NextResponse.json({ error: "receipt_not_found" }, { status: 404 });
  }

  const file = {
    blobRef: body.blobRef ?? `receipt-file:${receipt.id}`,
    mime: body.mime ?? "application/pdf",
    originalFilename: body.originalFilename ?? receipt.attachmentName,
    sha256: body.sha256 ?? stablePseudoSha256(`${receipt.id}:${receipt.attachmentName}`)
  };
  const command = prepareReceiptIngestCommand({
    companyId,
    file,
    receiptId: receipt.id
  });
  const proposal = createAccountingProposal({
    companyId,
    confidence: body.sourceText ? 0.82 : 0.72,
    createdByAgent: "receipt-extractor",
    evidence: {
      attachmentName: file.originalFilename,
      mime: file.mime,
      sha256: file.sha256,
      sourceTextPreview: body.sourceText?.slice(0, 500),
      vendorName: receipt.vendorName
    },
    kind: "receipt_ingest",
    proposedCommand: command,
    refId: receipt.id,
    refType: "receipt"
  });
  const audit = createAccountingAuditEvent({
    action: "receipt.prepare_ingest",
    actorId: "business-runtime",
    actorType: "system",
    after: { command, file, sourceTextPresent: Boolean(body.sourceText) },
    companyId,
    refId: receipt.id,
    refType: "receipt"
  });
  const outbox = createBusinessOutboxEvent({
    companyId,
    payload: { command, file, proposalId: proposal.id, sourceText: body.sourceText },
    topic: "business.receipt.prepare_ingest"
  });

  if (process.env.DATABASE_URL) {
    await saveAccountingWorkflowSnapshot({ audit, outbox, proposal });
  }

  return NextResponse.json({
    audit,
    command,
    outbox,
    persisted: Boolean(process.env.DATABASE_URL),
    proposal
  });
}

function stablePseudoSha256(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = Math.imul(31, hash) + value.charCodeAt(index) | 0;
  }
  return `sha256-demo-${Math.abs(hash).toString(16).padStart(8, "0")}`;
}
