"use client";

import { useMemo, useState } from "react";

type InvoiceListFilter = "all" | "overdue" | "reminders" | "paid";

type InvoiceListCopy = {
  all: string;
  amount: string;
  documentList: string;
  newInvoice: string;
  overdue: string;
  paid: string;
  receivables: string;
  reminders: string;
  searchInvoices: string;
};

export type InvoiceListItem = {
  amountLabel: string;
  collectionStatus?: string;
  customerName: string;
  documentTitle: string;
  href: string;
  id: string;
  meta: string;
  reminderLevel?: number;
  searchText: string;
  status: string;
};

export type InvoiceListMetric = {
  href: string;
  label: string;
  value: string;
};

export function InvoiceListSidebar({
  copy,
  createHref,
  items,
  metrics,
  selectedInvoiceId
}: {
  copy: InvoiceListCopy;
  createHref: string;
  items: InvoiceListItem[];
  metrics: InvoiceListMetric[];
  selectedInvoiceId: string;
}) {
  const [filter, setFilter] = useState<InvoiceListFilter>("all");
  const [query, setQuery] = useState("");
  const visibleItems = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase("de-DE");
    return items.filter((item) => {
      const matchesFilter =
        filter === "all" ||
        (filter === "overdue" && item.status === "Overdue") ||
        (filter === "paid" && item.status === "Paid") ||
        (filter === "reminders" && Boolean(item.reminderLevel && item.reminderLevel > 0));
      const matchesQuery = !normalized || item.searchText.toLocaleLowerCase("de-DE").includes(normalized);
      return matchesFilter && matchesQuery;
    });
  }, [filter, items, query]);

  return (
    <aside className="ops-pane invoice-list-pane" aria-label={copy.documentList}>
      <header className="invoice-list-head">
        <h2>{copy.documentList}</h2>
        <a
          aria-label={copy.newInvoice}
          data-context-action="create"
          data-context-item
          data-context-label={copy.newInvoice}
          data-context-module="business"
          data-context-record-id="invoice"
          data-context-record-type="invoice"
          data-context-submodule="invoices"
          href={createHref}
        >
          +
        </a>
      </header>
      <div className="invoice-toolbar">
        <input
          aria-label={copy.searchInvoices}
          className="invoice-search"
          onChange={(event) => setQuery(event.target.value)}
          placeholder={copy.searchInvoices}
          type="search"
          value={query}
        />
      </div>
      <div className="invoice-filter-row" aria-label="Rechnungsfilter">
        <FilterButton active={filter === "all"} label={copy.all} onClick={() => setFilter("all")} />
        <FilterButton active={filter === "overdue"} label={copy.overdue} onClick={() => setFilter("overdue")} />
        <FilterButton active={filter === "reminders"} label={copy.reminders} onClick={() => setFilter("reminders")} />
        <FilterButton active={filter === "paid"} label={copy.paid} onClick={() => setFilter("paid")} />
      </div>
      <a className="invoice-new-row" href={createHref}>+ {copy.newInvoice}</a>
      <div className="invoice-compact-list">
        {visibleItems.map((invoice) => (
          <a
            className={`invoice-compact-row ${selectedInvoiceId === invoice.id ? "is-selected" : ""}`}
            data-context-item
            data-context-label={invoice.documentTitle}
            data-context-module="business"
            data-context-record-id={invoice.id}
            data-context-record-type="invoice"
            data-context-submodule="invoices"
            href={invoice.href}
            key={invoice.id}
          >
            <span>
              <strong>{invoice.customerName}</strong>
              <small>{invoice.documentTitle} - {invoice.meta}</small>
            </span>
            <span>
              <b>{invoice.amountLabel}</b>
              <small>{invoice.status === "Paid" ? copy.paid : invoice.collectionStatus ?? invoice.status}</small>
            </span>
          </a>
        ))}
      </div>
      <div className="invoice-list-metrics">
        {metrics.map((metric) => (
          <a className="invoice-list-metric" href={metric.href} key={metric.label}>
            <span>{metric.label}</span>
            <strong>{metric.value}</strong>
          </a>
        ))}
      </div>
    </aside>
  );
}

function FilterButton({
  active,
  label,
  onClick
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={active ? "is-active" : ""} onClick={onClick} type="button">
      {label}
    </button>
  );
}
