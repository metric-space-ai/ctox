"use client";

import { useState } from "react";
import { createPortal } from "react-dom";

type SupportedLocale = "en" | "de";

type InvoiceDeliveryCopy = {
  attachment: string;
  cancel: string;
  close: string;
  completeAndPrint: string;
  completeAndSend: string;
  draftSave: string;
  emailCopy: string;
  emailSend: string;
  emailShipping: string;
  print: string;
  recipient: string;
  sendByEmail: string;
  signature: string;
  standardTemplate: string;
  subject: string;
  template: string;
};

type InvoiceDeliveryCustomer = {
  billingEmail?: string;
  name?: string;
};

type InvoiceDeliveryInvoice = {
  documentTitle?: string;
  id: string;
  issueDate: string;
  number: string;
};

export function InvoiceDeliveryActions({
  copy,
  customer,
  invoice,
  locale
}: {
  copy: InvoiceDeliveryCopy;
  customer?: InvoiceDeliveryCustomer;
  invoice: InvoiceDeliveryInvoice;
  locale: SupportedLocale;
}) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [isEmailOpen, setIsEmailOpen] = useState(false);
  const [sendCopy, setSendCopy] = useState(true);
  const pdfHref = `/api/business/invoices/${invoice.id}/pdf?locale=${locale}`;
  const senderName = locale === "de" ? "Metric Space UG (haftungsbeschränkt)" : "Metric Space UG";
  const subject = `${invoice.documentTitle ?? (locale === "de" ? "Rechnung" : "Invoice")} ${invoice.number} ${locale === "de" ? "von" : "from"} ${senderName}`;
  const message = locale === "de"
    ? `Sehr geehrte Damen und Herren,\n\nim Anhang finden Sie Ihre Rechnung ${invoice.number} vom ${invoice.issueDate}.\n\nBei Fragen stehen wir Ihnen gerne zur Verfügung.\n\nMit freundlichen Grüßen`
    : `Dear Sir or Madam,\n\nPlease find attached invoice ${invoice.number} dated ${invoice.issueDate}.\n\nPlease contact us if you have any questions.\n\nKind regards`;

  const openPdf = () => {
    setIsMenuOpen(false);
    window.open(new URL(pdfHref, window.location.href).toString(), "_blank");
  };

  const openEmail = () => {
    setIsMenuOpen(false);
    setIsEmailOpen(true);
  };
  const emailDialog = isEmailOpen ? (
    <div className="invoice-email-overlay" role="dialog" aria-modal="true" aria-label={copy.emailShipping}>
      <button className="invoice-email-backdrop" onClick={() => setIsEmailOpen(false)} type="button" />
      <section className="invoice-email-drawer">
        <header>
          <h3>{copy.emailShipping}</h3>
          <button aria-label={copy.close} onClick={() => setIsEmailOpen(false)} type="button">x</button>
        </header>
        <label className="invoice-email-select">
          <span>{copy.recipient}</span>
          <select defaultValue={customer?.billingEmail ?? ""}>
            <option value={customer?.billingEmail ?? ""}>{customer?.billingEmail ?? customer?.name ?? copy.recipient}</option>
          </select>
        </label>
        <label className="invoice-field invoice-select-field">
          <span>{copy.template}</span>
          <select defaultValue="standard">
            <option value="standard">{copy.standardTemplate}</option>
          </select>
        </label>
        <label className="invoice-field invoice-email-subject">
          <span>{copy.subject}</span>
          <input defaultValue={subject} />
        </label>
        <label className="invoice-email-body">
          <textarea defaultValue={message} />
        </label>
        <label className="invoice-field invoice-email-signature is-muted">
          <span>{copy.signature}</span>
          <input aria-label={copy.signature} />
        </label>
        <section className="invoice-email-attachment">
          <h4>{copy.attachment}</h4>
          <a href={pdfHref} target="_blank" rel="noreferrer">
            <span>PDF</span>
            Rechnung_{invoice.number}_{invoice.issueDate}.pdf
          </a>
        </section>
        <footer>
          <label className="invoice-email-copy">
            <input checked={sendCopy} onChange={(event) => setSendCopy(event.target.checked)} type="checkbox" />
            <span>{copy.emailCopy}</span>
          </label>
          <button onClick={() => setIsEmailOpen(false)} type="button">{copy.cancel}</button>
          <button className="drawer-primary" disabled type="button">{copy.completeAndSend}</button>
        </footer>
      </section>
    </div>
  ) : null;

  return (
    <>
      <button className="invoice-save-draft" type="button">{copy.draftSave}</button>
      <div className="invoice-delivery-split">
        <button className="drawer-primary" onClick={openPdf} type="button">
          <span className="invoice-print-icon" aria-hidden="true" />
          {copy.completeAndPrint}
        </button>
        <button aria-label={copy.template} className="invoice-delivery-menu-trigger" onClick={() => setIsMenuOpen((value) => !value)} type="button">
          <span aria-hidden="true" />
        </button>
        {isMenuOpen ? (
          <div className="invoice-delivery-menu">
            <button onClick={openPdf} type="button">
              <span className="invoice-print-icon" aria-hidden="true" />
              {copy.print}
            </button>
            <button onClick={openEmail} type="button">
              <span className="invoice-send-icon" aria-hidden="true" />
              {copy.sendByEmail}
            </button>
          </div>
        ) : null}
      </div>

      {emailDialog && typeof document !== "undefined" ? createPortal(emailDialog, document.body) : null}
    </>
  );
}
