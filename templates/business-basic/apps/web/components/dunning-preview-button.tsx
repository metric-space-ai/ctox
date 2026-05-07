"use client";

import { useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type DunningResult = {
  persisted?: boolean;
  proposals: Array<{
    command: {
      payload: {
        invoiceNumber: string;
        level: number;
      };
    };
  }>;
  workflow?: unknown;
};

export function DunningPreviewButton({ label }: { label: string }) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  async function run() {
    setBusy(true);
    setStatus("");
    try {
      const response = await fetch("/api/business/accounting/dunning", { method: "POST" });
      const payload = await response.json() as DunningResult | { error?: string };
      if (!response.ok || isDunningError(payload)) {
        setStatus(isDunningError(payload) ? payload.error ?? "Dunning failed." : "Dunning failed.");
        return;
      }
      const first = payload.proposals[0]?.command.payload;
      setStatus(first ? `${payload.proposals.length} Vorschlag: ${first.invoiceNumber} Level ${first.level}.` : "Keine Mahnung faellig.");
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
      <button className="drawer-primary" disabled={busy} onClick={run} type="button">
        {busy ? "..." : label}
      </button>
      {status ? <small className="invoice-delivery-status is-sent">{status}</small> : null}
    </div>
  );
}

function isDunningError(payload: DunningResult | { error?: string }): payload is { error?: string } {
  return "error" in payload;
}
