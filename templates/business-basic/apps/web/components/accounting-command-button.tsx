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
      accounting?: {
        command?: { type?: string };
        proposal?: {
          id?: string;
          proposedCommand?: Record<string, unknown>;
        };
      };
      accountingPersistence?: { persisted?: boolean; error?: string; reason?: string };
      error?: string;
      ok?: boolean;
    };

    if (!response.ok || !result.ok) {
      setStatus("error");
      setMessage(result.error ?? "Accounting command failed.");
      return;
    }

    if (result.accounting?.proposal?.id && result.accountingPersistence?.persisted) {
      const decisionResponse = await fetch(`/api/business/accounting/workflow/proposals/${encodeURIComponent(result.accounting.proposal.id)}`, {
        body: JSON.stringify({
          decision: "accept",
          proposedCommand: result.accounting.proposal.proposedCommand
        }),
        headers: { "content-type": "application/json" },
        method: "POST"
      });
      const decision = await decisionResponse.json().catch(() => ({ error: "invalid_response" })) as {
        error?: string;
        persisted?: boolean;
      };
      if (!decisionResponse.ok || !decision.persisted) {
        setStatus("error");
        setMessage(decision.error ?? "Accounting proposal could not be accepted.");
        return;
      }
    } else if (result.accounting?.proposal?.id) {
      setStatus("error");
      setMessage(result.accountingPersistence?.error ?? result.accountingPersistence?.reason ?? "Accounting proposal was not persisted.");
      return;
    }

    setStatus("done");
    setMessage(`${result.accounting?.command?.type ?? "Accounting command"} posted.`);
    notifyAccountingWorkflowUpdated({
      accounting: result.accounting,
      persisted: result.accountingPersistence?.persisted
    });
    window.location.reload();
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
