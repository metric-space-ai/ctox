"use client";

export type AccountingWorkflowEventDetail = {
  accounting?: unknown;
  audit?: unknown;
  outbox?: unknown;
  persisted?: boolean;
  proposal?: unknown;
  proposals?: unknown;
  workflow?: unknown;
};

export function notifyAccountingWorkflowUpdated(detail?: AccountingWorkflowEventDetail) {
  window.dispatchEvent(new CustomEvent("ctox-accounting-workflow-updated", { detail }));
}
