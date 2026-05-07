import {
  buildAccountingSnapshot,
  buildLedgerRows,
  buildReceiptQueue,
  buildReconciliationRows,
  buildTrialBalance
} from "../../../../lib/accounting-runtime";
import { businessCurrency, getBusinessBundle, text } from "../../../../lib/business-seed";
import { prepareExistingInvoiceForAccounting } from "../../../../lib/business-accounting";
import { InvoiceDeliveryActions } from "../../../../components/invoice-delivery-actions";

export default async function BusinessAccountingQaPage({
  searchParams
}: {
  searchParams: Promise<{ locale?: string }>;
}) {
  const query = await searchParams;
  const locale = query.locale === "en" ? "en" : "de";
  const data = await getBusinessBundle();
  const invoice = data.invoices[0]!;
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const accounting = prepareExistingInvoiceForAccounting({ data, invoice, locale });
  const snapshot = buildAccountingSnapshot(data);
  const ledgerRows = buildLedgerRows(data).slice(0, 8);
  const trialBalance = buildTrialBalance(data).slice(0, 8);
  const receipts = buildReceiptQueue(data).slice(0, 5);
  const bankRows = buildReconciliationRows(data).slice(0, 5);

  return (
    <main className="accounting-qa-page">
      <header className="accounting-qa-header">
        <div>
          <p>{locale === "de" ? "Business Basic Buchhaltung" : "Business Basic accounting"}</p>
          <h1>{locale === "de" ? "Interaktive Accounting-QA" : "Interactive accounting QA"}</h1>
        </div>
        <InvoiceDeliveryActions
          copy={{
            attachment: locale === "de" ? "Anhang" : "Attachment",
            cancel: locale === "de" ? "Abbrechen" : "Cancel",
            close: locale === "de" ? "Schliessen" : "Close",
            completeAndPrint: locale === "de" ? "Pruefen und PDF oeffnen" : "Check and open PDF",
            completeAndSend: locale === "de" ? "Pruefen und senden" : "Check and send",
            draftSave: locale === "de" ? "Entwurf sichern" : "Save draft",
            emailCopy: locale === "de" ? "Kopie an mich" : "Send me a copy",
            emailSend: locale === "de" ? "E-Mail senden" : "Send email",
            emailShipping: locale === "de" ? "Rechnung per E-Mail" : "Invoice email",
            print: locale === "de" ? "PDF oeffnen" : "Open PDF",
            recipient: locale === "de" ? "Empfaenger" : "Recipient",
            sendByEmail: locale === "de" ? "Per E-Mail vorbereiten" : "Prepare email",
            signature: locale === "de" ? "Signatur" : "Signature",
            standardTemplate: locale === "de" ? "Standard" : "Standard",
            subject: locale === "de" ? "Betreff" : "Subject",
            template: locale === "de" ? "Vorlage" : "Template"
          }}
          customer={customer}
          invoice={invoice}
          locale={locale}
        />
      </header>

      <section className="accounting-qa-grid">
        <QaCard title="Command" value={accounting.command.type} meta={accounting.command.idempotencyKey} />
        <QaCard title="Proposal" value={accounting.proposal.status} meta={`${Math.round(accounting.proposal.confidence * 100)}% confidence`} />
        <QaCard title="Journal" value={accounting.journalDraft ? "balanced" : "blocked"} meta={accounting.validation.errors.join(", ") || "no blockers"} />
        <QaCard title="VAT payable" value={businessCurrency(snapshot.vatPayable, "EUR", locale)} meta="output minus input VAT" />
      </section>

      <section className="accounting-qa-columns">
        <QaTable
          title={locale === "de" ? "Ledger" : "Ledger"}
          rows={ledgerRows.map((row) => [row.entry.number, row.account.code, row.refLabel, businessCurrency(Math.abs(row.signedAmount), row.account.currency, locale)])}
        />
        <QaTable
          title={locale === "de" ? "Summen- und Saldenliste" : "Trial balance"}
          rows={trialBalance.map((row) => [row.account.code, row.account.name, businessCurrency(row.debit, row.account.currency, locale), businessCurrency(row.credit, row.account.currency, locale)])}
        />
        <QaTable
          title={locale === "de" ? "Eingangsbelege" : "Receipts"}
          rows={receipts.map((receipt) => [receipt.number, receipt.vendorName, receipt.status, businessCurrency(receipt.total, receipt.currency, locale)])}
        />
        <QaTable
          title={locale === "de" ? "Bankabgleich" : "Bank reconciliation"}
          rows={bankRows.map((row) => [row.bookingDate, row.counterparty, row.status, row.nextAction])}
        />
      </section>

      <section className="accounting-qa-document">
        <h2>{invoice.documentTitle ?? (locale === "de" ? "Rechnung" : "Invoice")} {invoice.number}</h2>
        <p>{text(invoice.introText ?? invoice.notes, locale)}</p>
        <dl>
          <div><dt>{locale === "de" ? "Kunde" : "Customer"}</dt><dd>{customer?.name ?? invoice.customerId}</dd></div>
          <div><dt>{locale === "de" ? "Netto" : "Net"}</dt><dd>{businessCurrency(invoice.netAmount ?? invoice.total - invoice.taxAmount, invoice.currency, locale)}</dd></div>
          <div><dt>{locale === "de" ? "Steuer" : "Tax"}</dt><dd>{businessCurrency(invoice.taxAmount, invoice.currency, locale)}</dd></div>
          <div><dt>{locale === "de" ? "Brutto" : "Gross"}</dt><dd>{businessCurrency(invoice.total, invoice.currency, locale)}</dd></div>
        </dl>
      </section>
    </main>
  );
}

function QaCard({ meta, title, value }: { meta: string; title: string; value: string }) {
  return (
    <article className="accounting-qa-card">
      <span>{title}</span>
      <strong>{value}</strong>
      <small>{meta}</small>
    </article>
  );
}

function QaTable({ rows, title }: { rows: string[][]; title: string }) {
  return (
    <article className="accounting-qa-table">
      <h2>{title}</h2>
      <table>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={`${title}-${rowIndex}-${row.join(":")}`}>
              {row.map((cell, cellIndex) => <td key={`${rowIndex}-${cellIndex}`}>{cell}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </article>
  );
}
