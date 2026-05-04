import { createCtoxCoreTask, emitCtoxCoreEvent } from "@/lib/ctox-core-bridge";
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

  const core = await createCtoxCoreTask({
    title: `Bug: ${summary.slice(0, 80)}`,
    prompt: [
      "A user reported a bug from the CTOX Business OS UI.",
      "",
      `Summary: ${summary}`,
      `Expected: ${report.expected || "Not specified"}`,
      `Page: ${report.pageUrl}`,
      `Context: ${[report.moduleId, report.submoduleId].filter(Boolean).join(" / ") || "workspace"}`,
      "",
      "Use the attached structured payload to reproduce, triage, and create the necessary implementation task."
    ].join("\n"),
    source: "business-bug-report",
    context: { report },
    priority: "high",
    skill: "product_engineering/business-stack",
    threadKey: ["business", report.moduleId, report.submoduleId, "bugs"].filter(Boolean).join("/")
  });

  const persistedReport = await appendCtoxBugReport({
    ...report,
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
