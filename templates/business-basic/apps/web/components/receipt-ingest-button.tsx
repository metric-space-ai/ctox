"use client";

import { useRef, useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type ReceiptIngestButtonProps = {
  label: string;
  path: string;
};

type ReceiptIngestResult = {
  audit?: unknown;
  command?: { type?: string };
  error?: string;
  outbox?: unknown;
  persisted?: boolean;
  proposal?: { confidence?: number };
};

export function ReceiptIngestButton({ label, path }: ReceiptIngestButtonProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [sourceText, setSourceText] = useState("");
  const [status, setStatus] = useState("");
  const [selectedFile, setSelectedFile] = useState<File | null>(null);

  async function runIngest() {
    setBusy(true);
    setStatus("");
    try {
      const filePayload = selectedFile ? await fileMetadata(selectedFile) : {};
      const response = await fetch(path, {
        body: JSON.stringify({
          ...filePayload,
          sourceText: sourceText.trim() || undefined
        }),
        headers: { "content-type": "application/json" },
        method: "POST"
      });
      const payload = await response.json().catch(() => ({})) as ReceiptIngestResult;
      if (!response.ok || payload.error) {
        setStatus(payload.error ?? "Ingest failed.");
        return;
      }
      const confidence = payload.proposal?.confidence ? `${Math.round(payload.proposal.confidence * 100)}%` : "n/a";
      setStatus(`${payload.command?.type ?? "IngestReceipt"} prepared, ${confidence} confidence.`);
      notifyAccountingWorkflowUpdated(payload);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="accounting-command-control accounting-file-command">
      <input
        accept="application/pdf,image/*,text/plain"
        className="accounting-file-input"
        onChange={(event) => setSelectedFile(event.target.files?.[0] ?? null)}
        ref={fileInputRef}
        type="file"
      />
      <button className="business-accounting-download" onClick={() => fileInputRef.current?.click()} type="button">
        {selectedFile ? selectedFile.name : "Datei waehlen"}
      </button>
      <textarea
        aria-label="OCR source text"
        className="accounting-preview-input"
        onChange={(event) => setSourceText(event.target.value)}
        placeholder="Optionaler OCR-Text oder Notiz zum Eingangsbeleg"
        rows={3}
        value={sourceText}
      />
      <button className="drawer-primary" disabled={busy} onClick={runIngest} type="button">
        {busy ? "..." : label}
      </button>
      {status ? <small className="invoice-delivery-status is-sent">{status}</small> : null}
    </div>
  );
}

async function fileMetadata(file: File) {
  const bytes = await file.arrayBuffer();
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return {
    blobRef: `local-upload:${Array.from(new Uint8Array(digest)).slice(0, 8).map((byte) => byte.toString(16).padStart(2, "0")).join("")}`,
    mime: file.type || "application/octet-stream",
    originalFilename: file.name,
    sha256: `sha256-${hex(digest)}`
  };
}

function hex(buffer: ArrayBuffer) {
  return Array.from(new Uint8Array(buffer)).map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
