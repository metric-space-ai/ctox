const inferredPanels: Record<string, Record<string, string>> = {
  business: {
    bookkeeping: "export",
    customers: "customer",
    invoices: "invoice",
    products: "product",
    reports: "report"
  },
  ctox: {
    bugs: "bug",
    knowledge: "knowledge",
    queue: "queue",
    runs: "run",
    settings: "setting",
    sync: "sync"
  },
  marketing: {
    assets: "asset",
    commerce: "commerce",
    "competitive-analysis": "competitor",
    research: "research",
    website: "page"
  },
  operations: {
    boards: "work-set",
    knowledge: "knowledge",
    meetings: "meeting",
    planning: "operations-set",
    projects: "project",
    "work-items": "work-item"
  },
  sales: {
    campaigns: "campaign",
    contacts: "contact",
    customers: "customer",
    leads: "lead",
    pipeline: "opportunity",
    tasks: "task"
  }
};

export function inferDeepLinkPanel(moduleId: string, submoduleId: string) {
  return inferredPanels[moduleId]?.[submoduleId] ?? "record";
}
