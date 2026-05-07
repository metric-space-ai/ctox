"use client";

import { useState } from "react";
import { notifyAccountingWorkflowUpdated } from "./accounting-workflow-events";

type AccountingCtoxActionButtonProps = {
  label: string;
  locale: "de" | "en";
  storyId: string;
};

type WorkflowResponse = {
  error?: string;
  persisted?: boolean;
  workflow?: unknown;
};

export function AccountingCtoxActionButton({ label, locale, storyId }: AccountingCtoxActionButtonProps) {
  const [status, setStatus] = useState<"idle" | "running" | "done" | "error">("idle");

  async function run() {
    setStatus("running");
    const response = await fetch("/api/business/accounting/story-workflows", {
      body: JSON.stringify({ locale, storyId }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const payload = await response.json().catch(() => ({ error: "invalid_response" })) as WorkflowResponse;

    if (!response.ok || payload.error) {
      setStatus("error");
      return;
    }

    notifyAccountingWorkflowUpdated({
      persisted: payload.persisted,
      workflow: payload.workflow
    });
    setStatus("done");
  }

  const runningLabel = locale === "de" ? "CTOX prueft" : "CTOX checks";
  const doneLabel = locale === "de" ? "Vorschlag bereit" : "Proposal ready";
  const errorLabel = locale === "de" ? "Fehler" : "Error";

  return (
    <button
      className={`reference-ctox-button is-${status}`}
      disabled={status === "running"}
      onClick={() => void run()}
      title={status === "done" ? doneLabel : undefined}
      type="button"
    >
      {status === "running" ? runningLabel : status === "done" ? doneLabel : status === "error" ? errorLabel : label}
    </button>
  );
}
