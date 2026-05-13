import { emitCtoxCoreEvent, upsertCtoxCoreTask } from "@/lib/ctox-core-bridge";
import { appendCtoxBugReport, businessOsBugReportTag, getCtoxResource, type CtoxBugRecord } from "@/lib/ctox-seed";
import { NextResponse } from "next/server";

type BugReportRequest = {
  summary?: string;
  expected?: string;
  pageUrl?: string;
  moduleId?: string;
  submoduleId?: string;
  viewport?: Record<string, unknown>;
  annotation?: Record<string, unknown>;
  userAgent?: string;
};

export async function GET() {
  return NextResponse.json({ ok: true, data: await getCtoxResource("bugs") });
}

export async function POST(request: Request) {
  const body = await request.json() as BugReportRequest;
  const summary = body.summary?.trim();

  if (!summary) {
    return NextResponse.json(
      { ok: false, error: "summary_required" },
      { status: 400 }
    );
  }

  const report: CtoxBugRecord = {
    id: crypto.randomUUID(),
    type: "bug_report",
    title: summary.slice(0, 120),
    status: "open",
    severity: "high",
    summary,
    expected: body.expected?.trim() ?? "",
    pageUrl: body.pageUrl ?? "",
    moduleId: body.moduleId ?? "ctox",
    submoduleId: body.submoduleId ?? "bugs",
    viewport: body.viewport ?? {},
    annotation: body.annotation ?? null,
    userAgent: body.userAgent ?? "",
    tags: [businessOsBugReportTag],
    source: "business-os-bug-report",
    createdAt: new Date().toISOString()
  };

  const queuedReport = await appendCtoxBugReport(report);
  const threadKey = ["business", queuedReport.moduleId, queuedReport.submoduleId, "bugs"].filter(Boolean).join("/");
  const prompt = buildBugQueuePrompt(queuedReport);
  const core = await upsertCtoxCoreTask({
    title: `Bug inbox: ${[queuedReport.moduleId, queuedReport.submoduleId].filter(Boolean).join(" / ")}`,
    prompt,
    source: "business-bug-report",
    context: {
      latestReport: summarizeBugReportForQueue(queuedReport),
      bugReportsEndpoint: "/business-os/api/ctox/bug-reports",
      businessOsCodeSync: businessOsCodeSyncContext()
    },
    priority: "high",
    skill: "product_engineering/business-stack",
    threadKey
  }, {
    threadKey,
    updateTitle: `Bug inbox: ${[queuedReport.moduleId, queuedReport.submoduleId].filter(Boolean).join(" / ")}`,
    updatePrompt: prompt
  });

  const persistedReport = await appendCtoxBugReport({
    ...queuedReport,
    coreTaskId: core.taskId
  });

  await emitCtoxCoreEvent({
    type: "business.bug_reported",
    module: report.moduleId ?? "ctox",
    recordType: "bug_report",
    recordId: report.id,
    payload: { report: persistedReport, core }
  });

  return NextResponse.json({ ok: true, accepted: true, report: persistedReport, core });
}

function buildBugQueuePrompt(report: CtoxBugRecord) {
  return [
    "A user reported one or more bugs from the CTOX Business OS UI.",
    "",
    "Do not spawn one CTOX queue task per bug report. This task is the shared inbox for the current module/submodule; read the bug report store/API and process all open reports in this scope.",
    "",
    `Latest report: ${report.id}`,
    `Summary: ${report.summary}`,
    `Expected: ${report.expected || "Not specified"}`,
    `Page: ${report.pageUrl || "Not specified"}`,
    `Context: ${[report.moduleId, report.submoduleId].filter(Boolean).join(" / ") || "workspace"}`,
    "",
    "Use the structured bug report payload, including annotation metadata and screenshot/markup data when present, to reproduce, triage, and create the necessary implementation task."
  ].join("\n");
}

function businessOsCodeSyncContext() {
  return {
    app: "Kunstmen Business OS",
    mountedPath: "/business-os",
    canonicalTemplate: "templates/business-basic",
    codeSyncPolicy: [
      "If the fix changes reusable Business OS code, apply it to the running Kunstmen Business OS and backport the generic code change to the CTOX Business OS template.",
      "Do not copy tenant data, customer records, screenshots, credentials, database rows, or .ctox-business runtime JSON into the template.",
      "Persistent Business OS data belongs in Postgres. A durable file-backed store is a defect.",
      "If immediate backport is unsafe, create a tracked follow-up with changed file paths and migration notes."
    ]
  };
}

function summarizeBugReportForQueue(report: CtoxBugRecord) {
  const annotation = isRecord(report.annotation) ? report.annotation : {};
  const strokes = Array.isArray(annotation.strokes) ? annotation.strokes : [];
  return {
    id: report.id,
    title: report.title,
    moduleId: report.moduleId,
    submoduleId: report.submoduleId,
    summary: report.summary,
    expected: report.expected,
    pageUrl: report.pageUrl,
    createdAt: report.createdAt,
    annotation: {
      captureMode: annotation.captureMode,
      capturedAt: annotation.capturedAt,
      rect: annotation.rect,
      strokeCount: strokes.length,
      hasScreenshotDataUrl: typeof annotation.screenshotDataUrl === "string",
      hasCompositeDataUrl: typeof annotation.compositeDataUrl === "string"
    }
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}
