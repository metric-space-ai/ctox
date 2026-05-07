export type AccountingAgentContract = {
  id: "asset-accountant" | "bank-reconciler" | "datev-exporter" | "dunning-assistant" | "invoice-checker" | "receipt-extractor" | "report-explainer";
  humanGate: "always" | "configurable" | "never";
  trigger: string;
  writes: string;
};

export const accountingAgentManifest: AccountingAgentContract[] = [
  { id: "asset-accountant", trigger: "receipt capitalization, depreciation due, or asset disposal", writes: "asset_activation, asset_depreciation, and asset_disposal proposals", humanGate: "always" },
  { id: "receipt-extractor", trigger: "receipt file uploaded", writes: "extracted fields and receipt_extraction proposal", humanGate: "always" },
  { id: "invoice-checker", trigger: "draft invoice before send", writes: "invoice validation findings and invoice_check proposal", humanGate: "always" },
  { id: "bank-reconciler", trigger: "bank statement imported", writes: "bank_match proposals", humanGate: "configurable" },
  { id: "dunning-assistant", trigger: "scheduled overdue scan", writes: "dunning_run proposal and letter draft", humanGate: "always" },
  { id: "datev-exporter", trigger: "month-end schedule", writes: "datev_export proposal and export checklist", humanGate: "always" },
  { id: "report-explainer", trigger: "user asks CTOX on a report", writes: "narrative answer only", humanGate: "never" }
];
