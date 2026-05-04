import { NextResponse } from "next/server";
import { buildImportPlan, type DocumentImportFormat } from "@/lib/document-engine";
import { queueOperationsMutation } from "@/lib/operations-runtime";

export async function POST(request: Request) {
  const contentType = request.headers.get("content-type") ?? "";
  const url = new URL(request.url);
  const body = contentType.includes("multipart/form-data")
    ? await parseMultipart(request)
    : await request.json().catch(() => ({})) as Record<string, unknown>;
  const filename = typeof body.filename === "string" ? body.filename : "imported-document";
  const format = parseFormat(typeof body.format === "string" ? body.format : filename.split(".").pop());
  const plan = buildImportPlan({
    filename,
    format,
    targetProjectId: typeof body.targetProjectId === "string" ? body.targetProjectId : undefined,
    targetKnowledgeId: typeof body.targetKnowledgeId === "string" ? body.targetKnowledgeId : undefined,
    templateId: typeof body.templateId === "string" ? body.templateId : undefined,
    locale: typeof body.locale === "string" ? body.locale : undefined
  });
  const queued = await queueOperationsMutation({
    action: "create",
    resource: "documents",
    title: `Import document: ${filename}`,
    instruction: plan.queueInstruction,
    payload: {
      filename,
      format,
      templateId: plan.template?.id,
      targetProjectId: plan.targetProjectId,
      targetKnowledgeId: plan.targetKnowledgeId
    },
    source: "operations-document-import-api",
    locale: typeof body.locale === "string" ? body.locale : undefined
  }, url.origin);

  return NextResponse.json({
    ok: true,
    import: {
      filename,
      format,
      templateId: plan.template?.id,
      mode: "queued-conversion"
    },
    queued
  });
}

async function parseMultipart(request: Request) {
  const formData = await request.formData();
  const file = formData.get("file");
  return {
    filename: file instanceof File ? file.name : formData.get("filename"),
    format: file instanceof File ? file.name.split(".").pop() : formData.get("format"),
    targetProjectId: formData.get("targetProjectId"),
    targetKnowledgeId: formData.get("targetKnowledgeId"),
    templateId: formData.get("templateId"),
    locale: formData.get("locale")
  };
}

function parseFormat(value: unknown): DocumentImportFormat {
  if (value === "docx" || value === "pdf" || value === "html" || value === "txt") return value;
  return "docx";
}
