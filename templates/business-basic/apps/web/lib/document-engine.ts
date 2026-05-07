import { operationsDocuments, operationsDocumentTemplates, text, type OperationsDocumentRecord, type OperationsDocumentTemplate, type SupportedLocale } from "./operations-seed";

export type DocumentExportFormat = "html" | "docx" | "pdf";
export type DocumentImportFormat = "html" | "docx" | "pdf" | "txt";

export type DocumentExportRequest = {
  documentId: string;
  format: DocumentExportFormat;
  locale?: string;
  includeCtoxContext?: boolean;
};

export type DocumentImportRequest = {
  filename: string;
  format: DocumentImportFormat;
  targetProjectId?: string;
  targetKnowledgeId?: string;
  templateId?: string;
  locale?: string;
};

export function listDocuments() {
  return operationsDocuments;
}

export function listDocumentTemplates() {
  return operationsDocumentTemplates;
}

export function getDocument(documentId: string) {
  return operationsDocuments.find((document) => document.id === documentId) ?? operationsDocuments[0];
}

export function getTemplate(templateId?: string) {
  return templateId ? operationsDocumentTemplates.find((template) => template.id === templateId) : undefined;
}

export function renderTemplatePreview(template: OperationsDocumentTemplate, locale: SupportedLocale) {
  return template.blocks.map((block) => text(block.html, locale)).join("\n");
}

export function renderDocumentHtml(document: OperationsDocumentRecord, locale: SupportedLocale) {
  return [
    "<!doctype html>",
    "<html>",
    "<head>",
    "<meta charset=\"utf-8\" />",
    `<title>${escapeHtml(document.title)}</title>`,
    "<style>body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;line-height:1.5;margin:40px;color:#162126} h1,h2{line-height:1.15} table{border-collapse:collapse;width:100%} td,th{border:1px solid #ccd6dd;padding:6px}</style>",
    "</head>",
    "<body>",
    text(document.bodyHtml, locale),
    "</body>",
    "</html>"
  ].join("");
}

export function buildExportPlan(request: DocumentExportRequest) {
  const locale = request.locale === "de" ? "de" : "en";
  const document = getDocument(request.documentId);
  const template = getTemplate(document.templateId);
  const html = renderDocumentHtml(document, locale);
  const extension = request.format;

  return {
    document,
    template,
    filename: `${slug(document.title)}-v${document.version}.${extension}`,
    format: request.format,
    mimeType: mimeTypeFor(request.format),
    html,
    queueInstruction: [
      `Export CTOX document "${document.title}" as ${request.format.toUpperCase()}.`,
      template ? `Template: ${template.name}` : null,
      `Document version: ${document.version}`,
      request.includeCtoxContext ? "Include linked CTOX, Operations, Sales, Business, and Knowledge context." : "Export only the document body and standard metadata."
    ].filter(Boolean).join("\n")
  };
}

export function buildImportPlan(request: DocumentImportRequest) {
  const template = getTemplate(request.templateId);

  return {
    filename: request.filename,
    format: request.format,
    template,
    targetProjectId: request.targetProjectId,
    targetKnowledgeId: request.targetKnowledgeId,
    queueInstruction: [
      `Import document "${request.filename}" as a CTOX editable document.`,
      `Source format: ${request.format.toUpperCase()}.`,
      template ? `Apply template structure: ${template.name}.` : "Preserve source structure where possible.",
      request.targetProjectId ? `Link to Operations project: ${request.targetProjectId}.` : null,
      request.targetKnowledgeId ? `Link to Knowledge item: ${request.targetKnowledgeId}.` : null,
      "Extract text, tables, headings, links, and attachments where possible; queue manual review for unsupported formatting."
    ].filter(Boolean).join("\n")
  };
}

function mimeTypeFor(format: DocumentExportFormat) {
  if (format === "docx") return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
  if (format === "pdf") return "application/pdf";
  return "text/html; charset=utf-8";
}

function slug(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "document";
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#039;"
  })[char] ?? char);
}
