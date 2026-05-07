"use client";

import { useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type DatevExportResult = {
  csv?: string;
  error?: string;
  filename?: string;
  persisted?: boolean;
  workflow?: unknown;
};

export function DatevExportButton({ label }: { label: string }) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  async function runExport() {
    setBusy(true);
    setStatus("");
    try {
      const response = await fetch("/api/business/accounting/datev-export?workflow=json", { cache: "no-store" });
      const payload = await response.json().catch(() => ({})) as DatevExportResult;
      if (!response.ok || payload.error || !payload.csv) {
        setStatus(payload.error ?? "DATEV export failed.");
        return;
      }

      downloadCsv(payload.csv, payload.filename ?? "datev-export.csv");
      setStatus(`${payload.filename ?? "DATEV CSV"} vorbereitet.`);
      notifyAccountingWorkflowUpdated({
        persisted: payload.persisted,
        workflow: payload.workflow
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="accounting-command-control">
      <button className="business-accounting-download" disabled={busy} onClick={() => void runExport()} type="button">
        {busy ? "..." : label}
      </button>
      {status ? <small className="invoice-delivery-status is-sent">{status}</small> : null}
    </div>
  );
}

function downloadCsv(csv: string, filename: string) {
  const url = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.rel = "noreferrer";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
