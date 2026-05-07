import { integer, pgEnum, pgTable, text, timestamp, uuid } from "drizzle-orm/pg-core";

export const moduleEnum = pgEnum("business_module", [
  "sales",
  "marketing",
  "operations",
  "business"
]);

export const organizations = pgTable("organizations", {
  id: uuid("id").primaryKey().defaultRandom(),
  name: text("name").notNull(),
  slug: text("slug").notNull().unique(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const ctoxSyncEvents = pgTable("ctox_sync_events", {
  id: uuid("id").primaryKey().defaultRandom(),
  module: moduleEnum("module").notNull(),
  eventType: text("event_type").notNull(),
  recordType: text("record_type").notNull(),
  recordId: text("record_id").notNull(),
  payloadJson: text("payload_json").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow()
});

export const ctoxBugReports = pgTable("ctox_bug_reports", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  title: text("title").notNull(),
  moduleId: text("module_id").notNull(),
  submoduleId: text("submodule_id").notNull(),
  status: text("status").notNull(),
  severity: text("severity").notNull(),
  tagsJson: text("tags_json").notNull().default("[]"),
  payloadJson: text("payload_json").notNull(),
  coreTaskId: text("core_task_id"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingCompetitorWatchlist = pgTable("marketing_competitor_watchlist", {
  id: uuid("id").primaryKey().defaultRandom(),
  name: text("name").notNull(),
  url: text("url").notNull(),
  source: text("source").notNull().default("manual"),
  ctoxScrapeTargetKey: text("ctox_scrape_target_key").notNull().default("marketing-competitive-analysis"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingCompetitiveScrapeRuns = pgTable("marketing_competitive_scrape_runs", {
  id: uuid("id").primaryKey().defaultRandom(),
  targetKey: text("target_key").notNull(),
  triggerKind: text("trigger_kind").notNull(),
  status: text("status").notNull(),
  criterion: text("criterion"),
  ctoxRunId: text("ctox_run_id"),
  payloadJson: text("payload_json").notNull(),
  scheduledFor: timestamp("scheduled_for", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesAccounts = pgTable("sales_accounts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesContacts = pgTable("sales_contacts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesOpportunities = pgTable("sales_opportunities", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesLeads = pgTable("sales_leads", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesCampaigns = pgTable("sales_campaigns", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesCustomers = pgTable("sales_customers", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesOffers = pgTable("sales_offers", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const salesTasks = pgTable("sales_tasks", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingWebsitePages = pgTable("marketing_website_pages", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingAssets = pgTable("marketing_assets", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingCampaigns = pgTable("marketing_campaigns", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingResearchItems = pgTable("marketing_research_items", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const marketingCommerceItems = pgTable("marketing_commerce_items", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessCustomers = pgTable("business_customers", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessAccounts = pgTable("business_accounts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessBankTransactions = pgTable("business_bank_transactions", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessJournalEntries = pgTable("business_journal_entries", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessProducts = pgTable("business_products", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessInvoices = pgTable("business_invoices", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessBookkeepingExports = pgTable("business_bookkeeping_exports", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessReceipts = pgTable("business_receipts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessReports = pgTable("business_reports", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  label: text("label").notNull(),
  status: text("status").notNull(),
  ownerId: text("owner_id"),
  payloadJson: text("payload_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessAccountingProposals = pgTable("business_accounting_proposals", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  kind: text("kind").notNull(),
  status: text("status").notNull(),
  refType: text("ref_type").notNull(),
  refId: text("ref_id").notNull(),
  proposedCommandJson: text("proposed_command_json").notNull(),
  evidenceJson: text("evidence_json").notNull(),
  confidence: integer("confidence").notNull().default(0),
  createdByAgent: text("created_by_agent").notNull(),
  decidedBy: text("decided_by"),
  decidedAt: timestamp("decided_at", { withTimezone: true }),
  resultingJournalEntryId: text("resulting_journal_entry_id"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessOutboxEvents = pgTable("business_outbox_events", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  topic: text("topic").notNull(),
  payloadJson: text("payload_json").notNull(),
  status: text("status").notNull().default("pending"),
  attempts: integer("attempts").notNull().default(0),
  deliveredAt: timestamp("delivered_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const businessAccountingAuditEvents = pgTable("business_accounting_audit_events", {
  id: uuid("id").primaryKey().defaultRandom(),
  companyId: text("company_id").notNull(),
  actorType: text("actor_type").notNull(),
  actorId: text("actor_id").notNull(),
  action: text("action").notNull(),
  refType: text("ref_type").notNull(),
  refId: text("ref_id").notNull(),
  beforeJson: text("before_json"),
  afterJson: text("after_json"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingAccounts = pgTable("accounting_accounts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  code: text("code").notNull(),
  name: text("name").notNull(),
  rootType: text("root_type").notNull(),
  accountType: text("account_type").notNull(),
  parentExternalId: text("parent_external_id"),
  isGroup: integer("is_group").notNull().default(0),
  currency: text("currency").notNull().default("EUR"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingParties = pgTable("accounting_parties", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  kind: text("kind").notNull(),
  name: text("name").notNull(),
  taxId: text("tax_id"),
  vatId: text("vat_id"),
  defaultReceivableAccountExternalId: text("default_receivable_account_external_id"),
  defaultPayableAccountExternalId: text("default_payable_account_external_id"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingTaxRates = pgTable("accounting_tax_rates", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  code: text("code").notNull(),
  rate: integer("rate").notNull().default(0),
  accountExternalId: text("account_external_id"),
  type: text("type").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingFiscalPeriods = pgTable("accounting_fiscal_periods", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  startDate: text("start_date").notNull(),
  endDate: text("end_date").notNull(),
  status: text("status").notNull().default("open"),
  closedAt: timestamp("closed_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingInvoices = pgTable("accounting_invoices", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  customerExternalId: text("customer_external_id").notNull(),
  number: text("number").notNull(),
  status: text("status").notNull(),
  issueDate: text("issue_date").notNull(),
  serviceDate: text("service_date"),
  dueDate: text("due_date").notNull(),
  currency: text("currency").notNull().default("EUR"),
  netAmountMinor: integer("net_amount_minor").notNull().default(0),
  taxAmountMinor: integer("tax_amount_minor").notNull().default(0),
  totalAmountMinor: integer("total_amount_minor").notNull().default(0),
  balanceDueMinor: integer("balance_due_minor").notNull().default(0),
  pdfBlobRef: text("pdf_blob_ref"),
  zugferdXml: text("zugferd_xml"),
  postedJournalEntryExternalId: text("posted_journal_entry_external_id"),
  sentAt: timestamp("sent_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingInvoiceLines = pgTable("accounting_invoice_lines", {
  id: uuid("id").primaryKey().defaultRandom(),
  invoiceExternalId: text("invoice_external_id").notNull(),
  lineNo: integer("line_no").notNull(),
  productExternalId: text("product_external_id"),
  description: text("description").notNull(),
  quantity: integer("quantity").notNull().default(1),
  unitPriceMinor: integer("unit_price_minor").notNull().default(0),
  lineNetMinor: integer("line_net_minor").notNull().default(0),
  taxRate: integer("tax_rate").notNull().default(0),
  taxAmountMinor: integer("tax_amount_minor").notNull().default(0),
  lineTotalMinor: integer("line_total_minor").notNull().default(0),
  revenueAccountExternalId: text("revenue_account_external_id")
});

export const accountingReceipts = pgTable("accounting_receipts", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  vendorExternalId: text("vendor_external_id"),
  number: text("number").notNull(),
  vendorInvoiceNumber: text("vendor_invoice_number"),
  status: text("status").notNull(),
  receiptDate: text("receipt_date").notNull(),
  dueDate: text("due_date"),
  currency: text("currency").notNull().default("EUR"),
  netAmountMinor: integer("net_amount_minor").notNull().default(0),
  taxAmountMinor: integer("tax_amount_minor").notNull().default(0),
  totalAmountMinor: integer("total_amount_minor").notNull().default(0),
  expenseAccountExternalId: text("expense_account_external_id"),
  payableAccountExternalId: text("payable_account_external_id"),
  taxCode: text("tax_code"),
  ocrText: text("ocr_text"),
  extractedJson: text("extracted_json"),
  postedJournalEntryExternalId: text("posted_journal_entry_external_id"),
  reviewedAt: timestamp("reviewed_at", { withTimezone: true }),
  postedAt: timestamp("posted_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingReceiptFiles = pgTable("accounting_receipt_files", {
  id: uuid("id").primaryKey().defaultRandom(),
  receiptExternalId: text("receipt_external_id").notNull(),
  blobRef: text("blob_ref").notNull(),
  mime: text("mime").notNull(),
  originalFilename: text("original_filename").notNull(),
  sha256: text("sha256").notNull(),
  uploadedAt: timestamp("uploaded_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingReceiptLines = pgTable("accounting_receipt_lines", {
  id: uuid("id").primaryKey().defaultRandom(),
  receiptExternalId: text("receipt_external_id").notNull(),
  lineNo: integer("line_no").notNull(),
  description: text("description").notNull(),
  expenseAccountExternalId: text("expense_account_external_id").notNull(),
  netAmountMinor: integer("net_amount_minor").notNull().default(0),
  taxCode: text("tax_code"),
  taxAmountMinor: integer("tax_amount_minor").notNull().default(0),
  totalAmountMinor: integer("total_amount_minor").notNull().default(0)
});

export const accountingPayments = pgTable("accounting_payments", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  partyExternalId: text("party_external_id"),
  kind: text("kind").notNull(),
  paymentDate: text("payment_date").notNull(),
  amountMinor: integer("amount_minor").notNull().default(0),
  currency: text("currency").notNull().default("EUR"),
  bankAccountExternalId: text("bank_account_external_id").notNull(),
  bankStatementLineExternalId: text("bank_statement_line_external_id"),
  postedJournalEntryExternalId: text("posted_journal_entry_external_id"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingPaymentAllocations = pgTable("accounting_payment_allocations", {
  id: uuid("id").primaryKey().defaultRandom(),
  paymentExternalId: text("payment_external_id").notNull(),
  invoiceExternalId: text("invoice_external_id"),
  receiptExternalId: text("receipt_external_id"),
  amountMinor: integer("amount_minor").notNull().default(0)
});

export const accountingBankStatements = pgTable("accounting_bank_statements", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  accountExternalId: text("account_external_id").notNull(),
  format: text("format").notNull(),
  importedBy: text("imported_by"),
  sourceFilename: text("source_filename").notNull(),
  sourceSha256: text("source_sha256").notNull(),
  startDate: text("start_date"),
  endDate: text("end_date"),
  openingBalanceMinor: integer("opening_balance_minor").notNull().default(0),
  closingBalanceMinor: integer("closing_balance_minor").notNull().default(0),
  importedAt: timestamp("imported_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingBankStatementLines = pgTable("accounting_bank_statement_lines", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  statementExternalId: text("statement_external_id").notNull(),
  lineNo: integer("line_no").notNull(),
  bookingDate: text("booking_date").notNull(),
  valueDate: text("value_date"),
  amountMinor: integer("amount_minor").notNull().default(0),
  currency: text("currency").notNull().default("EUR"),
  remitterName: text("remitter_name"),
  remitterIban: text("remitter_iban"),
  purpose: text("purpose"),
  endToEndRef: text("end_to_end_ref"),
  matchStatus: text("match_status").notNull().default("unmatched"),
  matchedJournalEntryExternalId: text("matched_journal_entry_external_id"),
  duplicateOfLineExternalId: text("duplicate_of_line_external_id")
});

export const accountingNumberSeries = pgTable("accounting_number_series", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  key: text("key").notNull(),
  fiscalYear: integer("fiscal_year").notNull(),
  prefix: text("prefix").notNull(),
  nextValue: integer("next_value").notNull().default(1),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingJournalEntries = pgTable("accounting_journal_entries", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  postingDate: text("posting_date").notNull(),
  type: text("type").notNull(),
  refType: text("ref_type").notNull(),
  refId: text("ref_id").notNull(),
  number: text("number").notNull(),
  narration: text("narration"),
  createdBy: text("created_by").notNull(),
  reversedByExternalId: text("reversed_by_external_id"),
  postedAt: timestamp("posted_at", { withTimezone: true }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow()
});

export const accountingJournalEntryLines = pgTable("accounting_journal_entry_lines", {
  id: uuid("id").primaryKey().defaultRandom(),
  journalEntryExternalId: text("journal_entry_external_id").notNull(),
  lineNo: integer("line_no").notNull(),
  accountExternalId: text("account_external_id").notNull(),
  partyExternalId: text("party_external_id"),
  debitMinor: integer("debit_minor").notNull().default(0),
  creditMinor: integer("credit_minor").notNull().default(0),
  costCenterExternalId: text("cost_center_external_id"),
  projectExternalId: text("project_external_id")
});

export const accountingLedgerEntries = pgTable("accounting_ledger_entries", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  companyId: text("company_id").notNull(),
  postingDate: text("posting_date").notNull(),
  accountExternalId: text("account_external_id").notNull(),
  partyExternalId: text("party_external_id"),
  debitMinor: integer("debit_minor").notNull().default(0),
  creditMinor: integer("credit_minor").notNull().default(0),
  refType: text("ref_type").notNull(),
  refId: text("ref_id").notNull(),
  journalEntryExternalId: text("journal_entry_external_id").notNull(),
  reverted: integer("reverted").notNull().default(0),
  revertsExternalId: text("reverts_external_id"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsProjects = pgTable("operations_projects", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  code: text("code").notNull(),
  name: text("name").notNull(),
  ownerId: text("owner_id").notNull(),
  customerId: text("customer_id"),
  health: text("health").notNull(),
  progress: integer("progress").notNull().default(0),
  startDate: text("start_date").notNull(),
  endDate: text("end_date").notNull(),
  nextMilestone: text("next_milestone").notNull(),
  summaryJson: text("summary_json").notNull(),
  linkedModulesJson: text("linked_modules_json").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsWorkItems = pgTable("operations_work_items", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  projectExternalId: text("project_external_id").notNull(),
  subject: text("subject").notNull(),
  type: text("type").notNull(),
  status: text("status").notNull(),
  priority: text("priority").notNull(),
  assigneeId: text("assignee_id").notNull(),
  dueDate: text("due_date").notNull(),
  estimate: integer("estimate").notNull().default(0),
  descriptionJson: text("description_json").notNull(),
  linkedKnowledgeIdsJson: text("linked_knowledge_ids_json").notNull().default("[]"),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsMilestones = pgTable("operations_milestones", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  projectExternalId: text("project_external_id").notNull(),
  title: text("title").notNull(),
  dueDate: text("due_date").notNull(),
  status: text("status").notNull(),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsKnowledgeItems = pgTable("operations_knowledge_items", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  projectExternalId: text("project_external_id").notNull(),
  title: text("title").notNull(),
  kind: text("kind").notNull(),
  ownerId: text("owner_id").notNull(),
  sectionsJson: text("sections_json").notNull(),
  linkedWorkItemIdsJson: text("linked_work_item_ids_json").notNull().default("[]"),
  updatedOn: text("updated_on").notNull(),
  ctoxKnowledgeKey: text("ctox_knowledge_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsMeetings = pgTable("operations_meetings", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  projectExternalId: text("project_external_id").notNull(),
  title: text("title").notNull(),
  startsAt: text("starts_at").notNull(),
  facilitatorId: text("facilitator_id").notNull(),
  agendaJson: text("agenda_json").notNull(),
  decisionIdsJson: text("decision_ids_json").notNull().default("[]"),
  actionItemIdsJson: text("action_item_ids_json").notNull().default("[]"),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsDecisions = pgTable("operations_decisions", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  meetingExternalId: text("meeting_external_id").notNull(),
  projectExternalId: text("project_external_id").notNull(),
  textJson: text("text_json").notNull(),
  linkedWorkItemIdsJson: text("linked_work_item_ids_json").notNull().default("[]"),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});

export const operationsActionItems = pgTable("operations_action_items", {
  id: uuid("id").primaryKey().defaultRandom(),
  externalId: text("external_id").notNull().unique(),
  ownerId: text("owner_id").notNull(),
  dueDate: text("due_date").notNull(),
  textJson: text("text_json").notNull(),
  workItemExternalId: text("work_item_external_id"),
  ctoxSyncKey: text("ctox_sync_key").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow()
});
