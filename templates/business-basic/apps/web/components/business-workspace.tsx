import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import {
  businessCurrency,
  getBusinessBundle,
  text,
  type BusinessBookkeepingExport,
  type BusinessBundle,
  type BusinessCustomer,
  type BusinessInvoice,
  type BusinessProduct,
  type BusinessReport,
  type SupportedLocale
} from "../lib/business-seed";
import { BusinessCreateForm, BusinessQueueButton } from "./business/business-actions";
import { InvoiceCustomerEditor, type InvoiceCustomerOption } from "./invoice-customer-editor";
import { InvoiceDeliveryActions } from "./invoice-delivery-actions";
import { InvoiceDocumentSelector, type InvoiceDocumentOption } from "./invoice-document-selector";
import { InvoiceListSidebar, type InvoiceListItem, type InvoiceListMetric } from "./invoice-list-sidebar";
import { InvoiceLinesEditor, type InvoiceLineDraft } from "./invoice-lines-editor";
import { LexicalRichTextEditor } from "./lexical-rich-text-editor";

type QueryState = {
  locale?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
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
  const data = await getBusinessBundle();

  if (submoduleId === "products") return <ProductsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (submoduleId === "invoices") return <InvoicesView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
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
  const data = await getBusinessBundle();
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
  const exportBatch = data.bookkeeping.find((item) => item.id === recordId);
  const report = data.reports.find((item) => item.id === recordId);

  if (panel === "business-set") {
    const businessSet = resolveBusinessSet(recordId, data, locale, copy);

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

  if (panel === "text-template") {
    return <InvoiceTextTemplatePanel copy={copy} query={query} recordId={recordId} submoduleId={submoduleId} />;
  }

  if (customer) return <CustomerPanel copy={copy} customer={customer} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (product) return <ProductPanel copy={copy} data={data} locale={locale} product={product} query={query} submoduleId={submoduleId} />;
  if (invoice) return <InvoicePanel copy={copy} data={data} invoice={invoice} locale={locale} query={query} submoduleId={submoduleId} />;
  if (exportBatch) return <BookkeepingPanel copy={copy} data={data} exportBatch={exportBatch} locale={locale} query={query} submoduleId={submoduleId} />;
  if (report) return <ReportPanel copy={copy} data={data} locale={locale} query={query} report={report} submoduleId={submoduleId} />;

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

function BookkeepingView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
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
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{copy.export}</span>
            <span>{copy.invoice}</span>
            <span>{copy.tax}</span>
            <span>{copy.amount}</span>
          </div>
          {data.bookkeeping.flatMap((exportBatch) => exportBatch.invoiceIds.map((invoiceId) => ({ exportBatch, invoice: data.invoices.find((item) => item.id === invoiceId) }))).map(({ exportBatch, invoice }) => invoice ? (
            <a
              className="ops-table-row"
              data-context-item
              data-context-label={`${exportBatch.id} ${invoice.number}`}
              data-context-module="business"
              data-context-record-id={exportBatch.id}
              data-context-record-type="bookkeeping"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "export", exportBatch.id, "right")}
              key={`${exportBatch.id}-${invoice.id}`}
            >
              <span><strong>{exportBatch.period}</strong><small>{exportBatch.system}</small></span>
              <span><strong>{invoice.number}</strong><small>{invoice.status}</small></span>
              <span><strong>{businessCurrency(invoice.taxAmount, invoice.currency, locale)}</strong><small>{invoice.lines.length} {copy.lines}</small></span>
              <span><strong>{businessCurrency(invoice.total, invoice.currency, locale)}</strong><small>{invoice.dueDate}</small></span>
            </a>
          ) : null)}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copy.exportReadiness}>
        <BusinessPaneHead title={copy.exportReadiness} description={copy.exportReadinessDescription} />
        <div className="ops-signal-list">
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "exports-ready", "right")} label={copy.ready} value={String(data.bookkeeping.filter((item) => item.status === "Ready").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "tax-review", "right")} label={copy.review} value={String(data.bookkeeping.filter((item) => item.status === "Needs review").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "exports-queued", "right")} label={copy.queued} value={String(data.bookkeeping.filter((item) => item.status === "Queued").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "export-tax", "right")} label={copy.tax} value={businessCurrency(data.bookkeeping.reduce((sum, item) => sum + item.taxAmount, 0), "EUR", locale)} />
        </div>
      </section>
    </div>
  );
}

function ReportsView({ copy, data, locale, query, submoduleId }: BusinessViewProps) {
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
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "revenue", "right")} label={copy.revenue} value={businessCurrency(data.invoices.reduce((sum, invoice) => sum + invoice.total, 0), "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "invoice-tax", "right")} label={copy.tax} value={businessCurrency(data.invoices.reduce((sum, invoice) => sum + invoice.taxAmount, 0), "EUR", locale)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "open-reports", "right")} label={copy.openReports} value={String(data.reports.filter((report) => report.status !== "Current").length)} />
          <BusinessSignal href={businessPanelHref(query, submoduleId, "business-set", "exports", "right")} label={copy.exports} value={String(data.bookkeeping.length)} />
        </div>
        <div className="ops-card-stack">
          {data.reports.map((report) => (
            <a
              className="ops-work-card"
              data-context-item
              data-context-label={report.title}
              data-context-module="business"
              data-context-record-id={report.id}
              data-context-record-type="report"
              data-context-submodule={submoduleId}
              href={businessPanelHref(query, submoduleId, "report", report.id, "right")}
              key={report.id}
            >
              <strong>{report.title}</strong>
              <small>{report.status} - {report.dueDate}</small>
              <span>{text(report.summary, locale)}</span>
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
  const queryString = params.toString();
  return queryString ? `/app/business/${submoduleId}?${queryString}` : `/app/business/${submoduleId}`;
}

function businessSelectionHref(query: QueryState, submoduleId: string, recordId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("selectedId", recordId);
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

function resolveBusinessSet(recordId: string | undefined, data: BusinessBundle, locale: SupportedLocale, copy: BusinessCopy) {
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
    if (key === "revenue") return { title: copy.revenue, description: copy.businessSetRevenueDescription, items: invoiceItems(data.invoices), resource: "invoices" };
    if (key === "invoice-tax") return { title: copy.tax, description: copy.businessSetTaxDescription, items: invoiceItems(data.invoices), resource: "invoices" };
    if (key === "open-reports") return { title: copy.openReports, description: copy.businessSetReportsDescription, items: reportItems(data.reports.filter((report) => report.status !== "Current")), resource: "reports" };
    if (key === "exports") return { title: copy.exports, description: copy.businessSetExportsDescription, items: exportItems(data.bookkeeping), resource: "bookkeeping" };
    return { title: copy.customers, description: copy.businessSetCustomersDescription, items: customerItems(data.customers), resource: "customers" };
  })();

  return {
    ...set,
    amount: set.items.reduce((sum, item) => sum + item.amount, 0)
  };
}

function resolveNewResource(recordId: string | undefined, submoduleId: string) {
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
    vat: "VAT"
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
    vat: "USt"
  }
} satisfies Record<SupportedLocale, Record<string, string>>;
