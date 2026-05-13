import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { loadSalesAutomationStoreLite } from "@/lib/sales-automation-store-lite";
import { NextResponse } from "next/server";

export async function GET() {
  const store = await loadSalesAutomationStoreLite();
  return NextResponse.json(compactSalesAutomationStore(store));
}

export async function POST(request: Request) {
  const form = await request.formData();
  const file = form.get("sourceFile");
  const sourceType = parseSourceType(form.get("sourceType"));
  const campaignName = stringValue(form.get("campaignName")) || "AI Vermittlung fuer Personalvermittler";
  const description = stringValue(form.get("description"))
    || stringValue(form.get("assignmentPrompt"))
    || stringValue(form.get("sourceHint"))
    || "Consulting campaign for staffing agencies and recruiters to build AI employee placement as a new business line.";
  const sourceText = stringValue(form.get("sourceText"));
  const { importSalesCampaignSource } = await loadSalesAutomationRuntime();
  const result = await importSalesCampaignSource({
    campaignId: stringValue(form.get("campaignId")),
    campaignName,
    description,
    sourceType,
    sourceName: file instanceof File && file.name ? file.name : stringValue(form.get("sourceUrl")) || "manual-source",
    file: file instanceof File ? file : undefined,
    sourceText
  });

  return NextResponse.json(result);
}

function parseSourceType(value: FormDataEntryValue | null) {
  return value === "URL" || value === "PDF" || value === "Text" || value === "Excel" ? value : "Excel";
}

function stringValue(value: FormDataEntryValue | null) {
  return typeof value === "string" ? value.trim() : "";
}

function compactSalesAutomationStore(store: any) {
  return {
    campaigns: Array.isArray(store?.campaigns) ? store.campaigns : [],
    rows: Array.isArray(store?.rows) ? store.rows.map(compactCampaignRow) : [],
    pipelineRuns: Array.isArray(store?.pipelineRuns) ? store.pipelineRuns.map(compactPipelineRun) : []
  };
}

function compactCampaignRow(row: any) {
  return {
    id: row.id,
    campaignId: row.campaignId,
    rowIndex: row.rowIndex,
    companyName: row.companyName,
    imported: compactRecord(row.imported),
    researchStatus: row.researchStatus,
    webEvidence: compactWebEvidence(row.webEvidence),
    research: compactResearch(row.research),
    pipeline: row.pipeline,
    error: truncateString(row.error, 600),
    updatedAt: row.updatedAt
  };
}

function compactWebEvidence(evidence: any) {
  if (!evidence) return undefined;
  return {
    query: evidence.query,
    ok: evidence.ok,
    provider: evidence.provider,
    toolCalls: Array.isArray(evidence.toolCalls)
      ? evidence.toolCalls.slice(0, 12).map((call: any) => ({
          tool: call.tool,
          query: truncateString(call.query, 240),
          url: truncateString(call.url, 500),
          ok: call.ok,
          note: truncateString(call.note, 240)
        }))
      : [],
    citations: Array.isArray(evidence.citations)
      ? evidence.citations.slice(0, 12).map((citation: any) => ({
          title: truncateString(citation.title, 240),
          url: truncateString(citation.url, 500)
        }))
      : [],
    results: Array.isArray(evidence.results)
      ? evidence.results.slice(0, 5).map((result: any) => ({
          title: truncateString(result.title, 240),
          url: truncateString(result.url, 500),
          snippet: truncateString(result.snippet ?? result.summary, 500)
        }))
      : [],
    error: truncateString(evidence.error, 600)
  };
}

function compactResearch(research: any) {
  if (!research) return undefined;
  return {
    companyName: research.companyName,
    likelyWebsite: research.likelyWebsite,
    phone: research.phone,
    email: research.email,
    address: research.address,
    contactCandidates: Array.isArray(research.contactCandidates)
      ? research.contactCandidates.slice(0, 5).map((candidate: any) => ({
          name: candidate.name,
          role: candidate.role,
          email: candidate.email,
          phone: candidate.phone,
          confidence: candidate.confidence,
          evidence: truncateString(candidate.evidence, 500)
        }))
      : [],
    qualification: research.qualification
      ? {
          fit: research.qualification.fit,
          reason: truncateString(research.qualification.reason, 900),
          consultingAngle: truncateString(research.qualification.consultingAngle, 900)
        }
      : undefined,
    missingFields: Array.isArray(research.missingFields) ? research.missingFields.slice(0, 20) : [],
    recommendedNextAction: truncateString(research.recommendedNextAction, 900),
    sourceNote: truncateString(research.sourceNote, 900)
  };
}

function compactPipelineRun(run: any) {
  return {
    id: run.id,
    candidateId: run.candidateId,
    campaignId: run.campaignId,
    mode: run.mode,
    status: run.status,
    currentGate: run.currentGate,
    gates: run.gates,
    questions: Array.isArray(run.questions) ? run.questions.slice(-10) : [],
    approvals: Array.isArray(run.approvals) ? run.approvals.slice(-10) : [],
    messages: Array.isArray(run.messages) ? run.messages.slice(-20).map((message: any) => ({
      ...message,
      body: truncateString(message.body, 900)
    })) : [],
    auditLog: Array.isArray(run.auditLog) ? run.auditLog.slice(-30) : [],
    createdAt: run.createdAt,
    updatedAt: run.updatedAt
  };
}

function compactRecord(value: any) {
  if (!value || typeof value !== "object") return {};
  return Object.fromEntries(Object.entries(value).map(([key, raw]) => [key, truncateString(String(raw ?? ""), 500)]));
}

function truncateString(value: unknown, limit: number) {
  if (typeof value !== "string") return value;
  return value.length > limit ? `${value.slice(0, limit - 1)}…` : value;
}
