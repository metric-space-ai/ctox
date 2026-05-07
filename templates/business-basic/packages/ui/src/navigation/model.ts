import { resolveLocale } from "../i18n/locales";
import { resolveThemeMode } from "../theme/modes";

export type BusinessModuleId = "sales" | "marketing" | "operations" | "business" | "ctox";

export type BusinessSubmodule = {
  id: string;
  label: string;
  href: string;
  resourceTypes: string[];
};

export type BusinessModule = {
  id: BusinessModuleId;
  label: string;
  href: string;
  summary: string;
  submodules: BusinessSubmodule[];
};

export const businessModules: BusinessModule[] = [
  {
    id: "marketing",
    label: "Marketing",
    href: "/app/marketing",
    summary: "Website, materials, market research, competitive analysis, and commerce surfaces.",
    submodules: [
      { id: "website", label: "Website", href: "/app/marketing/website", resourceTypes: ["page"] },
      { id: "assets", label: "Assets", href: "/app/marketing/assets", resourceTypes: ["asset"] },
      { id: "competitive-analysis", label: "Competitive Analysis", href: "/app/marketing/competitive-analysis", resourceTypes: ["competitor", "market_signal", "research_item"] },
      { id: "research", label: "Research", href: "/app/marketing/research", resourceTypes: ["research_item"] },
      { id: "commerce", label: "Commerce", href: "/app/marketing/commerce", resourceTypes: ["product", "shop_item"] }
    ]
  },
  {
    id: "sales",
    label: "Sales",
    href: "/app/sales",
    summary: "Campaigns, lead pipeline, sales activities, offers, and customer handoff.",
    submodules: [
      { id: "campaigns", label: "Campaigns", href: "/app/sales/campaigns", resourceTypes: ["campaign", "source_import", "research_rule"] },
      { id: "pipeline", label: "Pipeline", href: "/app/sales/pipeline", resourceTypes: ["opportunity"] },
      { id: "leads", label: "Leads", href: "/app/sales/leads", resourceTypes: ["account", "contact", "sales_activity", "lead"] },
      { id: "offers", label: "Offers", href: "/app/sales/offers", resourceTypes: ["offer", "quote"] },
      { id: "customers", label: "Customers", href: "/app/sales/customers", resourceTypes: ["customer", "onboarding_project"] }
    ]
  },
  {
    id: "operations",
    label: "Operations",
    href: "/app/operations",
    summary: "Projects, work items, boards, wiki, meetings, documents, and daily work.",
    submodules: [
      { id: "projects", label: "Projects", href: "/app/operations/projects", resourceTypes: ["project"] },
      { id: "work-items", label: "Work Items", href: "/app/operations/work-items", resourceTypes: ["work_item", "ticket"] },
      { id: "boards", label: "Boards", href: "/app/operations/boards", resourceTypes: ["board"] },
      { id: "planning", label: "Planning", href: "/app/operations/planning", resourceTypes: ["timeline", "calendar_event"] },
      { id: "knowledge", label: "Knowledge", href: "/app/operations/knowledge", resourceTypes: ["wiki_page", "document", "runbook"] },
      { id: "meetings", label: "Meetings", href: "/app/operations/meetings", resourceTypes: ["meeting"] }
    ]
  },
  {
    id: "business",
    label: "Business",
    href: "/app/business",
    summary: "Products, customers, invoices, bookkeeping exports, and reporting.",
    submodules: [
      { id: "customers", label: "Customers", href: "/app/business/customers", resourceTypes: ["customer"] },
      { id: "products", label: "Products", href: "/app/business/products", resourceTypes: ["product", "service"] },
      { id: "warehouse", label: "Warehouse", href: "/app/business/warehouse", resourceTypes: ["inventory_item", "stock_balance", "stock_reservation", "shipment"] },
      { id: "fulfillment", label: "Auftragsabwicklung", href: "/app/business/fulfillment", resourceTypes: ["sales_order", "work_order", "order_line", "shipment"] },
      { id: "invoices", label: "Invoices", href: "/app/business/invoices", resourceTypes: ["invoice"] },
      { id: "ledger", label: "Ledger", href: "/app/business/ledger", resourceTypes: ["account", "journal_entry", "ledger_entry"] },
      { id: "fixed-assets", label: "Assets", href: "/app/business/fixed-assets", resourceTypes: ["fixed_asset", "depreciation"] },
      { id: "receipts", label: "Receipts", href: "/app/business/receipts", resourceTypes: ["receipt", "vendor", "attachment"] },
      { id: "payments", label: "Payments", href: "/app/business/payments", resourceTypes: ["bank_transaction", "payment", "reconciliation"] },
      { id: "bookkeeping", label: "Exports", href: "/app/business/bookkeeping", resourceTypes: ["ledger_entry", "export"] },
      { id: "reports", label: "Reports", href: "/app/business/reports", resourceTypes: ["report"] }
    ]
  },
  {
    id: "ctox",
    label: "CTOX",
    href: "/app/ctox",
    summary: "Agent runs, sync health, bug reports, knowledge links, and bridge status.",
    submodules: [
      { id: "runs", label: "Runs", href: "/app/ctox/runs", resourceTypes: ["agent_run"] },
      { id: "harness", label: "Harness", href: "/app/ctox/harness", resourceTypes: ["harness_flow", "state_machine", "process_proof"] },
      { id: "queue", label: "Queue", href: "/app/ctox/queue", resourceTypes: ["queue_item"] },
      { id: "knowledge", label: "Knowledge", href: "/app/ctox/knowledge", resourceTypes: ["knowledge_record"] },
      { id: "bugs", label: "Bugs", href: "/app/ctox/bugs", resourceTypes: ["bug_report"] },
      { id: "sync", label: "Sync", href: "/app/ctox/sync", resourceTypes: ["sync_event"] },
      { id: "settings", label: "Settings", href: "/app/ctox/settings", resourceTypes: ["setting"] }
    ]
  }
];

export const shellRoutes = businessModules.map(({ id, label, href }) => ({ id, label, href }));

export function findBusinessModule(moduleId: string) {
  return businessModules.find((module) => module.id === moduleId);
}

export function findBusinessSubmodule(moduleId: string, submoduleId: string) {
  const normalizedSubmoduleId = moduleId === "operations" && submoduleId === "wiki" ? "knowledge" : submoduleId;
  return findBusinessModule(moduleId)?.submodules.find((submodule) => submodule.id === normalizedSubmoduleId);
}

export function businessDeepLink(input: {
  baseUrl?: string;
  module: string;
  submodule?: string;
  recordId?: string;
  panel?: string;
  drawer?: "left-bottom" | "bottom" | "right";
  locale?: string;
  theme?: string;
}) {
  const module = findBusinessModule(input.module);
  if (!module) return null;
  const submodule = input.submodule
    ? findBusinessSubmodule(module.id, input.submodule)
    : module.submodules[0];
  if (!submodule) return null;
  const params = new URLSearchParams();
  if (input.recordId) params.set("recordId", input.recordId);
  if (input.panel) params.set("panel", input.panel);
  if (input.drawer) params.set("drawer", input.drawer);
  if (input.locale) params.set("locale", resolveLocale(input.locale));
  if (input.theme) params.set("theme", resolveThemeMode(input.theme));
  const query = params.toString();
  const href = query ? `${submodule.href}?${query}` : submodule.href;
  const baseUrl = input.baseUrl?.replace(/\/$/, "");
  return {
    module,
    submodule,
    href,
    url: baseUrl ? `${baseUrl}${href}` : href
  };
}
