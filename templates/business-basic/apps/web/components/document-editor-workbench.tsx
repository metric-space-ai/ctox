"use client";

import { useMemo, useRef, useState } from "react";

type Locale = "en" | "de";

type Template = {
  id: string;
  name: string;
  kind: string;
  description: Record<Locale, string>;
  variables: string[];
  blocks: Array<{
    id: string;
    title: Record<Locale, string>;
    html: Record<Locale, string>;
  }>;
};

type DocumentRecord = {
  id: string;
  title: string;
  kind: string;
  templateId?: string;
  status: string;
  updated: string;
  version: number;
  bodyHtml: Record<Locale, string>;
  linkedRecords: Array<{
    module: string;
    recordType: string;
    recordId: string;
    label: string;
  }>;
};

export function DocumentEditorWorkbench({
  documents,
  locale,
  selectedDocumentId,
  templates
}: {
  documents: DocumentRecord[];
  locale: Locale;
  selectedDocumentId?: string;
  templates: Template[];
}) {
  const initialDocument = documents.find((document) => document.id === selectedDocumentId) ?? documents[0];
  const [documentId, setDocumentId] = useState(initialDocument?.id ?? "");
  const selectedDocument = documents.find((document) => document.id === documentId) ?? initialDocument;
  const [mode, setMode] = useState<"read" | "edit">("read");
  const [templateId, setTemplateId] = useState(selectedDocument?.templateId ?? templates[0]?.id ?? "");
  const selectedTemplate = templates.find((template) => template.id === templateId);
  const [html, setHtml] = useState(selectedDocument?.bodyHtml[locale] ?? "");
  const [message, setMessage] = useState("");
  const editorRef = useRef<HTMLDivElement | null>(null);
  const labels = locale === "de" ? deLabels : enLabels;

  const templatePreview = useMemo(() => selectedTemplate?.blocks.map((block) => block.html[locale] ?? block.html.en).join("") ?? "", [locale, selectedTemplate]);

  if (!selectedDocument) {
    return <div className="document-workbench"><p>{labels.noDocument}</p></div>;
  }

  return (
    <section
      className="document-workbench"
      data-context-item
      data-context-label={selectedDocument.title}
      data-context-module="operations"
      data-context-record-id={selectedDocument.id}
      data-context-record-type="document"
      data-context-submodule="knowledge"
    >
      <header className="document-workbench-head">
        <div>
          <strong>{selectedDocument.title}</strong>
          <span>{selectedDocument.kind} - {selectedDocument.status} - v{selectedDocument.version} - {selectedDocument.updated}</span>
        </div>
        <div className="document-mode-switch">
          <button className={mode === "read" ? "active" : ""} onClick={() => setMode("read")} type="button">{labels.read}</button>
          <button className={mode === "edit" ? "active" : ""} onClick={() => setMode("edit")} type="button">{labels.edit}</button>
        </div>
      </header>

      <div className="document-workbench-toolbar" data-visible={mode === "edit" ? "true" : "false"}>
        <select
          aria-label={labels.document}
          onChange={(event) => {
            const next = documents.find((document) => document.id === event.target.value);
            setDocumentId(event.target.value);
            setTemplateId(next?.templateId ?? templateId);
            setHtml(next?.bodyHtml[locale] ?? "");
            setMode("read");
          }}
          value={selectedDocument.id}
        >
          {documents.map((document) => <option key={document.id} value={document.id}>{document.title}</option>)}
        </select>
        <select aria-label={labels.template} onChange={(event) => setTemplateId(event.target.value)} value={templateId}>
          {templates.map((template) => <option key={template.id} value={template.id}>{template.name}</option>)}
        </select>
        {formatButtons.map((button) => (
          <button key={button.command} onClick={() => applyCommand(button.command, button.value)} title={button.label} type="button">
            {button.short}
          </button>
        ))}
        <button onClick={() => insertTemplate(templatePreview, editorRef.current, setHtml)} type="button">{labels.insertTemplate}</button>
        <button onClick={() => queueSave(selectedDocument.id, editorRef.current?.innerHTML ?? html, setMessage)} type="button">{labels.saveDraft}</button>
      </div>

      <div className="document-editor-layout">
        <article
          className="document-page"
          contentEditable={mode === "edit"}
          data-editing={mode === "edit" ? "true" : "false"}
          data-placeholder={labels.placeholder}
          onInput={(event) => setHtml(event.currentTarget.innerHTML)}
          ref={editorRef}
          suppressContentEditableWarning
          dangerouslySetInnerHTML={{ __html: html }}
        />
        <aside className="document-sidecar">
          <section>
            <h3>{labels.template}</h3>
            <strong>{selectedTemplate?.name ?? "-"}</strong>
            <p>{selectedTemplate?.description[locale] ?? selectedTemplate?.description.en}</p>
            <div className="document-variable-list">
              {(selectedTemplate?.variables ?? []).map((variable) => <button key={variable} onClick={() => insertText(`{{${variable}}}`, editorRef.current, setHtml)} type="button">{variable}</button>)}
            </div>
          </section>
          <section>
            <h3>{labels.linkedRecords}</h3>
            <div className="document-linked-list">
              {selectedDocument.linkedRecords.map((record) => (
                <span key={`${record.module}-${record.recordType}-${record.recordId}`}>{record.module} / {record.recordType}: {record.label}</span>
              ))}
            </div>
          </section>
          <section>
            <h3>{labels.importExport}</h3>
            <DocumentImportForm locale={locale} templates={templates} onMessage={setMessage} />
            <div className="document-export-row">
              {(["html", "docx", "pdf"] as const).map((format) => (
                <button key={format} onClick={() => queueExport(selectedDocument.id, format, locale, setMessage)} type="button">
                  {labels.export} {format.toUpperCase()}
                </button>
              ))}
            </div>
          </section>
          {message ? <p className="document-status">{message}</p> : null}
        </aside>
      </div>
    </section>
  );
}

function DocumentImportForm({
  locale,
  onMessage,
  templates
}: {
  locale: Locale;
  onMessage: (message: string) => void;
  templates: Template[];
}) {
  const [file, setFile] = useState<File | null>(null);
  const [templateId, setTemplateId] = useState(templates[0]?.id ?? "");
  const labels = locale === "de" ? deLabels : enLabels;

  return (
    <form
      className="document-import-form"
      onSubmit={async (event) => {
        event.preventDefault();
        if (!file) return;
        const formData = new FormData();
        formData.set("file", file);
        formData.set("templateId", templateId);
        formData.set("locale", locale);
        const response = await fetch("/api/operations/documents/import", { method: "POST", body: formData });
        const payload = await response.json().catch(() => null) as { import?: { mode?: string } } | null;
        onMessage(payload?.import?.mode ? `${labels.importQueued}: ${file.name}` : labels.queueFailed);
      }}
    >
      <input accept=".docx,.pdf,.html,.txt" aria-label={labels.importFile} onChange={(event) => setFile(event.target.files?.[0] ?? null)} type="file" />
      <select aria-label={labels.template} onChange={(event) => setTemplateId(event.target.value)} value={templateId}>
        {templates.map((template) => <option key={template.id} value={template.id}>{template.name}</option>)}
      </select>
      <button disabled={!file} type="submit">{labels.import}</button>
    </form>
  );
}

const formatButtons = [
  { command: "bold", label: "Bold", short: "B" },
  { command: "italic", label: "Italic", short: "I" },
  { command: "insertUnorderedList", label: "List", short: "List" },
  { command: "formatBlock", label: "Heading", short: "H2", value: "h2" }
];

function applyCommand(command: string, value?: string) {
  document.execCommand(command, false, value);
}

function insertTemplate(templateHtml: string, node: HTMLDivElement | null, setHtml: (html: string) => void) {
  if (!node) return;
  node.focus();
  document.execCommand("insertHTML", false, templateHtml);
  setHtml(node.innerHTML);
}

function insertText(value: string, node: HTMLDivElement | null, setHtml: (html: string) => void) {
  if (!node) return;
  node.focus();
  document.execCommand("insertText", false, value);
  setHtml(node.innerHTML);
}

async function queueSave(documentId: string, html: string, setMessage: (message: string) => void) {
  const response = await fetch("/api/operations/documents", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      action: "update",
      recordId: documentId,
      title: `Save document draft: ${documentId}`,
      payload: { html },
      instruction: "Save this CTOX document draft, create a new version, preserve linked records, and synchronize it with the Knowledge Store."
    })
  });
  const payload = await response.json().catch(() => null) as { core?: { taskId?: string | null } } | null;
  setMessage(payload?.core?.taskId ? `Queued ${payload.core.taskId}` : "Draft queued");
}

async function queueExport(documentId: string, format: "html" | "docx" | "pdf", locale: Locale, setMessage: (message: string) => void) {
  const response = await fetch("/api/operations/documents/export", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ documentId, format, locale, includeCtoxContext: true })
  });
  const payload = await response.json().catch(() => null) as { export?: { filename?: string; mode?: string } } | null;
  setMessage(payload?.export?.filename ? `${payload.export.mode}: ${payload.export.filename}` : "Export queued");
}

const enLabels = {
  document: "Document",
  edit: "Edit",
  export: "Export",
  import: "Import",
  importExport: "Import / export",
  importFile: "Import file",
  importQueued: "Import queued",
  insertTemplate: "Insert template",
  linkedRecords: "Linked records",
  noDocument: "No document selected.",
  placeholder: "Start writing...",
  queueFailed: "Queue failed",
  read: "Read",
  saveDraft: "Save draft",
  template: "Template"
};

const deLabels = {
  document: "Dokument",
  edit: "Bearbeiten",
  export: "Export",
  import: "Import",
  importExport: "Import / Export",
  importFile: "Datei importieren",
  importQueued: "Import gequeued",
  insertTemplate: "Template einfügen",
  linkedRecords: "Verknüpfte Records",
  noDocument: "Kein Dokument ausgewählt.",
  placeholder: "Schreiben...",
  queueFailed: "Queue fehlgeschlagen",
  read: "Lesen",
  saveDraft: "Draft speichern",
  template: "Template"
};
