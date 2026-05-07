import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import { SYSTEM_OWNER_PARTY_ID, type WarehouseState } from "@ctox-business/warehouse";
import {
  buildAccountingSnapshot,
  buildDatevLines,
  buildLedgerRows,
  buildReceiptQueue,
  buildReconciliationRows,
  buildTrialBalance,
  isBalanced
} from "../lib/accounting-runtime";
import {
  businessCurrency,
  getBusinessBundle,
  text,
  type BusinessAccount,
  type BusinessBankTransaction,
  type BusinessBookkeepingExport,
  type BusinessBundle,
  type BusinessCustomer,
  type BusinessInvoice,
  type BusinessJournalEntry,
  type BusinessProduct,
  type BusinessReceipt,
  type BusinessReport,
  type SupportedLocale
} from "../lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "../lib/business-db-bundle";
import { getWarehouseSnapshot } from "../lib/warehouse-runtime";
import { BusinessCreateForm, BusinessQueueButton } from "./business/business-actions";
import { InvoiceCustomerEditor, type InvoiceCustomerOption } from "./invoice-customer-editor";
import { InvoiceDeliveryActions } from "./invoice-delivery-actions";
import { InvoiceDocumentSelector, type InvoiceDocumentOption } from "./invoice-document-selector";
import { InvoiceListSidebar, type InvoiceListItem, type InvoiceListMetric } from "./invoice-list-sidebar";
import { InvoiceLinesEditor, type InvoiceLineDraft } from "./invoice-lines-editor";
import { LexicalRichTextEditor } from "./lexical-rich-text-editor";
import { WarehouseLineWorkflow } from "./warehouse-line-workflow";
import { WarehouseLayoutActions } from "./warehouse-layout-actions";
import { WarehouseOrderActionButton } from "./warehouse-order-action-button";
import { WarehouseStorageWorkbench } from "./warehouse-storage-workbench";
import { WarehouseWorkStepButton } from "./warehouse-work-step-button";

type QueryState = {
  locale?: string;
  orderSearch?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
  warehouseSearch?: string;
  selectedId?: string;
  drawer?: string;
};

type BusinessCopy = typeof businessCopy.en;

export async function BusinessWorkspace({
  submoduleId,
  query
}: {
  submoduleId: string;
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = businessCopy[locale];
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());

  if (submoduleId === "products") return <ProductsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "warehouse") return await WarehouseStorageView({ copy, data, locale, query, submoduleId });
  if (submoduleId === "fulfillment") return await FulfillmentView({ copy, data, locale, query, submoduleId });
  if (submoduleId === "invoices") return <InvoicesView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "ledger") return <LedgerView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "receipts") return <ReceiptsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "payments") return <PaymentsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "bookkeeping") return <BookkeepingView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "reports") return <ReportsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  return <CustomersView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
}

export async function BusinessPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = businessCopy[locale];
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const panel = panelState?.panel;
  const recordId = panelState?.recordId;

  if (panel === "new") {
    const resource = resolveNewResource(recordId, submoduleId);
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={copy.newRecord} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{copy.newRecordDescription}</p>
        <BusinessCreateForm
          amountLabel={copy.amount}
          customerLabel={copy.customer}
          customers={data.customers.map((customer) => ({ label: customer.name, value: customer.id }))}
          dueLabel={copy.due}
          queueLabel={copy.queueCreate}
          resource={resource}
          statusLabel={copy.status}
          subjectLabel={copy.subject}
          subjectPlaceholder={copy.subjectPlaceholder}
          taxLabel={copy.tax}
        />
      </div>
    );
  }

  const customer = data.customers.find((item) => item.id === recordId);
  const product = data.products.find((item) => item.id === recordId);
  const invoice = data.invoices.find((item) => item.id === recordId);
  const account = data.accounts.find((item) => item.id === recordId);
  const bankTransaction = data.bankTransactions.find((item) => item.id === recordId);
  const journalEntry = data.journalEntries.find((item) => item.id === recordId);
  const receipt = data.receipts.find((item) => item.id === recordId);
  const exportBatch = data.bookkeeping.find((item) => item.id === recordId);
  const report = data.reports.find((item) => item.id === recordId);
  const warehouse = (await getWarehouseSnapshot()).snapshot;
  const warehouseBalance = warehouse.balances.find((item) => item.balanceKey === recordId);
  const warehouseRecord = warehouseBalance
    ? {
        id: warehouseBalance.balanceKey,
        name: warehouseBalance.inventoryItemId,
        status: warehouseBalance.stockStatus,
        version: 1
      }
    : warehouse.locations.find((item) => item.id === recordId) ??
      warehouse.items.find((item) => item.id === recordId) ??
      warehouse.reservations.find((item) => item.id === recordId) ??
      warehouse.pickLists.find((item) => item.id === recordId) ??
      warehouse.shipments.find((item) => item.id === recordId) ??
      warehouse.returns.find((item) => item.id === recordId) ??
      warehouse.scannerSessions.find((item) => item.id === recordId) ??
      warehouse.scanEvents.find((item) => item.id === recordId) ??
      warehouse.cycleCounts.find((item) => item.id === recordId) ??
      warehouse.inventoryAdjustments.find((item) => item.id === recordId) ??
      warehouse.shipmentPackages.find((item) => item.id === recordId) ??
      warehouse.fulfillmentLabels.find((item) => item.id === recordId) ??
      warehouse.shipmentTrackingEvents.find((item) => item.id === recordId) ??
      warehouse.integrationEvents.find((item) => item.id === recordId) ??
      warehouse.roboticsEvents.find((item) => item.id === recordId) ??
      warehouse.wavePlans.find((item) => item.id === recordId) ??
      warehouse.slottingRecommendations.find((item) => item.id === recordId) ??
      warehouse.transfers.find((item) => item.id === recordId) ??
      warehouse.offlineSyncBatches.find((item) => item.id === recordId) ??
      warehouse.threePlCharges.find((item) => item.id === recordId) ??
      warehouse.receipts.find((item) => item.id === recordId);

  if (panel === "business-set") {
    const businessSet = resolveBusinessSet(recordId, data, locale, copy, warehouse);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={businessSet.title} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{businessSet.description}</p>
        <dl className="drawer-facts">
          <div><dt>{copy.items}</dt><dd>{businessSet.items.length}</dd></div>
          <div><dt>{copy.amount}</dt><dd>{businessCurrency(businessSet.amount, "EUR", locale)}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.selectedItems}</h3>
          <div className="ops-mini-list">
            {businessSet.items.map((item) => (
              <a
                data-context-item
                data-context-label={item.label}
                data-context-module="business"
                data-context-record-id={item.id}
                data-context-record-type={item.type}
                data-context-submodule={submoduleId}
                href={businessRecordHref(query, submoduleId, item.panel, item.id)}
                key={`${item.type}-${item.id}`}
              >
                {item.label} · {item.meta}
              </a>
            ))}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.syncRail}</h3>
          <BusinessQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Review and synchronize this Business context set: ${businessSet.title}.`}
            payload={{ filter: recordId, items: businessSet.items }}
            recordId={recordId ?? "business-set"}
            resource={businessSet.resource}
            title={`Sync Business set: ${businessSet.title}`}
          >
            {copy.askCtoxSet}
          </BusinessQueueButton>
        </section>
      </div>
    );
  }

  if (panel === "warehouse-admin") {
    return <WarehouseAdminPanel query={query} submoduleId={submoduleId} />;
  }

  if (panel === "warehouse-match") {
    return <WarehouseMatchPanel query={query} submoduleId={submoduleId} />;
  }

  if (panel === "text-template") {
    return <InvoiceTextTemplatePanel copy={copy} query={query} recordId={recordId} submoduleId={submoduleId} />;
  }

  if (customer) return <CustomerPanel copy={copy} customer={customer} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (product) return <ProductPanel copy={copy} data={data} locale={locale} product={product} query={query} submoduleId={submoduleId} />;
  if (invoice) return <InvoicePanel copy={copy} data={data} invoice={invoice} locale={locale} query={query} submoduleId={submoduleId} />;
  if (account) return <AccountPanel account={account} copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (bankTransaction) return <BankTransactionPanel bankTransaction={bankTransaction} copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (journalEntry) return <JournalEntryPanel copy={copy} data={data} entry={journalEntry} locale={locale} query={query} submoduleId={submoduleId} />;
  if (receipt) return <ReceiptPanel copy={copy} data={data} locale={locale} query={query} receipt={receipt} submoduleId={submoduleId} />;
  if (exportBatch) return <BookkeepingPanel copy={copy} data={data} exportBatch={exportBatch} locale={locale} query={query} submoduleId={submoduleId} />;
  if (report) return <ReportPanel copy={copy} data={data} locale={locale} query={query} report={report} submoduleId={submoduleId} />;
  if (warehouseRecord) return <WarehousePanel query={query} record={warehouseRecord} submoduleId={submoduleId} />;

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={copy.businessRecord} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{copy.noRecordSelected}</p>
    </div>
  );
}

function CustomersView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const openInvoices = data.invoices.filter((invoice) => invoice.status !== "Paid");
  const totalReceivables = data.customers.reduce((sum, customer) => sum + customer.arBalance, 0);

  return (
    <div className="ops-workspace ops-project-workspace">
      <section className="ops-pane ops-project-tree" aria-label={copy.customers}>
        <BusinessPaneHead description={copy.customersDescription} title={copy.customers}>
          <a
            aria-label={copy.newCustomer}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newCustomer}
            data-context-module="business"
            data-context-record-id="customer"
            data-context-record-type="customer"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "customer", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="ops-project-list">
          {data.customers.map((customer) => (
            <a
              className="ops-project-row"
              data-context-item
              data-context-label={customer.name}
              data-context-module="business"
              data-context-record-id={customer.id}
              data-context-record-type="customer"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "customer", customer.id, "right")}
              key={customer.id}
            >
              <span className={`ops-health ${customer.status === "Active" ? "ops-health-green" : "ops-health-amber"}`} />
              <strong>{customer.name}</strong>
              <small>{customer.segment} - {customer.country} - {customer.owner}</small>
              <small>{businessCurrency(customer.mrr, "EUR", locale)} MRR - {businessCurrency(customer.arBalance, "EUR", locale)} AR</small>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-work-items" aria-label={copy.receivables}>
        <BusinessPaneHead description={copy.receivablesDescription} title={copy.receivables} />
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.invoice}</span>
            <span>{copy.customer}</span>
            <span>{copy.due}</span>
            <span>{copy.amount}</span>
          </div>
          {openInvoices.map((invoice) => {
            const customer = data.customers.find((item) => item.id === invoice.customerId);
            return (
              <a
                className="ops-table-row"
                data-context-item
                data-context-label={invoice.number}
                data-context-module="business"
                data-context-record-id={invoice.id}
                data-context-record-type="invoice"
                data-context-submodule={submoduleId}
                href={businessPanelHref(query, submoduleId, "invoice", invoice.id, "right")}
                key={invoice.id}
              >
                <span><strong>{invoice.number}</strong><small>{invoice.status}</small></span>
                <span><strong>{customer?.name}</strong><small>{customer?.paymentTerms}</small></span>
                <span><strong>{invoice.dueDate}</strong><small>{invoice.issueDate}</small></span>
                <span><strong>{businessCurrency(invoice.total, invoice.currency, locale)}</strong><small>{copy.tax}: {businessCurrency(invoice.taxAmount, invoice.currency, locale)}</small></span>
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.syncRail}>
        <BusinessPaneHead title={copy.syncRail} description={copy.syncRailDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "customers", "right")} label={copy.customers} value={String(data.customers.length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "receivables", "right")} label={copy.receivables} value={businessCurrency(totalReceivables, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "overdue", "right")} label={copy.overdue} value={String(openInvoices.filter((invoice) => invoice.status === "Overdue").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "tax-review", "right")} label={copy.taxReview} value={String(data.bookkeeping.filter((item) => item.status === "Needs review").length)} />
        </div>
        <div className="ops-card-stack">
          {data.customers.map((customer) => (
            <a
              className="ops-work-card priority-high"
              data-context-item
              data-context-label={`${customer.name} ${copy.billing}`}
              data-context-module="business"
              data-context-record-id={customer.id}
              data-context-record-type="customer"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "customer", customer.id, "right")}
              key={customer.id}
            >
              <strong>{customer.name}</strong>
              <small>{customer.billingEmail}</small>
              <span>{text(customer.notes, locale)}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function ProductsView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  return (
    <div className="ops-workspace ops-project-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.products}>
        <BusinessPaneHead description={copy.productsDescription} title={copy.products}>
          <a
            aria-label={copy.newProduct}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newProduct}
            data-context-module="business"
            data-context-record-id="product"
            data-context-record-type="product"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "product", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.product}</span>
            <span>{copy.price}</span>
            <span>{copy.tax}</span>
            <span>{copy.account}</span>
          </div>
          {data.products.map((product) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={product.name}
              data-context-module="business"
              data-context-record-id={product.id}
              data-context-record-type="product"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "product", product.id, "right")}
              key={product.id}
            >
              <span><strong>{product.name}</strong><small>{product.sku} - {product.type}</small></span>
              <span><strong>{businessCurrency(product.price, "EUR", locale)}</strong><small>{copy.margin}: {product.margin}%</small></span>
              <span><strong>{product.taxRate}%</strong><small>{product.status}</small></span>
              <span><strong>{product.revenueAccount}</strong><small>{text(product.description, locale)}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-project-tree" aria-label={copy.revenueUse}>
        <BusinessPaneHead description={copy.revenueUseDescription} title={copy.revenueUse} />
        <div className="ops-project-list">
          {data.products.map((product) => {
            const lines = data.invoices.flatMap((invoice) => invoice.lines.map((line) => ({ invoice, line }))).filter(({ line }) => line.productId === product.id);
            const revenue = lines.reduce((sum, { line }) => sum + line.quantity * line.unitPrice, 0);
            return (
              <a
                className="ops-project-row"
                data-context-item
                data-context-label={`${product.name} revenue`}
                data-context-module="business"
                data-context-record-id={product.id}
                data-context-record-type="product"
                data-context-submodule={submoduleId}
                href={businessPanelHref(query, submoduleId, "product", product.id, "right")}
                key={product.id}
              >
                <strong>{product.name}</strong>
                <small>{lines.length} {copy.invoiceLines} - {businessCurrency(revenue, "EUR", locale)}</small>
                <meter max="100" min="0" value={product.margin} />
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.taxSetup}>
        <BusinessPaneHead title={copy.taxSetup} description={copy.taxSetupDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "products-billable", "right")} label={copy.billable} value={String(data.products.filter((item) => item.status === "Billable").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "products-review", "right")} label={copy.review} value={String(data.products.filter((item) => item.status === "Review").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "products-draft", "right")} label={copy.draft} value={String(data.products.filter((item) => item.status === "Draft").length)} />
        </div>
      </section>
    </div>
  );
}

function InvoicesView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const selectedInvoiceId = query.selectedId ?? (query.panel === "invoice" ? query.recordId : undefined);
  const selectedInvoice = data.invoices.find((invoice) => invoice.id === selectedInvoiceId) ?? data.invoices[0];
  const receivableInvoices = data.invoices.filter((invoice) => invoice.status !== "Paid" && invoice.status !== "Draft");
  const overdueInvoices = data.invoices.filter((invoice) => invoice.status === "Overdue");
  const reminderInvoices = data.invoices.filter((invoice) => invoice.collectionStatus === "Reminder due" || invoice.collectionStatus === "Reminder sent" || invoice.collectionStatus === "Final notice");
  const openAmount = receivableInvoices.reduce((sum, invoice) => sum + invoiceBalance(invoice), 0);
  const listItems: InvoiceListItem[] = data.invoices.map((invoice) => {
    const customer = data.customers.find((item) => item.id === invoice.customerId);
    return {
      amountLabel: businessCurrency(invoice.total, invoice.currency, locale),
      collectionStatus: invoiceAgeLabel(invoice, locale, copy),
      customerName: customer?.name ?? invoice.customerId,
      documentTitle: invoice.documentTitle ?? copy.invoice,
      href: businessSelectionHref(query, submoduleId, invoice.id),
      id: invoice.id,
      meta: invoice.number,
      reminderLevel: invoice.reminderLevel,
      searchText: [customer?.name, invoice.number, invoice.documentTitle, invoice.total, invoice.currency, invoice.status].filter(Boolean).join(" "),
      status: invoice.status
    };
  });
  const metrics: InvoiceListMetric[] = [
    { href: businessPanelHref(query, submoduleId, "business-set", "receivables", "right"), label: copy.receivables, value: businessCurrency(openAmount, "EUR", locale) },
    { href: businessPanelHref(query, submoduleId, "business-set", "overdue", "right"), label: copy.overdue, value: String(overdueInvoices.length) },
    { href: businessPanelHref(query, submoduleId, "business-set", "reminders-due", "right"), label: copy.reminders, value: String(reminderInvoices.length) }
  ];

  return (
    <div className="invoice-workspace invoice-editor-workspace">
      <InvoiceListSidebar
        copy={copy}
        createHref={businessPanelHref(query, submoduleId, "new", "invoice", "left-bottom")}
        items={listItems}
        metrics={metrics}
        selectedInvoiceId={selectedInvoice.id}
      />

      {selectedInvoice ? (
        selectedInvoice.status === "Draft" ? (
          <InvoiceEditor copy={copy} data={data} invoice={selectedInvoice} locale={locale} query={query} submoduleId={submoduleId} />
        ) : (
          <InvoicePreviewPane copy={copy} data={data} invoice={selectedInvoice} locale={locale} query={query} submoduleId={submoduleId} />
        )
      ) : null}
    </div>
  );
}

function LedgerView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const snapshot = buildAccountingSnapshot(data);
  const trialBalance = buildTrialBalance(data);
  const ledgerRows = buildLedgerRows(data).slice(0, 18);
  const unbalanced = data.journalEntries.filter((entry) => !isBalanced(entry));

  return (
    <div className="ops-workspace accounting-workspace">
      <section className="ops-pane accounting-control-pane" aria-label={copy.ledger}>
        <BusinessPaneHead description={copy.ledgerDescription} title={copy.ledger}>
          <a
            aria-label={copy.newJournalEntry}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newJournalEntry}
            data-context-module="business"
            data-context-record-id="journal-entry"
            data-context-record-type="journal-entry"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "journal-entry", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="accounting-kpi-strip">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "receivables", "right")} label={copy.receivables} value={businessCurrency(snapshot.receivableBalance, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "payables", "right")} label={copy.payables} value={businessCurrency(snapshot.payableBalance, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "vat-payable", "right")} label={copy.vatPayable} value={businessCurrency(snapshot.vatPayable, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "unbalanced", "right")} label={copy.unbalanced} value={String(unbalanced.length)} />
        </div>
        <div className="ops-table ops-work-table accounting-ledger-table">
          <div className="ops-table-head">
            <span>{copy.date}</span>
            <span>{copy.account}</span>
            <span>{copy.reference}</span>
            <span>{copy.debit}</span>
            <span>{copy.credit}</span>
          </div>
          {ledgerRows.map((row) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={`${row.entry.number} ${row.account.code}`}
              data-context-module="business"
              data-context-record-id={row.entry.id}
              data-context-record-type="journal-entry"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "journal-entry", row.entry.id, "right")}
              key={row.id}
            >
              <span><strong>{row.entry.postingDate}</strong><small>{row.entry.number}</small></span>
              <span><strong>{row.account.code} {row.account.name}</strong><small>{row.partyLabel}</small></span>
              <span><strong>{row.refLabel}</strong><small>{text(row.entry.narration, locale)}</small></span>
              <span><strong>{row.debit ? businessCurrency(row.debit, row.account.currency, locale) : "-"}</strong><small>{row.entry.status}</small></span>
              <span><strong>{row.credit ? businessCurrency(row.credit, row.account.currency, locale) : "-"}</strong><small>{row.account.rootType}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane accounting-trial-pane" aria-label={copy.trialBalance}>
        <BusinessPaneHead description={copy.trialBalanceDescription} title={copy.trialBalance} />
        <div className="ops-table ops-work-table accounting-balance-table">
          <div className="ops-table-head">
            <span>{copy.account}</span>
            <span>{copy.debit}</span>
            <span>{copy.credit}</span>
            <span>{copy.balance}</span>
          </div>
          {trialBalance.map((row) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={`${row.account.code} ${row.account.name}`}
              data-context-module="business"
              data-context-record-id={row.account.id}
              data-context-record-type="account"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "account", row.account.id, "right")}
              key={row.account.id}
            >
              <span><strong>{row.account.code} {row.account.name}</strong><small>{row.account.rootType} · {row.account.accountType}</small></span>
              <span><strong>{businessCurrency(row.debit, row.account.currency, locale)}</strong><small>{copy.debit}</small></span>
              <span><strong>{businessCurrency(row.credit, row.account.currency, locale)}</strong><small>{copy.credit}</small></span>
              <span><strong>{businessCurrency(row.balance, row.account.currency, locale)}</strong><small>{row.account.taxCode ?? copy.noTaxCode}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.accountingControls}>
        <BusinessPaneHead title={copy.accountingControls} description={copy.accountingControlsDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "posted-journal", "right")} label={copy.posted} value={String(data.journalEntries.filter((entry) => entry.status === "Posted").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "draft-journal", "right")} label={copy.draft} value={String(data.journalEntries.filter((entry) => entry.status === "Draft").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "revenue", "right")} label={copy.revenue} value={businessCurrency(snapshot.revenueTotal, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "expenses", "right")} label={copy.expenses} value={businessCurrency(snapshot.expenseTotal, "EUR", locale)} />
        </div>
        <div className="ops-card-stack">
          {data.journalEntries.slice(0, 5).map((entry) => (
            <a
              className={`ops-work-card ${isBalanced(entry) ? "" : "priority-urgent"}`}
              data-context-item
              data-context-label={entry.number}
              data-context-module="business"
              data-context-record-id={entry.id}
              data-context-record-type="journal-entry"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "journal-entry", entry.id, "right")}
              key={entry.id}
            >
              <strong>{entry.number} · {entry.type}</strong>
              <small>{entry.postingDate} · {entry.status}</small>
              <span>{text(entry.narration, locale)}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function ReceiptsView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const receipts = buildReceiptQueue(data);
  const reviewTotal = receipts.filter((receipt) => receipt.status === "Needs review" || receipt.status === "Inbox").reduce((sum, receipt) => sum + receipt.total, 0);

  return (
    <div className="ops-workspace accounting-workspace accounting-receipts-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.receipts}>
        <BusinessPaneHead description={copy.receiptsDescription} title={copy.receipts}>
          <a
            aria-label={copy.newReceipt}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newReceipt}
            data-context-module="business"
            data-context-record-id="receipt"
            data-context-record-type="receipt"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "receipt", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="receipt-inbox-toolbar">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "receipts-review", "right")} label={copy.needsReview} value={String(receipts.filter((receipt) => receipt.status === "Needs review").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "receipts-inbox", "right")} label={copy.inbox} value={String(receipts.filter((receipt) => receipt.status === "Inbox").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "receipts-open-total", "right")} label={copy.openAmount} value={businessCurrency(reviewTotal, "EUR", locale)} />
        </div>
        <div className="ops-table ops-work-table accounting-receipt-table">
          <div className="ops-table-head">
            <span>{copy.receipt}</span>
            <span>{copy.vendor}</span>
            <span>{copy.tax}</span>
            <span>{copy.bankMatch}</span>
            <span>{copy.amount}</span>
          </div>
          {receipts.map((receipt) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={receipt.number}
              data-context-module="business"
              data-context-record-id={receipt.id}
              data-context-record-type="receipt"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "receipt", receipt.id, "right")}
              key={receipt.id}
            >
              <span><strong>{receipt.number}</strong><small>{receipt.receiptDate} · {receipt.status}</small></span>
              <span><strong>{receipt.vendorName}</strong><small>{receipt.attachmentName}</small></span>
              <span><strong>{receipt.taxCode}</strong><small>{businessCurrency(receipt.taxAmount, receipt.currency, locale)}</small></span>
              <span><strong>{receipt.bankTransaction?.status ?? copy.notMatched}</strong><small>{receipt.bankTransaction?.counterparty ?? receipt.source}</small></span>
              <span><strong>{businessCurrency(receipt.total, receipt.currency, locale)}</strong><small>{receipt.expenseAccount.code} {receipt.expenseAccount.name}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.receiptReview}>
        <BusinessPaneHead title={copy.receiptReview} description={copy.receiptReviewDescription} />
        <div className="ops-card-stack">
          {receipts.filter((receipt) => receipt.status === "Needs review" || receipt.status === "Inbox").map((receipt) => (
            <a
              className="ops-work-card priority-high"
              data-context-item
              data-context-label={receipt.number}
              data-context-module="business"
              data-context-record-id={receipt.id}
              data-context-record-type="receipt"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "receipt", receipt.id, "right")}
              key={receipt.id}
            >
              <strong>{receipt.vendorName}</strong>
              <small>{receipt.number} · {businessCurrency(receipt.total, receipt.currency, locale)}</small>
              <span>{text(receipt.notes, locale)}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function PaymentsView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const snapshot = buildAccountingSnapshot(data);
  const bankRows = buildReconciliationRows(data);

  return (
    <div className="ops-workspace accounting-workspace accounting-payments-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.payments}>
        <BusinessPaneHead description={copy.paymentsDescription} title={copy.payments} />
        <div className="receipt-inbox-toolbar">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "bank-matched", "right")} label={copy.matched} value={String(bankRows.filter((row) => row.status === "Matched").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "bank-suggested", "right")} label={copy.suggested} value={String(bankRows.filter((row) => row.status === "Suggested").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "bank-unmatched", "right")} label={copy.unmatched} value={String(bankRows.filter((row) => row.status === "Unmatched").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "bank-balance", "right")} label={copy.bankBalance} value={businessCurrency(snapshot.bankBalance, "EUR", locale)} />
        </div>
        <div className="ops-table ops-work-table accounting-bank-table">
          <div className="ops-table-head">
            <span>{copy.date}</span>
            <span>{copy.counterparty}</span>
            <span>{copy.match}</span>
            <span>{copy.confidence}</span>
            <span>{copy.amount}</span>
          </div>
          {bankRows.map((row) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={`${row.counterparty} ${row.amount}`}
              data-context-module="business"
              data-context-record-id={row.id}
              data-context-record-type="bank-transaction"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "bank-transaction", row.id, "right")}
              key={row.id}
            >
              <span><strong>{row.bookingDate}</strong><small>{row.valueDate}</small></span>
              <span><strong>{row.counterparty}</strong><small>{row.purpose}</small></span>
              <span><strong>{row.status}</strong><small>{row.matchedLabel}</small></span>
              <span><strong>{row.confidence}%</strong><small>{row.nextAction}</small></span>
              <span><strong>{businessCurrency(row.amount, row.currency, locale)}</strong><small>{row.matchType ?? copy.manual}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.reconciliation}>
        <BusinessPaneHead title={copy.reconciliation} description={copy.reconciliationDescription} />
        <div className="ops-card-stack">
          {bankRows.filter((row) => row.status !== "Matched").map((row) => (
            <a
              className={`ops-work-card ${row.status === "Unmatched" ? "priority-urgent" : "priority-high"}`}
              data-context-item
              data-context-label={row.counterparty}
              data-context-module="business"
              data-context-record-id={row.id}
              data-context-record-type="bank-transaction"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "bank-transaction", row.id, "right")}
              key={row.id}
            >
              <strong>{row.counterparty}</strong>
              <small>{row.status} · {businessCurrency(row.amount, row.currency, locale)}</small>
              <span>{row.nextAction}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function BookkeepingView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const snapshot = buildAccountingSnapshot(data);
  const datevLines = buildDatevLines(data);

  return (
    <div className="ops-workspace ops-project-workspace">
      <section className="ops-pane ops-project-tree" aria-label={copy.bookkeeping}>
        <BusinessPaneHead description={copy.bookkeepingDescription} title={copy.bookkeeping}>
          <a
            aria-label={copy.newExport}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newExport}
            data-context-module="business"
            data-context-record-id="export"
            data-context-record-type="bookkeeping"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "export", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="ops-project-list">
          {data.bookkeeping.map((exportBatch) => (
            <a
              className="ops-project-row"
              data-context-item
              data-context-label={`${exportBatch.system} ${exportBatch.period}`}
              data-context-module="business"
              data-context-record-id={exportBatch.id}
              data-context-record-type="bookkeeping"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "export", exportBatch.id, "right")}
              key={exportBatch.id}
            >
              <strong>{exportBatch.system} - {exportBatch.period}</strong>
              <small>{exportBatch.status} - {exportBatch.reviewer}</small>
              <small>{businessCurrency(exportBatch.netAmount, "EUR", locale)} net - {businessCurrency(exportBatch.taxAmount, "EUR", locale)} tax</small>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-work-items" aria-label={copy.exportLines}>
        <BusinessPaneHead description={copy.exportLinesDescription} title={copy.exportLines} />
        <div className="ops-table ops-work-table accounting-datev-table">
          <div className="ops-table-head">
            <span>{copy.date}</span>
            <span>{copy.account}</span>
            <span>{copy.contraAccount}</span>
            <span>{copy.taxCode}</span>
            <span>{copy.amount}</span>
          </div>
          {datevLines.slice(0, 18).map((line) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={`${line.entry.number} ${line.account.code}`}
              data-context-module="business"
              data-context-record-id={line.entry.id}
              data-context-record-type="journal-entry"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, "ledger", "journal-entry", line.entry.id, "right")}
              key={`${line.entry.id}-${line.account.id}-${line.side}-${line.amount}`}
            >
              <span><strong>{line.entry.postingDate}</strong><small>{line.entry.number}</small></span>
              <span><strong>{line.account.code}</strong><small>{line.account.name}</small></span>
              <span><strong>{line.contraAccount?.code ?? "-"}</strong><small>{line.contraAccount?.name ?? copy.splitPosting}</small></span>
              <span><strong>{line.taxCode || "-"}</strong><small>{line.side}</small></span>
              <span><strong>{businessCurrency(line.amount, line.account.currency, locale)}</strong><small>{line.entry.status}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.exportReadiness}>
        <BusinessPaneHead title={copy.exportReadiness} description={copy.exportReadinessDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "exports-ready", "right")} label={copy.ready} value={String(data.bookkeeping.filter((item) => item.status === "Ready").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "tax-review", "right")} label={copy.review} value={String(data.bookkeeping.filter((item) => item.status === "Needs review").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "exports-queued", "right")} label={copy.queued} value={String(data.bookkeeping.filter((item) => item.status === "Queued").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "vat-payable", "right")} label={copy.vatPayable} value={businessCurrency(snapshot.vatPayable, "EUR", locale)} />
        </div>
      </section>
    </div>
  );
}

function ReportsView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
  const snapshot = buildAccountingSnapshot(data);
  const trialBalance = buildTrialBalance(data);
  const profit = snapshot.revenueTotal - snapshot.expenseTotal;

  return (
    <div className="ops-workspace ops-project-workspace">
      <section className="ops-pane ops-work-items" aria-label={copy.reports}>
        <BusinessPaneHead description={copy.reportsDescription} title={copy.reports}>
          <a
            aria-label={copy.newReport}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newReport}
            data-context-module="business"
            data-context-record-id="report"
            data-context-record-type="report"
            data-context-submodule={submoduleId}
            href={businessPanelHref(query, submoduleId, "new", "report", "left-bottom")}
          >
            +
          </a>
        </BusinessPaneHead>
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.report}</span>
            <span>{copy.status}</span>
            <span>{copy.due}</span>
            <span>{copy.amount}</span>
          </div>
          {data.reports.map((report) => (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={report.title}
              data-context-module="business"
              data-context-record-id={report.id}
              data-context-record-type="report"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "report", report.id, "right")}
              key={report.id}
            >
              <span><strong>{report.title}</strong><small>{report.period}</small></span>
              <span><strong>{report.status}</strong><small>{report.exportContext}</small></span>
              <span><strong>{report.dueDate}</strong><small>{report.taxContext}</small></span>
              <span><strong>{businessCurrency(report.amount, "EUR", locale)}</strong><small>{report.linkedExportIds.join(", ")}</small></span>
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.reportSignals}>
        <BusinessPaneHead title={copy.reportSignals} description={copy.reportSignalsDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "revenue", "right")} label={copy.revenue} value={businessCurrency(snapshot.revenueTotal, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "expenses", "right")} label={copy.expenses} value={businessCurrency(snapshot.expenseTotal, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "vat-payable", "right")} label={copy.vatPayable} value={businessCurrency(snapshot.vatPayable, "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "open-reports", "right")} label={copy.openReports} value={String(data.reports.filter((report) => report.status !== "Current").length)} />
        </div>
        <div className="finance-report-summary">
          <div>
            <span>{copy.profit}</span>
            <strong>{businessCurrency(profit, "EUR", locale)}</strong>
          </div>
          <div>
            <span>{copy.bankBalance}</span>
            <strong>{businessCurrency(snapshot.bankBalance, "EUR", locale)}</strong>
          </div>
        </div>
        <div className="ops-card-stack">
          {trialBalance.slice(0, 4).map((row) => (
            <a
              className="ops-work-card"
              data-context-item
              data-context-label={`${row.account.code} ${row.account.name}`}
              data-context-module="business"
              data-context-record-id={row.account.id}
              data-context-record-type="account"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, "ledger", "account", row.account.id, "right")}
              key={row.account.id}
            >
              <strong>{row.account.code} {row.account.name}</strong>
              <small>{businessCurrency(row.balance, row.account.currency, locale)}</small>
              <span>{row.account.rootType} · {row.account.accountType}</span>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function CustomerPanel({ copy, customer, data, locale, query, submoduleId }: {
  copy: BusinessCopy;
  customer: BusinessCustomer;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const invoices = data.invoices.filter((invoice) => invoice.customerId === customer.id);
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={customer.name} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(customer.notes, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.segment}</dt><dd>{customer.segment}</dd></div>
        <div><dt>{copy.owner}</dt><dd>{customer.owner}</dd></div>
        <div><dt>{copy.taxId}</dt><dd>{customer.taxId}</dd></div>
        <div><dt>{copy.paymentTerms}</dt><dd>{customer.paymentTerms}</dd></div>
        <div><dt>{copy.receivables}</dt><dd>{businessCurrency(customer.arBalance, "EUR", locale)}</dd></div>
      </dl>
      <BusinessRecordList title={copy.invoices} items={invoices.map((invoice) => `${invoice.number} - ${invoice.status} - ${businessCurrency(invoice.total, invoice.currency, locale)}`)} />
      <BusinessQueueButton
        action="sync"
        className="drawer-primary"
        instruction={`Synchronize Business customer ${customer.name} with invoices, tax profile, Sales CRM account, and CTOX core context.`}
        payload={{ customer, invoices }}
        recordId={customer.id}
        resource="customers"
        title={`Sync customer: ${customer.name}`}
      >
        {copy.askCtoxSync}
      </BusinessQueueButton>
    </div>
  );
}

function ProductPanel({ copy, data, locale, product, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  product: BusinessProduct;
  query: QueryState;
  submoduleId: string;
}) {
  const invoiceLines = data.invoices.flatMap((invoice) => invoice.lines.map((line) => ({ invoice, line }))).filter(({ line }) => line.productId === product.id);
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={product.name} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(product.description, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.sku}</dt><dd>{product.sku}</dd></div>
        <div><dt>{copy.price}</dt><dd>{businessCurrency(product.price, "EUR", locale)}</dd></div>
        <div><dt>{copy.tax}</dt><dd>{product.taxRate}%</dd></div>
        <div><dt>{copy.account}</dt><dd>{product.revenueAccount}</dd></div>
        <div><dt>{copy.margin}</dt><dd>{product.margin}%</dd></div>
      </dl>
      <BusinessRecordList title={copy.invoiceLines} items={invoiceLines.map(({ invoice, line }) => `${invoice.number} - ${line.quantity} x ${businessCurrency(line.unitPrice, invoice.currency, locale)}`)} />
      <BusinessQueueButton
        action="sync"
        className="drawer-primary"
        instruction={`Synchronize Business product ${product.name} with invoice line items, revenue account, tax setup, and product benchmark context.`}
        payload={{ product, invoiceLines }}
        recordId={product.id}
        resource="products"
        title={`Sync product: ${product.name}`}
      >
        {copy.askCtoxSync}
      </BusinessQueueButton>
    </div>
  );
}

function InvoicePanel({ copy, data, invoice, locale, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  invoice: BusinessInvoice;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const lines = invoice.lines.map((line) => {
    const product = data.products.find((item) => item.id === line.productId);
    return `${product?.name ?? line.productId} - ${line.quantity} x ${businessCurrency(line.unitPrice, invoice.currency, locale)} - ${line.taxRate}%`;
  });

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={invoice.number} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(invoice.notes, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.customer}</dt><dd>{customer?.name}</dd></div>
        <div><dt>{copy.status}</dt><dd>{invoice.status}</dd></div>
        <div><dt>{copy.collection}</dt><dd>{invoice.collectionStatus ?? "-"}</dd></div>
        <div><dt>{copy.due}</dt><dd>{invoice.dueDate}</dd></div>
        <div><dt>{copy.amount}</dt><dd>{businessCurrency(invoice.total, invoice.currency, locale)}</dd></div>
        <div><dt>{copy.toReceive}</dt><dd>{businessCurrency(invoiceBalance(invoice), invoice.currency, locale)}</dd></div>
        <div><dt>{copy.tax}</dt><dd>{businessCurrency(invoice.taxAmount, invoice.currency, locale)}</dd></div>
      </dl>
      <section className="ops-drawer-section">
        <h3>{copy.documentTexts}</h3>
        <div className="ops-mini-list">
          <span>{text(invoice.introText ?? invoice.notes, locale)}</span>
          <span>{text(invoice.paymentTermsText ?? customer?.paymentTerms ?? copy.paymentTerms, locale)}</span>
          <span>{text(invoice.closingText ?? invoice.notes, locale)}</span>
        </div>
      </section>
      <BusinessRecordList title={copy.lines} items={lines} />
      <BusinessRecordList title={copy.payments} items={(invoice.payments ?? []).map((payment) => `${payment.date} - ${text(payment.label, locale)} - ${businessCurrency(payment.amount, invoice.currency, locale)}`)} />
      <div className="ops-action-dock">
        <BusinessQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Synchronize Business invoice ${invoice.number} with customer, products, tax, and CTOX core record context.`}
          payload={{ invoice, customer }}
          recordId={invoice.id}
          resource="invoices"
          title={`Sync invoice: ${invoice.number}`}
        >
          {copy.askCtoxSync}
        </BusinessQueueButton>
        <BusinessQueueButton
          action="export"
          instruction={`Prepare invoice ${invoice.number} for bookkeeping export and preserve tax context.`}
          payload={{ invoice, customer }}
          recordId={invoice.id}
          resource="invoices"
          title={`Export invoice: ${invoice.number}`}
        >
          {copy.queueExport}
        </BusinessQueueButton>
      </div>
    </div>
  );
}

function InvoiceEditor({ copy, data, invoice, locale, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  invoice: BusinessInvoice;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const address = invoice.addressLines ?? [customer?.name ?? "", "", "", customer?.country ?? ""];
  const addressExtra = address[1] ?? "";
  const street = address[2] ?? "";
  const postalCity = address[3] ?? "";
  const country = address[4] ?? customer?.country ?? "";
  const [postalCode = "", ...cityParts] = postalCity.split(" ");
  const city = cityParts.join(" ");
  const customerOptions: InvoiceCustomerOption[] = data.customers.map((item) => ({
    addressExtra: item.id === customer?.id ? addressExtra : "",
    city: item.id === customer?.id ? city : "",
    country: item.id === customer?.id ? country || item.country : item.country,
    customerNumber: item.id === customer?.id ? invoice.customerNumber : "",
    id: item.id,
    name: item.name,
    postalCode: item.id === customer?.id ? postalCode : "",
    street: item.id === customer?.id ? street : ""
  }));
  const lineDrafts: InvoiceLineDraft[] = invoice.lines.map((line, index) => {
    const product = data.products.find((item) => item.id === line.productId);
    return {
      currency: invoice.currency,
      description: text(product?.description ?? "", locale),
      id: `${invoice.id}-${line.productId}-${index}`,
      quantity: line.quantity,
      taxRate: line.taxRate,
      title: product?.name ?? line.productId,
      unit: product?.type === "Service" ? copy.hour : copy.piece,
      unitPrice: line.unitPrice
    };
  });

  return (
    <section className="ops-pane invoice-editor-pane" aria-label={copy.documentEditor}>
      <div className="invoice-editor-topbar">
        <div>
          <h2>{invoice.documentTitle ?? copy.invoice} {copy.edit}</h2>
        </div>
        <div className="invoice-mode-toggle" aria-label={copy.taxMode}>
          <span>{copy.gross}</span>
          <strong>{copy.net}</strong>
        </div>
      </div>

      <div className="invoice-editor-scroll">
        <InvoiceCustomerEditor
          copy={copy}
          customers={customerOptions}
          initialCustomerId={customer?.id ?? customerOptions[0]?.id ?? "manual"}
          invoice={{
            dueDate: invoice.dueDate,
            issueDate: invoice.issueDate,
            number: invoice.number,
            serviceDate: invoice.serviceDate
          }}
        />

        <section className="invoice-editor-card invoice-text-card invoice-document-text-card">
          <InvoiceField icon label={copy.documentTitle} value={invoice.documentTitle ?? copy.invoice} />
          <LexicalRichTextEditor
            initialText={text(invoice.introText ?? invoice.notes, locale)}
            label={copy.introText}
            locale={locale}
            namespace={`invoice-${invoice.id}-intro`}
            placeholder={copy.introTextPlaceholder}
            templateHref={businessPanelHref(query, submoduleId, "text-template", "introText", "right")}
          />
        </section>

        <InvoiceLinesEditor copy={copy} initialLines={lineDrafts} locale={locale} />

        <section className="invoice-editor-card invoice-text-card invoice-payment-text-card">
          <LexicalRichTextEditor
            initialText={text(invoice.paymentTermsText ?? customer?.paymentTerms ?? copy.paymentConditionPlaceholder, locale)}
            label={copy.paymentCondition}
            locale={locale}
            namespace={`invoice-${invoice.id}-payment-terms`}
            placeholder={copy.paymentConditionPlaceholder}
            templateHref={businessPanelHref(query, submoduleId, "text-template", "paymentCondition", "right")}
          />
          <LexicalRichTextEditor
            initialText={text(invoice.closingText ?? invoice.notes, locale)}
            label={copy.closingNote}
            locale={locale}
            namespace={`invoice-${invoice.id}-closing`}
            placeholder={copy.closingNotePlaceholder}
            templateHref={businessPanelHref(query, submoduleId, "text-template", "closingNote", "right")}
          />
        </section>
      </div>

      <div className="invoice-editor-footer">
        <a href={businessPanelHref(query, submoduleId, "invoice", invoice.id, "right")}>{copy.moreDetails}</a>
        <BusinessQueueButton
          action="payment"
          instruction={`Register or reconcile payment for invoice ${invoice.number}.`}
          payload={{ invoice }}
          recordId={invoice.id}
          resource="invoices"
          title={`Register payment: ${invoice.number}`}
        >
          {copy.capturePayment}
        </BusinessQueueButton>
        <InvoiceDeliveryActions copy={copy} customer={customer} invoice={invoice} locale={locale} />
      </div>
    </section>
  );
}

function InvoicePreviewPane({ copy, data, invoice, locale, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  invoice: BusinessInvoice;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const documents = invoiceDocumentsForInvoice(copy, data, invoice, locale);

  return (
    <section className="ops-pane invoice-editor-pane invoice-preview-editor-pane" aria-label={copy.preview}>
      <div className="invoice-editor-topbar">
        <div>
          <h2>{invoice.documentTitle ?? copy.invoice} {invoice.number}</h2>
          <p>{customer?.name} - {invoice.status} - {invoiceAgeLabel(invoice, locale, copy)}</p>
        </div>
        <div className="invoice-mode-toggle" aria-label={copy.status}>
          <strong>{copy.preview}</strong>
        </div>
      </div>
      <div className="invoice-preview-scroll">
        <InvoiceDocumentSelector documents={documents} />
      </div>
      <div className="invoice-editor-footer">
        <a href={businessPanelHref(query, submoduleId, "invoice", invoice.id, "right")}>{copy.moreDetails}</a>
        <BusinessQueueButton
          action="payment"
          instruction={`Register or reconcile payment for invoice ${invoice.number}.`}
          payload={{ invoice }}
          recordId={invoice.id}
          resource="invoices"
          title={`Register payment: ${invoice.number}`}
        >
          {copy.capturePayment}
        </BusinessQueueButton>
        <InvoiceDeliveryActions copy={copy} customer={customer} invoice={invoice} locale={locale} />
      </div>
    </section>
  );
}

function InvoiceField({ className, icon, label, muted, value }: {
  className?: string;
  icon?: boolean;
  label: string;
  muted?: boolean;
  value: string;
}) {
  return (
    <div className={`invoice-field ${className ?? ""} ${icon ? "has-icon" : ""} ${muted ? "is-muted" : ""}`.trim()}>
      <span>{label}</span>
      <strong>{value}</strong>
      {icon ? <small aria-hidden="true">...</small> : null}
    </div>
  );
}

function InvoiceTextTemplatePanel({ copy, query, recordId, submoduleId }: {
  copy: BusinessCopy;
  query: QueryState;
  recordId?: string;
  submoduleId: string;
}) {
  const templateSet = invoiceTextTemplates(copy, recordId);

  return (
    <div className="drawer-content ops-drawer invoice-template-drawer">
      <DrawerHeader title={templateSet.title} query={query} submoduleId={submoduleId} />
      <a className="invoice-template-new" href={businessPanelHref(query, submoduleId, "new", "text-template", "left-bottom")}>+ {copy.newTemplate}</a>
      <div className="invoice-template-options">
        {templateSet.templates.map((template, index) => (
          <a
            className={`invoice-template-option ${index === 0 ? "is-selected" : ""}`}
            href={businessBaseHref(query, submoduleId)}
            key={template.title}
          >
            <span aria-hidden="true" />
            <strong>{template.title}</strong>
            <small>{template.meta}</small>
            {template.standard ? <em>{copy.standard}</em> : null}
          </a>
        ))}
      </div>
      <a className="drawer-primary invoice-template-done" href={businessBaseHref(query, submoduleId)}>{copy.done}</a>
    </div>
  );
}

function invoiceTextTemplates(copy: BusinessCopy, recordId?: string) {
  if (recordId === "paymentCondition") {
    return {
      title: copy.paymentConditionPlural,
      templates: [
        { title: "Zahlungsziel 5 Tage", meta: "Zahlungsziel: 5 Tag" },
        { title: "Zahlungsziel 30 Tage, 15% Skonto bei unter 3 Tagen", meta: "Zahlungsziel: 30 Tag; Skontoziel: 3 Tag; Skonto: 15 %" },
        { title: "Zahlungsziel 15 Tage, 2% Skonto bei unter 5 Tagen", meta: "Zahlungsziel: 15 Tag; Skontoziel: 5 Tag; Skonto: 2 %", standard: true }
      ]
    };
  }

  if (recordId === "closingNote") {
    return {
      title: copy.closingNotePlural,
      templates: [
        { title: copy.closingNotePlaceholder, meta: copy.closingNotePlaceholder, standard: true },
        { title: copy.bankTransferNote, meta: copy.bankTransferNote },
        { title: copy.projectReferenceNote, meta: copy.projectReferenceNote }
      ]
    };
  }

  return {
    title: copy.introTextPlural,
    templates: [
      { title: copy.introTextPlaceholder, meta: copy.introTextPlaceholder, standard: true },
      { title: copy.offerIntroTemplate, meta: copy.offerIntroTemplate },
      { title: copy.serviceIntroTemplate, meta: copy.serviceIntroTemplate }
    ]
  };
}

function invoiceDocumentsForInvoice(copy: BusinessCopy, data: BusinessBundle, invoice: BusinessInvoice, locale: SupportedLocale): InvoiceDocumentOption[] {
  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const senderLines = [
    "Metric Space UG (haftungsbeschränkt)",
    "Lämmersieht 21",
    "22305 Hamburg",
    "Tel.: +49 176 23424399",
    "info@metric-space.ai",
    "metric-space.ai"
  ];
  const footerLeft = [
    "Metric Space UG (haftungsbeschränkt)",
    "Lämmersieht 21",
    "22305 Hamburg",
    "Tel.: +49 176 23424399",
    "info@metric-space.ai"
  ];
  const footerRight = [
    "Steuernummer: 43/743/02774",
    "Handelsregister B des Amtsgerichts",
    "Hamburg HRB 176693",
    "Geschäftsführer Michael Welsch"
  ];
  const invoiceLines = invoice.lines.map((line) => {
    const product = data.products.find((item) => item.id === line.productId);
    return {
      description: text(product?.description ?? "", locale),
      quantity: line.quantity.toLocaleString(locale === "de" ? "de-DE" : "en-US", { maximumFractionDigits: 2 }),
      title: product?.name ?? line.productId,
      total: moneyPlain(line.quantity * line.unitPrice, invoice.currency, locale),
      unit: product?.type === "Service" ? copy.hour : copy.piece,
      unitPrice: moneyPlain(line.unitPrice, invoice.currency, locale)
    };
  });
  const netAmount = invoiceNet(invoice);
  const base: InvoiceDocumentOption[] = [
    {
      amountLabel: businessCurrency(invoice.total, invoice.currency, locale),
      body: text(invoice.introText ?? invoice.notes, locale),
      closingText: text(invoice.closingText ?? invoice.notes, locale),
      customerNumber: invoice.customerNumber,
      dueDate: invoice.dueDate,
      footerLeft,
      footerRight,
      id: "invoice",
      issueDate: invoice.issueDate,
      lines: invoiceLines,
      meta: `${invoice.number} - ${invoice.issueDate}`,
      number: invoice.number,
      paymentTerms: text(invoice.paymentTermsText ?? customer?.paymentTerms ?? "", locale),
      recipientLines: invoice.addressLines ?? [customer?.name ?? invoice.customerId, customer?.country ?? ""],
      senderLine: "Metric Space UG (haftungsbeschränkt), Lämmersieht 21, 22305 Hamburg",
      senderLines,
      serviceDate: invoice.serviceDate,
      subtotalAmount: moneyPlain(netAmount, invoice.currency, locale),
      subtotalLabel: `${locale === "de" ? "Zwischensumme (netto)" : "Subtotal (net)"}`,
      taxAmount: moneyPlain(invoice.taxAmount, invoice.currency, locale),
      taxLabel: locale === "de" ? "Umsatzsteuer" : "VAT",
      title: invoice.documentTitle ?? copy.invoice,
      totalLabel: copy.totalGross,
      typeLabel: copy.invoice
    }
  ];

  if ((invoice.reminderLevel ?? 0) >= 1) {
    base.push({
      amountLabel: businessCurrency(invoiceBalance(invoice), invoice.currency, locale),
      body: locale === "de"
        ? `Bitte gleichen Sie den offenen Betrag aus der Rechnung ${invoice.number} bis zum ${invoice.reminderDueDate ?? invoice.dueDate} aus.`
        : `Please settle the outstanding amount from invoice ${invoice.number} by ${invoice.reminderDueDate ?? invoice.dueDate}.`,
      closingText: locale === "de" ? "Sollten Sie die Zahlung bereits veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos." : "If payment has already been made, please disregard this letter.",
      customerNumber: invoice.customerNumber,
      footerLeft,
      footerRight,
      id: "reminder-1",
      issueDate: invoice.reminderDueDate ?? invoice.dueDate,
      lines: [{
        description: `${invoice.documentTitle ?? copy.invoice} ${invoice.number}`,
        quantity: "1",
        title: locale === "de" ? "Offener Rechnungsbetrag" : "Outstanding invoice amount",
        total: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
        unit: copy.piece,
        unitPrice: moneyPlain(invoiceBalance(invoice), invoice.currency, locale)
      }],
      meta: `${copy.reminderLevel} 1 - ${invoice.reminderDueDate ?? invoice.dueDate}`,
      number: `${invoice.number}-M1`,
      paymentTerms: locale === "de" ? `Bitte zahlen Sie bis zum ${invoice.reminderDueDate ?? invoice.dueDate}.` : `Please pay by ${invoice.reminderDueDate ?? invoice.dueDate}.`,
      recipientLines: invoice.addressLines ?? [customer?.name ?? invoice.customerId, customer?.country ?? ""],
      senderLine: "Metric Space UG (haftungsbeschränkt), Lämmersieht 21, 22305 Hamburg",
      senderLines,
      serviceDate: invoice.serviceDate,
      subtotalAmount: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
      subtotalLabel: locale === "de" ? "Offener Betrag" : "Outstanding amount",
      taxAmount: "",
      taxLabel: "",
      title: `${copy.reminder} 1`,
      totalLabel: copy.toReceive,
      typeLabel: copy.reminder
    });
  }

  if ((invoice.reminderLevel ?? 0) >= 2) {
    base.push({
      amountLabel: businessCurrency(invoiceBalance(invoice), invoice.currency, locale),
      body: locale === "de"
        ? `Dies ist die zweite Mahnung zur Rechnung ${invoice.number}. Bitte begleichen Sie den offenen Betrag kurzfristig.`
        : `This is the second reminder for invoice ${invoice.number}. Please settle the outstanding amount promptly.`,
      closingText: locale === "de" ? "Bei Rückfragen melden Sie sich bitte unter Angabe der Rechnungsnummer." : "Please include the invoice number in any questions.",
      customerNumber: invoice.customerNumber,
      footerLeft,
      footerRight,
      id: "reminder-2",
      issueDate: invoice.reminderDueDate ?? invoice.dueDate,
      lines: [{
        description: `${invoice.documentTitle ?? copy.invoice} ${invoice.number}`,
        quantity: "1",
        title: locale === "de" ? "Offener Rechnungsbetrag" : "Outstanding invoice amount",
        total: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
        unit: copy.piece,
        unitPrice: moneyPlain(invoiceBalance(invoice), invoice.currency, locale)
      }],
      meta: `${copy.reminderLevel} 2 - ${invoice.reminderDueDate ?? invoice.dueDate}`,
      number: `${invoice.number}-M2`,
      paymentTerms: locale === "de" ? `Bitte zahlen Sie bis zum ${invoice.reminderDueDate ?? invoice.dueDate}.` : `Please pay by ${invoice.reminderDueDate ?? invoice.dueDate}.`,
      recipientLines: invoice.addressLines ?? [customer?.name ?? invoice.customerId, customer?.country ?? ""],
      senderLine: "Metric Space UG (haftungsbeschränkt), Lämmersieht 21, 22305 Hamburg",
      senderLines,
      serviceDate: invoice.serviceDate,
      subtotalAmount: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
      subtotalLabel: locale === "de" ? "Offener Betrag" : "Outstanding amount",
      taxAmount: "",
      taxLabel: "",
      title: `${copy.reminder} 2`,
      totalLabel: copy.toReceive,
      typeLabel: copy.reminder
    });
  }

  if ((invoice.reminderLevel ?? 0) >= 3 || invoice.collectionStatus === "Final notice") {
    base.push({
      amountLabel: businessCurrency(invoiceBalance(invoice), invoice.currency, locale),
      body: locale === "de"
        ? `Letzte Mahnung zur Rechnung ${invoice.number}. Ohne Zahlung wird der Vorgang an das Mahnwesen übergeben.`
        : `Final notice for invoice ${invoice.number}. Without payment, the case will be handed over to collections.`,
      closingText: locale === "de" ? "Bitte veranlassen Sie die Zahlung unverzüglich." : "Please arrange payment immediately.",
      customerNumber: invoice.customerNumber,
      footerLeft,
      footerRight,
      id: "final-notice",
      issueDate: invoice.reminderDueDate ?? invoice.dueDate,
      lines: [{
        description: `${invoice.documentTitle ?? copy.invoice} ${invoice.number}`,
        quantity: "1",
        title: locale === "de" ? "Offener Rechnungsbetrag" : "Outstanding invoice amount",
        total: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
        unit: copy.piece,
        unitPrice: moneyPlain(invoiceBalance(invoice), invoice.currency, locale)
      }],
      meta: invoice.reminderDueDate ?? invoice.dueDate,
      number: `${invoice.number}-LM`,
      paymentTerms: locale === "de" ? "Sofort fällig." : "Due immediately.",
      recipientLines: invoice.addressLines ?? [customer?.name ?? invoice.customerId, customer?.country ?? ""],
      senderLine: "Metric Space UG (haftungsbeschränkt), Lämmersieht 21, 22305 Hamburg",
      senderLines,
      serviceDate: invoice.serviceDate,
      subtotalAmount: moneyPlain(invoiceBalance(invoice), invoice.currency, locale),
      subtotalLabel: locale === "de" ? "Offener Betrag" : "Outstanding amount",
      taxAmount: "",
      taxLabel: "",
      title: locale === "de" ? "Letzte Mahnung" : "Final notice",
      totalLabel: copy.toReceive,
      typeLabel: copy.reminder
    });
  }

  return base;
}

function moneyPlain(amount: number, currency: BusinessInvoice["currency"], locale: SupportedLocale) {
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", {
    maximumFractionDigits: 2,
    minimumFractionDigits: 2
  }).format(amount);
}

function InvoiceDocumentPreview({ copy, data, invoice, locale }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  invoice: BusinessInvoice;
  locale: SupportedLocale;
}) {
  const customer = data.customers.find((item) => item.id === invoice.customerId);

  return (
    <div
      className="invoice-document-preview"
      data-context-item
      data-context-label={invoice.number}
      data-context-module="business"
      data-context-record-id={invoice.id}
      data-context-record-type="invoice"
      data-context-submodule="invoices"
    >
      <header>
        <div>
          <span>{invoice.documentTitle ?? copy.invoice}</span>
          <strong>{invoice.number}</strong>
        </div>
        <small>{invoice.issueDate}</small>
      </header>
      <address>
        {(invoice.addressLines ?? [customer?.name ?? copy.customer, customer?.country ?? ""]).map((line) => <span key={line}>{line}</span>)}
      </address>
      <section>
        <h3>{invoice.documentTitle ?? copy.invoice}</h3>
        <p>{text(invoice.introText ?? invoice.notes, locale)}</p>
      </section>
      <div className="invoice-preview-lines">
        {invoice.lines.map((line) => {
          const product = data.products.find((item) => item.id === line.productId);
          return (
            <span key={`${invoice.id}-${line.productId}`}>
              <b>{product?.name ?? line.productId}</b>
              <em>{line.quantity} x {businessCurrency(line.unitPrice, invoice.currency, locale)}</em>
            </span>
          );
        })}
      </div>
      <footer>
        <span>{copy.gross}</span>
        <strong>{businessCurrency(invoice.total, invoice.currency, locale)}</strong>
      </footer>
    </div>
  );
}

function AccountPanel({ account, copy, data, locale, query, submoduleId }: {
  account: BusinessAccount;
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const rows = buildLedgerRows(data).filter((row) => row.account.id === account.id);
  const debit = rows.reduce((sum, row) => sum + row.debit, 0);
  const credit = rows.reduce((sum, row) => sum + row.credit, 0);

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={`${account.code} ${account.name}`} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{copy.accountDrawerDescription}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.accountType}</dt><dd>{account.accountType}</dd></div>
        <div><dt>{copy.rootType}</dt><dd>{account.rootType}</dd></div>
        <div><dt>{copy.debit}</dt><dd>{businessCurrency(debit, account.currency, locale)}</dd></div>
        <div><dt>{copy.credit}</dt><dd>{businessCurrency(credit, account.currency, locale)}</dd></div>
        <div><dt>{copy.taxCode}</dt><dd>{account.taxCode ?? "-"}</dd></div>
      </dl>
      <BusinessRecordList title={copy.ledger} items={rows.slice(0, 8).map((row) => `${row.entry.postingDate} - ${row.entry.number} - ${businessCurrency(Math.max(row.debit, row.credit), account.currency, locale)} - ${row.refLabel}`)} />
      <BusinessQueueButton
        action="sync"
        className="drawer-primary"
        instruction={`Review account ${account.code} ${account.name}, ledger entries, tax mapping, and export readiness.`}
        payload={{ account, rows }}
        recordId={account.id}
        resource="accounts"
        title={`Review account: ${account.code}`}
      >
        {copy.askCtoxSync}
      </BusinessQueueButton>
    </div>
  );
}

function JournalEntryPanel({ copy, data, entry, locale, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  entry: BusinessJournalEntry;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const debit = entry.lines.reduce((sum, line) => sum + line.debit, 0);
  const credit = entry.lines.reduce((sum, line) => sum + line.credit, 0);
  const lineItems = entry.lines.map((line) => {
    const account = data.accounts.find((item) => item.id === line.accountId);
    return `${account?.code ?? line.accountId} ${account?.name ?? ""} - ${copy.debit}: ${businessCurrency(line.debit, account?.currency ?? "EUR", locale)} - ${copy.credit}: ${businessCurrency(line.credit, account?.currency ?? "EUR", locale)}`;
  });

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={entry.number} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(entry.narration, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.date}</dt><dd>{entry.postingDate}</dd></div>
        <div><dt>{copy.status}</dt><dd>{entry.status}</dd></div>
        <div><dt>{copy.reference}</dt><dd>{entry.refId}</dd></div>
        <div><dt>{copy.debit}</dt><dd>{businessCurrency(debit, "EUR", locale)}</dd></div>
        <div><dt>{copy.credit}</dt><dd>{businessCurrency(credit, "EUR", locale)}</dd></div>
        <div><dt>{copy.balance}</dt><dd>{isBalanced(entry) ? copy.balanced : copy.unbalanced}</dd></div>
      </dl>
      <BusinessRecordList title={copy.lines} items={lineItems} />
      <BusinessQueueButton
        action="sync"
        className="drawer-primary"
        instruction={`Review journal entry ${entry.number}, validate debit and credit balance, and prepare bookkeeping export context.`}
        payload={{ entry }}
        recordId={entry.id}
        resource="ledger"
        title={`Review journal entry: ${entry.number}`}
      >
        {copy.askCtoxSync}
      </BusinessQueueButton>
    </div>
  );
}

function ReceiptPanel({ copy, data, locale, query, receipt, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  receipt: BusinessReceipt;
  submoduleId: string;
}) {
  const expenseAccount = data.accounts.find((account) => account.id === receipt.expenseAccountId);
  const bankTransaction = receipt.bankTransactionId ? data.bankTransactions.find((transaction) => transaction.id === receipt.bankTransactionId) : undefined;

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={receipt.number} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(receipt.notes, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.vendor}</dt><dd>{receipt.vendorName}</dd></div>
        <div><dt>{copy.status}</dt><dd>{receipt.status}</dd></div>
        <div><dt>{copy.due}</dt><dd>{receipt.dueDate}</dd></div>
        <div><dt>{copy.account}</dt><dd>{expenseAccount?.code} {expenseAccount?.name}</dd></div>
        <div><dt>{copy.taxCode}</dt><dd>{receipt.taxCode}</dd></div>
        <div><dt>{copy.amount}</dt><dd>{businessCurrency(receipt.total, receipt.currency, locale)}</dd></div>
      </dl>
      <BusinessRecordList title={copy.extractedFields} items={receipt.extractedFields.map((field) => `${field.label}: ${field.value} (${field.confidence}%)`)} />
      <BusinessRecordList title={copy.bankMatch} items={bankTransaction ? [`${bankTransaction.bookingDate} - ${bankTransaction.counterparty} - ${businessCurrency(bankTransaction.amount, bankTransaction.currency, locale)} - ${bankTransaction.status}`] : [copy.notMatched]} />
      <div className="ops-action-dock">
        <BusinessQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Review inbound receipt ${receipt.number}, confirm OCR fields, tax code, account mapping, and posting readiness.`}
          payload={{ receipt, bankTransaction }}
          recordId={receipt.id}
          resource="receipts"
          title={`Review receipt: ${receipt.number}`}
        >
          {copy.reviewReceipt}
        </BusinessQueueButton>
        <BusinessQueueButton
          action="payment"
          instruction={`Match or create payment posting for receipt ${receipt.number}.`}
          payload={{ receipt, bankTransaction }}
          recordId={receipt.id}
          resource="receipts"
          title={`Reconcile receipt: ${receipt.number}`}
        >
          {copy.reconcile}
        </BusinessQueueButton>
      </div>
    </div>
  );
}

function BankTransactionPanel({ bankTransaction, copy, data, locale, query, submoduleId }: {
  bankTransaction: BusinessBankTransaction;
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const reconciliation = buildReconciliationRows(data).find((row) => row.id === bankTransaction.id);
  const entries = data.journalEntries.filter((entry) => entry.refId === bankTransaction.id);

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={bankTransaction.counterparty} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{bankTransaction.purpose}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.date}</dt><dd>{bankTransaction.bookingDate}</dd></div>
        <div><dt>{copy.status}</dt><dd>{bankTransaction.status}</dd></div>
        <div><dt>{copy.match}</dt><dd>{reconciliation?.matchedLabel ?? "-"}</dd></div>
        <div><dt>{copy.confidence}</dt><dd>{bankTransaction.confidence}%</dd></div>
        <div><dt>{copy.amount}</dt><dd>{businessCurrency(bankTransaction.amount, bankTransaction.currency, locale)}</dd></div>
      </dl>
      <BusinessRecordList title={copy.ledger} items={entries.map((entry) => `${entry.number} - ${entry.status} - ${text(entry.narration, locale)}`)} />
      <BusinessQueueButton
        action="payment"
        className="drawer-primary"
        instruction={`Reconcile bank transaction ${bankTransaction.id}, confirm match, and create or adjust payment posting.`}
        payload={{ bankTransaction, entries }}
        recordId={bankTransaction.id}
        resource="payments"
        title={`Reconcile bank transaction: ${bankTransaction.counterparty}`}
      >
        {copy.reconcile}
      </BusinessQueueButton>
    </div>
  );
}

function BookkeepingPanel({ copy, data, exportBatch, locale, query, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  exportBatch: BusinessBookkeepingExport;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
}) {
  const invoices = exportBatch.invoiceIds.map((id) => data.invoices.find((invoice) => invoice.id === id)).filter(Boolean) as BusinessInvoice[];
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={`${exportBatch.system} ${exportBatch.period}`} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(exportBatch.context, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.status}</dt><dd>{exportBatch.status}</dd></div>
        <div><dt>{copy.reviewer}</dt><dd>{exportBatch.reviewer}</dd></div>
        <div><dt>{copy.due}</dt><dd>{exportBatch.dueDate}</dd></div>
        <div><dt>{copy.net}</dt><dd>{businessCurrency(exportBatch.netAmount, "EUR", locale)}</dd></div>
        <div><dt>{copy.tax}</dt><dd>{businessCurrency(exportBatch.taxAmount, "EUR", locale)}</dd></div>
      </dl>
      <BusinessRecordList title={copy.invoices} items={invoices.map((invoice) => `${invoice.number} - ${invoice.status} - ${businessCurrency(invoice.total, invoice.currency, locale)}`)} />
      <BusinessQueueButton
        action="export"
        className="drawer-primary"
        instruction={`Run Business bookkeeping export ${exportBatch.id}, verify tax mappings, and queue CTOX follow-up tasks for exceptions.`}
        payload={{ exportBatch, invoices }}
        recordId={exportBatch.id}
        resource="bookkeeping"
        title={`Export bookkeeping batch: ${exportBatch.id}`}
      >
        {copy.queueExport}
      </BusinessQueueButton>
    </div>
  );
}

function ReportPanel({ copy, data, locale, query, report, submoduleId }: {
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  report: BusinessReport;
  submoduleId: string;
}) {
  const exports = report.linkedExportIds.map((id) => data.bookkeeping.find((exportBatch) => exportBatch.id === id)).filter(Boolean) as BusinessBookkeepingExport[];
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={report.title} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(report.summary, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.period}</dt><dd>{report.period}</dd></div>
        <div><dt>{copy.status}</dt><dd>{report.status}</dd></div>
        <div><dt>{copy.amount}</dt><dd>{businessCurrency(report.amount, "EUR", locale)}</dd></div>
        <div><dt>{copy.taxContext}</dt><dd>{report.taxContext}</dd></div>
        <div><dt>{copy.exportContext}</dt><dd>{report.exportContext}</dd></div>
      </dl>
      <BusinessRecordList title={copy.exports} items={exports.map((exportBatch) => `${exportBatch.system} ${exportBatch.period} - ${exportBatch.status}`)} />
      <BusinessQueueButton
        action="sync"
        className="drawer-primary"
        instruction={`Refresh Business report ${report.title} from invoices, products, bookkeeping exports, tax context, and CTOX core tasks.`}
        payload={{ report, exports }}
        recordId={report.id}
        resource="reports"
        title={`Refresh report: ${report.title}`}
      >
        {copy.askCtoxReport}
      </BusinessQueueButton>
    </div>
  );
}

async function WarehouseStorageView({ query, submoduleId }: BusinessViewProps) {
  const warehouseSnapshot = await getWarehouseSnapshot();
  const locale = resolveLocale(query.locale) as SupportedLocale;
  return (
    <WarehouseStorageWorkbench
      initialSelectedWarehouseId={query.selectedId}
      initialSnapshot={warehouseSnapshot.snapshot}
      locale={locale}
      query={{ locale: query.locale, theme: query.theme, warehouseSearch: query.warehouseSearch }}
      submoduleId={submoduleId}
    />
  );
}

async function FulfillmentView({ query, submoduleId }: BusinessViewProps) {
  const warehouseSnapshot = await getWarehouseSnapshot();
  const warehouse = warehouseSnapshot.snapshot;
  const summary = warehouseSnapshot.summary;
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const de = locale === "de";
  const itemName = (id: string) => warehouse.items.find((item) => item.id === id)?.name ?? id;
  const itemSku = (id: string) => warehouse.items.find((item) => item.id === id)?.sku ?? id;
  const locationName = (id: string) => warehouse.locations.find((location) => location.id === id)?.name ?? id;
  const ownerName = (id: string) => id === "cust-nova" ? "Nova Logistics" : "Metric Space";
  const warehouses = warehouse.locations.filter((location) => location.kind === "warehouse");
  const warehouseSearch = String(query.warehouseSearch ?? "").trim().toLowerCase();
  const orderSearch = String(query.orderSearch ?? "").trim().toLowerCase();
  const matchesText = (parts: Array<string | number | undefined>) => parts.join(" ").toLowerCase().includes(orderSearch);
  const selectedWarehouse = warehouses.find((location) => location.id === query.selectedId) ?? warehouses[0];
  const childLocations = (parentId: string) => warehouse.locations.filter((location) => location.parentId === parentId);
  const descendantIds = (parentId: string): string[] => childLocations(parentId).flatMap((location) => [location.id, ...descendantIds(location.id)]);
  const matchesWarehouseText = (warehouseLocationId: string) => {
    if (!warehouseSearch) return true;
    const children = descendantIds(warehouseLocationId).map((id) => locationName(id));
    return [locationName(warehouseLocationId), ...children].join(" ").toLowerCase().includes(warehouseSearch);
  };
  const visibleWarehouses = warehouses.filter((warehouseLocation) => matchesWarehouseText(warehouseLocation.id));
  const selectedLocationIds = new Set(selectedWarehouse ? [selectedWarehouse.id, ...descendantIds(selectedWarehouse.id)] : []);
  const locationQuantity = (locationId: string, statuses: readonly string[] = ["available", "reserved", "picked", "receiving"]) => warehouse.balances
    .filter((balance) => balance.locationId === locationId && statuses.includes(balance.stockStatus))
    .reduce((sum, balance) => sum + balance.quantity, 0);
  const warehouseQuantity = (status: string) => warehouse.balances
    .filter((balance) => selectedLocationIds.has(balance.locationId) && balance.stockStatus === status)
    .reduce((sum, balance) => sum + balance.quantity, 0);
  const sections = selectedWarehouse
    ? childLocations(selectedWarehouse.id)
        .filter((location) => location.kind === "zone")
        .map((section) => {
          const slots = childLocations(section.id).filter((location) => location.kind === "bin");
          const usedSlots = slots.filter((slot) => locationQuantity(slot.id) > 0).length;
          const available = slots.reduce((sum, slot) => sum + warehouse.balances.filter((balance) => balance.locationId === slot.id && balance.stockStatus === "available").reduce((slotSum, balance) => slotSum + balance.quantity, 0), 0);
          const committed = slots.reduce((sum, slot) => sum + warehouse.balances.filter((balance) => balance.locationId === slot.id && (balance.stockStatus === "reserved" || balance.stockStatus === "picked")).reduce((slotSum, balance) => slotSum + balance.quantity, 0), 0);
          return { available, committed, section, slots, usedSlots };
        })
    : [];
  const firstSectionId = sections[0]?.section.id;
  const availableByItem = new Map<string, number>();
  warehouse.balances
    .filter((balance) => selectedLocationIds.has(balance.locationId) && balance.stockStatus === "available")
    .forEach((balance) => availableByItem.set(balance.inventoryItemId, (availableByItem.get(balance.inventoryItemId) ?? 0) + balance.quantity));
  const totalRequired = warehouse.reservations.reduce((sum, reservation) => sum + reservation.lines.reduce((lineSum, line) => lineSum + line.quantity, 0), 0);
  const totalMatched = warehouse.reservations.reduce((sum, reservation) => sum + reservation.lines.reduce((lineSum, line) => lineSum + Math.min(line.quantity, line.quantity - line.releasedQuantity), 0), 0);
  const orderReadiness = totalRequired > 0 ? Math.round((totalMatched / totalRequired) * 100) : 0;
  const completedWorkSteps = new Set(warehouse.commandLog
    .filter((command) => command.type === "CompleteValueStep" && typeof command.payload.reservationId === "string" && typeof command.payload.step === "string")
    .map((command) => typeof command.payload.lineId === "string"
      ? `${command.payload.reservationId}:${command.payload.lineId}:${command.payload.step}`
      : `${command.payload.reservationId}:${command.payload.step}`));
  const assemblyRows = warehouse.reservations.map((reservation, index) => {
    const lines = reservation.lines.map((line) => {
      const matched = Math.max(0, line.quantity - line.releasedQuantity);
      const completed = Math.min(line.quantity, line.pickedQuantity + line.shippedQuantity);
      const missing = Math.max(0, line.quantity - matched);
      const available = availableByItem.get(line.inventoryItemId) ?? 0;
      return {
        available,
        completed,
        id: line.id,
        item: itemName(line.inventoryItemId),
        inventoryItemId: line.inventoryItemId,
        locationId: line.locationId,
        owner: ownerName(line.inventoryOwnerPartyId),
        matched,
        missing,
        picked: line.pickedQuantity,
        quantity: line.quantity,
        shipped: line.shippedQuantity,
        sku: itemSku(line.inventoryItemId),
        sourceLineId: line.sourceLineId
      };
    });
    const required = lines.reduce((sum, line) => sum + line.quantity, 0);
    const matched = lines.reduce((sum, line) => sum + line.matched, 0);
    const completed = lines.reduce((sum, line) => sum + line.completed, 0);
    const materialProgress = required > 0 ? Math.round((matched / required) * 100) : 0;
    const completionProgress = required > 0 ? Math.round((completed / required) * 100) : 0;
    const lineStepDone = (lineId: string, step: "build" | "qa" | "pack") =>
      completedWorkSteps.has(`${reservation.id}:${lineId}:${step}`) || completedWorkSteps.has(`${reservation.id}:${step}`);
    const buildDone = lines.length > 0 && lines.every((line) => lineStepDone(line.id, "build"));
    const qaDone = lines.length > 0 && lines.every((line) => lineStepDone(line.id, "qa"));
    const packDone = lines.length > 0 && lines.every((line) => lineStepDone(line.id, "pack"));
    const valueSteps = reservation.status === "consumed"
      ? [100, 100, 100, 100]
      : reservation.status === "partially_consumed"
        ? [100, buildDone ? 100 : 80, qaDone ? 100 : 60, packDone ? 100 : 30]
        : reservation.status === "reserved" || reservation.status === "partially_reserved"
          ? [materialProgress, buildDone ? 100 : Math.min(65, materialProgress), qaDone ? 100 : Math.min(35, materialProgress), packDone ? 100 : 0]
          : [materialProgress, 0, 0, 0];
    const scoredLines = lines.map((line) => {
      const lineMaterialScore = line.quantity > 0 ? Math.round((line.matched / line.quantity) * 100) : 0;
      const linePickScore = line.quantity > 0 ? Math.round((line.picked / line.quantity) * 100) : 0;
      const lineShipScore = line.quantity > 0 ? Math.round((line.shipped / line.quantity) * 100) : 0;
      const lineBuildDone = lineStepDone(line.id, "build");
      const lineQaDone = lineStepDone(line.id, "qa");
      const linePackDone = lineStepDone(line.id, "pack");
      const lineBuildScore = reservation.status === "consumed" ? 100 : lineBuildDone ? 100 : Math.min(65, lineMaterialScore);
      const lineQaScore = reservation.status === "consumed" ? 100 : lineQaDone ? 100 : Math.min(lineBuildDone ? 70 : 35, lineBuildScore);
      const linePackScore = reservation.status === "consumed" ? 100 : linePackDone ? 100 : lineQaDone ? 65 : 0;
      const linePerformanceScore = Math.round((linePickScore * 0.34) + (lineBuildScore * 0.22) + (lineQaScore * 0.22) + (linePackScore * 0.22));
      const lineDelightScore = Math.round(((line.available >= line.quantity ? 100 : line.available > 0 ? 65 : 0) * 0.5) + (lineShipScore * 0.5));
      const lineScore = Math.min(100, Math.round((lineMaterialScore * 0.5) + (linePerformanceScore * 0.35) + (lineDelightScore * 0.15)));
      const kanbanStage = reservation.status === "consumed" || lineScore === 100
        ? "ready"
        : lineMaterialScore < 100
          ? "material"
          : !lineBuildDone
            ? "build"
            : !lineQaDone
              ? "qa"
              : !linePackDone
                ? "pack"
                : "ready";
      return {
        ...line,
        basisScore: lineMaterialScore,
        buildScore: lineBuildScore,
        delightScore: lineDelightScore,
        kanbanStage,
        packScore: linePackScore,
        performanceScore: linePerformanceScore,
        qaScore: lineQaScore,
        score: lineScore
      };
    });
    const progress = Math.min(100, Math.round((materialProgress * 0.45) + (completionProgress * 0.25) + (valueSteps.reduce((sum, step) => sum + step, 0) / 4 * 0.3)));
    const nextAction = reservation.status === "consumed"
      ? { action: "ship" as const, disabled: true, label: "Done" }
      : reservation.status === "partially_consumed"
        ? { action: "ship" as const, disabled: progress < 100, label: progress === 100 ? "Ship" : "Locked" }
        : reservation.status === "reserved" || reservation.status === "partially_reserved"
          ? { action: "pick" as const, disabled: false, label: "Pick parts" }
          : { action: "reserve" as const, disabled: true, label: "Blocked" };
    return {
      href: businessPanelHref(query, submoduleId, "reservation", reservation.id, "right"),
      id: reservation.id,
      lines: scoredLines,
      nextAction,
      owner: ownerName(reservation.inventoryOwnerPartyId),
      progress,
      sourceId: reservation.sourceId,
      status: reservation.status,
      steps: [
        { done: valueSteps[0] === 100, enabled: false, key: "material" as const, label: "Material", value: valueSteps[0] },
        { done: valueSteps[1] === 100, enabled: valueSteps[0] === 100, key: "build" as const, label: "Build", value: valueSteps[1] },
        { done: valueSteps[2] === 100, enabled: valueSteps[1] === 100, key: "qa" as const, label: "QA", value: valueSteps[2] },
        { done: valueSteps[3] === 100, enabled: valueSteps[2] === 100, key: "pack" as const, label: "Pack", value: valueSteps[3] }
      ],
      tone: progress === 100 ? "ready" : progress >= 70 ? "active" : "blocked",
      workOrder: `WO-${String(index + 7101).padStart(4, "0")}`
    };
  });
  const visibleAssemblyRows = orderSearch
    ? assemblyRows.filter((order) => matchesText([
        order.sourceId,
        order.workOrder,
        order.owner,
        order.status,
        ...order.lines.flatMap((line) => [line.item, line.sku, line.owner, line.sourceLineId])
      ]))
    : assemblyRows;
  const readyOrders = assemblyRows.filter((order) => order.progress === 100).length;
  const selectedRecordParts = String(query.recordId ?? "").split(":");
  const selectedOrder = assemblyRows.find((order) => order.id === selectedRecordParts[0]) ?? assemblyRows[0];
  const selectedLine = selectedOrder?.lines.find((line) => line.id === selectedRecordParts[1] || line.sourceLineId === selectedRecordParts[1])
    ?? selectedOrder?.lines.find((line) => line.missing > 0)
    ?? selectedOrder?.lines[0];
  const selectedSources = selectedLine
    ? warehouse.balances
        .filter((balance) => selectedLocationIds.has(balance.locationId) && balance.inventoryItemId === selectedLine.inventoryItemId)
        .sort((a, b) => (b.stockStatus === "available" ? 1 : 0) - (a.stockStatus === "available" ? 1 : 0) || b.quantity - a.quantity)
        .slice(0, 6)
    : [];
  const selectedLineProgress = selectedLine?.score ?? 0;
  const selectedLineBasisScore = selectedLine?.basisScore ?? 0;
  const selectedLinePerformanceScore = selectedLine?.performanceScore ?? 0;
  const selectedLineDelightScore = selectedLine?.delightScore ?? 0;
  const selectedLineBuildScore = selectedLine?.buildScore ?? 0;
  const selectedLineQaScore = selectedLine?.qaScore ?? 0;
  const selectedLinePackScore = selectedLine?.packScore ?? 0;
  const requirementGroups = [
    {
      key: "base",
      label: de ? "Basis-Anforderungen" : "Base requirements",
      items: [
        { label: de ? "Materialdeckung" : "Material coverage", score: selectedLineBasisScore, detail: selectedLine ? `${selectedLine.matched}/${selectedLine.quantity}` : "0/0" },
        { label: de ? "Richtiger Owner" : "Correct owner", score: selectedLine ? 100 : 0, detail: selectedLine?.owner ?? "-" }
      ]
    },
    {
      key: "value",
      label: de ? "Leistungs-Anforderungen" : "Performance requirements",
      items: [
        { label: de ? "Gesamtleistung" : "Performance score", score: selectedLinePerformanceScore, detail: selectedLine ? `${selectedLine.picked}/${selectedLine.quantity} Pick` : "0/0" },
        { label: "Build", score: selectedLineBuildScore, detail: selectedLineBuildScore === 100 ? (de ? "fertig" : "done") : (de ? "offen" : "open") },
        { label: "QA", score: selectedLineQaScore, detail: selectedLineQaScore === 100 ? (de ? "freigegeben" : "approved") : (de ? "offen" : "open") }
      ]
    },
    {
      key: "fulfillment",
      label: de ? "Begeisterungsfaktoren" : "Delight factors",
      items: [
        { label: de ? "Robuste Quelle" : "Robust source", score: selectedLineDelightScore, detail: selectedLine ? `${selectedLine.available} verfuegbar` : "0" },
        { label: de ? "Packen" : "Pack", score: selectedLinePackScore, detail: selectedLinePackScore === 100 ? (de ? "fertig" : "done") : (de ? "offen" : "open") },
        { label: de ? "Quellen" : "Sources", score: selectedSources.some((source) => source.stockStatus === "available" && source.quantity > 0) ? 100 : 0, detail: `${selectedSources.length}` }
      ]
    }
  ];
  const readinessLabel = selectedOrder?.progress === 100 ? (de ? "auslieferbar" : "ready to ship") : (de ? "gesperrt" : "locked");
  const warehouseKpis = selectedWarehouse
    ? [
        { label: de ? "Verfuegbar" : "Available", value: warehouseQuantity("available") },
        { label: de ? "Gebunden" : "Committed", value: warehouseQuantity("reserved") + warehouseQuantity("picked") },
        { label: de ? "Versendet" : "Shipped", value: warehouseQuantity("shipped") },
        { label: de ? "Counts" : "Counts", value: summary.cycleCounts }
      ]
    : [];

  return (
    <div className="warehouse-workbench" data-context-module="business" data-context-submodule={submoduleId}>
      <section className="warehouse-panel warehouse-left" aria-label={de ? "Lager" : "Warehouses"}>
        <header className="warehouse-panel-head">
          <h2>{de ? "Lager" : "Warehouses"}</h2>
          <div className="warehouse-head-actions">
            <a
              className="warehouse-subtle-action"
              data-context-item
              data-context-label={de ? "Lager Stammdaten" : "Warehouse master data"}
              data-context-module="business"
              data-context-record-id={selectedWarehouse?.id ?? "warehouse"}
              data-context-record-type="warehouse_admin"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "warehouse-admin", selectedWarehouse?.id ?? "warehouse", "left-bottom")}
            >
              {de ? "Verwalten" : "Manage"}
            </a>
            {selectedWarehouse ? <WarehouseLayoutActions sectionId={firstSectionId} warehouseId={selectedWarehouse.id} /> : null}
          </div>
        </header>
        <div className="warehouse-tool-row">
          <form className="warehouse-search-form" action={`/app/business/${submoduleId}`}>
            {query.locale ? <input type="hidden" name="locale" value={query.locale} /> : null}
            {query.theme ? <input type="hidden" name="theme" value={query.theme} /> : null}
            {selectedWarehouse ? <input type="hidden" name="selectedId" value={selectedWarehouse.id} /> : null}
            {orderSearch ? <input type="hidden" name="orderSearch" value={orderSearch} /> : null}
            <input name="warehouseSearch" defaultValue={query.warehouseSearch ?? ""} placeholder={de ? "Lagerquelle suchen" : "Find warehouse source"} />
          </form>
          <a className="warehouse-subtle-action" href={businessSelectionHref(query, "warehouse", selectedWarehouse?.id ?? "")}>
            {de ? "Bestand" : "Stock"}
          </a>
        </div>
        <div className="warehouse-source-list">
          {visibleWarehouses.map((warehouseLocation) => {
            const locationIds = new Set([warehouseLocation.id, ...descendantIds(warehouseLocation.id)]);
            const available = warehouse.balances
              .filter((balance) => locationIds.has(balance.locationId) && balance.stockStatus === "available")
              .reduce((sum, balance) => sum + balance.quantity, 0);
            const held = warehouse.balances
              .filter((balance) => locationIds.has(balance.locationId) && (balance.stockStatus === "reserved" || balance.stockStatus === "picked"))
              .reduce((sum, balance) => sum + balance.quantity, 0);
            const sectionCount = childLocations(warehouseLocation.id).filter((location) => location.kind === "zone").length;
            return (
              <a
                aria-current={warehouseLocation.id === selectedWarehouse?.id ? "page" : undefined}
                className={warehouseLocation.id === selectedWarehouse?.id ? "warehouse-source-card is-active" : "warehouse-source-card"}
                data-context-item
                data-context-label={warehouseLocation.name}
                data-context-module="business"
                data-context-record-id={warehouseLocation.id}
                data-context-record-type="warehouse_source"
                data-context-submodule={submoduleId}
                href={businessSelectionHref(query, submoduleId, warehouseLocation.id)}
                key={warehouseLocation.id}
              >
                <span className="warehouse-avatar">{warehouseLocation.name.slice(0, 1)}</span>
                <span>
                  <strong>{warehouseLocation.name}</strong>
                  <small>{sectionCount} {de ? "Bereiche" : "areas"} · {held} {de ? "fuer Auftraege gebunden" : "held for orders"}</small>
                </span>
                <em>{available}</em>
              </a>
            );
          })}
        </div>
        <div className="warehouse-left-metrics">
          {warehouseKpis.map((metric) => (
            <div key={metric.label}><span>{metric.label}</span><strong>{metric.value}</strong></div>
          ))}
        </div>
        <div className="warehouse-position-list">
          <section className="warehouse-match-panel">
            <h3>{de ? "Uebergabe fuer ausgewaehlte Position" : "Handoff for selected position"}</h3>
            {selectedOrder && selectedLine ? (
              <div className="warehouse-source-match-list">
                <a
                  className="warehouse-source-match is-reserved"
                  data-context-item
                  data-context-label={`${selectedOrder.sourceId} ${selectedLine.item}`}
                  data-context-module="business"
                  data-context-record-id={`${selectedOrder.id}:${selectedLine.id}`}
                  data-context-record-type="fulfillment_line_handoff"
                  data-context-submodule={submoduleId}
                  href={businessWarehouseSelectionHref(query, submoduleId, selectedWarehouse?.id, selectedOrder.id, selectedLine.id)}
                >
                  <span><strong>{selectedLine.item}</strong><small>{selectedLine.matched}/{selectedLine.quantity} {de ? "reserviert" : "reserved"} · {selectedLine.picked}/{selectedLine.quantity} Pick</small></span>
                  <em>{selectedLine.score}%</em>
                </a>
              </div>
            ) : (
              <div className="warehouse-empty-note">{de ? "Keine Position ausgewaehlt." : "No line selected."}</div>
            )}
          </section>
          <section className="warehouse-match-panel">
            <h3>{de ? "Verfuegbare Quellen" : "Available sources"}</h3>
            <div className="warehouse-source-match-list">
              {selectedSources.length > 0 ? selectedSources.map((source) => (
                <a
                  className={`warehouse-source-match is-${source.stockStatus}`}
                  data-context-item
                  data-context-label={`${selectedLine?.item ?? "Item"} ${locationName(source.locationId)}`}
                  data-context-module="business"
                  data-context-record-id={source.balanceKey}
                  data-context-record-type="warehouse_source_match"
                  data-context-submodule={submoduleId}
                  href={businessPanelHref(query, "warehouse", "balance", source.balanceKey, "right")}
                  key={source.balanceKey}
                >
                  <span><strong>{locationName(source.locationId)}</strong><small>{source.stockStatus}</small></span>
                  <em>{source.quantity}</em>
                </a>
              )) : (
                <div className="warehouse-empty-note">{de ? "Keine Quelle im gewaehlten Lager." : "No source in the selected warehouse."}</div>
              )}
            </div>
          </section>
        </div>
      </section>

      <section className="warehouse-panel warehouse-center" aria-label={de ? "Bestellabarbeitung" : "Order work"}>
        <header className="warehouse-panel-head warehouse-work-head">
          <div>
            <h2>{de ? "Bestellungen" : "Orders"}</h2>
            <p>{readyOrders}/{assemblyRows.length} {de ? "auslieferbar" : "ready"} · {orderReadiness}% {de ? "Material gematcht" : "material matched"}</p>
          </div>
          <nav className="warehouse-view-tabs" aria-label={de ? "Ansicht" : "View"}>
            <a className="is-active" href={businessBaseHref(query, submoduleId)}>{de ? "Liste" : "List"}</a>
            <a href={businessPanelHref(query, submoduleId, "business-set", "warehouse-replay", "right")}>{de ? "Audit" : "Audit"}</a>
          </nav>
        </header>
        <div className="warehouse-work-toolbar">
          <form className="warehouse-search-form" action={`/app/business/${submoduleId}`}>
            {query.locale ? <input type="hidden" name="locale" value={query.locale} /> : null}
            {query.theme ? <input type="hidden" name="theme" value={query.theme} /> : null}
            {selectedWarehouse ? <input type="hidden" name="selectedId" value={selectedWarehouse.id} /> : null}
            {warehouseSearch ? <input type="hidden" name="warehouseSearch" value={warehouseSearch} /> : null}
            <input name="orderSearch" defaultValue={query.orderSearch ?? ""} placeholder={de ? "Auftrag, Kunde, Artikel oder Quelle suchen" : "Find order, owner, item or source"} />
          </form>
          <span>{summary.outboxPending} {de ? "Events" : "events"}</span>
        </div>
        <div className="warehouse-order-list">
          {visibleAssemblyRows.length > 0 ? visibleAssemblyRows.map((order) => (
            <article
              className={`warehouse-work-order is-${order.tone} ${order.id === selectedOrder?.id ? "is-selected" : ""}`}
              data-context-item
              data-context-label={order.sourceId}
              data-context-module="business"
              data-context-record-id={order.id}
              data-context-record-type="warehouse_order"
              data-context-submodule={submoduleId}
              key={order.id}
            >
              <header>
                <a className="warehouse-open-mark" href={businessWarehouseSelectionHref(query, submoduleId, selectedWarehouse?.id, order.id)} aria-label={de ? "Auftrag auswaehlen" : "Select order"}>↗</a>
                <span>
                  <strong>{order.sourceId}</strong>
                  <small>{order.workOrder} · {order.owner} · {order.status}</small>
                </span>
                <div className="warehouse-score">
                  <strong>{order.progress}%</strong>
                  <small>{order.progress === 100 ? (de ? "bereit" : "ready") : (de ? "offen" : "open")}</small>
                </div>
                <div className="warehouse-order-actions">
                  <a href={businessPanelHref(query, submoduleId, "reservation", order.id, "right")}>{de ? "Akte" : "File"}</a>
                  <WarehouseOrderActionButton action={order.nextAction.action} disabled={order.nextAction.disabled} label={order.nextAction.label} reservationId={order.id} />
                </div>
              </header>
              <div className="warehouse-progress-track" aria-label={`Order progress ${order.progress}%`}>
                <span style={{ width: `${order.progress}%` }} />
              </div>
              <div className="warehouse-line-badges" aria-label={de ? "Arbeitspositionen im Auftrag" : "Work positions in order"}>
                {order.lines.map((line) => {
                  return (
                    <article
                      className={`warehouse-line-badge ${line.id === selectedLine?.id && order.id === selectedOrder?.id ? "is-selected" : ""}`}
                      data-context-item
                      data-context-label={`${order.sourceId} ${line.item}`}
                      data-context-module="business"
                      data-context-record-id={`${order.id}:${line.id}`}
                      data-context-record-type="warehouse_order_line"
                      data-context-submodule={submoduleId}
                      key={`${order.id}-${line.id}`}
                    >
                      <div className="warehouse-line-head">
                        <a href={businessWarehouseSelectionHref(query, submoduleId, selectedWarehouse?.id, order.id, line.id)}>
                          <strong>{line.item}</strong>
                          <small>{line.sku} · {line.missing > 0 ? `${line.missing} ${de ? "fehlt" : "missing"}` : de ? "gematcht" : "matched"}</small>
                        </a>
                        <a className="warehouse-line-score" href={businessPanelHref(query, submoduleId, "warehouse-match", `${order.id}:${line.id}`, "bottom")}>{line.score}%</a>
                      </div>
                      <i><b style={{ width: `${line.score}%` }} /></i>
                      <div className="warehouse-line-status">
                        <span className={line.kanbanStage === "material" ? "is-active" : line.basisScore === 100 ? "is-done" : undefined}>Material</span>
                        <span className={line.kanbanStage === "build" ? "is-active" : line.buildScore === 100 ? "is-done" : undefined}>Build</span>
                        <span className={line.kanbanStage === "qa" ? "is-active" : line.qaScore === 100 ? "is-done" : undefined}>QA</span>
                        <span className={line.kanbanStage === "pack" ? "is-active" : line.packScore === 100 ? "is-done" : undefined}>Pack</span>
                        <span className={line.kanbanStage === "ready" ? "is-done" : undefined}>Ready</span>
                      </div>
                      <div className="warehouse-line-actions">
                        {line.kanbanStage === "material" ? (
                          <WarehouseOrderActionButton action="pick" disabled={order.status === "consumed"} label={de ? "Pick" : "Pick"} reservationId={order.id} />
                        ) : line.kanbanStage === "build" ? (
                          <WarehouseWorkStepButton disabled={line.basisScore < 100} done={false} label="Build" lineId={line.id} reservationId={order.id} sourceId={order.sourceId} step="build" />
                        ) : line.kanbanStage === "qa" ? (
                          <WarehouseWorkStepButton disabled={line.buildScore < 100} done={false} label="QA" lineId={line.id} reservationId={order.id} sourceId={order.sourceId} step="qa" />
                        ) : line.kanbanStage === "pack" ? (
                          <WarehouseWorkStepButton disabled={line.qaScore < 100} done={false} label="Pack" lineId={line.id} reservationId={order.id} sourceId={order.sourceId} step="pack" />
                        ) : (
                          <a href={businessPanelHref(query, submoduleId, "warehouse-match", `${order.id}:${line.id}`, "bottom")}>
                            {de ? "Nachweis" : "Evidence"}
                          </a>
                        )}
                      </div>
                      <span className="warehouse-line-tools" aria-label={de ? "Positionsaktionen" : "Line actions"}>
                        <b>{de ? "Basis" : "Base"} {line.basisScore}%</b>
                        <b>{de ? "Leistung" : "Performance"} {line.performanceScore}%</b>
                        <b>{de ? "Begeisterung" : "Delight"} {line.delightScore}%</b>
                      </span>
                    </article>
                  );
                })}
              </div>
            </article>
          )) : <div className="warehouse-empty-note">{de ? "Keine Auftraege im aktuellen Filter." : "No orders in the current filter."}</div>}
        </div>
      </section>

      <section className="warehouse-panel warehouse-right" aria-label={de ? "Arbeitspositionen" : "Work positions"}>
        <header className="warehouse-panel-head">
          <div>
            <h2>{de ? "Positionen" : "Positions"}</h2>
            <p>{selectedOrder?.sourceId ?? (de ? "Kein Auftrag" : "No order")} · {readinessLabel}</p>
          </div>
          {selectedOrder ? (
            <a className="warehouse-subtle-action" href={businessPanelHref(query, submoduleId, "reservation", selectedOrder.id, "right")}>
              {de ? "rechts oeffnen" : "open right"}
            </a>
          ) : null}
        </header>
        {selectedOrder ? (
          <div className="warehouse-position-list">
            {selectedOrder.lines.map((line) => {
              return (
                <article
                  className={`warehouse-position-card ${line.id === selectedLine?.id ? "is-selected" : ""}`}
                  data-context-item
                  data-context-label={`${selectedOrder.sourceId} ${line.item}`}
                  data-context-module="business"
                  data-context-record-id={`${selectedOrder.id}:${line.id}`}
                  data-context-record-type="warehouse_order_line"
                  data-context-submodule={submoduleId}
                  key={`${selectedOrder.id}-${line.id}`}
                >
                  <div className="warehouse-position-main">
                    <a
                      className="warehouse-position-open"
                      href={businessWarehouseSelectionHref(query, submoduleId, selectedWarehouse?.id, selectedOrder.id, line.id)}
                    >
                      <span className="warehouse-position-icon">{line.sku.slice(0, 2)}</span>
                      <span>
                        <strong>{line.item}</strong>
                        <small>{line.sku} · {line.owner}</small>
                        <small>{line.missing > 0 ? `${line.missing} ${de ? "fehlt" : "missing"}` : de ? "vollstaendig" : "complete"} · {line.available} {de ? "im Lager" : "in warehouse"}</small>
                      </span>
                    </a>
                    <a
                      className="warehouse-position-score"
                      href={businessPanelHref(query, submoduleId, "warehouse-match", `${selectedOrder.id}:${line.id}`, "bottom")}
                    >
                      {line.score}%
                    </a>
                  </div>
                  <i><b style={{ width: `${line.score}%` }} /></i>
                  <div className="warehouse-position-workflow">
                    <span className={line.kanbanStage === "material" ? "is-active" : line.basisScore === 100 ? "is-done" : undefined}>Material</span>
                    <span className={line.kanbanStage === "build" ? "is-active" : line.buildScore === 100 ? "is-done" : undefined}>Build</span>
                    <span className={line.kanbanStage === "qa" ? "is-active" : line.qaScore === 100 ? "is-done" : undefined}>QA</span>
                    <span className={line.kanbanStage === "pack" ? "is-active" : line.packScore === 100 ? "is-done" : undefined}>Pack</span>
                    <span className={line.kanbanStage === "ready" ? "is-done" : undefined}>Ready</span>
                  </div>
                  <div className="warehouse-position-buttons">
                    <WarehouseWorkStepButton
                      disabled={line.basisScore < 100}
                      done={line.buildScore === 100}
                      label="Build"
                      lineId={line.id}
                      reservationId={selectedOrder.id}
                      sourceId={selectedOrder.sourceId}
                      step="build"
                    />
                    <WarehouseWorkStepButton
                      disabled={line.buildScore < 100}
                      done={line.qaScore === 100}
                      label="QA"
                      lineId={line.id}
                      reservationId={selectedOrder.id}
                      sourceId={selectedOrder.sourceId}
                      step="qa"
                    />
                    <WarehouseWorkStepButton
                      disabled={line.qaScore < 100}
                      done={line.packScore === 100}
                      label="Pack"
                      lineId={line.id}
                      reservationId={selectedOrder.id}
                      sourceId={selectedOrder.sourceId}
                      step="pack"
                    />
                    <a href={businessPanelHref(query, submoduleId, "warehouse-match", `${selectedOrder.id}:${line.id}`, "bottom")}>
                      Score
                    </a>
                  </div>
                  <span className="warehouse-position-actions">
                    <span>{de ? "Basis" : "Base"} {line.basisScore}%</span>
                    <span>{de ? "Leistung" : "Performance"} {line.performanceScore}%</span>
                    <span>{de ? "Begeisterung" : "Delight"} {line.delightScore}%</span>
                  </span>
                </article>
              );
            })}
            <section className="warehouse-match-panel">
              <h3>{de ? "Quellen fuer ausgewaehlte Position" : "Sources for selected position"}</h3>
              <div className="warehouse-source-match-list">
                {selectedSources.length > 0 ? selectedSources.map((source) => (
                  <a
                    className={`warehouse-source-match is-${source.stockStatus}`}
                    data-context-item
                    data-context-label={`${selectedLine?.item ?? "Item"} ${locationName(source.locationId)}`}
                    data-context-module="business"
                    data-context-record-id={source.balanceKey}
                    data-context-record-type="warehouse_source_match"
                    data-context-submodule={submoduleId}
                    href={businessPanelHref(query, submoduleId, "balance", source.balanceKey, "right")}
                    key={source.balanceKey}
                  >
                    <span><strong>{locationName(source.locationId)}</strong><small>{source.stockStatus}</small></span>
                    <em>{source.quantity}</em>
                  </a>
                )) : (
                  <div className="warehouse-empty-note">{de ? "Keine Lagerquelle im aktuellen Lager gefunden." : "No source in the selected warehouse."}</div>
                )}
              </div>
            </section>
          </div>
        ) : (
          <div className="warehouse-empty-note">{de ? "Kein Auftrag vorhanden." : "No order available."}</div>
        )}
      </section>

      <aside className="warehouse-match-bar" aria-label={de ? "Match Details" : "Match details"}>
        <div>
          <span>{selectedOrder?.sourceId ?? "Order"}</span>
          <strong>{selectedLine?.item ?? (de ? "Keine Position" : "No line")}</strong>
          {selectedOrder && selectedLine ? (
            <a className="warehouse-match-open" href={businessPanelHref(query, submoduleId, "warehouse-match", `${selectedOrder.id}:${selectedLine.id}`, "bottom")}>
              {de ? "Nachweis oeffnen" : "Open evidence"}
            </a>
          ) : null}
        </div>
        <div className="warehouse-match-score"><strong>{selectedLineProgress}%</strong><span>{de ? "Anforderung erfuellt" : "requirement met"}</span></div>
        <div className="warehouse-match-pill-grid">
          {requirementGroups.map((group) => (
            <section className={`warehouse-match-pill-column is-${group.key}`} key={group.key}>
              <header>{group.label}</header>
              <div>
                {group.items.map((item) => (
                  <span className={item.score === 100 ? "is-done" : item.score > 0 ? "is-active" : undefined} key={`${group.key}-${item.label}`}>
                    <b>{item.label}</b>
                    <small>{item.detail}</small>
                    <em>{item.score}%</em>
                  </span>
                ))}
              </div>
            </section>
          ))}
        </div>
      </aside>
    </div>
  );
}

function WarehousePanel({
  query,
  record,
  submoduleId
}: {
  query: QueryState;
  record: { id: string; status?: string; name?: string; sourceId?: string; lines?: unknown[]; version?: number };
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const de = locale === "de";
  const lines = Array.isArray(record.lines) ? record.lines as Array<Record<string, unknown>> : [];
  const lineLabel = (line: Record<string, unknown>, index: number) => {
    const item = typeof line.inventoryItemId === "string" ? line.inventoryItemId : `line-${index + 1}`;
    const qty = typeof line.quantity === "number" ? line.quantity : 0;
    const picked = typeof line.pickedQuantity === "number" ? line.pickedQuantity : 0;
    const shipped = typeof line.shippedQuantity === "number" ? line.shippedQuantity : 0;
    const released = typeof line.releasedQuantity === "number" ? line.releasedQuantity : 0;
    return { item, picked, qty, released, shipped };
  };

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={record.name ?? record.sourceId ?? record.id} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">
        {de
          ? "Auftragsakte mit Materialdeckung, Wertschritten, Versandgate und Ledger-Kontext."
          : "Order file with material coverage, value steps, shipping gate, and ledger context."}
      </p>
      <dl className="drawer-facts">
        <div><dt>ID</dt><dd>{record.id}</dd></div>
        <div><dt>Status</dt><dd>{record.status ?? "master"}</dd></div>
        <div><dt>Version</dt><dd>{record.version ?? 1}</dd></div>
        <div><dt>Lines</dt><dd>{record.lines?.length ?? 0}</dd></div>
      </dl>
      <section className="ops-drawer-section">
        <h3>{de ? "Positionen" : "Positions"}</h3>
        <div className="ops-mini-list">
          {lines.length > 0 ? lines.map((line, index) => {
            const label = lineLabel(line, index);
            return (
              <span key={`${record.id}-line-${index}`}>
                {label.item} · {label.picked}/{label.qty} {de ? "gepickt" : "picked"} · {label.shipped}/{label.qty} {de ? "versendet" : "shipped"} · {label.released} {de ? "offen" : "open"}
              </span>
            );
          }) : <span>{de ? "Keine Positionen verknuepft." : "No linked positions."}</span>}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{de ? "Freigabe-Gates" : "Release gates"}</h3>
        <div className="ops-mini-list">
          <span>{de ? "Material: alle Positionen 100% gedeckt" : "Material: every line covered at 100%"}</span>
          <span>{de ? "Wertschoepfung: Build, QA und Pack abgeschlossen" : "Value creation: build, QA, and pack complete"}</span>
          <span>{de ? "Versand: Ship erst nach 100% Fortschritt" : "Shipping: ship only after 100% progress"}</span>
          <span>{de ? "Audit: Status entsteht aus Warehouse Commands" : "Audit: status comes from warehouse commands"}</span>
        </div>
      </section>
    </div>
  );
}

async function WarehouseAdminPanel({
  query,
  submoduleId
}: {
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const de = locale === "de";
  const warehouse = (await getWarehouseSnapshot()).snapshot;
  const warehouses = warehouse.locations.filter((location) => location.kind === "warehouse");
  const selectedWarehouse = warehouses.find((location) => location.id === query.recordId) ?? warehouses[0];
  const childLocations = (parentId: string) => warehouse.locations.filter((location) => location.parentId === parentId);
  const zones = selectedWarehouse ? childLocations(selectedWarehouse.id).filter((location) => location.kind === "zone") : [];
  const firstZoneId = zones[0]?.id;

  return (
    <div className="drawer-content ops-drawer warehouse-admin-drawer">
      <DrawerHeader title={de ? "Lagerverwaltung" : "Warehouse management" } query={query} submoduleId={submoduleId} />
      <p className="drawer-description">
        {de
          ? "Stammdaten, Lagerstruktur und Inventurkontext bleiben links unten, waehrend die Abarbeitung sichtbar bleibt."
          : "Master data, layout, and count context stay in the lower-left drawer while order work remains visible."}
      </p>
      <section className="ops-drawer-section">
        <h3>{de ? "Aktives Lager" : "Active warehouse"}</h3>
        <div className="drawer-field-grid">
          <label className="drawer-field">
            <span>Name</span>
            <input readOnly value={selectedWarehouse?.name ?? ""} />
          </label>
          <label className="drawer-field">
            <span>Owner</span>
            <input readOnly value={selectedWarehouse?.defaultOwnerPartyId ?? SYSTEM_OWNER_PARTY_ID} />
          </label>
          <label className="drawer-field">
            <span>{de ? "Wareneingang" : "Receiving"}</span>
            <select defaultValue={selectedWarehouse?.receivable ? "yes" : "no"}>
              <option value="yes">{de ? "aktiv" : "active"}</option>
              <option value="no">{de ? "gesperrt" : "blocked"}</option>
            </select>
          </label>
          <label className="drawer-field">
            <span>{de ? "Pickbar" : "Pickable"}</span>
            <select defaultValue={selectedWarehouse?.pickable ? "yes" : "no"}>
              <option value="yes">{de ? "aktiv" : "active"}</option>
              <option value="no">{de ? "gesperrt" : "blocked"}</option>
            </select>
          </label>
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{de ? "Struktur anlegen" : "Create structure"}</h3>
        {selectedWarehouse ? <WarehouseLayoutActions sectionId={firstZoneId} warehouseId={selectedWarehouse.id} /> : null}
        <div className="ops-mini-list">
          {zones.length > 0 ? zones.map((zone) => {
            const slots = childLocations(zone.id).filter((location) => location.kind === "bin");
            return <span key={zone.id}>{zone.name} · {slots.length} Slots · {zone.pickable ? "pickbar" : "nicht pickbar"}</span>;
          }) : <span>{de ? "Noch keine Bereiche." : "No sections yet."}</span>}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{de ? "Inventur" : "Cycle count"}</h3>
        <div className="drawer-field-grid">
          <label className="drawer-field">
            <span>{de ? "Zaehlliste" : "Count sheet"}</span>
            <select defaultValue={warehouse.cycleCounts[0]?.id ?? "new"}>
              <option value={warehouse.cycleCounts[0]?.id ?? "new"}>{warehouse.cycleCounts[0]?.id ?? (de ? "Neue Zaehlliste" : "New count")}</option>
            </select>
          </label>
          <label className="drawer-field">
            <span>{de ? "Modus" : "Mode"}</span>
            <select defaultValue="slot">
              <option value="slot">{de ? "Slot zaehlen" : "Count slot"}</option>
              <option value="item">{de ? "Artikel pruefen" : "Check item"}</option>
            </select>
          </label>
        </div>
      </section>
    </div>
  );
}

async function WarehouseMatchPanel({
  query,
  submoduleId
}: {
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const de = locale === "de";
  const warehouse = (await getWarehouseSnapshot()).snapshot;
  const [reservationId, lineId] = String(query.recordId ?? "").split(":");
  const reservation = warehouse.reservations.find((item) => item.id === reservationId) ?? warehouse.reservations[0];
  const line = reservation?.lines.find((item) => item.id === lineId) ?? reservation?.lines[0];
  const item = line ? warehouse.items.find((entry) => entry.id === line.inventoryItemId) : undefined;
  const sources = line
    ? warehouse.balances
        .filter((balance) => balance.inventoryItemId === line.inventoryItemId)
        .sort((a, b) => (b.stockStatus === "available" ? 1 : 0) - (a.stockStatus === "available" ? 1 : 0) || b.quantity - a.quantity)
        .slice(0, 8)
    : [];
  const completedWorkSteps = new Set(warehouse.commandLog
    .filter((command) => command.type === "CompleteValueStep" && typeof command.payload.reservationId === "string" && typeof command.payload.step === "string")
    .map((command) => typeof command.payload.lineId === "string"
      ? `${command.payload.reservationId}:${command.payload.lineId}:${command.payload.step}`
      : `${command.payload.reservationId}:${command.payload.step}`));
  const locationName = (id: string) => warehouse.locations.find((location) => location.id === id)?.name ?? id;
  const materialScore = line && line.quantity > 0 ? Math.round(((line.quantity - line.releasedQuantity) / line.quantity) * 100) : 0;
  const pickedScore = line && line.quantity > 0 ? Math.round((line.pickedQuantity / line.quantity) * 100) : 0;
  const shippedScore = line && line.quantity > 0 ? Math.round((line.shippedQuantity / line.quantity) * 100) : 0;
  const availableQuantity = sources
    .filter((source) => source.stockStatus === "available")
    .reduce((sum, source) => sum + source.quantity, 0);
  const requiredQuantity = line?.quantity ?? 0;
  const shortage = Math.max(0, requiredQuantity - (line ? line.quantity - line.releasedQuantity : 0));
  const sourceScore = requiredQuantity > 0 ? Math.min(100, Math.round((availableQuantity / requiredQuantity) * 100)) : 0;
  const lineStepDone = (step: "build" | "qa" | "pack") => Boolean(reservation && line && (
    completedWorkSteps.has(`${reservation.id}:${line.id}:${step}`) || completedWorkSteps.has(`${reservation.id}:${step}`)
  ));
  const buildScore = lineStepDone("build") ? 100 : reservation?.status === "consumed" ? 100 : pickedScore >= 100 ? 80 : Math.min(60, pickedScore);
  const qaScore = lineStepDone("qa") ? 100 : reservation?.status === "consumed" ? 100 : buildScore >= 100 ? 70 : Math.min(35, buildScore);
  const packScore = lineStepDone("pack") ? 100 : reservation?.status === "consumed" ? 100 : shippedScore >= 100 ? 100 : Math.min(40, qaScore);
  const auditScore = warehouse.commandLog.some((command) => command.payload && JSON.stringify(command.payload).includes(reservation?.id ?? "")) ? 100 : 40;
  const backupScore = requiredQuantity > 0 && availableQuantity > requiredQuantity ? 100 : availableQuantity > 0 ? 65 : 0;
  const cleanHandoffScore = shortage === 0 && packScore === 100 && shippedScore === 100 ? 100 : shortage === 0 ? 75 : 20;
  const scoreTone = (score: number) => score >= 90 ? "is-full" : score >= 60 ? "is-partial" : "is-missing";
  const scoreLabel = (score: number) => score >= 90 ? (de ? "erfuellt" : "fulfilled") : score >= 60 ? (de ? "teilweise" : "partial") : (de ? "offen" : "open");
  const requirementColumns = [
    {
      key: "base",
      title: de ? "Basis-Anforderungen" : "Base requirements",
      subtitle: de ? "Muss erfuellt sein, sonst keine Freigabe." : "Must pass before the order can be released.",
      items: [
        {
          evidence: item ? `${item.sku} · ${item.trackingMode}` : "-",
          gap: item ? (de ? "keine Artikelluecke" : "no item gap") : (de ? "Artikel fehlt" : "item missing"),
          label: de ? "Richtiger Artikel" : "Correct item",
          score: item ? 100 : 0
        },
        {
          evidence: line ? `${line.quantity - line.releasedQuantity}/${line.quantity} ${item?.uom ?? ""}` : "0/0",
          gap: shortage > 0 ? `${shortage} ${de ? "fehlt" : "missing"}` : (de ? "voll gedeckt" : "fully covered"),
          label: de ? "Menge gedeckt" : "Quantity covered",
          score: materialScore
        },
        {
          evidence: reservation?.inventoryOwnerPartyId ?? "-",
          gap: de ? "Owner-Dimension gesetzt" : "owner dimension present",
          label: de ? "Bestands-Owner passt" : "Inventory owner matches",
          score: reservation?.inventoryOwnerPartyId ? 100 : 0
        }
      ]
    },
    {
      key: "performance",
      title: de ? "Leistungs-Anforderungen" : "Performance requirements",
      subtitle: de ? "Bewertet die eigentliche Wertschöpfung der Auftragsposition." : "Scores the value-creation work on this order line.",
      items: [
        {
          evidence: line ? `${line.pickedQuantity}/${line.quantity}` : "0/0",
          gap: pickedScore === 100 ? (de ? "Pick abgeschlossen" : "pick complete") : (de ? "Pick offen" : "pick open"),
          label: de ? "Kommissioniert" : "Picked",
          score: pickedScore
        },
        {
          evidence: buildScore === 100 ? (de ? "Build-Command vorhanden" : "build command present") : (de ? "aus Pick-Fortschritt abgeleitet" : "inferred from pick progress"),
          gap: buildScore === 100 ? (de ? "fertig" : "done") : (de ? "Fertigung offen" : "build open"),
          label: de ? "Fertigung / Montage" : "Build / assembly",
          score: buildScore
        },
        {
          evidence: qaScore === 100 ? (de ? "QA-Command vorhanden" : "QA command present") : (de ? "Gate noch nicht voll freigegeben" : "gate not fully approved"),
          gap: qaScore === 100 ? (de ? "freigegeben" : "approved") : (de ? "QA offen" : "QA open"),
          label: de ? "Qualitaetsfreigabe" : "Quality approval",
          score: qaScore
        }
      ]
    },
    {
      key: "enthusiasm",
      title: de ? "Begeisterungsfaktoren" : "Delight factors",
      subtitle: de ? "Nicht zwingend, aber zeigt robuste und schnelle Auslieferung." : "Not mandatory, but signals robust, fast fulfillment.",
      items: [
        {
          evidence: `${availableQuantity} ${de ? "verfuegbar" : "available"} · ${sources.length} ${de ? "Quellen" : "sources"}`,
          gap: sourceScore >= 100 ? (de ? "Sofortquelle vorhanden" : "instant source available") : (de ? "Quelle pruefen" : "check source"),
          label: de ? "Beste Lagerquelle" : "Best warehouse source",
          score: sourceScore
        },
        {
          evidence: backupScore === 100 ? (de ? "Pufferbestand vorhanden" : "buffer stock exists") : `${availableQuantity}/${requiredQuantity}`,
          gap: backupScore === 100 ? (de ? "Reserve vorhanden" : "reserve available") : (de ? "kein voller Puffer" : "no full buffer"),
          label: de ? "Puffer gegen Stoerung" : "Buffer against disruption",
          score: backupScore
        },
        {
          evidence: `${reservation?.status ?? "-"} · audit ${auditScore}%`,
          gap: cleanHandoffScore === 100 ? (de ? "versandklar" : "clean handoff") : (de ? "Handoff noch offen" : "handoff still open"),
          label: de ? "Sauberer Handoff" : "Clean handoff",
          score: cleanHandoffScore
        }
      ]
    }
  ];
  const allScores = requirementColumns.flatMap((column) => column.items.map((entry) => entry.score));
  const overallScore = allScores.length ? Math.round(allScores.reduce((sum, score) => sum + score, 0) / allScores.length) : 0;

  return (
    <div className="drawer-content ops-drawer warehouse-match-drawer warehouse-ai-score-drawer">
      <DrawerHeader title={de ? "KI Match-Scoring" : "AI match scoring"} query={query} submoduleId={submoduleId} />
      <div className="warehouse-ai-score-summary">
        <div>
          <span>{reservation?.sourceId ?? "-"}</span>
          <strong>{item?.name ?? (de ? "Auftragsposition" : "Order line")}</strong>
          <small>{item?.sku ?? "-"} · {de ? "Bestellung gegen gelieferten Zustand" : "order requirements against delivered state"}</small>
        </div>
        <em className={scoreTone(overallScore)}>{overallScore}%</em>
      </div>
      <div className="warehouse-ai-score-grid">
        {requirementColumns.map((column) => (
          <section className={`warehouse-ai-score-column is-${column.key}`} key={column.key}>
            <header>
              <strong>{column.title}</strong>
              <span>{column.subtitle}</span>
            </header>
            <div className="warehouse-ai-requirements">
              {column.items.map((entry) => (
                <article className={scoreTone(entry.score)} key={`${column.key}-${entry.label}`}>
                  <div>
                    <strong>{entry.label}</strong>
                    <em>{entry.score}%</em>
                  </div>
                  <p>{entry.evidence}</p>
                  <small>{scoreLabel(entry.score)} · {entry.gap}</small>
                </article>
              ))}
            </div>
          </section>
        ))}
      </div>
      <div className="warehouse-ai-evidence-row">
        <section>
          <h3>{de ? "Quellen" : "Sources"}</h3>
          <div>
            {sources.map((source) => (
              <span key={source.balanceKey}>{locationName(source.locationId)} · {source.stockStatus} · {source.quantity}</span>
            ))}
          </div>
        </section>
        <section>
          <h3>{de ? "KI-Begruendung" : "AI rationale"}</h3>
          <p>
            {overallScore >= 90
              ? (de ? "Die Position ist fachlich sauber gematcht. Basis, Leistung und Handoff sind konsistent nachweisbar." : "The line is cleanly matched. Base, performance, and handoff evidence are consistent.")
              : shortage > 0
                ? (de ? "Die Basis-Anforderung ist blockiert, weil die bestellte Menge nicht voll gedeckt ist." : "The base requirement is blocked because quantity coverage is incomplete.")
                : (de ? "Die Basis passt, aber mindestens ein Leistungs- oder Handoff-Gate ist noch offen." : "Base requirements pass, but at least one performance or handoff gate is still open.")}
          </p>
        </section>
      </div>
    </div>
  );
}

function DrawerHeader({
  title,
  query,
  submoduleId
}: {
  title: string;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = businessCopy[locale];

  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <a href={businessBaseHref(query, submoduleId)}>{copy.close}</a>
    </div>
  );
}

function BusinessPaneHead({
  children,
  description,
  title
}: {
  children?: React.ReactNode;
  description?: string;
  title: string;
}) {
  return (
    <div className="ops-pane-head">
      <div>
        <h2>{title}</h2>
        {description ? <p>{description}</p> : null}
      </div>
      <div className="ops-pane-actions">{children}</div>
    </div>
  );
}

function BusinessSignal({ href, label, value }: { href?: string; label: string; value: string }) {
  const content = (
    <>
      <span>{label}</span>
      <strong>{value}</strong>
    </>
  );
  const context = href ? businessContextFromHref(href, label) : {};
  return href ? <a className="ops-signal" href={href} {...context}>{content}</a> : <div className="ops-signal" {...context}>{content}</div>;
}

function businessContextFromHref(href: string, label: string) {
  const [path, search = ""] = href.split("?");
  const [, moduleId = "business", submoduleId = "customers"] = path.match(/\/app\/([^/]+)\/([^/?]+)/) ?? [];
  const params = new URLSearchParams(search);
  const panel = params.get("panel") ?? "record";
  const recordId = params.get("recordId") ?? label.toLowerCase().replaceAll(" ", "-");

  return {
    "data-context-action": panel.includes("set") ? "open-set" : "open",
    "data-context-item": true,
    "data-context-label": label,
    "data-context-module": moduleId,
    "data-context-record-id": recordId,
    "data-context-record-type": panel,
    "data-context-submodule": submoduleId
  };
}

function BusinessRecordList({ items, title }: { items: string[]; title: string }) {
  return (
    <section className="ops-drawer-section">
      <h3>{title}</h3>
      <div className="ops-mini-list">
        {items.length > 0 ? items.map((item) => <span key={item}>{item}</span>) : <span>No linked records.</span>}
      </div>
    </section>
  );
}

function businessPanelHref(
  query: QueryState,
  submoduleId: string,
  panel: string,
  recordId: string,
  drawer: "left-bottom" | "bottom" | "right"
) {
  if (query.panel === panel && query.recordId === recordId) {
    return businessBaseHref(query, submoduleId);
  }

  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.selectedId) params.set("selectedId", query.selectedId);
  if (query.warehouseSearch) params.set("warehouseSearch", query.warehouseSearch);
  if (query.orderSearch) params.set("orderSearch", query.orderSearch);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  return `/app/business/${submoduleId}?${params.toString()}`;
}

function businessBaseHref(query: QueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.selectedId) params.set("selectedId", query.selectedId);
  if (query.warehouseSearch) params.set("warehouseSearch", query.warehouseSearch);
  if (query.orderSearch) params.set("orderSearch", query.orderSearch);
  const queryString = params.toString();
  return queryString ? `/app/business/${submoduleId}?${queryString}` : `/app/business/${submoduleId}`;
}

function businessSelectionHref(query: QueryState, submoduleId: string, recordId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.warehouseSearch) params.set("warehouseSearch", query.warehouseSearch);
  if (query.orderSearch) params.set("orderSearch", query.orderSearch);
  params.set("selectedId", recordId);
  return `/app/business/${submoduleId}?${params.toString()}`;
}

function businessWarehouseSelectionHref(query: QueryState, submoduleId: string, warehouseId: string | undefined, orderId: string, lineId?: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.warehouseSearch) params.set("warehouseSearch", query.warehouseSearch);
  if (query.orderSearch) params.set("orderSearch", query.orderSearch);
  if (warehouseId) params.set("selectedId", warehouseId);
  params.set("recordId", lineId ? `${orderId}:${lineId}` : orderId);
  return `/app/business/${submoduleId}?${params.toString()}`;
}

function businessRecordHref(query: QueryState, submoduleId: string, panel: string, recordId: string) {
  return businessPanelHref(query, submoduleId, panel, recordId, "right");
}

type BusinessSetItem = {
  id: string;
  label: string;
  meta: string;
  panel: string;
  type: string;
  amount: number;
};

function resolveBusinessSet(recordId: string | undefined, data: BusinessBundle, locale: SupportedLocale, copy: BusinessCopy, warehouse?: WarehouseState) {
  const key = recordId ?? "customers";
  const customerItems = (items: BusinessCustomer[]): BusinessSetItem[] => items.map((customer) => ({
    id: customer.id,
    label: customer.name,
    meta: `${customer.status} · ${businessCurrency(customer.arBalance, "EUR", locale)} AR`,
    panel: "customer",
    type: "customer",
    amount: customer.arBalance
  }));
  const invoiceItems = (items: BusinessInvoice[]): BusinessSetItem[] => items.map((invoice) => ({
    id: invoice.id,
    label: invoice.number,
    meta: `${invoice.status} · ${invoice.dueDate} · ${businessCurrency(invoice.total, invoice.currency, locale)}`,
    panel: "invoice",
    type: "invoice",
    amount: invoice.total
  }));
  const productItems = (items: BusinessProduct[]): BusinessSetItem[] => items.map((product) => ({
    id: product.id,
    label: product.name,
    meta: `${product.status} · ${product.taxRate}% tax · ${businessCurrency(product.price, "EUR", locale)}`,
    panel: "product",
    type: "product",
    amount: product.price
  }));
  const accountItems = (items: BusinessAccount[]): BusinessSetItem[] => items.map((account) => {
    const trialRow = buildTrialBalance(data).find((row) => row.account.id === account.id);
    return {
      id: account.id,
      label: `${account.code} ${account.name}`,
      meta: `${account.rootType} · ${account.accountType} · ${businessCurrency(trialRow?.balance ?? 0, account.currency, locale)}`,
      panel: "account",
      type: "account",
      amount: Math.abs(trialRow?.balance ?? 0)
    };
  });
  const journalItems = (items: BusinessJournalEntry[]): BusinessSetItem[] => items.map((entry) => ({
    id: entry.id,
    label: entry.number,
    meta: `${entry.status} · ${entry.postingDate} · ${text(entry.narration, locale)}`,
    panel: "journal-entry",
    type: "journal-entry",
    amount: entry.lines.reduce((sum, line) => sum + line.debit, 0)
  }));
  const receiptItems = (items: BusinessReceipt[]): BusinessSetItem[] => items.map((receipt) => ({
    id: receipt.id,
    label: receipt.number,
    meta: `${receipt.status} · ${receipt.vendorName} · ${businessCurrency(receipt.total, receipt.currency, locale)}`,
    panel: "receipt",
    type: "receipt",
    amount: receipt.total
  }));
  const bankItems = (items: BusinessBankTransaction[]): BusinessSetItem[] => items.map((transaction) => ({
    id: transaction.id,
    label: transaction.counterparty,
    meta: `${transaction.status} · ${transaction.bookingDate} · ${businessCurrency(transaction.amount, transaction.currency, locale)}`,
    panel: "bank-transaction",
    type: "bank-transaction",
    amount: Math.abs(transaction.amount)
  }));
  const exportItems = (items: BusinessBookkeepingExport[]): BusinessSetItem[] => items.map((exportBatch) => ({
    id: exportBatch.id,
    label: `${exportBatch.system} ${exportBatch.period}`,
    meta: `${exportBatch.status} · ${businessCurrency(exportBatch.taxAmount, "EUR", locale)} tax`,
    panel: "export",
    type: "export",
    amount: exportBatch.netAmount
  }));
  const reportItems = (items: BusinessReport[]): BusinessSetItem[] => items.map((report) => ({
    id: report.id,
    label: report.title,
    meta: `${report.status} · ${report.period} · ${businessCurrency(report.amount, "EUR", locale)}`,
    panel: "report",
    type: "report",
    amount: report.amount
  }));
  const warehouseReplayItems = (): BusinessSetItem[] => {
    const commands = warehouse?.commandLog.slice(-8).reverse() ?? [];
    if (commands.length > 0) {
      return commands.map((command) => ({
        id: command.refId,
        label: command.type,
        meta: `${command.refType} · ${command.requestedAt}`,
        panel: "warehouse-record",
        type: "warehouse-command",
        amount: 0
      }));
    }

    return (warehouse?.locations ?? []).slice(0, 8).map((location) => ({
      id: location.id,
      label: location.name,
      meta: `${location.kind} · ${location.pickable ? "pickable" : "not pickable"} · ${location.receivable ? "receivable" : "not receivable"}`,
      panel: "warehouse-record",
      type: "warehouse-location",
      amount: 0
    }));
  };

  const set = (() => {
    if (key === "receivables") return { title: copy.receivables, description: copy.businessSetReceivablesDescription, items: invoiceItems(data.invoices.filter((invoice) => invoice.status !== "Paid")), resource: "invoices" };
    if (key === "overdue") return { title: copy.overdue, description: copy.businessSetReceivablesDescription, items: invoiceItems(data.invoices.filter((invoice) => invoice.status === "Overdue")), resource: "invoices" };
    if (key === "reminders-due") return { title: copy.reminders, description: copy.businessSetReceivablesDescription, items: invoiceItems(data.invoices.filter((invoice) => invoice.collectionStatus === "Reminder due" || invoice.collectionStatus === "Reminder sent" || invoice.collectionStatus === "Final notice")), resource: "invoices" };
    if (key === "paid-invoices") return { title: copy.paid, description: copy.businessSetRevenueDescription, items: invoiceItems(data.invoices.filter((invoice) => invoice.status === "Paid" || invoice.balanceDue === 0)), resource: "invoices" };
    if (key === "tax-review") return { title: copy.taxReview, description: copy.businessSetTaxDescription, items: exportItems(data.bookkeeping.filter((item) => item.status === "Needs review")), resource: "bookkeeping" };
    if (key === "products-billable") return { title: copy.billable, description: copy.businessSetProductsDescription, items: productItems(data.products.filter((item) => item.status === "Billable")), resource: "products" };
    if (key === "products-review") return { title: copy.review, description: copy.businessSetProductsDescription, items: productItems(data.products.filter((item) => item.status === "Review")), resource: "products" };
    if (key === "products-draft") return { title: copy.draft, description: copy.businessSetProductsDescription, items: productItems(data.products.filter((item) => item.status === "Draft")), resource: "products" };
    if (key === "exports-ready") return { title: copy.ready, description: copy.businessSetExportsDescription, items: exportItems(data.bookkeeping.filter((item) => item.status === "Ready")), resource: "bookkeeping" };
    if (key === "exports-queued") return { title: copy.queued, description: copy.businessSetExportsDescription, items: exportItems(data.bookkeeping.filter((item) => item.status === "Queued")), resource: "bookkeeping" };
    if (key === "export-tax") return { title: copy.tax, description: copy.businessSetTaxDescription, items: exportItems(data.bookkeeping), resource: "bookkeeping" };
    if (key === "payables") return { title: copy.payables, description: copy.businessSetTaxDescription, items: accountItems(data.accounts.filter((item) => item.accountType === "payable")), resource: "accounts" };
    if (key === "vat-payable") return { title: copy.vatPayable, description: copy.businessSetTaxDescription, items: accountItems(data.accounts.filter((item) => item.accountType === "tax")), resource: "accounts" };
    if (key === "unbalanced") return { title: copy.unbalanced, description: copy.businessSetTaxDescription, items: journalItems(data.journalEntries.filter((entry) => !isBalanced(entry))), resource: "ledger" };
    if (key === "posted-journal") return { title: copy.posted, description: copy.ledgerDescription, items: journalItems(data.journalEntries.filter((entry) => entry.status === "Posted")), resource: "ledger" };
    if (key === "draft-journal") return { title: copy.draft, description: copy.ledgerDescription, items: journalItems(data.journalEntries.filter((entry) => entry.status === "Draft")), resource: "ledger" };
    if (key === "expenses") return { title: copy.expenses, description: copy.ledgerDescription, items: accountItems(data.accounts.filter((item) => item.rootType === "expense")), resource: "accounts" };
    if (key === "receipts-review") return { title: copy.needsReview, description: copy.receiptReviewDescription, items: receiptItems(data.receipts.filter((item) => item.status === "Needs review")), resource: "receipts" };
    if (key === "receipts-inbox") return { title: copy.inbox, description: copy.receiptsDescription, items: receiptItems(data.receipts.filter((item) => item.status === "Inbox")), resource: "receipts" };
    if (key === "receipts-open-total") return { title: copy.openAmount, description: copy.receiptsDescription, items: receiptItems(data.receipts.filter((item) => item.status === "Needs review" || item.status === "Inbox")), resource: "receipts" };
    if (key === "bank-matched") return { title: copy.matched, description: copy.reconciliationDescription, items: bankItems(data.bankTransactions.filter((item) => item.status === "Matched")), resource: "payments" };
    if (key === "bank-suggested") return { title: copy.suggested, description: copy.reconciliationDescription, items: bankItems(data.bankTransactions.filter((item) => item.status === "Suggested")), resource: "payments" };
    if (key === "bank-unmatched") return { title: copy.unmatched, description: copy.reconciliationDescription, items: bankItems(data.bankTransactions.filter((item) => item.status === "Unmatched")), resource: "payments" };
    if (key === "bank-balance") return { title: copy.bankBalance, description: copy.reconciliationDescription, items: accountItems(data.accounts.filter((item) => item.accountType === "bank")), resource: "accounts" };
    if (key === "revenue") return { title: copy.revenue, description: copy.businessSetRevenueDescription, items: invoiceItems(data.invoices), resource: "invoices" };
    if (key === "invoice-tax") return { title: copy.tax, description: copy.businessSetTaxDescription, items: invoiceItems(data.invoices), resource: "invoices" };
    if (key === "open-reports") return { title: copy.openReports, description: copy.businessSetReportsDescription, items: reportItems(data.reports.filter((report) => report.status !== "Current")), resource: "reports" };
    if (key === "warehouse-replay") return { title: "Replay", description: locale === "de" ? "Auditfaehige Lagerereignisse und bestaetigte Kommandos." : "Auditable warehouse events and confirmed commands.", items: warehouseReplayItems(), resource: "warehouse" };
    if (key === "exports") return { title: copy.exports, description: copy.businessSetExportsDescription, items: exportItems(data.bookkeeping), resource: "bookkeeping" };
    return { title: copy.customers, description: copy.businessSetCustomersDescription, items: customerItems(data.customers), resource: "customers" };
  })();

  return {
    ...set,
    amount: set.items.reduce((sum, item) => sum + item.amount, 0)
  };
}

function resolveNewResource(recordId: string | undefined, submoduleId: string) {
  if (recordId?.includes("journal") || submoduleId === "ledger") return "ledger";
  if (recordId?.includes("receipt") || submoduleId === "receipts") return "receipts";
  if (recordId?.includes("payment") || submoduleId === "payments") return "payments";
  if (recordId?.includes("product") || submoduleId === "products") return "products";
  if (recordId?.includes("invoice") || submoduleId === "invoices") return "invoices";
  if (recordId?.includes("export") || submoduleId === "bookkeeping") return "bookkeeping";
  if (recordId?.includes("report") || submoduleId === "reports") return "reports";
  return "customers";
}

function invoiceNet(invoice: BusinessInvoice) {
  return invoice.netAmount ?? invoice.total - invoice.taxAmount;
}

function invoiceBalance(invoice: BusinessInvoice) {
  if (typeof invoice.balanceDue === "number") return invoice.balanceDue;
  return invoice.status === "Paid" ? 0 : invoice.total;
}

function invoiceAgeLabel(invoice: BusinessInvoice, locale: SupportedLocale, copy: BusinessCopy) {
  if (invoice.status === "Paid") return copy.paid;
  if (invoice.status === "Draft") return copy.draft;
  if (invoice.status !== "Overdue") return collectionStatusLabel(invoice.collectionStatus, locale) ?? copy.open;

  const due = new Date(`${invoice.dueDate}T00:00:00.000Z`).getTime();
  const today = new Date("2026-05-02T00:00:00.000Z").getTime();
  const days = Math.max(1, Math.round((today - due) / 86400000));
  return locale === "de" ? `seit ${days} Tagen überfällig` : `${days} days overdue`;
}

function collectionStatusLabel(status: BusinessInvoice["collectionStatus"], locale: SupportedLocale) {
  if (!status) return undefined;
  const de: Record<NonNullable<BusinessInvoice["collectionStatus"]>, string> = {
    Clear: "OK",
    "Due soon": "demnächst fällig",
    "Final notice": "letzte Mahnung",
    "Reminder due": "Mahnung fällig",
    "Reminder sent": "Mahnung gesendet"
  };
  if (locale === "de") return de[status];
  return status;
}

type BusinessViewProps = {
  copy: BusinessCopy;
  data: BusinessBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
};

const businessCopy = {
  en: {
    account: "Account",
    addDocument: "Add document",
    addDocumentDescription: "Drop or select PDF, JPEG, PNG, or XML evidence for this invoice.",
    addLineItem: "Line item",
    address: "Address",
    addressExtra: "Address addition",
    aging: "Aging",
    agingDescription: "Invoices grouped by operating status with one-click drawers for follow-up.",
    all: "All",
    amount: "Amount",
    attachment: "Attachment",
    askCtoxReport: "Ask CTOX to refresh report",
    askCtoxSet: "Ask CTOX to process this set",
    askCtoxSync: "Ask CTOX to sync Business",
    billable: "Billable",
    billing: "Billing",
    bookkeeping: "Bookkeeping",
    bookkeepingDescription: "Export batches, tax checks, and accounting handoff context.",
    businessRecord: "Business record",
    businessSetCustomersDescription: "All customers as one billing, receivables, and CRM synchronization context.",
    businessSetExportsDescription: "Bookkeeping exports grouped for accounting handoff and exception review.",
    businessSetProductsDescription: "Products and services grouped by billing readiness, tax setup, and revenue mapping.",
    businessSetReceivablesDescription: "Open receivables grouped for follow-up, escalation, and cash-flow review.",
    businessSetReportsDescription: "Management reports that need refresh, export context, or CTOX follow-up.",
    businessSetRevenueDescription: "Revenue-carrying invoices grouped for reporting and forecast checks.",
    businessSetTaxDescription: "Tax-relevant records grouped for review before export or reporting.",
    close: "Close",
    completeAndPrint: "Complete & print",
    completeAndSend: "Complete & send",
    capturePayment: "Register payment",
    autoGenerated: "assigned automatically",
    businessAccount: "Business account",
    bankTransferNote: "Please transfer the invoice amount to the bank account stated on the invoice.",
    cancel: "Cancel",
    city: "City",
    closingNote: "Closing note",
    closingNotePlural: "Closing notes",
    closingNotePlaceholder: "Thank you for the good collaboration.",
    collection: "Collection",
    collectionQueue: "Collection queue",
    company: "Company",
    companyName: "Company name",
    continuePreview: "Continue to preview",
    country: "Country",
    createAsCustomer: "Create customer",
    createCustomer: "Create customer",
    customer: "Customer",
    customerNotFoundPrompt: "This customer is not in master data yet.",
    customerNumber: "Customer number",
    customerType: "Contact",
    customers: "Customers",
    customersDescription: "Accounts with billing, tax, receivable, and CRM sync context.",
    date: "Date",
    deliveryOrService: "Delivery or service",
    deliveryDate: "Delivery date",
    deliveryPeriod: "Delivery period",
    description: "Description",
    discount: "Discount",
    discountIn: "Discount in",
    deleteLine: "Delete line",
    draft: "Draft",
    due: "Due",
    document: "Document",
    documentEditor: "Document editor",
    documentList: "Documents",
    documentTexts: "Document texts",
    documentTitle: "Document title",
    done: "Done",
    draftSave: "Save draft",
    duplicateLine: "Duplicate line",
    edit: "Edit",
    export: "Export",
    exportContext: "Export context",
    exportLines: "Export lines",
    exportLinesDescription: "Invoice lines included in each accounting export.",
    exportReadiness: "Export readiness",
    exportReadinessDescription: "Accounting handoff status with tax totals and review pressure.",
    exports: "Exports",
    email: "Email",
    emailCopy: "Copy",
    emailSend: "Send email",
    emailShipping: "Email dispatch",
    freeText: "Free text",
    gross: "Gross",
    hour: "Hour",
    invoice: "Invoice",
    invoiceEditorDescription: "Create the selected invoice in the document editor, including customer data, line items, tax, payment terms, and payment methods.",
    invoiceListDescription: "Select an invoice or add a new document.",
    invoiceLines: "Invoice lines",
    invoiceNumber: "Invoice number",
    invoices: "Invoices",
    invoicesDescription: "Receivables, due dates, tax, and bookkeeping export status in one view.",
    introText: "Intro text",
    introTextPlural: "Intro texts",
    introTextPlaceholder: "We are pleased to invoice the following services:",
    offerIntroTemplate: "We are pleased to submit the following offer:",
    issueDate: "Date",
    items: "Items",
    lineItem: "Line item",
    lines: "Lines",
    manageUnits: "Manage units",
    margin: "Margin",
    moreDetails: "Details",
    moveDown: "Move down",
    moveUp: "Move up",
    net: "Net",
    newCustomer: "New customer",
    newExport: "New export",
    newInvoice: "New invoice",
    newProduct: "New product",
    newRecord: "New Business record",
    newRecordDescription: "Create the record in this Business submodule and queue CTOX for synchronization.",
    newReport: "New report",
    newTemplate: "New",
    noRecordSelected: "Open or create a Business record to inspect facts and queue CTOX follow-up.",
    noServiceDate: "No delivery date",
    noReminders: "No reminder work is currently due.",
    notExported: "Not exported",
    open: "Open",
    openReports: "Open reports",
    overdue: "Overdue",
    owner: "Owner",
    paid: "Paid",
    paymentCondition: "Payment condition",
    paymentConditionPlural: "Payment conditions",
    paymentConditionPlaceholder: "Payment target 14 days. Please reference the invoice number.",
    paymentTerms: "Payment terms",
    payments: "Payments",
    period: "Period",
    person: "Person",
    percent: "Percent",
    piece: "Piece",
    phone: "Phone number",
    postalCode: "Postal code",
    preview: "Preview",
    previewDescription: "Selected invoice with document facts, payment state, and tags.",
    print: "Print",
    price: "Price",
    product: "Product",
    products: "Products",
    productsDescription: "Products and services with pricing, margin, tax, and revenue-account context.",
    profit: "Profit",
    projectReferenceNote: "Please include the project reference in all correspondence.",
    quantity: "Quantity",
    queueCreate: "Queue create",
    queueExport: "Queue export",
    queued: "Queued",
    ready: "Ready",
    receivables: "Receivables",
    receivablesDescription: "Open invoice exposure across customers.",
    recipient: "Recipient",
    report: "Report",
    reportSignals: "Report signals",
    reportSignalsDescription: "Management reporting totals from invoices, tax, and export state.",
    reports: "Reports",
    reportsDescription: "Finance reports with status, due date, tax context, and export links.",
    revenue: "Revenue",
    revenueUse: "Revenue use",
    revenueUseDescription: "Where each product or service appears in current invoices.",
    review: "Review",
    reviewer: "Reviewer",
    reminder: "Reminder",
    reminderLevel: "Reminder level",
    reminders: "Reminders",
    searchInvoices: "Search by customer, document number, or amount",
    selectedItems: "Selected items",
    selectCustomer: "Select customer",
    sendByEmail: "Send by email",
    serviceDate: "Service date",
    servicePeriod: "Service period",
    signature: "Signature",
    serviceIntroTemplate: "The agreed services are invoiced as follows:",
    segment: "Segment",
    save: "Save",
    sku: "SKU",
    standard: "Standard",
    standardTemplate: "Standard template",
    status: "Status",
    street: "Street",
    subject: "Subject",
    subjectPlaceholder: "Name the customer, invoice, export, or report...",
    syncRail: "Sync",
    syncRailDescription: "Business records CTOX should keep aligned with CRM, Operations, and core queue context.",
    tax: "Tax",
    taxContext: "Tax context",
    taxId: "Tax ID",
    taxMode: "Tax mode",
    taxRate: "Tax rate",
    taxReview: "Tax review",
    taxSetup: "Tax setup",
    taxSetupDescription: "Products that need billing or tax mapping attention.",
    template: "Template",
    toReceive: "To receive",
    totalDiscount: "Total discount",
    totalGross: "Total amount",
    subtotalNet: "Net total",
    unit: "Unit",
    unitNet: "Sales price (net)",
    useWithoutMasterData: "Use without master data",
    value: "Value",
    accountDrawerDescription: "Account ledger, tax mapping, and posting activity.",
    accountingControls: "Accounting controls",
    accountingControlsDescription: "Journal state, balances, and exceptions that need review.",
    accountType: "Account type",
    balance: "Balance",
    balanced: "Balanced",
    bankBalance: "Bank balance",
    bankMatch: "Bank match",
    confidence: "Confidence",
    contraAccount: "Contra account",
    counterparty: "Counterparty",
    credit: "Credit",
    debit: "Debit",
    expenses: "Expenses",
    extractedFields: "Extracted fields",
    inbox: "Inbox",
    ledger: "Ledger",
    ledgerDescription: "Double-entry journal, chart of accounts, and audit-ready ledger lines.",
    manual: "Manual",
    match: "Match",
    matched: "Matched",
    needsReview: "Needs review",
    newJournalEntry: "New journal entry",
    newReceipt: "New receipt",
    noTaxCode: "No tax code",
    notMatched: "Not matched",
    openAmount: "Open amount",
    payables: "Payables",
    paymentsDescription: "Bank feed, payment matches, confidence, and reconciliation work.",
    posted: "Posted",
    receipt: "Receipt",
    receiptReview: "Receipt review",
    receiptReviewDescription: "Inbound documents that still need OCR, tax, or account confirmation.",
    receipts: "Receipts",
    receiptsDescription: "Inbound receipts with extracted fields, tax treatment, and posting state.",
    reconcile: "Reconcile",
    reconciliation: "Reconciliation",
    reconciliationDescription: "Bank lines that need matching, posting, or human confirmation.",
    reference: "Reference",
    reviewReceipt: "Review receipt",
    rootType: "Root type",
    splitPosting: "Split posting",
    suggested: "Suggested",
    taxCode: "Tax code",
    trialBalance: "Trial balance",
    trialBalanceDescription: "Debit, credit, and signed balances by posting account.",
    unbalanced: "Unbalanced",
    unmatched: "Unmatched",
    vat: "VAT",
    vatPayable: "VAT payable",
    vendor: "Vendor"
  },
  de: {
    account: "Konto",
    addDocument: "Belegdokument hinzufügen",
    addDocumentDescription: "PDF, JPEG, PNG oder XML für diese Rechnung ablegen oder auswählen.",
    addLineItem: "Artikel",
    address: "Adresse",
    addressExtra: "Adresszusatz",
    aging: "Faelligkeiten",
    agingDescription: "Rechnungen nach Arbeitsstatus gruppiert, mit Drawern fuer Follow-up.",
    all: "Alle",
    amount: "Betrag",
    attachment: "Anhang",
    askCtoxReport: "CTOX Report aktualisieren lassen",
    askCtoxSet: "CTOX mit dieser Auswahl beauftragen",
    askCtoxSync: "CTOX Business OS synchronisieren lassen",
    billable: "Abrechenbar",
    billing: "Billing",
    bookkeeping: "Buchhaltung",
    bookkeepingDescription: "Export-Batches, Steuerchecks und Buchhaltungsuebergabe.",
    businessRecord: "Business-Datensatz",
    businessSetCustomersDescription: "Alle Kunden als Billing-, Forderungs- und CRM-Synchronisierungskontext.",
    businessSetExportsDescription: "Buchhaltungsexporte gruppiert fuer Uebergabe und Ausnahmepruefung.",
    businessSetProductsDescription: "Produkte und Services gruppiert nach Billing-Bereitschaft, Steuer-Setup und Erloesmapping.",
    businessSetReceivablesDescription: "Offene Forderungen gruppiert fuer Follow-up, Eskalation und Cashflow Review.",
    businessSetReportsDescription: "Management-Reports, die Refresh, Exportkontext oder CTOX-Follow-up brauchen.",
    businessSetRevenueDescription: "Revenue-tragende Rechnungen gruppiert fuer Reporting und Forecast Checks.",
    businessSetTaxDescription: "Steuerrelevante Datensaetze gruppiert fuer Review vor Export oder Reporting.",
    close: "Schliessen",
    completeAndPrint: "Abschließen & Drucken",
    completeAndSend: "Abschließen & Senden",
    capturePayment: "Zahlung erfassen",
    autoGenerated: "wird automatisch vergeben",
    businessAccount: "Geschäftskonto",
    bankTransferNote: "Überweisung bitte auf das in der Rechnung angegebene Geschäftskonto.",
    cancel: "Abbrechen",
    city: "Ort",
    closingNote: "Nachbemerkung",
    closingNotePlural: "Nachbemerkungen",
    closingNotePlaceholder: "Vielen Dank für die gute Zusammenarbeit.",
    collection: "Mahnwesen",
    collectionQueue: "Mahnqueue",
    company: "Firma",
    companyName: "Firmenname",
    continuePreview: "Weiter zur Vorschau",
    country: "Land",
    createAsCustomer: "Kunden anlegen",
    createCustomer: "Kunde erstellen",
    customer: "Kunde",
    customerNotFoundPrompt: "Dieser Kunde ist noch nicht in den Stammdaten.",
    customerNumber: "Kundennummer",
    customerType: "Kontakt",
    customers: "Kunden",
    customersDescription: "Accounts mit Billing-, Steuer-, Forderungs- und CRM-Sync-Kontext.",
    date: "Datum",
    deliveryOrService: "Lieferung oder Leistung",
    deliveryDate: "Lieferdatum",
    deliveryPeriod: "Lieferzeitraum",
    description: "Beschreibung",
    discount: "Rabatt",
    discountIn: "Rabatt in",
    deleteLine: "Position loeschen",
    draft: "Entwurf",
    due: "Fällig",
    document: "Beleg",
    documentEditor: "Belegeditor",
    documentList: "Belege",
    documentTexts: "Belegtexte",
    documentTitle: "Belegtitel",
    done: "Fertig",
    draftSave: "Entwurf speichern",
    duplicateLine: "Position duplizieren",
    edit: "Bearbeiten",
    export: "Export",
    exportContext: "Export-Kontext",
    exportLines: "Export-Zeilen",
    exportLinesDescription: "Rechnungszeilen in den jeweiligen Buchhaltungsexporten.",
    exportReadiness: "Export-Bereitschaft",
    exportReadinessDescription: "Buchhaltungsstatus mit Steuersummen und Review-Druck.",
    exports: "Exporte",
    email: "E-Mail",
    emailCopy: "Kopie",
    emailSend: "E-Mail senden",
    emailShipping: "E-Mail Versand",
    freeText: "Freitext",
    gross: "Brutto",
    hour: "Stunde",
    invoice: "Rechnung",
    invoiceEditorDescription: "Ausgewaehlte Rechnung im Belegeditor erstellen: Kundendaten, Positionen, Steuer, Zahlungsbedingungen und Zahlungsarten.",
    invoiceListDescription: "Rechnung auswaehlen oder neuen Beleg hinzufuegen.",
    invoiceLines: "Rechnungszeilen",
    invoiceNumber: "Rechnungsnummer",
    invoices: "Rechnungen",
    invoicesDescription: "Forderungen, Faelligkeiten, Steuer und Exportstatus in einer Ansicht.",
    introText: "Einleitungstext",
    introTextPlural: "Einleitungstexte",
    introTextPlaceholder: "Unsere Leistungen stellen wir Ihnen wie folgt in Rechnung.",
    offerIntroTemplate: "Gerne bieten wir Ihnen folgende Leistungen an:",
    issueDate: "Datum",
    items: "Eintraege",
    lineItem: "Artikel",
    lines: "Zeilen",
    manageUnits: "Einheiten verwalten",
    margin: "Marge",
    moreDetails: "Details",
    moveDown: "Nach unten verschieben",
    moveUp: "Nach oben verschieben",
    net: "Netto",
    newCustomer: "Neuer Kunde",
    newExport: "Neuer Export",
    newInvoice: "Neue Rechnung",
    newProduct: "Neues Produkt",
    newRecord: "Neuer Business-Datensatz",
    newRecordDescription: "Datensatz in diesem Business-Submodul anlegen und CTOX fuer Sync queuen.",
    newReport: "Neuer Report",
    newTemplate: "Neu",
    noRecordSelected: "Business-Datensatz öffnen oder anlegen, um Fakten zu prüfen und CTOX zu queuen.",
    noServiceDate: "Kein Lieferdatum",
    noReminders: "Aktuell ist keine Mahnarbeit faellig.",
    notExported: "Nicht exportiert",
    open: "Offen",
    openReports: "Offene Reports",
    overdue: "Überfällig",
    owner: "Owner",
    paid: "Bezahlt",
    paymentCondition: "Zahlungsbedingung",
    paymentConditionPlural: "Zahlungsbedingungen",
    paymentConditionPlaceholder: "Zahlungsziel 14 Tage. Bitte Rechnungsnummer als Referenz angeben.",
    paymentTerms: "Zahlungsziel",
    payments: "Zahlungen",
    period: "Periode",
    person: "Person",
    percent: "Prozent",
    piece: "Stück",
    phone: "Rufnummer",
    postalCode: "PLZ",
    preview: "Vorschau",
    previewDescription: "Ausgewaehlte Rechnung mit Belegfakten, Zahlungsstand und Tags.",
    print: "Drucken",
    price: "Preis",
    product: "Produkt",
    products: "Produkte",
    productsDescription: "Produkte und Services mit Pricing, Marge, Steuer und Erloskonten.",
    profit: "Ergebnis",
    projectReferenceNote: "Bitte geben Sie bei Rueckfragen die Projektreferenz mit an.",
    quantity: "Menge",
    queueCreate: "Create queuen",
    queueExport: "Export queuen",
    queued: "Queued",
    ready: "Bereit",
    receivables: "Forderungen",
    receivablesDescription: "Offene Rechnungsexposure ueber Kunden.",
    recipient: "Empfänger:in",
    report: "Report",
    reportSignals: "Report-Signale",
    reportSignalsDescription: "Management-Reporting-Summen aus Rechnungen, Steuer und Exportstatus.",
    reports: "Reports",
    reportsDescription: "Finance Reports mit Status, Due Date, Steuerkontext und Exportlinks.",
    revenue: "Revenue",
    revenueUse: "Revenue-Nutzung",
    revenueUseDescription: "Wo jedes Produkt oder jeder Service in Rechnungen auftaucht.",
    review: "Review",
    reviewer: "Reviewer",
    reminder: "Mahnung",
    reminderLevel: "Mahnstufe",
    reminders: "Mahnungen",
    searchInvoices: "Nach Kunde, Belegnummer oder Betrag suchen",
    selectedItems: "Ausgewaehlte Eintraege",
    selectCustomer: "Kunde auswaehlen",
    sendByEmail: "Per E-Mail senden",
    serviceDate: "Leistungsdatum",
    servicePeriod: "Leistungszeitraum",
    signature: "Signatur",
    serviceIntroTemplate: "Die vereinbarten Leistungen stellen wir Ihnen wie folgt in Rechnung.",
    segment: "Segment",
    save: "Speichern",
    sku: "SKU",
    standard: "Standard",
    standardTemplate: "Standard-Vorlage",
    status: "Status",
    street: "Straße",
    subject: "Betreff",
    subjectPlaceholder: "Kunde, Rechnung, Export oder Report benennen...",
    syncRail: "Sync",
    syncRailDescription: "Business-Datensaetze, die CTOX mit CRM, Operations und Core Queue verbinden soll.",
    tax: "Steuer",
    taxContext: "Steuerkontext",
    taxId: "Steuer-ID",
    taxMode: "Steuermodus",
    taxRate: "Steuersatz",
    taxReview: "Steuer-Review",
    taxSetup: "Steuer-Setup",
    taxSetupDescription: "Produkte, die Billing- oder Steuermapping-Aufmerksamkeit brauchen.",
    template: "Vorlage",
    toReceive: "Zu erhalten",
    totalDiscount: "Gesamtrabatt",
    totalGross: "Gesamtbetrag",
    subtotalNet: "Summe Netto",
    unit: "Einheit",
    unitNet: "VK (Netto)",
    useWithoutMasterData: "Ohne Stammdaten verwenden",
    value: "Wert",
    accountDrawerDescription: "Kontenblatt, Steuermapping und Buchungsaktivitaet.",
    accountingControls: "Buchhaltungskontrollen",
    accountingControlsDescription: "Journalstatus, Salden und Ausnahmen mit Review-Bedarf.",
    accountType: "Kontotyp",
    balance: "Saldo",
    balanced: "Ausgeglichen",
    bankBalance: "Banksaldo",
    bankMatch: "Bankabgleich",
    confidence: "Konfidenz",
    contraAccount: "Gegenkonto",
    counterparty: "Gegenpartei",
    credit: "Haben",
    debit: "Soll",
    expenses: "Aufwand",
    extractedFields: "Extrahierte Felder",
    inbox: "Eingang",
    ledger: "Journal",
    ledgerDescription: "Doppelte Buchfuehrung mit Kontenrahmen, Journal und pruefbaren Buchungszeilen.",
    manual: "Manuell",
    match: "Abgleich",
    matched: "Abgeglichen",
    needsReview: "Review noetig",
    newJournalEntry: "Neue Buchung",
    newReceipt: "Neuer Eingangsbeleg",
    noTaxCode: "Kein Steuerschluessel",
    notMatched: "Nicht abgeglichen",
    openAmount: "Offener Betrag",
    payables: "Verbindlichkeiten",
    paymentsDescription: "Bankfeed, Zahlungsabgleich, Konfidenz und offene Reconciliation-Arbeit.",
    posted: "Gebucht",
    receipt: "Eingangsbeleg",
    receiptReview: "Belegreview",
    receiptReviewDescription: "Eingangsbelege mit OCR-, Steuer- oder Kontierungsbedarf.",
    receipts: "Eingangsbelege",
    receiptsDescription: "Eingangsbelege mit extrahierten Feldern, Steuerbehandlung und Buchungsstatus.",
    reconcile: "Abgleichen",
    reconciliation: "Reconciliation",
    reconciliationDescription: "Bankzeilen, die Abgleich, Buchung oder menschliche Bestaetigung brauchen.",
    reference: "Referenz",
    reviewReceipt: "Beleg pruefen",
    rootType: "Kontenklasse",
    splitPosting: "Splitbuchung",
    suggested: "Vorgeschlagen",
    taxCode: "Steuerschluessel",
    trialBalance: "Summen- und Saldenliste",
    trialBalanceDescription: "Soll, Haben und vorzeichenrichtige Salden je bebuchtem Konto.",
    unbalanced: "Nicht ausgeglichen",
    unmatched: "Ungeklaert",
    vat: "USt",
    vatPayable: "USt Zahllast",
    vendor: "Lieferant"
  }
} satisfies Record<SupportedLocale, Record<string, string>>;
