import { NextResponse } from "next/server";
import { buildExportPlan, type DocumentExportFormat } from "@/lib/document-engine";
import { queueOperationsMutation } from "@/lib/operations-runtime";

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as {
    documentId?: string;
    format?: string;
    locale?: string;
    includeCtoxContext?: boolean;
  };
  const documentId = body.documentId ?? "doc-operating-model";
  const format = parseFormat(body.format);
  const plan = buildExportPlan({
    documentId,
    format,
    locale: body.locale,
    includeCtoxContext: body.includeCtoxContext
  });
  const url = new URL(request.url);
  const queued = await queueOperationsMutation({
    action: "sync",
    resource: "documents",
    recordId: documentId,
    title: `Export document: ${plan.document.title}`,
    instruction: plan.queueInstruction,
    payload: {
      documentId,
      format,
      filename: plan.filename,
      mimeType: plan.mimeType,
      templateId: plan.template?.id,
      linkedRecords: plan.document.linkedRecords
    },
    source: "operations-document-export-api",
    locale: body.locale
  }, url.origin);

  return NextResponse.json({
    ok: true,
    export: {
      documentId,
      filename: plan.filename,
      format,
      mimeType: plan.mimeType,
      previewHtml: format === "html" ? plan.html : undefined,
      mode: format === "html" ? "immediate-preview-and-queued" : "queued-conversion"
    },
    queued
  });
}

function parseFormat(value: unknown): DocumentExportFormat {
  if (value === "docx" || value === "pdf" || value === "html") return value;
  return "html";
}
