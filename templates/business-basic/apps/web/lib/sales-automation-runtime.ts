export type SalesAutomationJobStatus = "pending" | "running" | "complete" | "failed";
export type SalesPipelineStageId = "company" | "contact" | "decision" | "conversation" | "lead-ready";
export type SalesPipelineRunGateId = "company_identity" | "contact_discovery" | "decision_contact" | "outreach_draft" | "delivery" | "lead_handoff";
export type SalesPipelineRunMode = "dry_run" | "live";
export type SalesPipelineRunStatus = "idle" | "running" | "waiting_for_user" | "completed" | "failed";
export type SalesPipelineGateStatus = "not_started" | "running" | "waiting_for_user" | "passed" | "blocked" | "failed";

export type SalesPipelineRunQuestion = {
  id: string;
  gate: SalesPipelineRunGateId;
  severity: "blocking" | "clarifying" | "approval";
  question: string;
  options: Array<{ id: string; label: string; effect: "continue" | "search_more" | "approve" | "reject" | "park" }>;
  freeTextAllowed: boolean;
  contextRefs: string[];
  answeredAt?: string;
  answer?: { choiceId?: string; text?: string };
};

export type SalesPipelineGateState = {
  status: SalesPipelineGateStatus;
  summary: string;
  score: number;
  sources: Array<{ title?: string; url?: string }>;
  risks: string[];
  output?: Record<string, unknown>;
  updatedAt: string;
};

export type SalesPipelineRun = {
  id: string;
  candidateId: string;
  campaignId: string;
  mode: SalesPipelineRunMode;
  status: SalesPipelineRunStatus;
  currentGate: SalesPipelineRunGateId;
  gates: Partial<Record<SalesPipelineRunGateId, SalesPipelineGateState>>;
  messages: Array<{ id: string; at: string; role: "system" | "agent" | "user"; gate: SalesPipelineRunGateId; body: string }>;
  questions: SalesPipelineRunQuestion[];
  approvals: Array<{ id: string; gate: SalesPipelineRunGateId; approvedAt: string; note?: string }>;
  auditLog: Array<{ at: string; event: string; detail: string }>;
  createdAt: string;
  updatedAt: string;
};

export type SalesCampaignImportRow = {
  id: string;
  campaignId: string;
  rowIndex: number;
  companyName: string;
  imported: Record<string, string>;
  researchStatus: SalesAutomationJobStatus;
  webEvidence?: {
    ok?: boolean;
    provider?: string;
    query?: string;
    toolCalls?: Array<{
      note?: string;
      ok: boolean;
      query?: string;
      tool: "search" | "read" | "probe";
      url?: string;
    }>;
    citations: Array<{ title?: string; url?: string }>;
    results: Array<{ title?: string; url?: string; snippet?: string; summary?: string; excerpts: string[] }>;
  };
  research?: {
    companyName?: string;
    likelyWebsite?: string;
    phone?: string;
    email?: string;
    contactCandidates: Array<{
      name?: string;
      role?: string;
      email?: string;
      phone?: string;
      confidence: "low" | "medium" | "high";
      evidence?: string;
    }>;
    qualification: {
      fit: "low" | "medium" | "high";
      reason: string;
      consultingAngle: string;
    };
    missingFields: string[];
    recommendedNextAction: string;
    sourceNote: string;
  };
  pipeline?: {
    status: "active" | "lead-ready" | "transferred-to-leads";
    stageId: SalesPipelineStageId;
    transferredAt: string;
    transferredBy: "campaign-gate" | "manual" | "ctox";
    gateReasons: string[];
    score: number;
  };
  error?: string;
  updatedAt: string;
};

export type SalesAutomationStore = {
  campaigns: Array<{
    completedRows: number;
    description: string;
    id: string;
    name: string;
    rowCount: number;
    sourceName?: string;
    sourceType: "Excel" | "URL" | "PDF" | "Text";
    status: "imported" | "researching" | "ready" | "failed";
  }>;
  rows: SalesCampaignImportRow[];
  pipelineRuns?: SalesPipelineRun[];
};

export type SalesAutomationRuntime = {
  answerSalesPipelineRunQuestion: (options: { choiceId?: string; questionId: string; runId: string; text?: string }) => Promise<unknown>;
  importSalesCampaignSource: (input: {
    campaignId?: string;
    campaignName: string;
    description: string;
    file?: File;
    sourceName: string;
    sourceText?: string;
    sourceType: "Excel" | "URL" | "PDF" | "Text";
  }) => Promise<unknown>;
  loadSalesAutomationStore: () => Promise<SalesAutomationStore>;
  runSalesResearchJobs: (options?: { campaignId?: string; limit?: number; retryFailed?: boolean; rerunComplete?: boolean; rowId?: string; useWebSearch?: boolean }) => Promise<unknown>;
  startSalesPipelineRuns: (options: { candidateIds: string[]; gate?: SalesPipelineRunGateId | "next"; mode?: SalesPipelineRunMode }) => Promise<unknown>;
  transferReadySalesCampaignRowsToPipeline: (options?: { campaignId?: string; force?: boolean; rowId?: string }) => Promise<unknown>;
};

export function inferSalesPipelineStage(row: SalesCampaignImportRow): SalesPipelineStageId {
  const research = row.research;
  const candidate = research?.contactCandidates[0];
  const hasContact = Boolean(candidate?.name || candidate?.role || candidate?.email || candidate?.phone);
  const isDecisionContact = Boolean(candidate && (candidate.confidence === "high" || isDecisionRole(candidate.role)));
  const hasContactRoute = Boolean(candidate?.email || candidate?.phone || research?.phone || research?.email);
  if (!hasContact) return "company";
  if (!isDecisionContact) return "contact";
  if (!hasContactRoute) return "decision";
  return "conversation";
}

export function scoreSalesPipelineRow(row: SalesCampaignImportRow) {
  const research = row.research;
  const candidate = research?.contactCandidates[0];
  const fit = research?.qualification.fit ?? "low";
  const hasWebsite = Boolean(research?.likelyWebsite);
  const hasContact = Boolean(candidate?.name || candidate?.role || candidate?.email || candidate?.phone);
  const isDecisionContact = Boolean(candidate && (candidate.confidence === "high" || isDecisionRole(candidate.role)));
  const hasContactRoute = Boolean(candidate?.email || candidate?.phone || research?.phone || research?.email);
  return [
    hasWebsite ? 25 : 0,
    fit === "high" ? 25 : fit === "medium" ? 12 : 0,
    hasContact ? 20 : 0,
    isDecisionContact ? 20 : 0,
    hasContactRoute ? 10 : 0
  ].reduce((sum, value) => sum + value, 0);
}

function isDecisionRole(value?: string) {
  const role = value?.toLowerCase() ?? "";
  return ["ceo", "cfo", "coo", "founder", "owner", "geschäftsführer", "geschaeftsfuehrer", "inhaber", "leitung"].some((token) => role.includes(token));
}
