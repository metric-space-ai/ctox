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
