"use client";

import { useMemo, useRef, useState } from "react";
import { AccountingCtoxActionButton } from "./accounting-ctox-action-button";

type Locale = "de" | "en";

type PaymentRow = {
  amount: number;
  bookingDate: string;
  confidence: number;
  counterparty: string;
  currency: "EUR" | "USD";
  id: string;
  matchedLabel: string;
  nextAction: string;
  purpose: string;
  status: "Ignored" | "Matched" | "Suggested" | "Unmatched";
  valueDate: string;
};

type PaymentAccount = {
  accountType: string;
  code: string;
  id: string;
  isPosting?: boolean;
  name: string;
  rootType: string;
};

type PaymentsReconciliationWorkspaceProps = {
  accounts: PaymentAccount[];
  bankRows: PaymentRow[];
  locale: Locale;
};

type FinanceTab = "accounts" | "transactions" | "transfers";
type StatusFilter = "all" | "suggested" | "unmatched";
type DirectionFilter = "all" | "incoming" | "outgoing";
type PeriodFilter = "all" | "may" | "today";

export function PaymentsHeaderActions({ locale }: { locale: Locale }) {
  const [status, setStatus] = useState<"done" | "error" | "idle" | "running">("idle");
  const fileInputRef = useRef<HTMLInputElement>(null);

  async function importBankStatement(file: File) {
    setStatus("running");

    try {
      const content = await file.text();
      const response = await fetch("/api/business/accounting/bank-import", {
        body: JSON.stringify({ content, format: inferBankImportFormat(file.name), sourceFilename: file.name }),
        headers: { "content-type": "application/json" },
        method: "POST"
      });
      const payload = await response.json().catch(() => ({ error: "invalid_response" })) as {
        error?: string;
        persisted?: boolean;
      };

      if (!response.ok || payload.error || !payload.persisted) {
        setStatus("error");
        return;
      }

      setStatus("done");
      window.location.reload();
    } catch {
      setStatus("error");
    }
  }

  return (
    <>
      <input
        accept=".csv,.xml,.txt,.sta"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) void importBankStatement(file);
          event.currentTarget.value = "";
        }}
        ref={fileInputRef}
        type="file"
      />
      <button className="finance-secondary-action is-primary" disabled={status === "running"} onClick={() => fileInputRef.current?.click()} type="button">
        {status === "running" ? (locale === "de" ? "Import läuft" : "Importing") : status === "done" ? (locale === "de" ? "Import gespeichert" : "Import saved") : status === "error" ? (locale === "de" ? "Import fehlgeschlagen" : "Import failed") : locale === "de" ? "Bankimport" : "Bank import"}
      </button>
    </>
  );
}

function inferBankImportFormat(filename: string): "camt053" | "csv" | "mt940" {
  const normalized = filename.toLowerCase();
  if (normalized.endsWith(".xml")) return "camt053";
  if (normalized.endsWith(".sta") || normalized.includes("mt940")) return "mt940";
  return "csv";
}

export function PaymentsReconciliationWorkspace({ accounts, bankRows, locale }: PaymentsReconciliationWorkspaceProps) {
  const de = locale === "de";
  const rows = bankRows;
  const [activeTab, setActiveTab] = useState<FinanceTab>("transactions");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [directionFilter, setDirectionFilter] = useState<DirectionFilter>("all");
  const [periodFilter, setPeriodFilter] = useState<PeriodFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(bankRows[0]?.id ?? "");
  const [notice, setNotice] = useState("");
  const [manualAccountId, setManualAccountId] = useState("");
  const [manualPostingId, setManualPostingId] = useState("");
  const [runningMatchId, setRunningMatchId] = useState("");

  const orderedRows = useMemo(
    () => [...rows].sort((a, b) => bankRowPriority(a.status) - bankRowPriority(b.status)),
    [rows]
  );

  const visibleRows = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return orderedRows.filter((row) => {
      const statusMatches = statusFilter === "all"
        || (statusFilter === "suggested" && row.status === "Suggested")
        || (statusFilter === "unmatched" && row.status === "Unmatched");
      const directionMatches = directionFilter === "all"
        || (directionFilter === "incoming" && row.amount > 0)
        || (directionFilter === "outgoing" && row.amount < 0);
      const periodMatches = periodFilter === "all"
        || (periodFilter === "today" && row.bookingDate === "2026-05-07")
        || (periodFilter === "may" && row.bookingDate.startsWith("2026-05"));
      const queryMatches = !normalizedQuery
        || `${row.counterparty} ${row.purpose} ${row.matchedLabel} ${row.amount}`.toLowerCase().includes(normalizedQuery);
      return statusMatches && directionMatches && periodMatches && queryMatches;
    });
  }, [directionFilter, orderedRows, periodFilter, query, statusFilter]);

  const selected = visibleRows.find((row) => row.id === selectedId) ?? visibleRows[0];
  const suggestedCount = rows.filter((row) => row.status === "Suggested").length;
  const unmatchedCount = rows.filter((row) => row.status === "Unmatched").length;
  const transfers = orderedRows.filter((row) => row.amount < 0);
  const balance = orderedRows.reduce((sum, row) => sum + row.amount, 0);
  const manualPostingAccounts = useMemo(() => {
    const preferredRoot = selected?.amount && selected.amount < 0 ? "expense" : "income";
    const candidates = accounts.filter((account) => account.isPosting !== false && account.id !== "acc-bank" && account.rootType === preferredRoot);
    return candidates.length ? candidates : accounts.filter((account) => account.isPosting !== false && account.id !== "acc-bank");
  }, [accounts, selected?.amount]);
  const selectedManualAccountId = manualAccountId || manualPostingAccounts[0]?.id || "";

  function selectStatus(next: StatusFilter) {
    setStatusFilter(next);
    setActiveTab("transactions");
  }

  async function acceptMatch(row: PaymentRow) {
    setRunningMatchId(row.id);
    setNotice("");
    const response = await fetch("/api/business/bank-transactions", {
      body: JSON.stringify({ action: "match", locale, recordId: row.id }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const payload = await response.json().catch(() => ({ error: "invalid_response", ok: false })) as {
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

    setRunningMatchId("");
    if (!response.ok || !payload.ok) {
      setNotice(payload.error ?? (de ? "Zuordnung konnte nicht gebucht werden." : "Match could not be posted."));
      return;
    }

    if (!payload.accountingPersistence?.persisted) {
      setNotice(payload.accountingPersistence?.error ?? payload.accountingPersistence?.reason ?? (de ? "Postgres-Persistenz fehlt." : "Postgres persistence is missing."));
      return;
    }

    const proposalId = payload.accounting?.proposal?.id;
    if (proposalId) {
      const decisionResponse = await fetch(`/api/business/accounting/workflow/proposals/${encodeURIComponent(proposalId)}`, {
        body: JSON.stringify({
          decision: "accept",
          proposedCommand: payload.accounting?.proposal?.proposedCommand
        }),
        headers: { "content-type": "application/json" },
        method: "POST"
      });
      const decisionPayload = await decisionResponse.json().catch(() => ({ error: "invalid_response" })) as {
        error?: string;
        persisted?: boolean;
      };
      if (!decisionResponse.ok || !decisionPayload.persisted) {
        setNotice(decisionPayload.error ?? (de ? "Match-Vorschlag konnte nicht festgeschrieben werden." : "Match proposal could not be accepted."));
        return;
      }
    }

    setSelectedId(row.id);
    setNotice(`${payload.accounting?.command?.type ?? "Bank match"} ${de ? "gespeichert. Daten werden neu geladen." : "saved. Reloading data."}`);
    window.location.reload();
  }

  async function postManualBankTransaction(row: PaymentRow) {
    const accountId = selectedManualAccountId;
    if (!accountId) {
      setNotice(de ? "Kein Buchungskonto verfügbar." : "No posting account available.");
      return;
    }

    setManualPostingId(row.id);
    setNotice("");
    const response = await fetch("/api/business/accounting/bank-manual-posting", {
      body: JSON.stringify({ accountId, recordId: row.id }),
      headers: { "content-type": "application/json" },
      method: "POST"
    });
    const payload = await response.json().catch(() => ({ error: "invalid_response" })) as {
      error?: string;
      persisted?: boolean;
    };
    setManualPostingId("");
    if (!response.ok || !payload.persisted) {
      setNotice(payload.error ?? (de ? "Manuelle Buchung konnte nicht gespeichert werden." : "Manual posting could not be saved."));
      return;
    }
    setNotice(de ? "Manuelle Buchung gespeichert. Daten werden neu geladen." : "Manual posting saved. Reloading data.");
    window.location.reload();
  }

  return (
    <>
      <nav className="reference-top-tabs" aria-label={de ? "Finanzbereiche" : "Finance areas"}>
        {[
          ["transactions", de ? "Umsätze" : "Transactions"],
          ["transfers", de ? "Überweisungen" : "Transfers"],
          ["accounts", de ? "Konten" : "Accounts"]
        ].map(([id, label]) => (
          <button aria-selected={activeTab === id} key={id} onClick={() => setActiveTab(id as FinanceTab)} type="button">
            {label}
          </button>
        ))}
      </nav>

      {activeTab === "transactions" ? (
        <>
          <section className="payment-status-bar" aria-label={de ? "Zuordnungsstatus" : "Assignment status"}>
            <button aria-selected={statusFilter === "suggested"} onClick={() => selectStatus("suggested")} type="button">
              <span>{de ? "Vorschläge prüfen" : "Review suggestions"}</span><strong>{suggestedCount}</strong>
            </button>
            <button aria-selected={statusFilter === "unmatched"} onClick={() => selectStatus("unmatched")} type="button">
              <span>{de ? "Umsätze zuordnen" : "Assign transactions"}</span><strong>{unmatchedCount}</strong>
            </button>
            <button aria-selected={statusFilter === "all"} onClick={() => selectStatus("all")} type="button">
              <span>{de ? "Alle Umsätze" : "All transactions"}</span><strong>{rows.length}</strong>
            </button>
          </section>

          <section className="reference-filter-row" aria-label={de ? "Filter" : "Filters"}>
            <label>
              <span>{de ? "Zeitraum" : "Period"}</span>
              <select aria-label={de ? "Zeitraum" : "Period"} value={periodFilter} onChange={(event) => setPeriodFilter(event.target.value as PeriodFilter)}>
                <option value="all">{de ? "Alle Zeiträume" : "All periods"}</option>
                <option value="today">{de ? "Heute" : "Today"}</option>
                <option value="may">Mai 2026</option>
              </select>
            </label>
            <label>
              <span>{de ? "Umsatztyp" : "Transaction type"}</span>
              <select aria-label={de ? "Umsatztyp" : "Transaction type"} value={directionFilter} onChange={(event) => setDirectionFilter(event.target.value as DirectionFilter)}>
                <option value="all">{de ? "Alle Typen" : "All types"}</option>
                <option value="incoming">{de ? "Eingänge" : "Incoming"}</option>
                <option value="outgoing">{de ? "Ausgänge" : "Outgoing"}</option>
              </select>
            </label>
            <label>
              <span>{de ? "Suche" : "Search"}</span>
              <input
                aria-label={de ? "Suche" : "Search"}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={de ? "Name, Zweck, Betrag" : "Name, purpose, amount"}
                value={query}
              />
            </label>
          </section>

          {notice ? <p className="payment-empty-state is-notice">{notice}</p> : null}
          <section className="payment-ledger-list" aria-label={de ? "Umsätze" : "Transactions"}>
            {visibleRows.length ? visibleRows.map((row) => (
              <article
                aria-current={selected?.id === row.id ? "true" : undefined}
                className={`payment-ledger-row is-${row.status.toLowerCase()}`}
                key={row.id}
                onClick={() => setSelectedId(row.id)}
              >
                <button className="payment-row-icon" type="button" aria-label={de ? "Umsatz auswählen" : "Select transaction"}>{row.amount < 0 ? "A" : "E"}</button>
                <div className="payment-row-party">
                  <strong>{row.counterparty}</strong>
                  <span>{row.bookingDate} · {row.purpose}</span>
                </div>
                <strong className={row.amount < 0 ? "is-negative" : "is-positive"}>{businessCurrency(row.amount, row.currency, locale)}</strong>
                <div className="payment-row-assignment">
                  <span>{statusLabel(row.status, locale)} · {formatConfidence(row.confidence)}</span>
                  <strong>{row.matchedLabel}</strong>
                </div>
                <div className="payment-row-actions" onClick={(event) => event.stopPropagation()}>
                  {row.status === "Suggested" ? (
                    <button disabled={runningMatchId === row.id} onClick={() => void acceptMatch(row)} type="button">
                      {runningMatchId === row.id ? (de ? "Bucht" : "Posting") : paymentActionLabel(row.nextAction, locale)}
                    </button>
                  ) : row.status === "Matched" ? (
                    <button disabled type="button">{de ? "Gebucht" : "Posted"}</button>
                  ) : row.status === "Unmatched" ? (
                    <button onClick={() => setSelectedId(row.id)} type="button">{de ? "Manuell buchen" : "Manual posting"}</button>
                  ) : null}
                  <AccountingCtoxActionButton label="CTOX" locale={locale} storyId={row.amount < 0 ? "story-20" : "story-05"} />
                </div>
              </article>
            )) : (
              <p className="payment-empty-state">{de ? "Keine Umsätze für diese Filter." : "No transactions for these filters."}</p>
            )}
          </section>

          {selected ? (
            <aside className="payment-focus-panel" aria-live="polite">
              <div>
                <span>{de ? "Ausgewählt" : "Selected"}</span>
                <strong>{selected.counterparty}</strong>
                <small>{selected.purpose}</small>
              </div>
              <dl>
                <div><dt>{de ? "Betrag" : "Amount"}</dt><dd>{businessCurrency(selected.amount, selected.currency, locale)}</dd></div>
                <div><dt>{de ? "Status" : "Status"}</dt><dd>{statusLabel(selected.status, locale)}</dd></div>
                <div><dt>{de ? "Vorschlag" : "Suggestion"}</dt><dd>{selected.matchedLabel}</dd></div>
              </dl>
              <div className="payment-focus-actions">
                {selected.status === "Suggested" ? (
                  <button disabled={runningMatchId === selected.id} onClick={() => void acceptMatch(selected)} type="button">
                    {runningMatchId === selected.id ? (de ? "Bucht" : "Posting") : paymentActionLabel(selected.nextAction, locale)}
                  </button>
                ) : selected.status === "Matched" ? (
                  <button disabled type="button">{de ? "Gebucht" : "Posted"}</button>
                ) : null}
                <AccountingCtoxActionButton label={de ? "CTOX Vorschlag" : "CTOX proposal"} locale={locale} storyId={selected.amount < 0 ? "story-20" : "story-05"} />
              </div>
              {selected.status === "Unmatched" ? (
                <form className="payment-manual-posting" onSubmit={(event) => {
                  event.preventDefault();
                  void postManualBankTransaction(selected);
                }}>
                  <label>
                    <span>{de ? "Buchungskonto" : "Posting account"}</span>
                    <select value={selectedManualAccountId} onChange={(event) => setManualAccountId(event.target.value)}>
                      {manualPostingAccounts.map((account) => (
                        <option key={account.id} value={account.id}>{account.code} {account.name}</option>
                      ))}
                    </select>
                  </label>
                  <button disabled={manualPostingId === selected.id || !selectedManualAccountId} type="submit">
                    {manualPostingId === selected.id ? (de ? "Speichert" : "Saving") : de ? "Buchung speichern" : "Save posting"}
                  </button>
                </form>
              ) : null}
            </aside>
          ) : null}
        </>
      ) : null}

      {activeTab === "transfers" ? (
        <section className="payment-simple-panel">
          <header>
            <h2>{de ? "Überweisungen" : "Transfers"}</h2>
          </header>
          {notice ? <p className="payment-empty-state is-notice">{notice}</p> : null}
          {transfers.map((row) => (
            <button className="payment-transfer-row" key={row.id} onClick={() => setSelectedId(row.id)} type="button">
              <span>{row.bookingDate}</span>
              <strong>{row.counterparty}</strong>
              <small>{row.purpose}</small>
              <b>{businessCurrency(row.amount, row.currency, locale)}</b>
            </button>
          ))}
        </section>
      ) : null}

      {activeTab === "accounts" ? (
        <section className="payment-simple-panel">
          <header>
            <h2>{de ? "Konten" : "Accounts"}</h2>
            <button onClick={() => window.location.reload()} type="button">
              {de ? "Aktualisieren" : "Refresh"}
            </button>
          </header>
          <article className="payment-account-row">
            <span>{de ? "Geschäftskonto" : "Business account"}</span>
            <strong>{businessCurrency(balance, "EUR", locale)}</strong>
            <small>{rows.length} {de ? "Umsätze geladen" : "transactions loaded"}</small>
          </article>
          <article className="payment-account-row">
            <span>PayPal</span>
            <strong>{businessCurrency(0, "EUR", locale)}</strong>
            <small>{de ? "Online" : "Online"}</small>
          </article>
          {notice ? <p className="payment-empty-state is-notice">{notice}</p> : null}
        </section>
      ) : null}
    </>
  );
}

function bankRowPriority(status: PaymentRow["status"]) {
  if (status === "Suggested") return 0;
  if (status === "Unmatched") return 1;
  if (status === "Matched") return 2;
  return 3;
}

function businessCurrency(value: number, currency: PaymentRow["currency"], locale: Locale) {
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", { currency, style: "currency" }).format(value);
}

function formatConfidence(confidence: number) {
  return `${Math.round(confidence * 100)}%`;
}

function statusLabel(status: PaymentRow["status"], locale: Locale) {
  const de = locale === "de";
  if (status === "Suggested") return de ? "Vorschlag" : "Suggested";
  if (status === "Unmatched") return de ? "Offen" : "Open";
  if (status === "Matched") return de ? "Gematcht" : "Matched";
  return de ? "Ignoriert" : "Ignored";
}

function paymentActionLabel(action: string, locale: Locale) {
  const de = locale === "de";
  if (action === "confirm_match") return de ? "Match bestätigen" : "Confirm match";
  if (action === "Confirm match") return de ? "Match bestätigen" : "Confirm match";
  if (action === "create_receipt_or_manual_posting") return de ? "Beleg oder Buchung anlegen" : "Create receipt or posting";
  if (action === "Create receipt or manual posting") return de ? "Beleg oder Buchung anlegen" : "Create receipt or posting";
  if (action === "review_match") return de ? "Match prüfen" : "Review match";
  if (action === "Review fee account") return de ? "Gebührenkonto prüfen" : action;
  if (action === "Posted") return de ? "Gebucht" : action;
  return de ? "Zuordnen" : "Assign";
}
