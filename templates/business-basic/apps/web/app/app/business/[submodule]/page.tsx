import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import type { ReactNode } from "react";
import { findBusinessModule, findBusinessSubmodule, WorkSurface, type WorkSurfacePanelState } from "@ctox-business/ui";
import { AppShell } from "../../../../components/app-shell";
import { AccountingApiButton } from "../../../../components/accounting-api-button";
import { AccountingCommandButton } from "../../../../components/accounting-command-button";
import { AccountingCtoxActionButton } from "../../../../components/accounting-ctox-action-button";
import { AccountingStoryWorkflowPanel } from "../../../../components/accounting-story-workflow-panel";
import { AccountingWorkflowPanel } from "../../../../components/accounting-workflow-panel";
import { BankImportPreviewButton } from "../../../../components/bank-import-preview-button";
import { BusinessPanel, BusinessWorkspace } from "../../../../components/business-workspace";
import { DatevExportButton } from "../../../../components/datev-export-button";
import { DunningPreviewButton } from "../../../../components/dunning-preview-button";
import { InvoiceDeliveryActions } from "../../../../components/invoice-delivery-actions";
import { PaymentsHeaderActions, PaymentsReconciliationWorkspace } from "../../../../components/payments-reconciliation-workspace";
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
import { storyWorkflowsForSubmodule } from "../../../../lib/accounting-story-workflows";

export const dynamic = "force-dynamic";

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
  const storyWorkflows = storyWorkflowsForSubmodule(submoduleId);
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

  if (submoduleId === "payments") {
    return (
      <PaymentsReconciliationWorkspaceShell
        accounts={data.accounts}
        bankRows={bankRows}
        locale={locale}
        summaryItems={summaryItems}
      />
    );
  }

  if (submoduleId === "invoices") {
    return (
      <InvoiceDocumentWorkspace
        accounting={accounting}
        customer={customer}
        data={data}
        invoice={invoice}
        locale={locale}
        summaryItems={summaryItems}
      />
    );
  }

  if (submoduleId === "receipts") {
    return (
      <ReceiptDocumentWorkspace
        locale={locale}
        receiptToPost={receiptToPost}
        receipts={receipts}
        summaryItems={summaryItems}
      />
    );
  }

  if (submoduleId === "fixed-assets") {
    return (
      <FixedAssetsWorkspace
        assetToDepreciate={assetToDepreciate}
        fixedAssets={fixedAssets}
        locale={locale}
        summaryItems={summaryItems}
      />
    );
  }

  if (submoduleId === "ledger") {
    return (
      <LedgerEvidenceWorkspace
        data={data}
        ledgerRows={ledgerRows}
        locale={locale}
        summaryItems={summaryItems}
        trialBalance={trialBalance}
      />
    );
  }

  if (submoduleId === "bookkeeping") {
    return (
      <BookkeepingOperationsWorkspace
        bankRows={bankRows}
        bookkeeping={data.bookkeeping}
        datevLines={datevLines}
        fiscalPeriods={fiscalPeriods}
        locale={locale}
        summaryItems={summaryItems}
      />
    );
  }

  if (submoduleId === "reports") {
    return (
      <ReportsCloseWorkspace
        balanceSheet={balanceSheet}
        businessAnalysis={businessAnalysis}
        fiscalPeriods={fiscalPeriods}
        locale={locale}
        profitAndLoss={profitAndLoss}
        summaryItems={summaryItems}
        trialBalance={trialBalance}
        vatStatement={vatStatement}
      />
    );
  }

  return (
    <main className="business-accounting-page" data-submodule={submoduleId}>
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
        <FinanceModeTabs locale={locale} modes={financeModesForSubmodule({ balanceSheet, bankRows, data, fiscalPeriods, fixedAssets, locale, receipts, submoduleId, vatStatement })} />
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
          <FinanceDecisionPanel
            accounting={accounting}
            assetToDepreciate={assetToDepreciate}
            customer={customer}
            invoice={invoice}
            locale={locale}
            nextStep={nextStep}
            receiptToPost={receiptToPost}
            storyWorkflows={storyWorkflows}
            submoduleId={submoduleId}
            transactionToMatch={transactionToMatch}
          />
        </div>
      </section>
    </main>
  );
}

function FinanceReferenceHeader({
  actions,
  locale,
  summaryItems,
  title,
  subtitle
}: {
  actions?: ReactNode;
  locale: SupportedLocale;
  subtitle: string;
  summaryItems: Array<[string, string, string]>;
  title: string;
}) {
  return (
    <>
      <header className="business-accounting-header finance-page-header">
        <div>
          <h1>{title}</h1>
          {subtitle ? <span>{subtitle}</span> : null}
        </div>
        <div className="finance-header-actions">
          {actions}
        </div>
      </header>
      <section className="finance-summary-strip" aria-label={locale === "de" ? "Finanzlage" : "Financial state"}>
        {summaryItems.map(([label, value, meta]) => (
          <div key={label}>
            <span>{label}</span>
            <strong>{value}</strong>
            <small>{meta}</small>
          </div>
        ))}
      </section>
    </>
  );
}

function PaymentsReconciliationWorkspaceShell({
  accounts,
  bankRows,
  locale,
  summaryItems
}: {
  accounts: BusinessBundle["accounts"];
  bankRows: ReturnType<typeof buildReconciliationRows>;
  locale: SupportedLocale;
  summaryItems: Array<[string, string, string]>;
}) {
  const de = locale === "de";
  const orderedRows = [...bankRows].sort((a, b) => bankRowPriority(a.status) - bankRowPriority(b.status));
  return (
    <main className="business-accounting-page accounting-reference-page accounting-payments-page" data-submodule="payments">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Finanzen" : "Finance"}
        subtitle=""
        actions={<PaymentsHeaderActions locale={locale} />}
      />
      <PaymentsReconciliationWorkspace accounts={accounts} bankRows={orderedRows} locale={locale} />
    </main>
  );
}

function InvoiceDocumentWorkspace({
  accounting,
  customer,
  data,
  invoice,
  locale,
  summaryItems
}: {
  accounting: ReturnType<typeof prepareExistingInvoiceForAccounting>;
  customer: BusinessBundle["customers"][number] | undefined;
  data: BusinessBundle;
  invoice: BusinessBundle["invoices"][number];
  locale: SupportedLocale;
  summaryItems: Array<[string, string, string]>;
}) {
  const de = locale === "de";
  return (
    <main className="business-accounting-page accounting-reference-page document-workspace-page" data-submodule="invoices">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Alle Belege" : "All documents"}
        subtitle=""
        actions={null}
      />
      <section className="document-shell">
        <DocumentSideNav
          items={[
            [de ? "Alle Belege" : "All documents", data.invoices.length],
            [de ? "Ausgangsbelege" : "Outgoing", data.invoices.length],
            [de ? "Überfällige" : "Overdue", data.invoices.filter((item) => item.status === "Overdue").length],
            [de ? "Entwürfe" : "Drafts", data.invoices.filter((item) => item.status === "Draft").length],
            [de ? "Archiviert" : "Archived", data.invoices.filter((item) => item.status === "Paid").length]
          ]}
        />
        <section className="document-list-panel" id="document-list">
          <div className="document-list-toolbar">
            <input readOnly placeholder={de ? "Suchen Sie nach Name, Belegnummer oder Betrag" : "Search by name, document number or amount"} />
            <span>{de ? "Sortieren nach Belegdatum" : "Sort by document date"}</span>
          </div>
          {data.invoices.map((item) => (
            <article className={item.id === invoice.id ? "is-selected" : ""} key={item.id}>
              <input aria-label={item.number} readOnly checked={item.id === invoice.id} type="checkbox" />
              <div>
                <strong>{customerName(data, item.customerId)}</strong>
                <span>{item.number} · {item.issueDate} · {statusLabel(item.status, locale)}</span>
              </div>
              <strong>{businessCurrency(item.balanceDue ?? item.total, item.currency, locale)}</strong>
              <a href={`/api/business/invoices/${item.id}/pdf?locale=${locale}`}>PDF</a>
            </article>
          ))}
        </section>
        <aside className="document-detail-panel">
          <div className="document-preview">RE</div>
          <dl>
            <div><dt>{de ? "Belegnummer" : "Document no."}</dt><dd>{invoice.number}</dd></div>
            <div><dt>{de ? "Kontakt" : "Contact"}</dt><dd>{customer?.name ?? invoice.customerId}</dd></div>
            <div><dt>{de ? "Fälligkeit" : "Due"}</dt><dd>{invoice.dueDate}</dd></div>
            <div><dt>{de ? "Betrag" : "Amount"}</dt><dd>{businessCurrency(invoice.total, invoice.currency, locale)}</dd></div>
          </dl>
          <div className="document-action-stack">
            <InvoiceDeliveryActions copy={deliveryCopy(locale)} customer={customer} invoice={invoice} locale={locale} />
            <a className="business-accounting-inline-link" href={`/api/business/invoices/${invoice.id}/zugferd?locale=${locale}`}>ZUGFeRD XML</a>
            <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-04" />
          </div>
          {accounting.validation.errors.length ? <small>{accounting.validation.errors.join(", ")}</small> : null}
        </aside>
      </section>
    </main>
  );
}

function ReceiptDocumentWorkspace({
  locale,
  receiptToPost,
  receipts,
  summaryItems
}: {
  locale: SupportedLocale;
  receiptToPost: ReturnType<typeof buildReceiptQueue>[number] | undefined;
  receipts: ReturnType<typeof buildReceiptQueue>;
  summaryItems: Array<[string, string, string]>;
}) {
  const de = locale === "de";
  const selected = receiptToPost ?? receipts[0];
  return (
    <main className="business-accounting-page accounting-reference-page document-workspace-page" data-submodule="receipts">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Eingangsbelege" : "Inbound receipts"}
        subtitle=""
        actions={null}
      />
      <section className="document-shell">
        <DocumentSideNav
          items={[
            [de ? "Alle Belege" : "All receipts", receipts.length],
            [de ? "Eingangsbelege" : "Inbound", receipts.length],
            [de ? "Zu prüfen" : "Review", receipts.filter((item) => item.status === "Needs review").length],
            [de ? "Offene" : "Open", receipts.filter((item) => item.status === "Inbox").length],
            [de ? "Archiviert" : "Archived", receipts.filter((item) => item.status === "Posted" || item.status === "Paid").length]
          ]}
        />
        <section className="document-list-panel">
          <div className="document-list-toolbar">
            <input readOnly placeholder={de ? "Name, Belegnummer oder Betrag suchen" : "Search name, receipt number or amount"} />
            <span>{de ? "Sortieren nach Belegdatum" : "Sort by receipt date"}</span>
          </div>
          {receipts.map((item) => (
            <article className={item.id === selected?.id ? "is-selected" : ""} key={item.id}>
              <input aria-label={item.number} readOnly checked={item.id === selected?.id} type="checkbox" />
              <div>
                <strong>{item.vendorName}</strong>
                <span>{item.number} · {item.receiptDate} · {statusLabel(item.status, locale)}</span>
              </div>
              <strong>{businessCurrency(item.total, item.currency, locale)}</strong>
              <span>{item.status === "Needs review" ? (de ? "Zuordnen" : "Assign") : statusLabel(item.status, locale)}</span>
            </article>
          ))}
        </section>
        {selected ? (
          <aside className="document-detail-panel">
            <div className="document-preview">EG</div>
            <dl>
              <div><dt>{de ? "Lieferant" : "Vendor"}</dt><dd>{selected.vendorName}</dd></div>
              <div><dt>{de ? "Belegnummer" : "Receipt no."}</dt><dd>{selected.number}</dd></div>
              <div><dt>{de ? "Aufwandskonto" : "Expense account"}</dt><dd>{selected.expenseAccount.code} {selected.expenseAccount.name}</dd></div>
              <div><dt>{de ? "Betrag" : "Amount"}</dt><dd>{businessCurrency(selected.total, selected.currency, locale)}</dd></div>
            </dl>
            <div className="document-action-stack">
              <ReceiptIngestButton label={de ? "OCR vorbereiten" : "Prepare OCR"} path={`/api/business/receipts/${selected.id}/ingest`} />
              {selected.status === "Needs review" || selected.status === "Inbox" ? (
                <>
                  <AccountingCommandButton action="post" label={de ? "Buchen" : "Post"} recordId={selected.id} resource="receipts" />
                  <AccountingCommandButton action="capitalize" label={de ? "Als Anlage" : "As asset"} recordId={selected.id} resource="receipts" />
                </>
              ) : null}
              <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-13" />
            </div>
          </aside>
        ) : null}
      </section>
    </main>
  );
}

function DocumentSideNav({ items }: { items: Array<[string, number]> }) {
  return (
    <nav className="document-side-nav">
      {items.map(([label, count], index) => (
        <button aria-current={index === 0 ? "true" : undefined} key={label} type="button">
          <span>{label}</span>
          <strong>{count}</strong>
        </button>
      ))}
    </nav>
  );
}

function FixedAssetsWorkspace({
  assetToDepreciate,
  fixedAssets,
  locale,
  summaryItems
}: {
  assetToDepreciate: ReturnType<typeof buildFixedAssetRegister>[number] | undefined;
  fixedAssets: ReturnType<typeof buildFixedAssetRegister>;
  locale: SupportedLocale;
  summaryItems: Array<[string, string, string]>;
}) {
  const de = locale === "de";
  const selected = assetToDepreciate ?? fixedAssets[0];
  return (
    <main className="business-accounting-page accounting-reference-page fixed-assets-page" data-submodule="fixed-assets">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Anlagen" : "Fixed assets"}
        subtitle=""
        actions={null}
      />
      <section className="assets-shell">
        <nav className="assets-side-nav">
          <button aria-current="true" type="button">{de ? "Abschreibungsjahr" : "Depreciation year"} <strong>2026</strong></button>
          <button type="button">{de ? "Alle Anlagen" : "All assets"} <strong>{fixedAssets.length}</strong></button>
        </nav>
        <section className="assets-list" id="asset-list">
          <header><span>{de ? "Anlage" : "Asset"}</span><span>Status</span><span>{de ? "AfA 2026" : "Dep. 2026"}</span></header>
          {fixedAssets.map((asset) => (
            <article className={asset.id === selected?.id ? "is-selected" : ""} key={asset.id}>
              <div><strong>{asset.name}</strong><span>{asset.category} · {asset.supplier}</span></div>
              <span>{statusLabel(asset.status, locale)}</span>
              <strong>{businessCurrency(asset.currentYearDepreciation, asset.currency, locale)}</strong>
            </article>
          ))}
        </section>
        {selected ? (
          <aside className="asset-detail-panel">
            <header>
              <div>
                <h2>{selected.name}</h2>
                <p>{selected.category}</p>
              </div>
              <AccountingCommandButton action="depreciate" label={de ? "AfA buchen" : "Post depreciation"} recordId={selected.id} resource="fixed-assets" />
            </header>
            <dl>
              <div><dt>{de ? "Anschaffung" : "Acquisition"}</dt><dd>{businessCurrency(selected.acquisitionCost, selected.currency, locale)}</dd></div>
              <div><dt>{de ? "Buchwert" : "Book value"}</dt><dd>{businessCurrency(selected.bookValue, selected.currency, locale)}</dd></div>
              <div><dt>{de ? "Kumulierte AfA" : "Accumulated dep."}</dt><dd>{businessCurrency(selected.accumulatedDepreciation, selected.currency, locale)}</dd></div>
            </dl>
            <section className="asset-schedule">
              <h3>{de ? "Abschreibungsverlauf" : "Depreciation schedule"}</h3>
              {selected.schedule.map((row) => (
                <div className={row.fiscalYear === 2026 ? "is-current" : ""} key={row.fiscalYear}>
                  <span>{row.fiscalYear}</span>
                  <strong>{businessCurrency(row.amount, selected.currency, locale)}</strong>
                  <span>{businessCurrency(row.bookValue, selected.currency, locale)}</span>
                </div>
              ))}
            </section>
            <div className="document-action-stack">
              <AccountingCommandButton action="dispose" label={de ? "Abgang vorbereiten" : "Prepare disposal"} recordId={selected.id} resource="fixed-assets" />
              <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-39" />
            </div>
          </aside>
        ) : null}
      </section>
    </main>
  );
}

function ReportsCloseWorkspace({
  balanceSheet,
  businessAnalysis,
  fiscalPeriods,
  locale,
  profitAndLoss,
  summaryItems,
  trialBalance,
  vatStatement
}: {
  balanceSheet: ReturnType<typeof buildBalanceSheet>;
  businessAnalysis: ReturnType<typeof buildBusinessAnalysis>;
  fiscalPeriods: ReturnType<typeof buildFiscalPeriodState>;
  locale: SupportedLocale;
  profitAndLoss: ReturnType<typeof buildProfitAndLoss>;
  summaryItems: Array<[string, string, string]>;
  trialBalance: ReturnType<typeof buildTrialBalance>;
  vatStatement: ReturnType<typeof buildVatStatement>;
}) {
  const de = locale === "de";
  const assets = trialBalance.filter((row) => row.account.rootType === "asset").slice(0, 5);
  const passiva = trialBalance.filter((row) => row.account.rootType === "liability" || row.account.rootType === "equity").slice(0, 5);
  return (
    <main className="business-accounting-page accounting-reference-page reports-close-page" data-submodule="reports">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Berichte" : "Reports"}
        subtitle=""
        actions={<AccountingApiButton label={de ? "Periode schließen" : "Close period"} path="/api/business/accounting/period-close" />}
      />
      <section className="reports-dashboard">
        <article className="balance-report-panel">
          <header>
            <div><p>{de ? "Ledger-basiert" : "Ledger-based"}</p><h2>{de ? "Bilanz" : "Balance sheet"}</h2></div>
            <strong>{balanceSheet.balanced ? (de ? "stimmig" : "balanced") : businessCurrency(balanceSheet.difference, "EUR", locale)}</strong>
          </header>
          <div className="balance-total-row">
            <span>Aktiva</span><strong>{businessCurrency(balanceSheet.assets, "EUR", locale)}</strong>
            <span>Passiva</span><strong>{businessCurrency(balanceSheet.liabilities + balanceSheet.equity, "EUR", locale)}</strong>
          </div>
          <div className="balance-columns">
            <ReportAccountColumn title="Aktiva" rows={assets} locale={locale} />
            <ReportAccountColumn title="Passiva" rows={passiva} locale={locale} />
          </div>
        </article>
        <article className="report-summary-panel">
          <h2>{de ? "GuV" : "P&L"}</h2>
          <strong>{businessCurrency(profitAndLoss.netIncome, "EUR", locale)}</strong>
          <span>{de ? "Erloese" : "Income"} {businessCurrency(profitAndLoss.income, "EUR", locale)}</span>
          <span>{de ? "Aufwand" : "Expense"} {businessCurrency(profitAndLoss.expense, "EUR", locale)}</span>
        </article>
        <article className="report-summary-panel">
          <h2>BWA</h2>
          <strong>{businessCurrency(businessAnalysis.ebit, "EUR", locale)}</strong>
          <span>{de ? "Rohertrag" : "Gross profit"} {businessCurrency(businessAnalysis.grossProfit, "EUR", locale)}</span>
          <span>{de ? "AfA" : "Depreciation"} {businessCurrency(businessAnalysis.depreciation, "EUR", locale)}</span>
        </article>
        <article className="report-summary-panel">
          <h2>UStVA</h2>
          <strong>{businessCurrency(vatStatement.netPosition, "EUR", locale)}</strong>
          {vatStatement.boxes.slice(0, 3).map((box) => <span key={box.code}>Kz {box.code} {businessCurrency(box.amount, "EUR", locale)}</span>)}
        </article>
        <aside className="close-action-panel">
          <h2>{de ? "Abschluss" : "Close"}</h2>
          <p>{fiscalPeriods.nextClosablePeriod ? `${de ? "Nächste offene Periode" : "Next open period"} ${fiscalPeriods.nextClosablePeriod.id}` : (de ? "Keine überfällige Periode" : "No overdue period")}</p>
          <DunningPreviewButton label={de ? "Mahnlauf prüfen" : "Review dunning"} />
          <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-33" />
        </aside>
      </section>
    </main>
  );
}

function LedgerEvidenceWorkspace({
  data,
  ledgerRows,
  locale,
  summaryItems,
  trialBalance
}: {
  data: BusinessBundle;
  ledgerRows: ReturnType<typeof buildLedgerRows>;
  locale: SupportedLocale;
  summaryItems: Array<[string, string, string]>;
  trialBalance: ReturnType<typeof buildTrialBalance>;
}) {
  const de = locale === "de";
  const selected = ledgerRows[0];
  const accounts = trialBalance.slice(0, 10);
  return (
    <main className="business-accounting-page accounting-reference-page ledger-evidence-page" data-submodule="ledger">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title="Ledger"
        subtitle=""
        actions={<AccountingApiButton label={de ? "Setup prüfen" : "Check setup"} path="/api/business/accounting/setup" />}
      />
      <section className="ledger-shell">
        <nav className="ledger-account-nav" aria-label={de ? "Konten" : "Accounts"}>
          {accounts.map((row, index) => (
            <button aria-current={index === 0 ? "true" : undefined} key={row.account.id} type="button">
              <span>{row.account.code} {row.account.name}</span>
              <strong>{businessCurrency(row.balance, row.account.currency, locale)}</strong>
            </button>
          ))}
        </nav>
        <section className="ledger-entry-list">
          <header>
            <span>{de ? "Datum" : "Date"}</span>
            <span>{de ? "Buchung" : "Posting"}</span>
            <span>{de ? "Soll" : "Debit"}</span>
            <span>{de ? "Haben" : "Credit"}</span>
            <span>{de ? "Aktion" : "Action"}</span>
          </header>
          {ledgerRows.slice(0, 12).map((row, index) => (
            <article className={index === 0 ? "is-selected" : ""} key={row.id}>
              <span>{row.entry.postingDate}</span>
              <div>
                <strong>{text(row.entry.narration, locale)}</strong>
                <small>{row.entry.number} · {row.account.code} {row.account.name} · {row.refLabel}</small>
              </div>
              <strong>{row.debit ? businessCurrency(row.debit, row.account.currency, locale) : "-"}</strong>
              <strong>{row.credit ? businessCurrency(row.credit, row.account.currency, locale) : "-"}</strong>
              <span>{de ? "Festgeschrieben" : "Posted"}</span>
            </article>
          ))}
        </section>
        {selected ? (
          <aside className="ledger-detail-panel">
            <h2>{text(selected.entry.narration, locale)}</h2>
            <p>{selected.entry.number}</p>
            <dl>
              <div><dt>{de ? "Referenz" : "Reference"}</dt><dd>{selected.refLabel}</dd></div>
              <div><dt>{de ? "Status" : "Status"}</dt><dd>{statusLabel(selected.entry.status, locale)}</dd></div>
              <div><dt>{de ? "Buchungsdatum" : "Posting date"}</dt><dd>{selected.entry.postingDate}</dd></div>
              <div><dt>{de ? "Ausgeglichen" : "Balanced"}</dt><dd>{selected.entry.lines.reduce((sum, line) => sum + line.debit, 0) === selected.entry.lines.reduce((sum, line) => sum + line.credit, 0) ? "OK" : "!"}</dd></div>
            </dl>
            <section className="ledger-lines">
              {selected.entry.lines.map((line, index) => {
                const account = data.accounts.find((item) => item.id === line.accountId);
                return (
                  <div key={`${line.accountId}-${index}`}>
                    <span>{account?.code ?? line.accountId} {account?.name ?? ""}</span>
                    <strong>{line.debit ? `S ${businessCurrency(line.debit, account?.currency ?? "EUR", locale)}` : `H ${businessCurrency(line.credit, account?.currency ?? "EUR", locale)}`}</strong>
                  </div>
                );
              })}
            </section>
            <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-26" />
          </aside>
        ) : null}
      </section>
    </main>
  );
}

function BookkeepingOperationsWorkspace({
  bankRows,
  bookkeeping,
  datevLines,
  fiscalPeriods,
  locale,
  summaryItems
}: {
  bankRows: ReturnType<typeof buildReconciliationRows>;
  bookkeeping: BusinessBundle["bookkeeping"];
  datevLines: ReturnType<typeof buildDatevLines>;
  fiscalPeriods: ReturnType<typeof buildFiscalPeriodState>;
  locale: SupportedLocale;
  summaryItems: Array<[string, string, string]>;
}) {
  const de = locale === "de";
  const batch = bookkeeping[0];
  return (
    <main className="business-accounting-page accounting-reference-page bookkeeping-ops-page" data-submodule="bookkeeping">
      <FinanceReferenceHeader
        locale={locale}
        summaryItems={summaryItems}
        title={de ? "Buchhaltung" : "Bookkeeping"}
        subtitle=""
        actions={<DatevExportButton label={de ? "DATEV exportieren" : "Export DATEV"} />}
      />
      <section className="bookkeeping-shell">
        <article className="bookkeeping-primary">
          <header>
            <div>
              <p>{de ? "Nächster Stapel" : "Next batch"}</p>
              <h2>{batch ? `${batch.system} ${batch.period}` : "DATEV"}</h2>
            </div>
            <strong>{batch ? statusLabel(batch.status, locale) : "-"}</strong>
          </header>
          {batch ? (
            <dl>
              <div><dt>{de ? "Belege" : "Documents"}</dt><dd>{batch.invoiceIds.length}</dd></div>
              <div><dt>{de ? "Netto" : "Net"}</dt><dd>{businessCurrency(batch.netAmount, "EUR", locale)}</dd></div>
              <div><dt>{de ? "Steuer" : "Tax"}</dt><dd>{businessCurrency(batch.taxAmount, "EUR", locale)}</dd></div>
              <div><dt>{de ? "Fällig" : "Due"}</dt><dd>{batch.dueDate}</dd></div>
            </dl>
          ) : null}
          <div className="bookkeeping-action-row">
            <DatevExportButton label={de ? "DATEV CSV" : "DATEV CSV"} />
            <AccountingCtoxActionButton label={de ? "CTOX sagen" : "Ask CTOX"} locale={locale} storyId="story-32" />
          </div>
        </article>
        <article className="bookkeeping-secondary">
          <h2>{de ? "Bankimport" : "Bank import"}</h2>
          <p>{de ? `${bankRows.filter((row) => row.status !== "Matched").length} offene Bankzeilen vor dem Export klären.` : `${bankRows.filter((row) => row.status !== "Matched").length} open bank lines to clear before export.`}</p>
          <PaymentsHeaderActions locale={locale} />
        </article>
        <article className="bookkeeping-secondary">
          <h2>{de ? "Perioden" : "Periods"}</h2>
          <p>{fiscalPeriods.nextClosablePeriod ? `${de ? "Nächste Periode" : "Next period"} ${fiscalPeriods.nextClosablePeriod.id}` : (de ? "Keine überfällige Periode" : "No overdue period")}</p>
          <AccountingApiButton label={de ? "Periode schließen" : "Close period"} path="/api/business/accounting/period-close" />
        </article>
        <section className="datev-line-list">
          <header><span>{de ? "Konto" : "Account"}</span><span>{de ? "Gegenkonto" : "Contra"}</span><span>{de ? "Betrag" : "Amount"}</span><span>DATEV</span></header>
          {datevLines.slice(0, 10).map((line, index) => (
            <article key={`${line.entry.id}-${line.account.id}-${index}`}>
              <span>{line.account.code} {line.account.name}</span>
              <span>{line.contraAccount?.code ?? "-"}</span>
              <strong>{businessCurrency(line.amount, line.account.currency, locale)}</strong>
              <span>{line.taxCode || "-"}</span>
            </article>
          ))}
        </section>
      </section>
    </main>
  );
}

function ReportAccountColumn({ locale, rows, title }: { locale: SupportedLocale; rows: ReturnType<typeof buildTrialBalance>; title: string }) {
  return (
    <div className="report-account-column">
      <h3>{title}</h3>
      {rows.map((row) => (
        <div key={row.account.id}>
          <span>{row.account.code} {row.account.name}</span>
          <strong>{businessCurrency(row.balance, row.account.currency, locale)}</strong>
        </div>
      ))}
    </div>
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

function FinanceModeTabs({ locale, modes }: { locale: SupportedLocale; modes: FinanceMode[] }) {
  return (
    <div className="finance-mode-tabs" role="tablist" aria-label="Finance modes">
      <span className="finance-mode-label">{locale === "de" ? "Arbeitsfokus" : "Work focus"}</span>
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
        <span>Art</span>
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

type FinanceDecisionPanelProps = FinanceActionProps & {
  nextStep: BusinessNextStep;
  storyWorkflows: ReturnType<typeof storyWorkflowsForSubmodule>;
};

function FinanceDecisionPanel({
  accounting,
  assetToDepreciate,
  customer,
  invoice,
  locale,
  nextStep,
  receiptToPost,
  storyWorkflows,
  submoduleId,
  transactionToMatch
}: FinanceDecisionPanelProps) {
  return (
    <aside className="finance-inspector" aria-label={locale === "de" ? "Entscheidung und Freigabe" : "Decision and approval"}>
      <section className="finance-decision-panel">
        <FinanceInspector nextStep={nextStep} />
        <div className="finance-decision-actions">
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
        </div>
      </section>
      <AccountingStoryWorkflowPanel
        contextPrompt={ctoxPromptForDecision({ assetToDepreciate, customer, invoice, locale, receiptToPost, submoduleId, transactionToMatch })}
        locale={locale}
        recommendedStoryId={recommendedStoryIdForSubmodule(submoduleId)}
        stories={storyWorkflows}
        submoduleId={submoduleId}
      />
      <AccountingWorkflowPanel compact locale={locale} quiet />
    </aside>
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

  return null;
}

function ctoxPromptForDecision({
  assetToDepreciate,
  customer,
  invoice,
  locale,
  receiptToPost,
  submoduleId,
  transactionToMatch
}: Pick<FinanceActionProps, "assetToDepreciate" | "customer" | "invoice" | "locale" | "receiptToPost" | "submoduleId" | "transactionToMatch">) {
  const de = locale === "de";
  if (submoduleId === "payments" && transactionToMatch) {
    const amount = businessCurrency(transactionToMatch.amount, transactionToMatch.currency, locale);
    return de
      ? `Prüfe den Bankumsatz ${amount} von ${transactionToMatch.counterparty} und bereite den vorgeschlagenen Match zur Freigabe vor.`
      : `Review the ${amount} bank transaction from ${transactionToMatch.counterparty} and prepare the suggested match for approval.`;
  }
  if (submoduleId === "receipts" && receiptToPost) {
    return de
      ? `Prüfe den Eingangsbeleg ${receiptToPost.number} von ${receiptToPost.vendorName} und bereite die Buchung vor.`
      : `Review inbound receipt ${receiptToPost.number} from ${receiptToPost.vendorName} and prepare the posting.`;
  }
  if (submoduleId === "fixed-assets" && assetToDepreciate) {
    return de
      ? `Prüfe die Anlage ${assetToDepreciate.name} und bereite die nächste AfA-Buchung mit Bilanzwirkung vor.`
      : `Review asset ${assetToDepreciate.name} and prepare the next depreciation posting with balance sheet impact.`;
  }
  if (submoduleId === "bookkeeping") {
    return de
      ? "Bereite den nächsten DATEV-Export vor, prüfe offene Bankzeilen und markiere nur freigabefähige Buchungen."
      : "Prepare the next DATEV export, review open bank lines, and mark only postings ready for approval.";
  }
  if (submoduleId === "ledger") {
    return de
      ? `Zeige mir die Buchungskette zur Rechnung ${invoice.number} und markiere Storno- oder GoBD-relevante Auffälligkeiten.`
      : `Show me the posting chain for invoice ${invoice.number} and flag reversal or GoBD-relevant issues.`;
  }
  if (submoduleId === "reports") {
    return de
      ? "Leite Bilanz, GuV und UStVA aus dem Ledger ab und zeige mir vor dem Periodenabschluss nur die offenen Klärpunkte."
      : "Derive balance sheet, P&L, and VAT return from the ledger and show only open review points before period close.";
  }
  return de
    ? `Prüfe Rechnung ${invoice.number} für ${customer?.name ?? invoice.customerId}, bereite Versand, ZUGFeRD-PDF und Buchung zur Freigabe vor.`
    : `Review invoice ${invoice.number} for ${customer?.name ?? invoice.customerId}, then prepare delivery, ZUGFeRD PDF, and posting for approval.`;
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

function recommendedStoryIdForSubmodule(submoduleId: string) {
  const ids: Record<string, string> = {
    bookkeeping: "story-32",
    "fixed-assets": "story-39",
    invoices: "story-04",
    ledger: "story-26",
    payments: "story-05",
    receipts: "story-13",
    reports: "story-33"
  };
  return ids[submoduleId];
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
    return [...bankRows].sort((a, b) => bankRowPriority(a.status) - bankRowPriority(b.status)).slice(0, 10).map((row) => ({
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

function bankRowPriority(status: string) {
  if (status === "Suggested") return 0;
  if (status === "Unmatched") return 1;
  if (status === "Matched") return 3;
  return 2;
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
