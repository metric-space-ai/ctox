import { execFile } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type CtoxRun = {
  id: string;
  title: string;
  moduleId: string;
  submoduleId: string;
  status: "running" | "queued" | "completed" | "failed";
  model: string;
  startedAt: string;
  summary: string;
};

export type CtoxQueueItem = {
  id: string;
  title: string;
  source: string;
  priority: "low" | "normal" | "high" | "urgent";
  status: "queued" | "running" | "blocked" | "done";
  target: string;
  createdAt: string;
};

export type CtoxKnowledgeRecord = {
  id: string;
  title: string;
  moduleId: string;
  recordType: string;
  linkedRecords: string[];
  updatedAt: string;
  summary: string;
};

export type CtoxBugRecord = {
  id: string;
  type?: "bug_report";
  title: string;
  moduleId: string;
  submoduleId: string;
  status: "open" | "triaged" | "fixed";
  severity: "low" | "normal" | "high";
  createdAt: string;
  summary: string;
  expected?: string;
  pageUrl?: string;
  viewport?: Record<string, unknown>;
  annotation?: Record<string, unknown> | null;
  userAgent?: string;
  tags?: string[];
  source?: string;
  coreTaskId?: string | null;
};

export type CtoxSyncRecord = {
  id: string;
  moduleId: string;
  status: "healthy" | "queued" | "warning";
  lastEvent: string;
  lastSyncedAt: string;
  pending: number;
};

export type CtoxBundle = {
  runs: CtoxRun[];
  queue: CtoxQueueItem[];
  knowledge: CtoxKnowledgeRecord[];
  bugs: CtoxBugRecord[];
  sync: CtoxSyncRecord[];
};

export const ctoxSeed: CtoxBundle = {
  runs: [
    {
      id: "run-business-stack-smoke",
      title: "Business stack smoke verification",
      moduleId: "ctox",
      submoduleId: "runs",
      status: "completed",
      model: "ctox local agent",
      startedAt: "2026-05-02T14:40:00.000Z",
      summary: "Verified Sales, Marketing, Operations, Business routes, APIs, deep links, and queue mutations."
    },
    {
      id: "run-marketing-competitive-analysis",
      title: "Competitive analysis refresh",
      moduleId: "marketing",
      submoduleId: "competitive-analysis",
      status: "queued",
      model: "minimax m2.7 compatible webstack",
      startedAt: "2026-05-02T15:00:00.000Z",
      summary: "Scheduled scrape run waits for updated criteria and own-product benchmark inputs."
    },
    {
      id: "run-operations-knowledge-sync",
      title: "Operations knowledge sync",
      moduleId: "operations",
      submoduleId: "knowledge",
      status: "running",
      model: "ctox core",
      startedAt: "2026-05-02T14:56:00.000Z",
      summary: "Links wiki pages, decisions, action items, and project tickets into the CTOX Knowledge Store."
    }
  ],
  queue: [
    {
      id: "queue-right-click-context",
      title: "Right-click prompt context contract",
      source: "business-ui",
      priority: "high",
      status: "queued",
      target: "All modules / Prompt CTOX",
      createdAt: "2026-05-02T14:44:00.000Z"
    },
    {
      id: "queue-bug-reporter-polish",
      title: "Bug reporter screenshot annotation follow-up",
      source: "business-bug-report",
      priority: "normal",
      status: "running",
      target: "Shared shell / Bug reporter",
      createdAt: "2026-05-02T14:35:00.000Z"
    },
    {
      id: "queue-postgres-contract",
      title: "Postgres module schema verification",
      source: "business-stack",
      priority: "normal",
      status: "done",
      target: "Sales, Marketing, Operations, Business",
      createdAt: "2026-05-02T14:50:00.000Z"
    }
  ],
  knowledge: [
    {
      id: "know-business-stack-contract",
      title: "Business stack ownership contract",
      moduleId: "ctox",
      recordType: "contract",
      linkedRecords: ["ctox-business-install.json", "CUSTOMIZATION.md"],
      updatedAt: "2026-05-02",
      summary: "Generated business repos are customer-owned and must not be overwritten by CTOX core upgrades."
    },
    {
      id: "know-operations-projects",
      title: "Operations starter workspace",
      moduleId: "operations",
      recordType: "module",
      linkedRecords: ["projects", "work-items", "knowledge", "meetings"],
      updatedAt: "2026-05-02",
      summary: "Projects, work items, boards, planning, knowledge, and meetings are linked through drawers and queue actions."
    },
    {
      id: "know-competitive-analysis",
      title: "Competitive analysis workflow",
      moduleId: "marketing",
      recordType: "workflow",
      linkedRecords: ["score-model", "own-product", "scheduled-scraper"],
      updatedAt: "2026-05-02",
      summary: "Competitor ranking, axis switching, criteria editing, own benchmark, and scheduled scrape handoff are connected."
    }
  ],
  bugs: [
    {
      id: "bug-scroll-ranking",
      title: "Ranking scroll regression",
      moduleId: "marketing",
      submoduleId: "competitive-analysis",
      status: "fixed",
      severity: "high",
      createdAt: "2026-05-02T13:12:00.000Z",
      summary: "Ranking list had internal overflow issues after drawer changes."
    },
    {
      id: "bug-axis-flash",
      title: "Axis switch reload flash",
      moduleId: "marketing",
      submoduleId: "competitive-analysis",
      status: "triaged",
      severity: "normal",
      createdAt: "2026-05-02T13:22:00.000Z",
      summary: "Axis changes should feel like local OS state instead of full-page reload."
    },
    {
      id: "bug-ctox-module-integration",
      title: "CTOX module integration parity",
      moduleId: "ctox",
      submoduleId: "runs",
      status: "open",
      severity: "normal",
      createdAt: "2026-05-02T15:05:00.000Z",
      summary: "CTOX module should keep the same drawer, queue, knowledge, and bug-reporting behavior as every business module."
    }
  ],
  sync: [
    {
      id: "sync-sales",
      moduleId: "sales",
      status: "healthy",
      lastEvent: "sales.opportunities.sync",
      lastSyncedAt: "2026-05-02T14:59:00.000Z",
      pending: 1
    },
    {
      id: "sync-marketing",
      moduleId: "marketing",
      status: "queued",
      lastEvent: "marketing.campaigns.sync",
      lastSyncedAt: "2026-05-02T14:58:00.000Z",
      pending: 3
    },
    {
      id: "sync-operations",
      moduleId: "operations",
      status: "healthy",
      lastEvent: "operations.work-items.sync",
      lastSyncedAt: "2026-05-02T14:57:00.000Z",
      pending: 2
    },
    {
      id: "sync-business",
      moduleId: "business",
      status: "warning",
      lastEvent: "business.invoices.export",
      lastSyncedAt: "2026-05-02T14:52:00.000Z",
      pending: 4
    }
  ]
};

export const businessOsBugReportTag = "Business OS Bug Report";

export async function getCtoxBundle() {
  const [persistedBugs, coreQueue] = await Promise.all([
    readPersistedBugReports(),
    readCtoxCoreQueueItems()
  ]);
  return {
    ...ctoxSeed,
    queue: mergeQueueItems(coreQueue, ctoxSeed.queue),
    bugs: mergeBugReports(persistedBugs, ctoxSeed.bugs)
  };
}

export async function getCtoxResource(resource: string) {
  const normalized = normalizeCtoxResource(resource);
  if (!normalized) return null;
  const bundle = await getCtoxBundle();
  return bundle[normalized];
}

export async function appendCtoxBugReport(report: CtoxBugRecord) {
  const reports = await readPersistedBugReports();
  const normalizedReport = normalizeBugReport(report);
  const nextReports = mergeBugReports([normalizedReport], reports);
  await writePersistedBugReports(nextReports);
  return normalizedReport;
}

export function normalizeCtoxResource(resource: string): keyof CtoxBundle | null {
  if (resource === "runs") return "runs";
  if (resource === "queue") return "queue";
  if (resource === "knowledge") return "knowledge";
  if (resource === "bugs") return "bugs";
  if (resource === "sync") return "sync";
  return null;
}

async function readPersistedBugReports(): Promise<CtoxBugRecord[]> {
  const databaseReports = await readPostgresBugReports();
  if (databaseReports) return databaseReports;

  try {
    const raw = await readFile(bugStorePath(), "utf-8");
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.map((item) => normalizeBugReport(item)).filter(Boolean) as CtoxBugRecord[];
  } catch {
    return [];
  }
}

async function writePersistedBugReports(reports: CtoxBugRecord[]) {
  const latestReport = reports[0];
  if (latestReport && await writePostgresBugReport(latestReport)) return;

  await mkdir(".ctox-business", { recursive: true });
  await writeFile(bugStorePath(), JSON.stringify(reports, null, 2), "utf-8");
}

function bugStorePath() {
  return ".ctox-business/bug-reports.json";
}

function normalizeBugReport(value: unknown): CtoxBugRecord {
  if (!isRecord(value)) {
    return {
      id: crypto.randomUUID(),
      type: "bug_report",
      title: "Bug report",
      moduleId: "ctox",
      submoduleId: "bugs",
      status: "open",
      severity: "normal",
      createdAt: new Date().toISOString(),
      summary: "Bug report",
      expected: "",
      pageUrl: "",
      viewport: {},
      annotation: null,
      userAgent: "",
      tags: [businessOsBugReportTag],
      source: "business-os-bug-report",
      coreTaskId: null
    };
  }

  const record = value as Partial<CtoxBugRecord> & { id?: string; summary?: string; title?: string };
  const summary = String(record.summary ?? record.title ?? "Bug report").trim();
  const title = String(record.title ?? summary).trim();
  const tags = Array.from(new Set([businessOsBugReportTag, ...(Array.isArray(record.tags) ? record.tags.map(String) : [])]));

  return {
    id: String(record.id ?? crypto.randomUUID()),
    type: "bug_report",
    title,
    moduleId: String(record.moduleId ?? "ctox"),
    submoduleId: String(record.submoduleId ?? "bugs"),
    status: record.status === "fixed" || record.status === "triaged" ? record.status : "open",
    severity: record.severity === "low" || record.severity === "high" ? record.severity : "normal",
    createdAt: String(record.createdAt ?? new Date().toISOString()),
    summary,
    expected: typeof record.expected === "string" ? record.expected : "",
    pageUrl: typeof record.pageUrl === "string" ? record.pageUrl : "",
    viewport: isRecord(record.viewport) ? record.viewport : {},
    annotation: isRecord(record.annotation) ? record.annotation : null,
    userAgent: typeof record.userAgent === "string" ? record.userAgent : "",
    tags,
    source: String(record.source ?? "business-os-bug-report"),
    coreTaskId: typeof record.coreTaskId === "string" ? record.coreTaskId : null
  };
}

function mergeBugReports(primary: CtoxBugRecord[], secondary: CtoxBugRecord[]) {
  const byId = new Map<string, CtoxBugRecord>();
  [...secondary, ...primary].forEach((bug) => byId.set(bug.id, bug));
  return Array.from(byId.values()).sort((left, right) => right.createdAt.localeCompare(left.createdAt));
}

function mergeQueueItems(primary: CtoxQueueItem[], secondary: CtoxQueueItem[]) {
  const byId = new Map<string, CtoxQueueItem>();
  [...secondary, ...primary].forEach((item) => byId.set(item.id, item));
  return Array.from(byId.values()).sort((left, right) => right.createdAt.localeCompare(left.createdAt));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

async function readCtoxCoreQueueItems(): Promise<CtoxQueueItem[]> {
  if (process.env.CTOX_BUSINESS_QUEUE_MODE === "planned") return [];

  const command = [
    process.env.CTOX_BIN ?? process.env.CTOX_WEB_BIN ?? "ctox",
    "queue",
    "list",
    "--status",
    "pending",
    "--status",
    "leased",
    "--status",
    "blocked",
    "--status",
    "failed",
    "--limit",
    "2000",
    "--json"
  ];
  const [binary, ...args] = command;
  try {
    const { stdout } = await execFileAsync(binary, args, {
      cwd: process.env.CTOX_ROOT,
      maxBuffer: 32 * 1024 * 1024
    });
    const payload = JSON.parse(stdout) as { tasks?: unknown[] };
    if (!Array.isArray(payload.tasks)) return [];
    return payload.tasks.map(normalizeCoreQueueItem).filter(Boolean) as CtoxQueueItem[];
  } catch {
    return [];
  }
}

function normalizeCoreQueueItem(value: unknown): CtoxQueueItem | null {
  if (!isRecord(value)) return null;
  const messageKey = typeof value.message_key === "string" ? value.message_key : "";
  if (!messageKey) return null;
  const title = typeof value.title === "string" ? value.title : messageKey;
  const threadKey = typeof value.thread_key === "string" ? value.thread_key : "";
  const routeStatus = typeof value.route_status === "string" ? value.route_status : "";
  const source = threadKey.endsWith("/bugs") || title.startsWith("Bug")
    ? "business-bug-report"
    : "ctox-core";

  return {
    id: messageKey,
    title,
    source,
    priority: normalizePriority(value.priority),
    status: normalizeQueueStatus(routeStatus),
    target: threadKey || String(value.suggested_skill ?? "CTOX core"),
    createdAt: typeof value.created_at === "string" ? value.created_at : new Date().toISOString()
  };
}

function normalizePriority(value: unknown): CtoxQueueItem["priority"] {
  if (value === "urgent" || value === "high" || value === "normal" || value === "low") return value;
  return "normal";
}

function normalizeQueueStatus(value: string): CtoxQueueItem["status"] {
  if (value === "leased") return "running";
  if (value === "blocked" || value === "failed") return "blocked";
  if (value === "handled") return "done";
  return "queued";
}

async function readPostgresBugReports() {
  if (!shouldUsePostgres()) return null;

  try {
    const db = await import("@ctox-business/db/modules");
    const rows = await db.listCtoxBugReports();
    return rows.map((row) => normalizeBugReport(parseJson(row.payloadJson))).filter(Boolean);
  } catch (error) {
    console.warn("Falling back to local CTOX bug report store.", error);
    return null;
  }
}

async function writePostgresBugReport(report: CtoxBugRecord) {
  if (!shouldUsePostgres()) return false;

  try {
    const db = await import("@ctox-business/db/modules");
    await db.upsertCtoxBugReport(report);
    return true;
  } catch (error) {
    console.warn("Falling back to local CTOX bug report store.", error);
    return false;
  }
}

function parseJson(value: string) {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

function shouldUsePostgres() {
  const value = process.env.DATABASE_URL;
  return Boolean(value && !value.includes("user:password@localhost"));
}
