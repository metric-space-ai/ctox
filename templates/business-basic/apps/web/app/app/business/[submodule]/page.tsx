import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { findBusinessModule, findBusinessSubmodule, WorkSurface } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { AccountingApiButton } from "../../../../components/accounting-api-button";
import { AccountingCommandButton } from "../../../../components/accounting-command-button";
import { BankImportPreviewButton } from "../../../../components/bank-import-preview-button";
import { DunningPreviewButton } from "../../../../components/dunning-preview-button";
import { InvoiceDeliveryActions } from "../../../../components/invoice-delivery-actions";
import { ReceiptIngestButton } from "../../../../components/receipt-ingest-button";
import {
  buildAccountingSnapshot,
  buildDatevLines,
  buildLedgerRows,
  buildReceiptQueue,
  buildReconciliationRows,
  buildTrialBalance
} from "../../../../lib/accounting-runtime";
import { businessOsName, companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";
import { businessCurrency, getBusinessBundle, text, type BusinessBundle, type SupportedLocale } from "../../../../lib/business-seed";
import { prepareExistingInvoiceForAccounting } from "../../../../lib/business-accounting";

export default async function BusinessSubmodulePage({
  params,
  searchParams
}: {
  params: Promise<{ submodule: string }>;
  searchParams: Promise<{ locale?: string; theme?: string }>;
}) {
  const { submodule: submoduleId } = await params;
  const query = await searchParams;
  const locale: SupportedLocale = query.locale === "en" ? "en" : "de";
  const module = findBusinessModule("business");
  const submodule = findBusinessSubmodule("business", submoduleId);
  if (!module || !submodule) notFound();

  const cookieStore = await cookies();
  const companyName = normalizeCompanyName(cookieStore.get(companyNameCookieName)?.value);
  const data = await getBusinessBundle();

  return (
    <AppShell
      brandName={businessOsName(companyName)}
      currentHref={`/app/business/${submoduleId}?locale=${locale}${query.theme ? `&theme=${query.theme}` : ""}`}
      locale={locale}
      moduleId="business"
      submoduleId={submoduleId}
      theme={query.theme}
    >
      <WorkSurface
        hideHeader
        moduleId="business"
        submoduleId={submoduleId}
        title={submodule.label}
        description={`${module.label} workspace`}
      >
        <BusinessAccountingSurface data={data} locale={locale} submoduleId={submoduleId} />
      </WorkSurface>
    </AppShell>
  );
}

function BusinessAccountingSurface({ data, locale, submoduleId }: { data: BusinessBundle; locale: SupportedLocale; submoduleId: string }) {
  const invoice = data.invoices[0]!;
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const accounting = prepareExistingInvoiceForAccounting({ data, invoice, locale });
  const snapshot = buildAccountingSnapshot(data);
  const ledgerRows = buildLedgerRows(data);
  const trialBalance = buildTrialBalance(data);
  const receipts = buildReceiptQueue(data);
  const bankRows = buildReconciliationRows(data);
  const datevLines = buildDatevLines(data);
  const receiptToPost = receipts.find((receipt) => receipt.status === "Needs review" || receipt.status === "Inbox") ?? receipts[0];
  const transactionToMatch = bankRows.find((row) => row.status === "Suggested") ?? bankRows[0];
  const exportBatch = data.bookkeeping[0];
  const title = titleForSubmodule(submoduleId, locale);

  return (
    <main className="business-accounting-page">
      <header className="business-accounting-header">
        <div>
          <p>{locale === "de" ? "Business Basic" : "Business Basic"}</p>
          <h1>{title}</h1>
        </div>
        {submoduleId === "invoices" ? (
          <InvoiceDeliveryActions
            copy={deliveryCopy(locale)}
            customer={customer}
            invoice={invoice}
            locale={locale}
          />
        ) : null}
        {submoduleId === "receipts" && receiptToPost ? (
          <div className="business-accounting-header-actions">
            <ReceiptIngestButton label={locale === "de" ? "OCR vorbereiten" : "Prepare OCR"} path={`/api/business/receipts/${receiptToPost.id}/ingest`} />
            <AccountingCommandButton action="post" label={locale === "de" ? "Beleg buchen vorbereiten" : "Prepare receipt posting"} recordId={receiptToPost.id} resource="receipts" />
          </div>
        ) : null}
        {submoduleId === "payments" && transactionToMatch ? (
          <AccountingCommandButton action="match" label={locale === "de" ? "Bankmatch vorbereiten" : "Prepare bank match"} recordId={transactionToMatch.id} resource="bank-transactions" />
        ) : null}
        {submoduleId === "bookkeeping" && exportBatch ? (
          <div className="business-accounting-header-actions">
            <AccountingApiButton label={locale === "de" ? "Setup vorbereiten" : "Prepare setup"} path="/api/business/accounting/setup" />
            <BankImportPreviewButton label={locale === "de" ? "Bankimport pruefen" : "Check bank import"} />
            <a className="business-accounting-download" href="/api/business/accounting/datev-export">
              {locale === "de" ? "DATEV CSV laden" : "Download DATEV CSV"}
            </a>
            <AccountingCommandButton action="export" label={locale === "de" ? "DATEV Export vorbereiten" : "Prepare DATEV export"} recordId={exportBatch.id} resource="bookkeeping" />
          </div>
        ) : null}
        {submoduleId === "reports" ? (
          <div className="business-accounting-header-actions">
            <AccountingApiButton label={locale === "de" ? "Periode schliessen" : "Close period"} path="/api/business/accounting/period-close" />
            <DunningPreviewButton label={locale === "de" ? "Mahnlauf pruefen" : "Check dunning run"} />
          </div>
        ) : null}
      </header>

      <nav className="business-accounting-tabs" aria-label="Business accounting">
        {["invoices", "ledger", "receipts", "payments", "bookkeeping", "reports"].map((id) => (
          <a aria-current={id === submoduleId ? "page" : undefined} href={`/app/business/${id}?locale=${locale}`} key={id}>
            {titleForSubmodule(id, locale)}
          </a>
        ))}
      </nav>

      <section className="accounting-qa-grid">
        <QaCard title="Receivables" value={businessCurrency(snapshot.receivableBalance, "EUR", locale)} meta="posted AR" />
        <QaCard title="Payables" value={businessCurrency(snapshot.payableBalance, "EUR", locale)} meta="posted AP" />
        <QaCard title="VAT payable" value={businessCurrency(snapshot.vatPayable, "EUR", locale)} meta="output minus input VAT" />
        <QaCard title="Proposal" value={accounting.proposal.status} meta={`${Math.round(accounting.proposal.confidence * 100)}% confidence`} />
      </section>

      {submoduleId === "invoices" ? (
        <section className="accounting-qa-document">
          <h2>{invoice.documentTitle ?? titleForSubmodule("invoices", locale)} {invoice.number}</h2>
          <p>{text(invoice.introText ?? invoice.notes, locale)}</p>
          <dl>
            <div><dt>{locale === "de" ? "Kunde" : "Customer"}</dt><dd>{customer?.name ?? invoice.customerId}</dd></div>
            <div><dt>Command</dt><dd>{accounting.command.type}</dd></div>
            <div><dt>Journal</dt><dd>{accounting.journalDraft ? "balanced" : "blocked"}</dd></div>
            <div><dt>{locale === "de" ? "Brutto" : "Gross"}</dt><dd>{businessCurrency(invoice.total, invoice.currency, locale)}</dd></div>
          </dl>
          <p><a className="business-accounting-inline-link" href={`/api/business/invoices/${invoice.id}/zugferd?locale=${locale}`}>ZUGFeRD XML</a></p>
        </section>
      ) : null}

      <section className="accounting-qa-columns">
        {(submoduleId === "ledger" || submoduleId === "bookkeeping" || submoduleId === "reports") ? (
          <>
            <QaTable
              title={locale === "de" ? "Ledger" : "Ledger"}
              rows={ledgerRows.slice(0, 10).map((row) => [row.entry.number, row.account.code, row.refLabel, businessCurrency(Math.abs(row.signedAmount), row.account.currency, locale)])}
            />
            <QaTable
              title={locale === "de" ? "Summen- und Saldenliste" : "Trial balance"}
              rows={trialBalance.slice(0, 10).map((row) => [row.account.code, row.account.name, businessCurrency(row.debit, row.account.currency, locale), businessCurrency(row.credit, row.account.currency, locale)])}
            />
          </>
        ) : null}

        {submoduleId === "receipts" ? (
          <QaTable
            title={locale === "de" ? "Eingangsbelege" : "Receipts"}
            rows={receipts.map((receipt) => [receipt.number, receipt.vendorName, receipt.status, businessCurrency(receipt.total, receipt.currency, locale)])}
          />
        ) : null}

        {submoduleId === "payments" ? (
          <QaTable
            title={locale === "de" ? "Bankabgleich" : "Bank reconciliation"}
            rows={bankRows.map((row) => [row.bookingDate, row.counterparty, row.status, row.nextAction])}
          />
        ) : null}

        {submoduleId === "bookkeeping" ? (
          <QaTable
            title="DATEV"
            rows={datevLines.slice(0, 10).map((line) => [line.entry.number, line.account.code, line.side, businessCurrency(line.amount, line.account.currency, locale)])}
          />
        ) : null}

        {submoduleId === "invoices" ? (
          <QaTable
            title={locale === "de" ? "Rechnungen" : "Invoices"}
            rows={data.invoices.map((item) => [item.number, item.status, item.dueDate, businessCurrency(item.total, item.currency, locale)])}
          />
        ) : null}
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

function titleForSubmodule(submoduleId: string, locale: SupportedLocale) {
  const de: Record<string, string> = {
    bookkeeping: "Buchhaltung",
    invoices: "Rechnungen",
    ledger: "Ledger",
    payments: "Zahlungen",
    receipts: "Eingangsbelege",
    reports: "Berichte"
  };
  const en: Record<string, string> = {
    bookkeeping: "Bookkeeping",
    invoices: "Invoices",
    ledger: "Ledger",
    payments: "Payments",
    receipts: "Receipts",
    reports: "Reports"
  };
  return (locale === "de" ? de : en)[submoduleId] ?? submoduleId;
}

function deliveryCopy(locale: SupportedLocale) {
  return {
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
    standardTemplate: "Standard",
    subject: locale === "de" ? "Betreff" : "Subject",
    template: locale === "de" ? "Vorlage" : "Template"
  };
}
