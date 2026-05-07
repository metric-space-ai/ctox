"use client";

import { useState } from "react";

type ImportResult = {
  duplicateCount: number;
  statement: {
    lines: Array<{
      amount: number;
      bookingDate: string;
      currency: string;
      purpose?: string;
      remitterName?: string;
    }>;
  };
};

export function BankImportPreviewButton({ label }: { label: string }) {
  const [status, setStatus] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [content, setContent] = useState("");
  const [format, setFormat] = useState<"camt053" | "csv" | "mt940">("csv");
  const [sourceFilename, setSourceFilename] = useState("");
  const [lines, setLines] = useState<ImportResult["statement"]["lines"]>([]);

  async function runImportPreview() {
    setBusy(true);
    setStatus("");
    try {
      const response = await fetch("/api/business/accounting/bank-import", {
        body: JSON.stringify({
          content: content.trim() || undefined,
          format,
          sourceFilename: sourceFilename || undefined
        }),
        headers: { "content-type": "application/json" },
        method: "POST"
      });
      const payload = await response.json() as ImportResult | { error?: string };
      if (!response.ok || isImportError(payload)) {
        setStatus(isImportError(payload) ? payload.error ?? "Import failed." : "Import failed.");
        setLines([]);
        return;
      }
      const total = payload.statement.lines.reduce((sum: number, line) => sum + line.amount, 0);
      setStatus(`${payload.statement.lines.length} lines, ${payload.duplicateCount} duplicates, net ${total.toFixed(2)}.`);
      setLines(payload.statement.lines.slice(0, 4));
    } finally {
      setBusy(false);
    }
  }

  async function loadFile(file?: File) {
    if (!file) return;
    setSourceFilename(file.name);
    setContent(await file.text());
    if (file.name.toLowerCase().endsWith(".sta") || file.name.toLowerCase().endsWith(".mt940")) setFormat("mt940");
    else if (file.name.toLowerCase().endsWith(".xml")) setFormat("camt053");
    else setFormat("csv");
  }

  return (
    <div className="accounting-command-control accounting-import-preview">
      <div className="accounting-preview-controls">
        <select aria-label="Bank import format" onChange={(event) => setFormat(event.target.value as typeof format)} value={format}>
          <option value="csv">CSV</option>
          <option value="camt053">camt.053</option>
          <option value="mt940">MT940</option>
        </select>
        <input aria-label="Bank statement file" onChange={(event) => void loadFile(event.target.files?.[0])} type="file" />
      </div>
      <textarea
        aria-label="Bank statement content"
        className="accounting-preview-input"
        onChange={(event) => setContent(event.target.value)}
        placeholder="CSV, camt.053 XML oder MT940-Auszug einfuegen"
        rows={4}
        value={content}
      />
      <button className="drawer-primary" disabled={busy} onClick={runImportPreview} type="button">
        {busy ? "..." : label}
      </button>
      {status ? <small className="invoice-delivery-status is-sent">{status}</small> : null}
      {lines.length ? (
        <table className="accounting-import-lines">
          <tbody>
            {lines.map((line, index) => (
              <tr key={`${line.bookingDate}-${line.amount}-${index}`}>
                <td>{line.bookingDate}</td>
                <td>{line.remitterName ?? "-"}</td>
                <td>{line.purpose ?? "-"}</td>
                <td>{line.amount.toFixed(2)} {line.currency}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </div>
  );
}

function isImportError(payload: ImportResult | { error?: string }): payload is { error?: string } {
  return "error" in payload;
}
