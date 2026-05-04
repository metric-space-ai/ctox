"use client";

import { useState } from "react";

export type InvoiceDocumentOption = {
  amountLabel: string;
  body: string;
  closingText: string;
  customerNumber?: string;
  dueDate?: string;
  footerLeft: string[];
  footerRight: string[];
  id: string;
  issueDate: string;
  lines: Array<{
    description?: string;
    quantity: string;
    title: string;
    total: string;
    unit: string;
    unitPrice: string;
  }>;
  meta: string;
  number: string;
  paymentTerms: string;
  recipientLines: string[];
  senderLine: string;
  senderLines: string[];
  serviceDate?: string;
  subtotalAmount: string;
  subtotalLabel: string;
  taxAmount: string;
  taxLabel: string;
  title: string;
  totalLabel: string;
  typeLabel: string;
};

export function InvoiceDocumentSelector({
  documents
}: {
  documents: InvoiceDocumentOption[];
}) {
  const [selectedId, setSelectedId] = useState(documents[0]?.id ?? "");
  const selected = documents.find((document) => document.id === selectedId) ?? documents[0];

  if (!selected) return null;

  return (
    <section className="invoice-document-stack">
      <div className="invoice-document-tabs" aria-label="Schriftstücke">
        {documents.map((document) => (
          <button
            className={document.id === selected.id ? "is-active" : ""}
            key={document.id}
            onClick={() => setSelectedId(document.id)}
            type="button"
          >
            <strong>{document.title}</strong>
            <span>{document.meta}</span>
          </button>
        ))}
      </div>
      <div className="invoice-pdf-preview-stage">
        <aside className="invoice-pdf-tools" aria-hidden="true">
          <span>+</span>
          <span>□</span>
          <span>-</span>
        </aside>
        <article className="invoice-pdf-page" aria-label={selected.title}>
          <header className="invoice-pdf-header">
            <div className="invoice-pdf-recipient">
              <small>{selected.senderLine}</small>
              {selected.recipientLines.map((line) => <span key={line}>{line}</span>)}
            </div>
            <div className="invoice-pdf-sender">
              <strong>{selected.senderLines[0]}</strong>
              {selected.senderLines.slice(1).map((line) => <span key={line}>{line}</span>)}
            </div>
            <dl className="invoice-pdf-facts">
              <div><dt>{selected.typeLabel}snr.:</dt><dd>{selected.number}</dd></div>
              {selected.customerNumber ? <div><dt>Kundennr.:</dt><dd>{selected.customerNumber}</dd></div> : null}
              <div><dt>Datum:</dt><dd>{selected.issueDate}</dd></div>
              {selected.serviceDate ? <div><dt>Lieferdatum:</dt><dd>{selected.serviceDate}</dd></div> : null}
            </dl>
          </header>
          <section className="invoice-pdf-body">
            <h3>{selected.title}</h3>
            <p>{selected.body}</p>
            <table className="invoice-pdf-table">
              <thead>
                <tr>
                  <th>Pos.</th>
                  <th>Bezeichnung</th>
                  <th>Menge</th>
                  <th>Einheit</th>
                  <th>Einzel</th>
                  <th>Gesamt</th>
                </tr>
              </thead>
              <tbody>
                {selected.lines.map((line, index) => (
                  <tr key={`${line.title}-${index}`}>
                    <td>{index + 1}</td>
                    <td><strong>{line.title}</strong>{line.description ? <span>{line.description}</span> : null}</td>
                    <td>{line.quantity}</td>
                    <td>{line.unit}</td>
                    <td>{line.unitPrice}</td>
                    <td>{line.total}</td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr><td colSpan={5}>{selected.subtotalLabel}</td><td>{selected.subtotalAmount}</td></tr>
                <tr><td colSpan={5}>{selected.taxLabel}</td><td>{selected.taxAmount}</td></tr>
                <tr><td colSpan={5}>{selected.totalLabel}</td><td>{selected.amountLabel}</td></tr>
              </tfoot>
            </table>
            <p className="invoice-pdf-payment">{selected.paymentTerms}</p>
            <p className="invoice-pdf-closing">{selected.closingText}</p>
          </section>
          <footer className="invoice-pdf-footer">
            <div>{selected.footerLeft.map((line) => <span key={line}>{line}</span>)}</div>
            <div>{selected.footerRight.map((line) => <span key={line}>{line}</span>)}</div>
            <small>Seite 1/1</small>
          </footer>
        </article>
      </div>
    </section>
  );
}
