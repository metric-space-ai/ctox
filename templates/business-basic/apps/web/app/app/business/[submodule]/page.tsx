import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { findBusinessModule, findBusinessSubmodule, WorkSurface, type WorkSurfacePanelState } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { AccountingApiButton } from "../../../../components/accounting-api-button";
import { AccountingCommandButton } from "../../../../components/accounting-command-button";
import { AccountingWorkflowPanel } from "../../../../components/accounting-workflow-panel";
import { BankImportPreviewButton } from "../../../../components/bank-import-preview-button";
import { BusinessPanel, BusinessWorkspace } from "../../../../components/business-workspace";
import { DatevExportButton } from "../../../../components/datev-export-button";
import { DunningPreviewButton } from "../../../../components/dunning-preview-button";
import { InvoiceDeliveryActions } from "../../../../components/invoice-delivery-actions";
import { ReceiptIngestButton } from "../../../../components/receipt-ingest-button";
import {
  buildAccountingSnapshot,
  buildBalanceSheet,
  buildBusinessAnalysis,
  buildDatevLines,
  buildFiscalPeriodState,
  buildFixedAssetRegister,
  buildLedgerRows,
  buildProfitAndLoss,
  buildReceiptQueue,
  buildReconciliationRows,
  buildTrialBalance,
  buildVatStatement
} from "../../../../lib/accounting-runtime";
import { businessOsName, companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";
import { businessCurrency, getBusinessBundle, text, type BusinessBundle, type SupportedLocale } from "../../../../lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "../../../../lib/business-db-bundle";
import { prepareExistingInvoiceForAccounting } from "../../../../lib/business-accounting";

export default async function BusinessSubmodulePage({
  params,
  searchParams
}: {
  params: Promise<{ submodule: string }>;
  searchParams: Promise<{ drawer?: string; locale?: string; orderSearch?: string; panel?: string; recordId?: string; selectedId?: string; theme?: string; warehouseSearch?: string }>;
}) {
  const { submodule: submoduleId } = await params;
  const query = await searchParams;
  const locale: SupportedLocale = query.locale === "en" ? "en" : "de";
  const module = findBusinessModule("business");
  const submodule = findBusinessSubmodule("business", submoduleId);
  if (!module || !submodule) notFound();

  const cookieStore = await cookies();
  const companyName = normalizeCompanyName(cookieStore.get(companyNameCookieName)?.value);
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const drawer: WorkSurfacePanelState["drawer"] = query.drawer === "left-bottom" || query.drawer === "bottom" || query.drawer === "right" ? query.drawer : undefined;
  const panelState: WorkSurfacePanelState = {
    drawer,
    panel: query.panel,
    recordId: query.recordId
  };
  const viewQuery = {
    drawer: query.drawer,
    locale: query.locale,
    orderSearch: query.orderSearch,
    panel: query.panel,
    recordId: query.recordId,
    selectedId: query.selectedId,
    theme: query.theme,
    warehouseSearch: query.warehouseSearch
  };

  if (submoduleId === "warehouse" || submoduleId === "fulfillment") {
    return (
      <AppShell
        brandName={businessOsName(companyName)}
        currentHref={businessCurrentHref(submoduleId, query)}
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
          panelState={panelState}
          panelContent={<BusinessPanel panelState={panelState} query={viewQuery} submoduleId={submoduleId} />}
        >
          <BusinessWorkspace query={viewQuery} submoduleId={submoduleId} />
        </WorkSurface>
      </AppShell>
    );
  }

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
        panelContent={<BusinessPanel panelState={panelState} query={viewQuery} submoduleId={submoduleId} />}
        panelState={panelState}
        submoduleId={submoduleId}
        title={submodule.label}
        description={`${module.label} workspace`}
      >
        <BusinessAccountingSurface data={data} locale={locale} submoduleId={submoduleId} />
      </WorkSurface>
    </AppShell>
  );
}

function businessCurrentHref(submoduleId: string, query: { drawer?: string; locale?: string; orderSearch?: string; panel?: string; recordId?: string; selectedId?: string; theme?: string; warehouseSearch?: string }) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.orderSearch) params.set("orderSearch", query.orderSearch);
  if (query.warehouseSearch) params.set("warehouseSearch", query.warehouseSearch);
  if (query.panel) params.set("panel", query.panel);
  if (query.recordId) params.set("recordId", query.recordId);
  if (query.selectedId) params.set("selectedId", query.selectedId);
  if (query.drawer) params.set("drawer", query.drawer);
  const queryString = params.toString();
  return queryString ? `/app/business/${submoduleId}?${queryString}` : `/app/business/${submoduleId}`;
}

function BusinessAccountingSurface({ data, locale, submoduleId }: { data: BusinessBundle; locale: SupportedLocale; submoduleId: string }) {
  const invoice = data.invoices[0]!;
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const accounting = prepareExistingInvoiceForAccounting({ data, invoice, locale });
  const snapshot = buildAccountingSnapshot(data);
  const ledgerRows = buildLedgerRows(data);
  const trialBalance = buildTrialBalance(data);
  const balanceSheet = buildBalanceSheet(data);
  const businessAnalysis = buildBusinessAnalysis(data);
  const profitAndLoss = buildProfitAndLoss(data);
  const vatStatement = buildVatStatement(data);
  const fiscalPeriods = buildFiscalPeriodState(data);
  const fixedAssets = buildFixedAssetRegister(data);
  const receipts = buildReceiptQueue(data);
  const bankRows = buildReconciliationRows(data);
  const datevLines = buildDatevLines(data);
  const receiptToPost = receipts.find((receipt) => receipt.status === "Needs review" || receipt.status === "Inbox") ?? receipts[0];
  const assetToDepreciate = fixedAssets.find((asset) => asset.status === "Active") ?? fixedAssets[0];
  const transactionToMatch = bankRows.find((row) => row.status === "Suggested") ?? bankRows[0];
  const exportBatch = data.bookkeeping[0];
  const openInvoices = data.invoices.filter((item) => item.status !== "Paid");
  const overdueInvoices = data.invoices.filter((item) => item.status === "Overdue");
  const openReceipts = receipts.filter((item) => item.status === "Needs review" || item.status === "Inbox");
  const openBankRows = bankRows.filter((item) => item.status !== "Matched");
  const financeRows = financeRowsForSubmodule({
    bankRows,
    data,
    datevLines,
    fiscalPeriods,
    ledgerRows,
    balanceSheet,
    businessAnalysis,
    locale,
    fixedAssets,
    profitAndLoss,
    receipts,
    submoduleId,
    trialBalance,
    vatStatement
  });
  const nextStep = nextStepForSubmodule({
    copyLocale: locale,
    customer,
    exportBatch,
    invoice,
    openBankRows,
    openInvoices,
    openReceipts,
    receiptToPost,
    submoduleId,
    transactionToMatch
  });
  const title = titleForSubmodule(submoduleId, locale);
  const summaryItems = [
    [locale === "de" ? "Forderungen" : "Receivables", businessCurrency(snapshot.receivableBalance, "EUR", locale), `${openInvoices.length} ${locale === "de" ? "offen" : "open"}`],
    [locale === "de" ? "Verbindlichkeiten" : "Payables", businessCurrency(snapshot.payableBalance, "EUR", locale), `${openReceipts.length} ${locale === "de" ? "zu prüfen" : "to review"}`],
    [locale === "de" ? "USt Zahllast" : "VAT payable", businessCurrency(snapshot.vatPayable, "EUR", locale), locale === "de" ? "Ausgang minus Eingang" : "Output minus input"],
    submoduleId === "reports"
      ? [locale === "de" ? "Abschluss" : "Close", fiscalPeriods.nextClosablePeriod?.id ?? "-", fiscalPeriods.lastClosedPeriod ? `${locale === "de" ? "zuletzt" : "last"} ${fiscalPeriods.lastClosedPeriod.id}` : `${fiscalPeriods.closedCount}/${fiscalPeriods.periodCount}`]
      : [locale === "de" ? "Bankklaerung" : "Bank review", String(openBankRows.length), `${bankRows.filter((row) => row.status === "Matched").length} ${locale === "de" ? "gematcht" : "matched"}`]
  ] satisfies Array<[string, string, string]>;

  return (
    <main className="business-accounting-page">
      <header className="business-accounting-header finance-page-header">
        <div>
          <p>{locale === "de" ? "Business Basic" : "Business Basic"}</p>
          <h1>{title}</h1>
          <span>{submoduleLead(submoduleId, locale)}</span>
        </div>
        <div className="finance-header-actions" aria-label={locale === "de" ? "Hauptaktionen" : "Primary actions"}>
          <a className="finance-secondary-action" href={`/app/business/${submoduleId}?locale=${locale}`}>{locale === "de" ? "Aktualisieren" : "Refresh"}</a>
          <FinancePrimaryActions
            accounting={accounting}
            assetToDepreciate={assetToDepreciate}
            customer={customer}
            invoice={invoice}
            locale={locale}
            receiptToPost={receiptToPost}
            submoduleId={submoduleId}
            transactionToMatch={transactionToMatch}
          />
        </div>
      </header>

      <nav className="business-accounting-tabs" aria-label="Business accounting">
        {["invoices", "ledger", "fixed-assets", "receipts", "payments", "bookkeeping", "reports"].map((id) => (
          <a aria-current={id === submoduleId ? "page" : undefined} href={`/app/business/${id}?locale=${locale}`} key={id}>
            {titleForSubmodule(id, locale)}
          </a>
        ))}
      </nav>

      <section className="finance-workbench" aria-label={locale === "de" ? "Finanzarbeitsbereich" : "Finance workspace"}>
        <div className="finance-summary-strip" aria-label={locale === "de" ? "Finanzlage" : "Financial state"}>
          {summaryItems.map(([label, value, meta]) => (
            <div key={label}>
              <span>{label}</span>
              <strong>{value}</strong>
              <small>{meta}</small>
            </div>
          ))}
        </div>
        <FinanceModeTabs modes={financeModesForSubmodule({ balanceSheet, bankRows, data, fiscalPeriods, fixedAssets, locale, receipts, submoduleId, vatStatement })} />
        <FinanceFilters locale={locale} submoduleId={submoduleId} />
        <div className={`finance-workbench-body ${usesDocumentRail(submoduleId) ? "with-document-rail" : ""}`}>
          {usesDocumentRail(submoduleId) ? (
            <FinanceDocumentRail data={data} locale={locale} receipts={receipts} submoduleId={submoduleId} />
          ) : null}
          <FinanceRows
            caption={tableCaptionForSubmodule(submoduleId, locale)}
            rows={financeRows}
            title={worklistTitleForSubmodule(submoduleId, locale)}
          />
          <aside className="finance-inspector" aria-label={locale === "de" ? "Details und Review" : "Details and review"}>
            <FinanceInspector nextStep={nextStep} />
            <FinanceInlineActions
              accounting={accounting}
              assetToDepreciate={assetToDepreciate}
              customer={customer}
              invoice={invoice}
              locale={locale}
              receiptToPost={receiptToPost}
              submoduleId={submoduleId}
              transactionToMatch={transactionToMatch}
            />
            <AccountingWorkflowPanel compact locale={locale} />
          </aside>
        </div>
      </section>
    </main>
  );
}

type FinanceMode = {
  count?: number;
  label: string;
  tone: "active" | "attention" | "muted";
};

type FinanceRow = {
  action: string;
  amount?: string;
  amountTone?: "positive" | "negative" | "neutral";
  detail: string;
  id: string;
  marker: string;
  meta: string;
  tone?: "focus" | "review" | "quiet";
  status: string;
  title: string;
};

function FinanceModeTabs({ modes }: { modes: FinanceMode[] }) {
  return (
    <div className="finance-mode-tabs" role="tablist" aria-label="Finance modes">
      {modes.map((mode, index) => (
        <button aria-selected={index === 0} className={`finance-mode-tab tone-${mode.tone}`} key={mode.label} role="tab" type="button">
          <span>{mode.label}</span>
          {typeof mode.count === "number" ? <strong>{mode.count}</strong> : null}
        </button>
      ))}
    </div>
  );
}

function FinanceFilters({ locale, submoduleId }: { locale: SupportedLocale; submoduleId: string }) {
  const de = locale === "de";
  const labels = submoduleId === "payments" || submoduleId === "bookkeeping"
    ? [de ? "Bankkonto" : "Bank account", de ? "Zeitraum" : "Period", de ? "Umsatztyp" : "Transaction type", de ? "Suche" : "Search"]
    : [de ? "Status" : "Status", de ? "Zeitraum" : "Period", de ? "Kontakt" : "Contact", de ? "Suche" : "Search"];
  return (
    <div className="finance-filter-row" aria-label={de ? "Filter" : "Filters"}>
      {labels.map((label, index) => (
        <label key={label}>
          <span>{label}</span>
          <input
            aria-label={label}
            placeholder={index === labels.length - 1
              ? (de ? "Name, Zweck, Betrag" : "Name, purpose, amount")
              : (de ? `${label} wählen` : `Choose ${label.toLowerCase()}`)}
            readOnly
          />
        </label>
      ))}
    </div>
  );
}

function FinanceDocumentRail({
  data,
  locale,
  receipts,
  submoduleId
}: {
  data: BusinessBundle;
  locale: SupportedLocale;
  receipts: ReturnType<typeof buildReceiptQueue>;
  submoduleId: string;
}) {
  const de = locale === "de";
  const openInvoices = data.invoices.filter((invoice) => invoice.status !== "Paid");
  const overdueInvoices = data.invoices.filter((invoice) => invoice.status === "Overdue");
  const reviewReceipts = receipts.filter((receipt) => receipt.status === "Needs review" || receipt.status === "Inbox");
  const postedReceipts = receipts.filter((receipt) => receipt.status === "Posted" || receipt.status === "Paid");
  const items = submoduleId === "receipts"
    ? [
        [de ? "Alle Belege" : "All receipts", receipts.length],
        [de ? "Eingang" : "Inbox", reviewReceipts.length],
        [de ? "Gebucht" : "Posted", postedReceipts.length],
        [de ? "Abgelehnt" : "Rejected", receipts.filter((receipt) => receipt.status === "Rejected").length]
      ]
    : [
        [de ? "Alle Rechnungen" : "All invoices", data.invoices.length],
        [de ? "Offene" : "Open", openInvoices.length],
        [de ? "Überfällig" : "Overdue", overdueInvoices.length],
        [de ? "Entwürfe" : "Drafts", data.invoices.filter((invoice) => invoice.status === "Draft").length]
      ];
  return (
    <nav className="finance-document-rail" aria-label={de ? "Dokumentfilter" : "Document filters"}>
      {items.map(([label, count], index) => (
        <button aria-pressed={index === 0} key={label} type="button">
          <span>{label}</span>
          <strong>{count}</strong>
        </button>
      ))}
    </nav>
  );
}

function FinanceRows({ caption, rows, title }: { caption?: string; rows: FinanceRow[]; title: string }) {
  return (
    <article className="finance-row-board">
      <header>
        <div>
          <h2>{title}</h2>
          {caption ? <p>{caption}</p> : null}
        </div>
        <button type="button">Sortieren</button>
      </header>
      <div className="finance-row-board-head" aria-hidden="true">
        <span>Auswahl</span>
        <span>Vorgang</span>
        <span>Status</span>
        <span>Betrag</span>
        <span>Aktion</span>
      </div>
      <ul>
        {rows.map((row, index) => {
          const selected = index === 0;
          return (
          <li aria-current={selected ? "true" : undefined} className={`${selected ? "is-selected" : ""} is-${row.tone ?? "quiet"}`} key={row.id}>
            <label className="finance-row-check">
              <input aria-label={row.title} type="checkbox" defaultChecked={index === 0} />
            </label>
            <span className="finance-row-marker" aria-hidden="true">{row.marker}</span>
            <div className="finance-row-main">
              <strong>{row.title}</strong>
              <span>{row.meta}</span>
              <small>{row.detail}</small>
            </div>
            <span className="finance-row-status">{row.status}</span>
            {row.amount ? <strong className={`finance-row-amount tone-${row.amountTone ?? "neutral"}`}>{row.amount}</strong> : <span />}
            {selected ? (
              <button className="finance-row-action" type="button">{row.action}</button>
            ) : (
              <span className="finance-row-action is-passive">{row.action}</span>
            )}
          </li>
          );
        })}
      </ul>
    </article>
  );
}

function FinanceInspector({ nextStep }: { nextStep: BusinessNextStep }) {
  return (
    <section className="finance-inspector-card">
      <p>{nextStep.eyebrow}</p>
      <h2>{nextStep.title}</h2>
      <span>{nextStep.description}</span>
      <dl>
        {nextStep.facts.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function FinancePrimaryActions(props: FinanceActionProps) {
  if (props.submoduleId === "receipts" || props.submoduleId === "bookkeeping") {
    return (
      <div className="finance-primary-actions">
        <a className="finance-secondary-action is-primary" href="#finance-inspector-actions">
          {props.submoduleId === "receipts"
            ? (props.locale === "de" ? "Neuer Beleg" : "New receipt")
            : (props.locale === "de" ? "Import / Export" : "Import / export")}
        </a>
      </div>
    );
  }

  return (
    <div className="finance-primary-actions">
      <FinanceInlineActions {...props} compact />
    </div>
  );
}

type FinanceActionProps = {
  accounting: ReturnType<typeof prepareExistingInvoiceForAccounting>;
  assetToDepreciate: ReturnType<typeof buildFixedAssetRegister>[number] | undefined;
  customer: BusinessBundle["customers"][number] | undefined;
  invoice: BusinessBundle["invoices"][number];
  locale: SupportedLocale;
  receiptToPost: ReturnType<typeof buildReceiptQueue>[number] | undefined;
  submoduleId: string;
  transactionToMatch: ReturnType<typeof buildReconciliationRows>[number] | undefined;
};

function FinanceInlineActions({
  accounting,
  assetToDepreciate,
  customer,
  invoice,
  locale,
  receiptToPost,
  submoduleId,
  transactionToMatch,
  compact = false
}: FinanceActionProps & { compact?: boolean }) {
  return (
    <div className={`finance-inline-actions ${compact ? "is-compact" : ""}`} id={compact ? undefined : "finance-inspector-actions"}>
      {submoduleId === "invoices" ? (
        <>
          <InvoiceDeliveryActions copy={deliveryCopy(locale)} customer={customer} invoice={invoice} locale={locale} />
          {!compact ? <a className="business-accounting-inline-link" href={`/api/business/invoices/${invoice.id}/zugferd?locale=${locale}`}>ZUGFeRD XML</a> : null}
        </>
      ) : null}
      {submoduleId === "receipts" && receiptToPost ? (
        <>
          <ReceiptIngestButton label={locale === "de" ? "OCR vorbereiten" : "Prepare OCR"} path={`/api/business/receipts/${receiptToPost.id}/ingest`} />
          {!compact ? <AccountingCommandButton action="post" label={locale === "de" ? "Buchung vorbereiten" : "Prepare posting"} recordId={receiptToPost.id} resource="receipts" /> : null}
          {!compact ? <AccountingCommandButton action="capitalize" label={locale === "de" ? "Als Anlage aktivieren" : "Capitalize as asset"} recordId={receiptToPost.id} resource="receipts" /> : null}
        </>
      ) : null}
      {submoduleId === "fixed-assets" && assetToDepreciate ? (
        <>
          <AccountingCommandButton action="depreciate" label={locale === "de" ? "AfA buchen" : "Post depreciation"} recordId={assetToDepreciate.id} resource="fixed-assets" />
          {!compact ? <AccountingCommandButton action="dispose" label={locale === "de" ? "Abgang vorbereiten" : "Prepare disposal"} recordId={assetToDepreciate.id} resource="fixed-assets" /> : null}
        </>
      ) : null}
      {submoduleId === "payments" && transactionToMatch ? (
        <AccountingCommandButton action="match" label={locale === "de" ? "Match prüfen" : "Review match"} recordId={transactionToMatch.id} resource="bank-transactions" />
      ) : null}
      {submoduleId === "bookkeeping" ? (
        <>
          <BankImportPreviewButton label={locale === "de" ? "Bankimport" : "Bank import"} />
          {!compact ? <DatevExportButton label={locale === "de" ? "DATEV CSV" : "DATEV CSV"} /> : null}
        </>
      ) : null}
      {submoduleId === "ledger" ? (
        <AccountingApiButton label={locale === "de" ? "Setup prüfen" : "Check setup"} path="/api/business/accounting/setup" />
      ) : null}
      {submoduleId === "reports" ? (
        <>
          <AccountingApiButton label={locale === "de" ? "Periode schließen" : "Close period"} path="/api/business/accounting/period-close" />
          {!compact ? <DunningPreviewButton label={locale === "de" ? "Mahnlauf" : "Dunning"} /> : null}
        </>
      ) : null}
      {submoduleId === "invoices" && accounting.validation.errors.length && !compact ? (
        <small>{accounting.validation.errors.join(", ")}</small>
      ) : null}
    </div>
  );
}

function usesDocumentRail(submoduleId: string) {
  return submoduleId === "invoices" || submoduleId === "receipts";
}

function financeModesForSubmodule({
  balanceSheet,
  bankRows,
  data,
  fixedAssets,
  fiscalPeriods,
  locale,
  receipts,
  submoduleId,
  vatStatement
}: {
  balanceSheet: ReturnType<typeof buildBalanceSheet>;
  bankRows: ReturnType<typeof buildReconciliationRows>;
  data: BusinessBundle;
  fixedAssets: ReturnType<typeof buildFixedAssetRegister>;
  fiscalPeriods: ReturnType<typeof buildFiscalPeriodState>;
  locale: SupportedLocale;
  receipts: ReturnType<typeof buildReceiptQueue>;
  submoduleId: string;
  vatStatement: ReturnType<typeof buildVatStatement>;
}): FinanceMode[] {
  const de = locale === "de";
  if (submoduleId === "payments") {
    return [
      { count: bankRows.filter((row) => row.status === "Suggested").length, label: de ? "Vorschläge prüfen" : "Review proposals", tone: "active" },
      { count: bankRows.filter((row) => row.status === "Unmatched").length, label: de ? "Umsaetze zuordnen" : "Assign transactions", tone: "attention" },
      { count: bankRows.length, label: de ? "Alle Umsaetze" : "All transactions", tone: "muted" }
    ];
  }
  if (submoduleId === "receipts") {
    return [
      { count: receipts.filter((row) => row.status === "Needs review").length, label: de ? "Zu prüfen" : "Review", tone: "active" },
      { count: receipts.filter((row) => row.status === "Inbox").length, label: de ? "Eingang" : "Inbox", tone: "attention" },
      { count: receipts.length, label: de ? "Alle Eingangsbelege" : "All inbound receipts", tone: "muted" }
    ];
  }
  if (submoduleId === "invoices") {
    return [
      { count: data.invoices.filter((row) => row.status !== "Paid").length, label: de ? "Offene Rechnungen" : "Open invoices", tone: "active" },
      { count: data.invoices.filter((row) => row.status === "Overdue").length, label: de ? "Überfällig" : "Overdue", tone: "attention" },
      { count: data.invoices.length, label: de ? "Alle Ausgangsbelege" : "All outgoing docs", tone: "muted" }
    ];
  }
  if (submoduleId === "bookkeeping") {
    return [
      { count: data.bookkeeping.filter((row) => row.status !== "Exported").length, label: "DATEV", tone: "active" },
      { count: bankRows.filter((row) => row.status !== "Matched").length, label: de ? "Bankimport" : "Bank import", tone: "attention" },
      { count: data.journalEntries.filter((entry) => entry.status === "Posted").length, label: de ? "Journal" : "Journal", tone: "muted" }
    ];
  }
  if (submoduleId === "fixed-assets") {
    return [
      { count: fixedAssets.filter((asset) => asset.status === "Active").length, label: de ? "Aktive Anlagen" : "Active assets", tone: "active" },
      { count: fixedAssets.filter((asset) => asset.currentYearDepreciation > 0).length, label: de ? "AfA gebucht" : "Depreciated", tone: "muted" },
      { count: fixedAssets.length, label: de ? "Anlagenregister" : "Asset register", tone: "muted" }
    ];
  }
  if (submoduleId === "reports") {
    return [
      { count: balanceSheet.balanced ? 0 : 1, label: de ? "Bilanz" : "Balance sheet", tone: "active" },
      { count: vatStatement.payable > 0 ? 1 : 0, label: "UStVA", tone: vatStatement.payable > 0 ? "attention" : "muted" },
      { count: fiscalPeriods.closedCount, label: de ? "Perioden zu" : "Periods closed", tone: "muted" }
    ];
  }
  return [
    { count: data.journalEntries.filter((entry) => entry.status === "Posted").length, label: "Journal", tone: "active" },
    { count: data.accounts.length, label: de ? "Konten" : "Accounts", tone: "muted" },
    { count: data.journalEntries.filter((entry) => entry.status === "Reversed").length, label: de ? "Storno" : "Reversals", tone: "attention" }
  ];
}

function financeRowsForSubmodule({
  balanceSheet,
  bankRows,
  businessAnalysis,
  data,
  datevLines,
  fixedAssets,
  fiscalPeriods,
  ledgerRows,
  locale,
  profitAndLoss,
  receipts,
  submoduleId,
  trialBalance,
  vatStatement
}: {
  balanceSheet: ReturnType<typeof buildBalanceSheet>;
  bankRows: ReturnType<typeof buildReconciliationRows>;
  businessAnalysis: ReturnType<typeof buildBusinessAnalysis>;
  data: BusinessBundle;
  datevLines: ReturnType<typeof buildDatevLines>;
  fixedAssets: ReturnType<typeof buildFixedAssetRegister>;
  fiscalPeriods: ReturnType<typeof buildFiscalPeriodState>;
  ledgerRows: ReturnType<typeof buildLedgerRows>;
  locale: SupportedLocale;
  profitAndLoss: ReturnType<typeof buildProfitAndLoss>;
  receipts: ReturnType<typeof buildReceiptQueue>;
  submoduleId: string;
  trialBalance: ReturnType<typeof buildTrialBalance>;
  vatStatement: ReturnType<typeof buildVatStatement>;
}): FinanceRow[] {
  if (submoduleId === "invoices") {
    return data.invoices.slice(0, 10).map((item) => ({
      action: item.status === "Paid" ? "Archiv" : "Prüfen",
      amount: businessCurrency(item.balanceDue ?? item.total, item.currency, locale),
      amountTone: item.status === "Paid" ? "neutral" : "positive",
      detail: text(item.notes, locale),
      id: item.id,
      marker: "RE",
      meta: `${item.number} · ${item.issueDate} · ${locale === "de" ? "fällig" : "due"} ${item.dueDate}`,
      tone: item.status === "Overdue" || item.status === "Draft" ? "review" : item.status === "Paid" ? "quiet" : "focus",
      status: statusLabel(item.status, locale),
      title: customerName(data, item.customerId)
    }));
  }

  if (submoduleId === "receipts") {
    return receipts.slice(0, 10).map((receipt) => ({
      action: receipt.status === "Needs review" ? "Zuordnen" : "Details",
      amount: businessCurrency(receipt.total, receipt.currency, locale),
      amountTone: "negative",
      detail: `${receipt.attachmentName} · ${text(receipt.notes, locale)}`,
      id: receipt.id,
      marker: "EG",
      meta: `${receipt.number} · ${receipt.receiptDate} · ${sourceLabel(receipt.source, locale)}`,
      tone: receipt.status === "Needs review" || receipt.status === "Inbox" ? "review" : "quiet",
      status: statusLabel(receipt.status, locale),
      title: receipt.vendorName
    }));
  }

  if (submoduleId === "payments") {
    return bankRows.slice(0, 10).map((row) => ({
      action: paymentActionLabel(row.nextAction, locale),
      amount: businessCurrency(row.amount, row.currency, locale),
      amountTone: row.amount < 0 ? "negative" : "positive",
      detail: `${row.purpose} · ${row.matchedLabel}`,
      id: row.id,
      marker: row.amount < 0 ? "AB" : "ZU",
      meta: `${row.bookingDate} · ${row.valueDate} · ${formatConfidence(row.confidence)}`,
      tone: row.status === "Suggested" ? "focus" : row.status === "Unmatched" ? "review" : "quiet",
      status: statusLabel(row.status, locale),
      title: row.counterparty
    }));
  }

  if (submoduleId === "bookkeeping") {
    const exportRows: FinanceRow[] = data.bookkeeping.slice(0, 4).map((batch) => ({
      action: batch.system,
      amount: businessCurrency(batch.netAmount + batch.taxAmount, "EUR", locale),
      amountTone: "neutral",
      detail: `${text(batch.context, locale)} · ${batch.invoiceIds.length} ${locale === "de" ? "Belege" : "documents"}`,
      id: batch.id,
      marker: "EX",
      meta: `${batch.period} · ${batch.generatedAt.slice(0, 10)} · ${batch.reviewer}`,
      tone: batch.status === "Exported" ? "quiet" : "focus",
      status: statusLabel(batch.status, locale),
      title: locale === "de" ? `Exportstapel ${batch.period}` : `Export batch ${batch.period}`
    }));
    const postingRows: FinanceRow[] = datevLines.slice(0, Math.max(4, 10 - exportRows.length)).map((line, index) => ({
      action: line.taxCode || "Export",
      amount: businessCurrency(line.amount, line.account.currency, locale),
      amountTone: "neutral",
      detail: `${line.entry.number} · ${text(line.entry.narration, locale)}`,
      id: `${line.entry.id}-${line.account.id}-${index}`,
      marker: line.side,
      meta: `${line.account.code} · ${line.contraAccount?.code ?? "-"}`,
      tone: "quiet",
      status: statusLabel(line.entry.status, locale),
      title: line.account.name
    }));
    return [...exportRows, ...postingRows];
  }

  if (submoduleId === "fixed-assets") {
    return fixedAssets.slice(0, 10).map((asset) => ({
      action: asset.status === "Active" ? "AfA Plan" : "Details",
      amount: businessCurrency(asset.bookValue, asset.currency, locale),
      amountTone: "neutral",
      detail: `${asset.category} · ${asset.supplier} · ${text(asset.notes, locale)}`,
      id: asset.id,
      marker: "AV",
      meta: `${asset.acquisitionDate} · ${depreciationMethodLabel(asset.depreciationMethod, locale)} · ${asset.usefulLifeMonths} ${locale === "de" ? "Monate" : "months"}`,
      tone: asset.status === "Active" ? "focus" : "quiet",
      status: statusLabel(asset.status, locale),
      title: asset.name
    }));
  }

  if (submoduleId === "reports") {
    const vatBox81 = vatStatement.boxes.find((box) => box.code === "81");
    const vatBox86 = vatStatement.boxes.find((box) => box.code === "86");
    const vatBox66 = vatStatement.boxes.find((box) => box.code === "66");
    const vatBox83 = vatStatement.boxes.find((box) => box.code === "83");
    const vatBoxRc = vatStatement.boxes.find((box) => box.code === "RC");
    const vatMetaParts = [
      `Kz 81 ${businessCurrency(vatBox81?.amount ?? 0, "EUR", locale)}`,
      ...(vatBox86 && vatBox86.amount > 0 ? [`Kz 86 ${businessCurrency(vatBox86.amount, "EUR", locale)}`] : []),
      ...(vatBoxRc && vatBoxRc.amount > 0 ? [`0%/RC ${businessCurrency(vatBoxRc.amount, "EUR", locale)}`] : []),
      `Kz 66 ${businessCurrency(vatBox66?.amount ?? 0, "EUR", locale)}`,
      `Kz 83 ${businessCurrency(vatBox83?.amount ?? vatStatement.netPosition, "EUR", locale)}`
    ];
    const reportRows: FinanceRow[] = [
      {
        action: fiscalPeriods.nextClosablePeriod ? (locale === "de" ? "schließen" : "close") : "aktuell",
        amount: fiscalPeriods.nextClosablePeriod?.id ?? `${fiscalPeriods.closedCount}/${fiscalPeriods.periodCount}`,
        amountTone: "neutral",
        detail: fiscalPeriods.lastClosedPeriod
          ? `${locale === "de" ? "Letzte geschlossene Periode" : "Last closed period"} ${fiscalPeriods.lastClosedPeriod.id}`
          : (locale === "de" ? "Noch keine Periode festgeschrieben" : "No period locked yet"),
        id: "period-close-state",
        marker: "P",
        meta: fiscalPeriods.currentPeriod ? `${locale === "de" ? "laufend" : "current"} ${fiscalPeriods.currentPeriod.id}` : (locale === "de" ? "kein laufender Zeitraum" : "no current period"),
        tone: fiscalPeriods.nextClosablePeriod ? "review" : "quiet",
        status: locale === "de" ? "Periodenabschluss" : "Period close",
        title: fiscalPeriods.nextClosablePeriod
          ? (locale === "de" ? "Nächste Periode schließen" : "Close next period")
          : (locale === "de" ? "Perioden sind aktuell" : "Periods are current")
      },
      {
        action: balanceSheet.balanced ? "stimmig" : "Differenz",
        amount: businessCurrency(balanceSheet.assets, "EUR", locale),
        amountTone: "neutral",
        detail: `${locale === "de" ? "Passiva" : "Liabilities and equity"} ${businessCurrency(balanceSheet.liabilities + balanceSheet.equity, "EUR", locale)} · ${locale === "de" ? "Jahresergebnis" : "retained earnings"} ${businessCurrency(balanceSheet.retainedEarnings, "EUR", locale)}`,
        id: "balance-sheet-derived",
        marker: "B",
        meta: locale === "de" ? "aus Ledger und Kontenrahmen" : "from ledger and chart of accounts",
        tone: balanceSheet.balanced ? "focus" : "review",
        status: locale === "de" ? "Bilanz" : "Balance sheet",
        title: locale === "de" ? "Bilanz aus Ledger" : "Balance sheet from ledger"
      },
      {
        action: "GuV",
        amount: businessCurrency(profitAndLoss.netIncome, "EUR", locale),
        amountTone: profitAndLoss.netIncome < 0 ? "negative" : "positive",
        detail: `${locale === "de" ? "Erloese" : "Income"} ${businessCurrency(profitAndLoss.income, "EUR", locale)} · ${locale === "de" ? "Aufwand" : "Expense"} ${businessCurrency(profitAndLoss.expense, "EUR", locale)}`,
        id: "profit-and-loss-derived",
        marker: "G",
        meta: locale === "de" ? "Ergebnis fliesst in Eigenkapital" : "net income flows into equity",
        tone: "quiet",
        status: locale === "de" ? "GuV" : "P&L",
        title: locale === "de" ? "Gewinn und Verlust" : "Profit and loss"
      },
      {
        action: "BWA",
        amount: businessCurrency(businessAnalysis.ebit, "EUR", locale),
        amountTone: businessAnalysis.ebit < 0 ? "negative" : "positive",
        detail: `${locale === "de" ? "Rohertrag" : "Gross profit"} ${businessCurrency(businessAnalysis.grossProfit, "EUR", locale)} · ${locale === "de" ? "Betriebsaufwand" : "Operating expenses"} ${businessCurrency(businessAnalysis.operatingExpenses + businessAnalysis.personnelCosts + businessAnalysis.depreciation, "EUR", locale)}`,
        id: "business-analysis-derived",
        marker: "W",
        meta: locale === "de" ? "BWA aus GuV-Konten" : "business analysis from P&L accounts",
        tone: "quiet",
        status: "BWA",
        title: locale === "de" ? "Betriebswirtschaftliche Auswertung" : "Business analysis"
      },
      {
        action: vatStatement.payable > 0 ? "Zahllast" : "Erstattung",
        amount: businessCurrency(vatStatement.netPosition, "EUR", locale),
        amountTone: vatStatement.netPosition < 0 ? "positive" : "negative",
        detail: locale === "de"
          ? `Quelle ${vatBox81?.source ?? "Rechnungszeilen"} · 0%/RC ${businessCurrency(vatBoxRc?.amount ?? 0, "EUR", locale)}`
          : `Source ${vatBox81?.source ?? "invoice lines"} · 0%/RC ${businessCurrency(vatBoxRc?.amount ?? 0, "EUR", locale)}`,
        id: "vat-statement-derived",
        marker: "U",
        meta: vatMetaParts.join(" · "),
        tone: vatStatement.payable > 0 ? "review" : "quiet",
        status: "UStVA",
        title: locale === "de" ? "Umsatzsteuer-Voranmeldung" : "VAT statement"
      }
    ];
    const dunningRows: FinanceRow[] = data.invoices
      .filter((invoice) => (invoice.reminderLevel ?? 0) > 0)
      .slice(0, 4)
      .map((invoice) => ({
        action: invoice.reminderLevel && invoice.reminderLevel >= 3 ? "Letzte" : "Mahnung",
        amount: businessCurrency(invoice.balanceDue ?? invoice.total, invoice.currency, locale),
        amountTone: "negative",
        detail: `${locale === "de" ? "Fällig" : "Due"} ${invoice.dueDate} · ${invoice.collectionStatus ?? (locale === "de" ? "Mahnung versendet" : "Reminder sent")}`,
        id: `dunning-${invoice.id}`,
        marker: "M",
        meta: invoice.reminderDueDate ? `${locale === "de" ? "versendet" : "sent"} ${invoice.reminderDueDate}` : invoice.number,
        tone: "review",
        status: locale === "de" ? `Mahnstufe ${invoice.reminderLevel}` : `Dunning level ${invoice.reminderLevel}`,
        title: `${locale === "de" ? "Mahnlauf" : "Dunning run"} ${invoice.number}`
      }));
    return reportRows.concat(dunningRows, trialBalance.slice(0, Math.max(4, 8 - dunningRows.length)).map((row) => ({
      action: rootTypeLabel(row.account.rootType, locale),
      amount: businessCurrency(row.balance, row.account.currency, locale),
      amountTone: row.balance < 0 ? "negative" : "neutral",
      detail: `${businessCurrency(row.debit, row.account.currency, locale)} Soll · ${businessCurrency(row.credit, row.account.currency, locale)} Haben`,
      id: row.account.id,
      marker: row.account.code,
      meta: row.account.accountType,
      tone: "quiet",
      status: row.account.isPosting ? "Buchbar" : "Gruppe",
      title: row.account.name
    })));
  }

  return ledgerRows.slice(0, 10).map((row) => ({
    action: refTypeLabel(row.entry.refType, locale),
    amount: businessCurrency(Math.abs(row.signedAmount), row.account.currency, locale),
    amountTone: row.signedAmount < 0 ? "negative" : "neutral",
    detail: `${row.refLabel} · ${row.partyLabel}`,
    id: row.id,
    marker: row.account.code,
    meta: `${row.entry.number} · ${row.entry.postingDate}`,
    tone: row.entry.status === "Posted" ? "quiet" : "review",
    status: statusLabel(row.entry.status, locale),
    title: row.account.name
  }));
}

function statusLabel(status: string, locale: SupportedLocale) {
  if (locale !== "de") return status;
  const labels: Record<string, string> = {
    Draft: "Entwurf",
    Active: "Aktiv",
    Disposed: "Abgegangen",
    Exported: "Exportiert",
    "Export ready": "Exportbereit",
    Ignored: "Ignoriert",
    Inbox: "Eingang",
    Matched: "Zugeordnet",
    "Needs review": "Zu prüfen",
    "Fully depreciated": "Voll abgeschrieben",
    Overdue: "Überfällig",
    Paid: "Bezahlt",
    Posted: "Gebucht",
    Queued: "Wartet",
    Ready: "Bereit",
    Rejected: "Abgelehnt",
    Sent: "Gesendet",
    Suggested: "Vorschlag",
    Unmatched: "Offen"
  };
  return labels[status] ?? status;
}

function formatConfidence(confidence: number) {
  const percent = confidence > 1 ? confidence : confidence * 100;
  return `${Math.round(Math.max(0, Math.min(100, percent)))}%`;
}

function paymentActionLabel(action: string, locale: SupportedLocale) {
  if (locale !== "de") return action;
  const labels: Record<string, string> = {
    "Confirm match": "Match bestätigen",
    "Create receipt or manual posting": "Beleg oder Buchung anlegen",
    "Review fee account": "Gebührenkonto prüfen",
    "Review customer allocation": "Kundenzuordnung prüfen",
    "Review vendor allocation": "Lieferantenzuordnung prüfen",
    Ignore: "Ignorieren",
    Posted: "Gebucht"
  };
  return labels[action] ?? action;
}

function refTypeLabel(refType: string, locale: SupportedLocale) {
  if (locale !== "de") return refType;
  const labels: Record<string, string> = {
    asset: "Anlage",
    bank_transaction: "Bank",
    depreciation: "AfA",
    dunning: "Mahnung",
    invoice: "Rechnung",
    manual: "Manuell",
    payment: "Zahlung",
    receipt: "Beleg",
    reverse: "Storno"
  };
  return labels[refType] ?? refType;
}

function rootTypeLabel(rootType: string, locale: SupportedLocale) {
  if (locale !== "de") return rootType;
  const labels: Record<string, string> = {
    asset: "Aktiva",
    equity: "Eigenkapital",
    expense: "Aufwand",
    income: "Ertrag",
    liability: "Passiva"
  };
  return labels[rootType] ?? rootType;
}

function sourceLabel(source: string, locale: SupportedLocale) {
  if (locale !== "de") return source;
  const labels: Record<string, string> = {
    "Bank match": "Bankmatch",
    Email: "E-Mail",
    Upload: "Upload"
  };
  return labels[source] ?? source;
}

function depreciationMethodLabel(method: string, locale: SupportedLocale) {
  if (locale !== "de") return method;
  const labels: Record<string, string> = {
    "Straight line": "linear"
  };
  return labels[method] ?? method;
}

type BusinessNextStep = {
  description: string;
  eyebrow: string;
  facts: Array<[string, string]>;
  title: string;
};

function nextStepForSubmodule({
  copyLocale,
  customer,
  exportBatch,
  invoice,
  openBankRows,
  openInvoices,
  openReceipts,
  receiptToPost,
  submoduleId,
  transactionToMatch
}: {
  copyLocale: SupportedLocale;
  customer: BusinessBundle["customers"][number] | undefined;
  exportBatch: BusinessBundle["bookkeeping"][number] | undefined;
  invoice: BusinessBundle["invoices"][number];
  openBankRows: ReturnType<typeof buildReconciliationRows>;
  openInvoices: BusinessBundle["invoices"];
  openReceipts: ReturnType<typeof buildReceiptQueue>;
  receiptToPost: ReturnType<typeof buildReceiptQueue>[number] | undefined;
  submoduleId: string;
  transactionToMatch: ReturnType<typeof buildReconciliationRows>[number] | undefined;
}): BusinessNextStep {
  const de = copyLocale === "de";
  if (submoduleId === "receipts" && receiptToPost) {
    return {
      description: de ? "Erst OCR-Ergebnis prüfen, dann die vorgeschlagene Buchung freigeben." : "Review OCR output first, then approve the proposed posting.",
      eyebrow: de ? "Nächste Entscheidung" : "Next decision",
      facts: [
        [de ? "Beleg" : "Receipt", receiptToPost.number],
        [de ? "Lieferant" : "Vendor", receiptToPost.vendorName],
        [de ? "Betrag" : "Amount", businessCurrency(receiptToPost.total, receiptToPost.currency, copyLocale)]
      ],
      title: de ? "Eingangsbeleg klaeren" : "Clear inbound receipt"
    };
  }

  if (submoduleId === "payments" && transactionToMatch) {
    return {
      description: de ? "Bankzeile mit Rechnung oder Beleg abstimmen. Nur der Vorschlag mit Confidence ist prominent." : "Reconcile the bank line with an invoice or receipt. Only the confidence-backed proposal is prominent.",
      eyebrow: de ? "Abgleich" : "Reconciliation",
      facts: [
        [de ? "Gegenpartei" : "Counterparty", transactionToMatch.counterparty],
        ["Status", statusLabel(transactionToMatch.status, copyLocale)],
        [de ? "Betrag" : "Amount", businessCurrency(transactionToMatch.amount, transactionToMatch.currency, copyLocale)]
      ],
      title: de ? "Bankmatch prüfen" : "Review bank match"
    };
  }

  if (submoduleId === "bookkeeping" && exportBatch) {
    return {
      description: de ? "Bankimport und DATEV Export sind operative Aktionen. Rohzeilen bleiben darunter als Arbeitsliste." : "Bank import and DATEV export are operational actions. Raw lines stay below as the worklist.",
      eyebrow: de ? "Exportlauf" : "Export run",
      facts: [
        ["System", exportBatch.system],
        [de ? "Periode" : "Period", exportBatch.period],
        ["Status", statusLabel(exportBatch.status, copyLocale)]
      ],
      title: de ? "Buchungsstapel vorbereiten" : "Prepare posting batch"
    };
  }

  if (submoduleId === "fixed-assets") {
    return {
      description: de ? "Anlagen werden aktiviert, planmaessig abgeschrieben und gehen netto in die Bilanz ein." : "Assets are capitalized, depreciated on schedule, and flow net into the balance sheet.",
      eyebrow: de ? "Anlagevermögen" : "Fixed assets",
      facts: [
        [de ? "Aktive Anlagen" : "Active assets", "1"],
        [de ? "AfA Konto" : "Depreciation account", "4830"],
        [de ? "Bilanzkonto" : "Balance sheet account", "0480 / 0490"]
      ],
      title: de ? "Anlage und AfA prüfen" : "Review asset and depreciation"
    };
  }

  if (submoduleId === "reports") {
    return {
      description: de ? "Bilanz und GuV werden direkt aus gebuchten Ledger-Salden abgeleitet; Anlagen laufen netto über 0480/0490." : "Balance sheet and P&L are derived directly from posted ledger balances; assets flow net through fixed asset and accumulated depreciation accounts.",
      eyebrow: de ? "Abschluss" : "Close",
      facts: [
        [de ? "Bilanzlogik" : "Balance logic", de ? "Ledger-basiert" : "Ledger-based"],
        [de ? "Offene Bankzeilen" : "Open bank lines", String(openBankRows.length)],
        [de ? "Offene Belege" : "Open receipts", String(openReceipts.length)]
      ],
      title: de ? "Bilanz aus Ledger ableiten" : "Derive balance sheet from ledger"
    };
  }

  if (submoduleId === "ledger") {
    return {
      description: de ? "Ledger ist Nachweis, nicht Inbox. Die Liste priorisiert Buchungen, Summen bleiben leise." : "The ledger is evidence, not an inbox. The list prioritizes postings, totals stay quiet.",
      eyebrow: de ? "Nachweis" : "Evidence",
      facts: [
        [de ? "Rechnung" : "Invoice", invoice.number],
        [de ? "Kunde" : "Customer", customer?.name ?? invoice.customerId],
        ["Journal", "balanced"]
      ],
      title: de ? "Buchungen nachvollziehen" : "Trace postings"
    };
  }

  return {
    description: de ? "Rechnung prüfen, senden und die Buchung im Workflow nachvollziehen." : "Review and send the invoice, then follow the posting in the workflow.",
    eyebrow: de ? "Primaeraktion" : "Primary action",
    facts: [
      [de ? "Rechnung" : "Invoice", invoice.number],
      [de ? "Kunde" : "Customer", customer?.name ?? invoice.customerId],
      [de ? "Brutto" : "Gross", businessCurrency(invoice.total, invoice.currency, copyLocale)]
    ],
    title: de ? "Rechnung versenden" : "Send invoice"
  };
}

function submoduleLead(submoduleId: string, locale: SupportedLocale) {
  const de: Record<string, string> = {
    bookkeeping: "Export, Bankimport und Steuerprüfung in einer Arbeitsfolge.",
    "fixed-assets": "Anlagen aktivieren, AfA buchen und Bilanzwerte ableiten.",
    invoices: "Ausgangsrechnungen prüfen, senden und buchen.",
    ledger: "Buchungsnachweis mit ruhiger Summenkontrolle.",
    payments: "Bankzeilen abstimmen, Vorschläge freigeben.",
    receipts: "Eingangsbelege erfassen, prüfen und buchen.",
    reports: "Abschluss, Mahnungen und Berichte kontrollieren."
  };
  const en: Record<string, string> = {
    bookkeeping: "Export, bank import, and tax review in one flow.",
    "fixed-assets": "Capitalize assets, post depreciation, and derive balance sheet values.",
    invoices: "Review, send, and post outgoing invoices.",
    ledger: "Posting evidence with quiet total control.",
    payments: "Reconcile bank lines and approve proposals.",
    receipts: "Capture, review, and post inbound receipts.",
    reports: "Control close, dunning, and reports."
  };
  return (locale === "de" ? de : en)[submoduleId] ?? "";
}

function worklistTitleForSubmodule(submoduleId: string, locale: SupportedLocale) {
  const de: Record<string, string> = {
    bookkeeping: "Buchungszeilen",
    "fixed-assets": "Anlagenregister",
    invoices: "Rechnungsliste",
    ledger: "Journal",
    payments: "Bankabgleich",
    receipts: "Belegprüfung",
    reports: "Kontrollsalden"
  };
  const en: Record<string, string> = {
    bookkeeping: "Posting lines",
    "fixed-assets": "Fixed asset register",
    invoices: "Invoice list",
    ledger: "Journal",
    payments: "Bank reconciliation",
    receipts: "Receipt queue",
    reports: "Control balances"
  };
  return (locale === "de" ? de : en)[submoduleId] ?? submoduleId;
}

function tableCaptionForSubmodule(submoduleId: string, locale: SupportedLocale) {
  const de: Record<string, string> = {
    bookkeeping: "Maximal relevante Exportzeilen, nicht der gesamte technische Dump.",
    "fixed-assets": "Anschaffung, Restbuchwert und AfA-Status aus gebuchten Anlagenbuchungen.",
    invoices: "Status, Kunde und Betrag, damit offene Entscheidungen schnell sichtbar bleiben.",
    ledger: "Aktuelle Buchungen mit Referenz und Betrag.",
    payments: "Bankzeilen nach Klaerungsbedarf, gematchte Zeilen bleiben leiser.",
    receipts: "Belege nach Review-Status und Betrag.",
    reports: "Salden zur Kontrolle des Abschlusses."
  };
  const en: Record<string, string> = {
    bookkeeping: "Relevant export lines, not the full technical dump.",
    "fixed-assets": "Acquisition, net book value, and depreciation state from posted asset entries.",
    invoices: "Status, customer, and amount so open decisions stay visible.",
    ledger: "Current postings with reference and amount.",
    payments: "Bank lines by review need, matched lines stay quieter.",
    receipts: "Receipts by review status and amount.",
    reports: "Balances for close control."
  };
  return (locale === "de" ? de : en)[submoduleId];
}

function customerName(data: BusinessBundle, customerId: string) {
  return data.customers.find((item) => item.id === customerId)?.name ?? customerId;
}

function titleForSubmodule(submoduleId: string, locale: SupportedLocale) {
  const de: Record<string, string> = {
    bookkeeping: "Buchhaltung",
    "fixed-assets": "Anlagen",
    invoices: "Rechnungen",
    ledger: "Ledger",
    payments: "Zahlungen",
    receipts: "Eingangsbelege",
    reports: "Berichte"
  };
  const en: Record<string, string> = {
    bookkeeping: "Bookkeeping",
    "fixed-assets": "Assets",
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
    completeAndPrint: locale === "de" ? "Prüfen und PDF öffnen" : "Check and open PDF",
    completeAndSend: locale === "de" ? "Prüfen und senden" : "Check and send",
    draftSave: locale === "de" ? "Entwurf sichern" : "Save draft",
    emailCopy: locale === "de" ? "Kopie an mich" : "Send me a copy",
    emailSend: locale === "de" ? "E-Mail senden" : "Send email",
    emailShipping: locale === "de" ? "Rechnung per E-Mail" : "Invoice email",
    print: locale === "de" ? "PDF öffnen" : "Open PDF",
    recipient: locale === "de" ? "Empfaenger" : "Recipient",
    sendByEmail: locale === "de" ? "Per E-Mail vorbereiten" : "Prepare email",
    signature: locale === "de" ? "Signatur" : "Signature",
    standardTemplate: "Standard",
    subject: locale === "de" ? "Betreff" : "Subject",
    template: locale === "de" ? "Vorlage" : "Template"
  };
}
