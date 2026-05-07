"use client";

import { useEffect, useState } from "react";
import type { AccountingWorkflowEventDetail } from "./accounting-workflow-events";

type WorkflowProposal = {
  confidence?: number;
  createdByAgent?: string;
  externalId?: string;
  id?: string;
  kind?: string;
  proposedCommand?: Record<string, unknown>;
  refId?: string;
  refType?: string;
  resultingJournalEntryId?: string | null;
  status?: string;
};

type WorkflowOutbox = {
  externalId?: string;
  id?: string;
  status?: string;
  topic?: string;
};

type WorkflowAudit = {
  action?: string;
  actorId?: string;
  actorType?: string;
  createdAt?: string;
  refId?: string;
  refType?: string;
};

type WorkflowResponse = {
  audit: WorkflowAudit[];
  error?: string;
  outbox: WorkflowOutbox[];
  persistence: "disabled" | "enabled" | "error";
  proposals: WorkflowProposal[];
  reason?: string;
  source?: "database" | "demo";
};

export function AccountingWorkflowPanel({
  compact = false,
  locale,
  quiet = false
}: {
  compact?: boolean;
  locale: "de" | "en";
  quiet?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [data, setData] = useState<WorkflowResponse | null>(null);
  const [decisionMessage, setDecisionMessage] = useState("");

  async function refresh() {
    setBusy(true);
    try {
      const response = await fetch("/api/business/accounting/workflow", { cache: "no-store" });
      const payload = await response.json().catch(() => null) as WorkflowResponse | null;
      setData(payload ?? {
        audit: [],
        error: "invalid_response",
        outbox: [],
        persistence: "error",
        proposals: []
      });
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
    const listener = (event: Event) => {
      const detail = event instanceof CustomEvent ? event.detail as AccountingWorkflowEventDetail | undefined : undefined;
      if (!detail) {
        void refresh();
        return;
      }
      mergeWorkflowDetail(detail);
      if (detail.persisted) void refresh();
    };
    window.addEventListener("ctox-accounting-workflow-updated", listener);
    return () => window.removeEventListener("ctox-accounting-workflow-updated", listener);
  }, []);

  const proposals = data?.proposals ?? [];
  const openProposals = proposals.filter((proposal) => !proposal.status || proposal.status === "open");
  const decidedProposals = proposals.filter((proposal) => proposal.status && proposal.status !== "open");
  const outbox = data?.outbox ?? [];
  const audit = data?.audit ?? [];
  const isDemo = data?.source === "demo" || data?.persistence === "disabled";

  async function decideProposal(proposal: WorkflowProposal, decision: "accept" | "reject") {
    const externalId = proposal.externalId ?? proposal.id;
    if (!externalId) return;
    setDecisionMessage("");
    const response = await fetch(`/api/business/accounting/workflow/proposals/${encodeURIComponent(externalId)}`, {
      body: JSON.stringify({ decision, proposedCommand: proposal.proposedCommand }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const payload = await response.json().catch(() => ({})) as {
      error?: string;
      persisted?: boolean;
      proposal?: {
        resultingJournalEntryId?: string | null;
        status?: string;
      };
    };
    if (!response.ok || payload.error) {
      setDecisionMessage(payload.error ?? "Proposal decision failed.");
      return;
    }
    const status = payload.proposal?.status ?? (decision === "accept" ? "accepted" : "rejected");
    setData((current) => current ? {
      ...current,
      audit: [{
        action: `proposal.${status}`,
        actorId: payload.persisted ? "business-user" : "local-demo-user",
        actorType: "user",
        refId: proposal.refId,
        refType: proposal.refType
      }, ...current.audit].slice(0, 20),
      proposals: current.proposals.map((item) => {
        const itemId = item.externalId ?? item.id;
        return itemId === externalId ? {
          ...item,
          resultingJournalEntryId: payload.proposal?.resultingJournalEntryId ?? item.resultingJournalEntryId ?? resultingJournalEntryIdForCommand(item.proposedCommand),
          status
        } : item;
      })
    } : current);
    const resulting = payload.proposal?.resultingJournalEntryId ?? resultingJournalEntryIdForCommand(proposal.proposedCommand);
    setDecisionMessage(payload.persisted
      ? (locale === "de" ? `Entscheidung gespeichert${resulting ? `: ${resulting}` : "."}` : `Decision saved${resulting ? `: ${resulting}` : "."}`)
      : (locale === "de" ? `Demo-Entscheidung angewendet${resulting ? `: ${resulting}` : "."}` : `Demo decision applied${resulting ? `: ${resulting}` : "."}`));
  }

  function mergeWorkflowDetail(detail: AccountingWorkflowEventDetail) {
    const workflow = collectWorkflowDetail(detail);
    if (!workflow.audit.length && !workflow.outbox.length && !workflow.proposals.length) return;

    setData((current) => {
      const base = current ?? {
        audit: [],
        outbox: [],
        persistence: detail.persisted ? "enabled" : "disabled",
        proposals: [],
        source: detail.persisted ? "database" : "demo"
      } satisfies WorkflowResponse;
      return {
        ...base,
        audit: [...workflow.audit, ...base.audit].slice(0, 20),
        outbox: mergeByWorkflowId(workflow.outbox, base.outbox).slice(0, 20),
        proposals: mergeByWorkflowId(workflow.proposals, base.proposals).slice(0, 20)
      };
    });
    setDecisionMessage(locale === "de" ? "Workflow-Aktion uebernommen." : "Workflow action added.");
  }

  return (
    <section className={`accounting-workflow-panel ${quiet ? "is-quiet" : ""}`} aria-label="Accounting workflow">
      <header>
        <div>
          <p>{quiet ? (locale === "de" ? "Audit" : "Audit") : (locale === "de" ? "Workflow" : "Workflow")}</p>
          <h2>{quiet ? (locale === "de" ? "Nachweis" : "Evidence") : (locale === "de" ? "Review" : "Review")}</h2>
        </div>
        <button className="business-accounting-download" disabled={busy} onClick={() => void refresh()} type="button">
          {busy ? "..." : locale === "de" ? "Aktualisieren" : "Refresh"}
        </button>
      </header>
      {quiet ? (
        <p className={`accounting-workflow-status status-${data?.persistence ?? "loading"}`}>
          {locale === "de"
            ? `${openProposals.length} offen · ${decidedProposals.length} entschieden · ${outbox.filter((event) => event.status !== "delivered").length} Outbox`
            : `${openProposals.length} open · ${decidedProposals.length} decided · ${outbox.filter((event) => event.status !== "delivered").length} outbox`}
        </p>
      ) : (
        <>
          <p className={`accounting-workflow-status status-${data?.persistence ?? "loading"}`}>
            {workflowStatus(data, locale)}
          </p>
          <div className="accounting-workflow-summary" aria-label={locale === "de" ? "Workflow Zusammenfassung" : "Workflow summary"}>
            <div>
              <span>{locale === "de" ? "Offen" : "Open"}</span>
              <strong>{openProposals.length}</strong>
            </div>
            <div>
              <span>{locale === "de" ? "Entschieden" : "Decided"}</span>
              <strong>{decidedProposals.length}</strong>
            </div>
            <div>
              <span>Outbox</span>
              <strong>{outbox.filter((event) => event.status !== "delivered").length}</strong>
            </div>
          </div>
        </>
      )}
      {compact ? (
        <details className="accounting-workflow-details">
          <summary>{locale === "de" ? "Offene Vorschlaege" : "Open proposals"}</summary>
          <ProposalList
            empty={locale === "de" ? "Keine offenen Vorschlaege." : "No open proposals."}
            locale={locale}
            onDecision={decideProposal}
            proposals={openProposals.slice(0, 5)}
            title={locale === "de" ? "Jetzt pruefen" : "Needs review"}
          />
        </details>
      ) : (
        <div className="accounting-workflow-focus">
          <ProposalList
            empty={locale === "de" ? "Keine offenen Vorschlaege." : "No open proposals."}
            locale={locale}
            onDecision={decideProposal}
            proposals={openProposals.slice(0, 5)}
            title={locale === "de" ? "Jetzt pruefen" : "Needs review"}
          />
        </div>
      )}
      <details className="accounting-workflow-details">
        <summary>{locale === "de" ? "Systemverlauf" : "System trail"}</summary>
        <WorkflowList
          empty={locale === "de" ? "Keine Outbox-Events." : "No outbox events."}
          items={outbox.slice(0, 5).map((event) => ({
            id: event.externalId ?? event.id ?? event.topic ?? "outbox",
            meta: event.status ?? "pending",
            title: event.topic ?? "business.outbox"
          }))}
          title="Outbox"
        />
        <WorkflowList
          empty={locale === "de" ? "Keine Audit-Events." : "No audit events."}
          items={audit.slice(0, 5).map((event, index) => ({
            id: `${event.action}-${event.refId}-${index}`,
            meta: `${event.actorType ?? "system"}:${event.actorId ?? "-"} · ${event.refType ?? "ref"}:${event.refId ?? "-"}`,
            title: event.action ?? "audit"
          }))}
          title="Audit"
        />
      </details>
      {isDemo ? (
        <small className="accounting-workflow-note">
          {locale === "de"
            ? "Lokale Demo-Projektion, bis DATABASE_URL gesetzt ist. Button-Aktionen erzeugen denselben Workflow-Payload."
            : "Local demo projection until DATABASE_URL is configured. Button actions emit the same workflow payload."}
        </small>
      ) : null}
      {decisionMessage ? <small className="accounting-workflow-note">{decisionMessage}</small> : null}
    </section>
  );
}

function ProposalList({
  empty,
  locale,
  onDecision,
  proposals,
  title
}: {
  empty: string;
  locale: "de" | "en";
  onDecision: (proposal: WorkflowProposal, decision: "accept" | "reject") => Promise<void>;
  proposals: WorkflowProposal[];
  title: string;
}) {
  return (
    <article>
      <h3>{title}</h3>
      {proposals.length ? (
        <ul>
          {proposals.map((proposal) => {
            const id = proposal.externalId ?? proposal.id ?? `${proposal.kind}-${proposal.refId}`;
            const isOpen = !proposal.status || proposal.status === "open";
            return (
              <li key={id}>
                <strong>{humanProposalKind(proposal.kind, locale)}</strong>
                <span>{proposal.refType ?? "ref"}:{proposal.refId ?? "-"} · {confidence(proposal.confidence)}</span>
                <span>{proposal.createdByAgent ?? "agent"}</span>
                {proposal.resultingJournalEntryId ? <span>result: {proposal.resultingJournalEntryId}</span> : null}
                {isOpen ? (
                  <span className="accounting-workflow-actions">
                    <button onClick={() => void onDecision(proposal, "accept")} type="button">
                      {locale === "de" ? "Annehmen" : "Accept"}
                    </button>
                    <button onClick={() => void onDecision(proposal, "reject")} type="button">
                      {locale === "de" ? "Ablehnen" : "Reject"}
                    </button>
                  </span>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : (
        <p>{empty}</p>
      )}
    </article>
  );
}

function humanProposalKind(kind: string | undefined, locale: "de" | "en") {
  const de: Record<string, string> = {
    asset_activation: "Anlage aktivieren",
    asset_depreciation: "AfA buchen",
    asset_disposal: "Anlage abgehen",
    bank_match: "Bankmatch",
    business_analysis: "BWA",
    chart_setup: "Kontenrahmen",
    cost_center_assignment: "Kostenstelle",
    customer_masterdata: "Kunde",
    datev_export: "DATEV Export",
    dunning_run: "Mahnlauf",
    employee_expense: "Auslage",
    gobd_reversal: "GoBD-Storno",
    invoice_check: "Rechnung",
    invoice_cancellation_credit_note: "Storno-Gutschrift",
    invoice_partial_credit_note: "Teilgutschrift",
    loan_drawdown: "Darlehen",
    loan_installment: "Darlehensrate",
    manual_journal: "Journal",
    month_close: "Monatsabschluss",
    open_items_review: "Offene Posten",
    payables_payment: "Lieferantenzahlung",
    payables_payment_run: "Zahlungslauf",
    product_account_assignment: "Produktkonto",
    profit_and_loss_analysis: "GuV",
    purchase_order_match: "Bestellabgleich",
    quote_prepare: "Angebot",
    quote_to_invoice: "Angebot abrechnen",
    recurring_posting: "Dauerbuchung",
    receipt_clarification: "Belegrueckfrage",
    receipt_duplicate: "Dublette",
    receipt_extraction: "Belegbuchung",
    receipt_ingest: "OCR",
    receipt_variance: "Abweichung",
    report_balance_sheet: "Bilanz",
    reverse_charge_receipt: "Reverse Charge",
    story_workflow: "User Story",
    supplier_discount: "Skonto",
    tax_advisor_handoff: "Steuerberaterpaket",
    travel_expense_report: "Reisekosten",
    vat_return: "UStVA",
    vendor_creation: "Lieferant"
  };
  const en: Record<string, string> = {
    asset_activation: "Asset activation",
    asset_depreciation: "Depreciation",
    asset_disposal: "Asset disposal",
    bank_match: "Bank match",
    business_analysis: "Business analysis",
    chart_setup: "Chart setup",
    cost_center_assignment: "Cost center",
    customer_masterdata: "Customer",
    datev_export: "DATEV export",
    dunning_run: "Dunning",
    employee_expense: "Employee expense",
    gobd_reversal: "GoBD reversal",
    invoice_check: "Invoice",
    invoice_cancellation_credit_note: "Cancellation credit note",
    invoice_partial_credit_note: "Partial credit note",
    loan_drawdown: "Loan drawdown",
    loan_installment: "Loan installment",
    manual_journal: "Journal",
    month_close: "Month close",
    open_items_review: "Open items",
    payables_payment: "Supplier payment",
    payables_payment_run: "Payment run",
    product_account_assignment: "Product account",
    profit_and_loss_analysis: "P&L",
    purchase_order_match: "Purchase order match",
    quote_prepare: "Quote",
    quote_to_invoice: "Quote conversion",
    recurring_posting: "Recurring posting",
    receipt_clarification: "Receipt clarification",
    receipt_duplicate: "Duplicate",
    receipt_extraction: "Receipt posting",
    receipt_ingest: "OCR",
    receipt_variance: "Variance",
    report_balance_sheet: "Balance sheet",
    reverse_charge_receipt: "Reverse charge",
    story_workflow: "User story",
    supplier_discount: "Cash discount",
    tax_advisor_handoff: "Tax advisor handoff",
    travel_expense_report: "Travel expense",
    vat_return: "VAT return",
    vendor_creation: "Vendor"
  };
  return (locale === "de" ? de : en)[kind ?? ""] ?? kind ?? "Proposal";
}

function WorkflowList({
  empty,
  items,
  title
}: {
  empty: string;
  items: Array<{ id: string; meta: string; title: string }>;
  title: string;
}) {
  return (
    <article>
      <h3>{title}</h3>
      {items.length ? (
        <ul>
          {items.map((item) => (
            <li key={item.id}>
              <strong>{item.title}</strong>
              <span>{item.meta}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p>{empty}</p>
      )}
    </article>
  );
}

function workflowStatus(data: WorkflowResponse | null, locale: "de" | "en") {
  if (!data) return locale === "de" ? "Workflow wird geladen." : "Loading workflow.";
  if (data.error) return data.error;
  if (data.persistence === "enabled") return locale === "de" ? "Persistenz aktiv: Datenbank-Proposals und Outbox." : "Persistence enabled: database proposals and outbox.";
  if (data.persistence === "disabled") return locale === "de" ? "Persistenz aus: Demo-Workflow aus Seed-Daten." : "Persistence disabled: demo workflow from seed data.";
  return data.reason ?? "Workflow unavailable.";
}

function confidence(value?: number) {
  if (value === undefined) return "n/a";
  return `${value > 1 ? Math.round(value) : Math.round(value * 100)}%`;
}

function collectWorkflowDetail(detail: AccountingWorkflowEventDetail) {
  const containers = [
    detail,
    asRecord(detail.accounting),
    asRecord(detail.workflow)
  ].filter(Boolean) as Record<string, unknown>[];
  const workflowArray = Array.isArray(detail.workflow) ? detail.workflow : [];
  const proposalCandidates = [
    ...containers.flatMap((container) => [container.proposal, container.matchedProposal, container.proposals]),
    ...workflowArray.map((entry) => asRecord(entry)?.proposal)
  ];
  const outboxCandidates = [
    ...containers.map((container) => container.outbox),
    ...workflowArray.map((entry) => asRecord(entry)?.outbox)
  ];
  const auditCandidates = [
    ...containers.map((container) => container.audit),
    ...workflowArray.map((entry) => asRecord(entry)?.audit)
  ];

  return {
    audit: auditCandidates.flatMap((candidate) => normalizeItems<WorkflowAudit>(candidate)),
    outbox: outboxCandidates.flatMap((candidate) => normalizeItems<WorkflowOutbox>(candidate)),
    proposals: proposalCandidates.flatMap((candidate) => normalizeItems<WorkflowProposal>(candidate))
  };
}

function normalizeItems<T>(value: unknown): T[] {
  if (!value) return [];
  return (Array.isArray(value) ? value : [value]).filter((item): item is T => Boolean(item) && typeof item === "object");
}

function mergeByWorkflowId<T extends { externalId?: string; id?: string; kind?: string; refId?: string; topic?: string }>(incoming: T[], existing: T[]) {
  const result: T[] = [];
  const seen = new Set<string>();
  for (const item of [...incoming, ...existing]) {
    const id = item.externalId ?? item.id ?? `${item.kind ?? item.topic ?? "workflow"}-${item.refId ?? result.length}`;
    if (seen.has(id)) continue;
    seen.add(id);
    result.push(item);
  }
  return result;
}

function asRecord(value: unknown) {
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}

function resultingJournalEntryIdForCommand(command: Record<string, unknown> | undefined) {
  const type = command?.type;
  const refType = typeof command?.refType === "string" ? command.refType : null;
  const refId = typeof command?.refId === "string" ? command.refId : null;

  if (!refType || !refId) return null;
  if (type === "SendInvoice") return `je-invoice-${refType}-${refId}`;
  if (type === "PostReceipt") return `je-receipt-${refType}-${refId}`;
  if (type === "CapitalizeReceipt") return `je-manual-asset-asset-${refId}`;
  if (type === "DisposeAsset") return `je-manual-asset-${refId}`;
  if (type === "PostDepreciation") return `je-depreciation-${refType}-${refId}`;
  if (type === "AcceptBankMatch") return `je-payment-${refType}-${refId}`;
  return null;
}
