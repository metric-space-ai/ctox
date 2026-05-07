"use client";

import { useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type AccountingCommandButtonProps = {
  action: "capitalize" | "depreciate" | "dispose" | "export" | "match" | "post";
  label: string;
  recordId: string;
  resource: string;
};

export function AccountingCommandButton({ action, label, recordId, resource }: AccountingCommandButtonProps) {
  const [status, setStatus] = useState<"idle" | "running" | "done" | "error">("idle");
  const [message, setMessage] = useState("");

  async function run() {
    setStatus("running");
    setMessage("Preparing accounting command.");
    const response = await fetch(`/api/business/${resource}`, {
      body: JSON.stringify({ action, recordId }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const result = await response.json().catch(() => ({ ok: false, error: "invalid_response" })) as {
      accounting?: { command?: { type?: string } };
      accountingPersistence?: { persisted?: boolean };
      error?: string;
      ok?: boolean;
    };

    if (!response.ok || !result.ok) {
      setStatus("error");
      setMessage(result.error ?? "Accounting command failed.");
      return;
    }

    setStatus("done");
    setMessage(`${result.accounting?.command?.type ?? "Accounting command"} prepared.`);
    notifyAccountingWorkflowUpdated({
      accounting: result.accounting,
      persisted: result.accountingPersistence?.persisted
    });
  }

  return (
    <div className="accounting-command-control">
      <button className="drawer-primary" disabled={status === "running"} onClick={() => void run()} type="button">
        {status === "running" ? "Preparing..." : label}
      </button>
      {message ? <small className={`invoice-delivery-status is-${status === "error" ? "error" : status === "done" ? "sent" : "idle"}`}>{message}</small> : null}
    </div>
  );
}
