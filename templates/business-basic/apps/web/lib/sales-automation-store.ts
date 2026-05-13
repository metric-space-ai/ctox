import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { sql } from "drizzle-orm";

const execFileAsync = promisify(execFile);

export type SalesAutomationJobStatus = "pending" | "running" | "complete" | "failed";
export type SalesPipelineStageId = "company" | "contact" | "decision" | "conversation" | "lead-ready";
export type SalesPipelineRunGateId = "company_identity" | "contact_discovery" | "decision_contact" | "outreach_draft" | "delivery" | "lead_handoff";
export type SalesPipelineRunMode = "dry_run" | "live";
export type SalesPipelineRunStatus = "idle" | "running" | "waiting_for_user" | "completed" | "failed";
export type SalesPipelineGateStatus = "not_started" | "running" | "waiting_for_user" | "passed" | "blocked" | "failed";

export type SalesPipelineHandoff = {
  status: "active" | "lead-ready" | "transferred-to-leads";
  stageId: SalesPipelineStageId;
  transferredAt: string;
  transferredBy: "campaign-gate" | "manual" | "ctox";
  gateReasons: string[];
  score: number;
};

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

export type SalesPipelineRunMessage = {
  id: string;
  at: string;
  role: "system" | "agent" | "user";
  gate: SalesPipelineRunGateId;
  body: string;
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

type SalesPipelineAgentOutput = {
  gateStatus: "passed" | "waiting_for_user" | "blocked" | "failed";
  summary: string;
  score: number;
  sources: Array<{ title?: string; url?: string }>;
  risks: string[];
  output: Record<string, unknown>;
  question?: {
    severity: "blocking" | "clarifying" | "approval";
    question: string;
    options: Array<{ id: string; label: string; effect: "continue" | "search_more" | "approve" | "reject" | "park" }>;
    freeTextAllowed: boolean;
    contextRefs: string[];
  };
  researchPatch?: Partial<SalesCompanyResearch>;
};

export type SalesPipelineRun = {
  id: string;
  candidateId: string;
  campaignId: string;
  mode: SalesPipelineRunMode;
  status: SalesPipelineRunStatus;
  currentGate: SalesPipelineRunGateId;
  gates: Partial<Record<SalesPipelineRunGateId, SalesPipelineGateState>>;
  messages: SalesPipelineRunMessage[];
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
  webEvidence?: SalesWebEvidence;
  research?: SalesCompanyResearch;
  pipeline?: SalesPipelineHandoff;
  error?: string;
  updatedAt: string;
};

export type SalesWebEvidence = {
  query: string;
  ok: boolean;
  provider?: string;
  toolCalls?: Array<{
    tool: "search" | "read" | "probe";
    query?: string;
    url?: string;
    ok: boolean;
    note?: string;
  }>;
  citations: Array<{ title?: string; url?: string }>;
  results: Array<{
    title?: string;
    url?: string;
    snippet?: string;
    summary?: string;
    excerpts: string[];
  }>;
  error?: string;
};

export type SalesCompanyResearch = {
  companyName: string;
  likelyWebsite?: string;
  phone?: string;
  email?: string;
  address?: string;
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

export type SalesAutomationCampaign = {
  id: string;
  name: string;
  description: string;
  sourceType: "Excel" | "URL" | "PDF" | "Text";
  sourceName: string;
  model: "MiniMax-M2.7";
  status: "imported" | "researching" | "ready" | "failed";
  rowCount: number;
  completedRows: number;
  createdAt: string;
  updatedAt: string;
};

export type SalesAutomationStore = {
  campaigns: SalesAutomationCampaign[];
  rows: SalesCampaignImportRow[];
  pipelineRuns?: SalesPipelineRun[];
};

export type CampaignImportInput = {
  campaignId?: string;
  campaignName: string;
  description: string;
  sourceType: SalesAutomationCampaign["sourceType"];
  sourceName: string;
  file?: File;
  sourceText?: string;
};

export type RunResearchOptions = {
  campaignId?: string;
  rowId?: string;
  retryFailed?: boolean;
  rerunComplete?: boolean;
  useWebSearch?: boolean;
  limit?: number;
};

export type TransferReadyRowsOptions = {
  campaignId?: string;
  rowId?: string;
  force?: boolean;
};

export type StartPipelineRunOptions = {
  candidateIds: string[];
  mode?: SalesPipelineRunMode;
  gate?: SalesPipelineRunGateId | "next";
};

export type AnswerPipelineRunOptions = {
  runId: string;
  questionId: string;
  choiceId?: string;
  text?: string;
};

const SALES_AUTOMATION_STORE_KEY = "sales_automation";
const MINIMAX_MODEL = "MiniMax-M2.7";
const DEFAULT_RESEARCH_TIMEOUT_MS = 360_000;
const IRRELEVANT_SEARCH_HOSTS = [
  "microsoft.com",
  "office.com",
  "account.microsoft.com",
  "toppr.com",
  "brainly.",
  "zhihu.com",
  "baidu.",
  "zhidao.baidu.com"
];
const GENERIC_COMPANY_TOKENS = new Set([
  "expert",
  "experts",
  "engineering",
  "technical",
  "technik",
  "personal",
  "personaldienst",
  "personaldienstleistung",
  "personaldienstleistungen",
  "personalservice",
  "partner",
  "partners",
  "service",
  "services",
  "recruiting",
  "recruitment",
  "zeitarbeit",
  "arbeit",
  "work",
  "group",
  "gruppe",
  "gmbh",
  "deutschland",
  "germany"
]);

async function loadAgentRuntime() {
  const agents = await import("@openai/agents");
  const openai = await import("openai");
  const zod = await import("zod");
  return {
    Agent: agents.Agent,
    OpenAI: openai.default,
    OpenAIChatCompletionsModel: agents.OpenAIChatCompletionsModel,
    run: agents.run,
    setTracingDisabled: agents.setTracingDisabled,
    tool: agents.tool,
    z: zod.z
  };
}

async function loadXlsxRuntime() {
  return await import("xlsx");
}

export async function importSalesCampaignSource(input: CampaignImportInput) {
  const now = new Date().toISOString();
  const campaignId = input.campaignId || `campaign-${slug(input.campaignName)}-${crypto.randomUUID().slice(0, 8)}`;
  const rows = await parseImportRows(input);
  const store = await loadSalesAutomationStore();
  const importedRows = rows.map((row, index): SalesCampaignImportRow => ({
    id: `${campaignId}-row-${index + 1}`,
    campaignId,
    rowIndex: index + 1,
    companyName: inferCompanyName(row, index),
    imported: row,
    researchStatus: "pending",
    updatedAt: now
  })).filter((row) => row.companyName.trim().length > 0);

  const campaign: SalesAutomationCampaign = {
    id: campaignId,
    name: input.campaignName,
    description: input.description,
    sourceType: input.sourceType,
    sourceName: input.sourceName,
    model: MINIMAX_MODEL,
    status: importedRows.length > 0 ? "researching" : "failed",
    rowCount: importedRows.length,
    completedRows: 0,
    createdAt: now,
    updatedAt: now
  };

  const nextStore: SalesAutomationStore = {
    campaigns: [campaign, ...store.campaigns.filter((item) => item.id !== campaignId)],
    rows: [...importedRows, ...store.rows.filter((row) => row.campaignId !== campaignId)]
  };
  await saveSalesAutomationStore(nextStore);

  return {
    ok: true,
    campaign,
    importedRows: importedRows.length,
    firstRows: importedRows.slice(0, 8)
  };
}

export async function runSalesResearchJobs(options: RunResearchOptions = {}) {
  const limit = Math.max(1, Math.min(options.limit ?? 5, 25));
  const store = await loadSalesAutomationStore();
  const candidates = store.rows
    .filter((row) => (
      row.researchStatus === "pending"
      || (options.retryFailed && row.researchStatus === "failed")
      || (options.rerunComplete && row.researchStatus === "complete")
    ))
    .filter((row) => !options.campaignId || row.campaignId === options.campaignId)
    .filter((row) => !options.rowId || row.id === options.rowId)
    .slice(0, limit);

  if (candidates.length === 0) {
    return { ok: true, processed: 0, rows: [] };
  }

  const apiKey = await readMinimaxApiKey();
  const useWebSearch = options.useWebSearch ?? true;
  const startedRows = candidates.map((row) => updateRowInMemory(store, row.id, {
    researchStatus: "running",
    error: undefined,
    webEvidence: undefined,
    research: undefined
  }));
  await saveSalesAutomationStore(store);

  const rows: SalesCampaignImportRow[] = [];
  const concurrency = Math.max(1, Math.min(Number(process.env.SALES_RESEARCH_CONCURRENCY || 2), 5));
  for (let index = 0; index < startedRows.length; index += concurrency) {
    const batch = startedRows.slice(index, index + concurrency);
    rows.push(...await Promise.all(batch.map(async (started) => {
      try {
        const campaign = store.campaigns.find((item) => item.id === started.campaignId);
        const { research, webEvidence } = await researchCompanyWithSalesAgent(apiKey, started, campaign, { useWebSearch });
        return updateRowInMemory(store, started.id, { researchStatus: "complete", webEvidence, research });
      } catch (error) {
        return updateRowInMemory(store, started.id, {
          researchStatus: "failed",
          error: String(error instanceof Error ? error.message : error).replace(/sk-api-[A-Za-z0-9_-]+/g, "[redacted-secret]")
        });
      }
    })));
    await saveSalesAutomationStore(store);
  }

  await refreshCampaignProgress(store);
  await saveSalesAutomationStore(store);
  return { ok: true, processed: rows.length, rows };
}

export async function transferReadySalesCampaignRowsToPipeline(options: TransferReadyRowsOptions = {}) {
  const store = await loadSalesAutomationStore();
  const now = new Date().toISOString();
  const rows = store.rows
    .filter((row) => !options.campaignId || row.campaignId === options.campaignId)
    .filter((row) => !options.rowId || row.id === options.rowId)
    .filter((row) => options.force || !row.pipeline || row.pipeline.status !== "active")
    .map((row) => ({ row, gate: salesCampaignPipelineGate(row) }))
    .filter(({ gate }) => gate.status === "ready");

  for (const { row, gate } of rows) {
    const stageId = inferSalesPipelineStage(row);
    updateRowInMemory(store, row.id, {
      pipeline: {
        status: stageId === "lead-ready" ? "lead-ready" : "active",
        stageId,
        transferredAt: now,
        transferredBy: "campaign-gate",
        gateReasons: gate.reasons,
        score: scoreSalesPipelineRow(row)
      }
    });
  }

  await saveSalesAutomationStore(store);
  return {
    ok: true,
    transferred: rows.length,
    rows: rows.map(({ row }) => store.rows.find((item) => item.id === row.id)).filter(Boolean)
  };
}

export async function startSalesPipelineRuns(options: StartPipelineRunOptions) {
  const store = await loadSalesAutomationStore();
  const candidateIds = uniqueStrings(options.candidateIds).filter(Boolean);
  const mode = options.mode ?? "dry_run";
  const runs: SalesPipelineRun[] = [];
  for (const candidateId of candidateIds) {
    const row = store.rows.find((item) => item.id === candidateId);
    if (!row) throw new Error(`candidate_not_found:${candidateId}`);
    const existing = store.pipelineRuns?.find((runItem) => runItem.candidateId === candidateId && runItem.status !== "completed");
    const runItem = existing ?? createPipelineRun(row, mode);
    const gate = options.gate && options.gate !== "next" ? options.gate : nextGateForCandidate(row, runItem);
    runs.push(await executePipelineRunStep(store, runItem, row, gate, mode));
    await saveSalesAutomationStore(store);
  }
  await saveSalesAutomationStore(store);
  return { ok: true, runs };
}

export async function answerSalesPipelineRunQuestion(options: AnswerPipelineRunOptions) {
  const store = await loadSalesAutomationStore();
  const runItem = store.pipelineRuns?.find((item) => item.id === options.runId);
  if (!runItem) throw new Error(`run_not_found:${options.runId}`);
  const question = runItem.questions.find((item) => item.id === options.questionId);
  if (!question) throw new Error(`question_not_found:${options.questionId}`);
  const now = new Date().toISOString();
  question.answeredAt = now;
  question.answer = { choiceId: options.choiceId, text: options.text };
  runItem.messages.push({
    id: `msg-${crypto.randomUUID()}`,
    at: now,
    role: "user",
    gate: question.gate,
    body: [options.choiceId, options.text].filter(Boolean).join(" · ") || "Answered pipeline question."
  });
  runItem.auditLog.push({ at: now, event: "question_answered", detail: `${question.gate}:${options.choiceId ?? "free_text"}` });

  const selected = question.options.find((option) => option.id === options.choiceId);
  if (selected?.effect === "reject" || selected?.effect === "park") {
    runItem.status = "completed";
    runItem.gates[question.gate] = {
      ...(runItem.gates[question.gate] ?? emptyGateState(question.gate)),
      status: "blocked",
      summary: selected.effect === "reject" ? "Candidate rejected by user answer." : "Candidate parked for later review.",
      updatedAt: now
    };
  } else if (selected?.effect === "approve" || selected?.effect === "continue") {
    runItem.status = "completed";
    runItem.gates[question.gate] = {
      ...(runItem.gates[question.gate] ?? emptyGateState(question.gate)),
      status: "passed",
      summary: `${runItem.gates[question.gate]?.summary ?? "Gate reviewed"} User approved continuation.`,
      updatedAt: now
    };
    const row = store.rows.find((item) => item.id === runItem.candidateId);
    if (row) {
      const nextStage = nextStageAfterGate(question.gate, row.pipeline?.stageId);
      if (nextStage && row.pipeline) {
        updateRowInMemory(store, row.id, {
          pipeline: {
            ...row.pipeline,
            stageId: nextStage,
            status: nextStage === "lead-ready" ? "lead-ready" : "active",
            score: scoreSalesPipelineRow(row)
          }
        });
      }
    }
  } else {
    runItem.status = "idle";
  }
  runItem.updatedAt = now;
  await saveSalesAutomationStore(store);
  return { ok: true, run: runItem };
}

export function salesCampaignPipelineGate(row: SalesCampaignImportRow): {
  status: "ready" | "pending" | "stale" | "needs_evidence" | "reject" | "failed";
  label: string;
  reasons: string[];
} {
  const toolSteps = row.webEvidence?.toolCalls?.length ?? 0;
  const research = row.research;
  const sourceNote = research?.sourceNote ?? "";
  const nextAction = research?.recommendedNextAction ?? "";
  const reason = research?.qualification?.reason ?? "";
  const fit = research?.qualification?.fit ?? "medium";
  const hasVerifiedEvidence = toolSteps > 0 && !/No verified CTOX web evidence/i.test(sourceNote);
  const hasIdentity = Boolean(research?.likelyWebsite || hasVerifiedEvidence);
  const rejected = /REJECT ROW|not a valid prospect|does not correspond|not correspond|not a staffing|not a personnel|not a personal|not relevant|kein.*personaldienst|gar nicht/i.test(`${nextAction} ${reason} ${research?.missingFields?.join(" ") ?? ""}`);

  if (row.researchStatus === "failed") return { status: "failed", label: "Research failed", reasons: [row.error ?? "Research failed"] };
  if (row.researchStatus !== "complete") return { status: "pending", label: "Research offen", reasons: ["Research ist noch nicht abgeschlossen"] };
  if (toolSteps === 0) return { status: "stale", label: "Altbestand", reasons: ["Keine CTOX-Webstack-Toolschritte vorhanden"] };
  if (rejected || fit === "low") return { status: "reject", label: "Nicht passend", reasons: ["Unternehmen passt nicht zur Kampagnenidee"] };
  if (!hasIdentity) return { status: "needs_evidence", label: "Nicht identifiziert", reasons: ["Unternehmen konnte nicht belastbar identifiziert werden"] };

  const reasons = ["Unternehmen identifiziert", `Fit: ${fit}`];
  if (!research?.contactCandidates?.length) reasons.push("Ansprechpartner fehlt; als Pipeline-Aufgabe uebergeben");
  if (!research?.phone && !research?.email) reasons.push("Kontaktkanal fehlt; als Pipeline-Aufgabe uebergeben");
  return { status: "ready", label: "Pipeline-ready", reasons };
}

export async function loadSalesAutomationStore(): Promise<SalesAutomationStore> {
  const databaseStore = await loadSalesAutomationStoreFromDatabase();
  if (databaseStore) return databaseStore;
  return { campaigns: [], rows: [], pipelineRuns: [] };
}

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

function createPipelineRun(row: SalesCampaignImportRow, mode: SalesPipelineRunMode): SalesPipelineRun {
  const now = new Date().toISOString();
  const runItem: SalesPipelineRun = {
    id: `pipeline-run-${crypto.randomUUID()}`,
    candidateId: row.id,
    campaignId: row.campaignId,
    mode,
    status: "idle",
    currentGate: nextGateForCandidate(row),
    gates: {},
    messages: [{
      id: `msg-${crypto.randomUUID()}`,
      at: now,
      role: "system",
      gate: nextGateForCandidate(row),
      body: `Created ${mode} candidate automation run for ${row.companyName}.`
    }],
    questions: [],
    approvals: [],
    auditLog: [{ at: now, event: "run_created", detail: `${mode}:${row.id}` }],
    createdAt: now,
    updatedAt: now
  };
  return runItem;
}

async function executePipelineRunStep(
  store: SalesAutomationStore,
  runItem: SalesPipelineRun,
  row: SalesCampaignImportRow,
  gate: SalesPipelineRunGateId,
  mode: SalesPipelineRunMode
) {
  const now = new Date().toISOString();
  const runs = store.pipelineRuns ?? [];
  if (!runs.some((item) => item.id === runItem.id)) runs.push(runItem);
  store.pipelineRuns = runs;

  runItem.mode = mode;
  runItem.status = "running";
  runItem.currentGate = gate;
  runItem.updatedAt = now;
  runItem.auditLog.push({ at: now, event: "gate_started", detail: `${gate}:${row.id}:${mode}` });
  const agentResult = await runPipelineGateAgent(store, row, gate, mode, runItem);
  const gateStatus: SalesPipelineGateStatus = agentResult.gateStatus === "passed"
    ? "passed"
    : agentResult.gateStatus === "blocked"
      ? "blocked"
      : agentResult.gateStatus === "failed"
        ? "failed"
        : "waiting_for_user";
  runItem.gates[gate] = {
    status: gateStatus,
    summary: agentResult.summary,
    score: agentResult.score,
    sources: agentResult.sources,
    risks: agentResult.risks,
    output: agentResult.output,
    updatedAt: new Date().toISOString()
  };
  runItem.messages.push({
    id: `msg-${crypto.randomUUID()}`,
    at: new Date().toISOString(),
    role: "agent",
    gate,
    body: agentResult.summary
  });
  if (agentResult.researchPatch) applyPipelineResearchPatch(store, row, agentResult.researchPatch);
  if (gateStatus === "waiting_for_user" && agentResult.question) {
    runItem.status = "waiting_for_user";
    runItem.questions = [
      ...runItem.questions.filter((question) => question.answeredAt || question.gate !== gate),
      {
        id: `question-${crypto.randomUUID()}`,
        gate,
        severity: agentResult.question.severity,
        question: agentResult.question.question,
        options: agentResult.question.options,
        freeTextAllowed: agentResult.question.freeTextAllowed,
        contextRefs: agentResult.question.contextRefs
      }
    ];
  } else {
    runItem.status = gateStatus === "failed" ? "failed" : "completed";
    runItem.questions = runItem.questions.filter((question) => question.answeredAt || question.gate !== gate);
    if (gateStatus === "passed" && row.pipeline) {
      const nextStage = nextStageAfterGate(gate, row.pipeline.stageId);
      if (nextStage) {
        updateRowInMemory(store, row.id, {
          pipeline: {
            ...row.pipeline,
            stageId: nextStage,
            status: nextStage === "lead-ready" ? "lead-ready" : "active",
            score: scoreSalesPipelineRow(row)
          }
        });
      }
    }
  }
  runItem.updatedAt = new Date().toISOString();
  runItem.auditLog.push({ at: runItem.updatedAt, event: "gate_agent_run", detail: `${gate}:${row.id}:${gateStatus}` });
  return runItem;
}

async function runPipelineGateAgent(
  store: SalesAutomationStore,
  row: SalesCampaignImportRow,
  gate: SalesPipelineRunGateId,
  mode: SalesPipelineRunMode,
  runItem: SalesPipelineRun
): Promise<SalesPipelineAgentOutput> {
  const { Agent, OpenAI, OpenAIChatCompletionsModel, run, setTracingDisabled, tool, z } = await loadAgentRuntime();
  setTracingDisabled(true);
  const campaign = store.campaigns.find((item) => item.id === row.campaignId);
  const abortController = new AbortController();
  const timeout = setTimeout(() => abortController.abort("sales_pipeline_gate_timeout"), DEFAULT_RESEARCH_TIMEOUT_MS);
  const client = new OpenAI({
    apiKey: await readMinimaxApiKey(),
    baseURL: "https://api.minimax.io/v1"
  });
  let webEvidence = row.webEvidence;
  const researchDatabaseRequests: Array<{ database: string; query: string; purpose: string }> = [];

  const ctoxWebSearchTool = tool({
    name: "ctox_web_search",
    description: "Search public web evidence through the CTOX web stack. Use repeatedly with focused queries for official website, contact, imprint, decision makers, and campaign fit.",
    parameters: z.object({
      query: z.string().min(3),
      domains: z.array(z.string()).optional()
    }),
    async execute({ query, domains }) {
      webEvidence = mergeWebEvidence(webEvidence, await runCtoxWebSearch(query, domains ?? []));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxWebReadTool = tool({
    name: "ctox_web_read",
    description: "Read a specific page through the CTOX web stack. Use it after search on official pages, contact, imprint, team, jobs, LinkedIn/Xing snippets, or registry-like pages.",
    parameters: z.object({
      url: z.string().url(),
      query: z.string().optional(),
      find: z.array(z.string()).optional()
    }),
    async execute({ url, query, find }) {
      webEvidence = mergeWebEvidence(webEvidence, await readCompanyEvidenceWithCtoxWebStack(url, query, find ?? []));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxRegistrySearchTool = tool({
    name: "ctox_registry_search",
    description: "Search German registry/company sources through the CTOX web stack for legal entity, address, management, and disambiguation.",
    parameters: z.object({ query: z.string().min(3) }),
    async execute({ query }) {
      const registryEvidence = await searchRegistryEvidenceWithCtoxWebStack(row, query);
      registryEvidence.toolCalls = (registryEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "pipeline_registry_search" }));
      webEvidence = mergeWebEvidence(webEvidence, registryEvidence);
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxSocialSearchTool = tool({
    name: "ctox_social_search",
    description: "Search public LinkedIn/Xing/social-web snippets through the CTOX web stack for likely decision makers. Snippets are hints unless name and role are visible.",
    parameters: z.object({ query: z.string().min(3) }),
    async execute({ query }) {
      const socialEvidence = await searchSocialEvidenceWithCtoxWebStack(row, query);
      socialEvidence.toolCalls = (socialEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "pipeline_social_search" }));
      webEvidence = mergeWebEvidence(webEvidence, socialEvidence);
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxDirectSiteProbeTool = tool({
    name: "ctox_direct_site_probe",
    description: "Probe likely official website domains derived from the company name. Use this when search results are generic, noisy, or the company has a predictable domain.",
    parameters: z.object({
      domains: z.array(z.string()).optional(),
      query: z.string().optional()
    }),
    async execute({ domains, query }) {
      webEvidence = mergeWebEvidence(webEvidence, await probeLikelyCompanySites(row, domains, query));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const researchDatabaseRequestTool = tool({
    name: "research_database_request",
    description: "Request a non-web research database when public web evidence is insufficient. Use only after web, registry, and social attempts are not enough.",
    parameters: z.object({
      database: z.enum(["commercial_register", "linkedin_sales_navigator", "xing", "internal_crm", "manual_research", "other"]),
      query: z.string().min(3),
      purpose: z.string().min(3)
    }),
    async execute({ database, query, purpose }) {
      researchDatabaseRequests.push({ database, query, purpose });
      return JSON.stringify({
        ok: true,
        availableNow: false,
        queuedRequest: { database, query, purpose },
        note: "No external research database is configured for this Business OS instance yet. Convert this into a user question or blocker."
      });
    }
  });
  void ctoxWebSearchTool;
  void ctoxWebReadTool;
  void ctoxRegistrySearchTool;
  void ctoxSocialSearchTool;
  void ctoxDirectSiteProbeTool;
  void researchDatabaseRequestTool;

  webEvidence = await collectPipelineGateEvidence(row, gate, webEvidence, researchDatabaseRequests);
  if (webEvidence) updateRowInMemory(store, row.id, { webEvidence });

  const agent = new Agent({
    name: "Sales Pipeline Candidate Orchestrator",
    model: new OpenAIChatCompletionsModel(client, MINIMAX_MODEL),
    tools: [],
    modelSettings: {
      toolChoice: "none",
      parallelToolCalls: false,
      temperature: 0.15
    },
    instructions: buildPipelineGateSystemPrompt({ campaign, row, gate, mode })
  });

  try {
    const result = await run(agent, buildPipelineGateUserPrompt({ campaign, row, gate, mode, runItem, webEvidence }), {
      maxTurns: 2,
      signal: abortController.signal
    });
    if (webEvidence) updateRowInMemory(store, row.id, { webEvidence });
    const parsed = parseResearchJson(String(result.finalOutput ?? "").trim());
    const output = normalizePipelineAgentOutput(parsed, row, gate, mode, webEvidence, researchDatabaseRequests);
    if (output.researchPatch && webEvidence) {
      output.researchPatch = enforcePipelinePatchEvidence(output.researchPatch, row, webEvidence);
    }
    return output;
  } catch (error) {
    const fallback = simulatePipelineGate(row, gate, mode);
    return {
      gateStatus: "waiting_for_user",
      summary: `${fallback.summary} Pipeline agent failed before completing the multi-turn gate: ${String(error instanceof Error ? error.message : error)}`,
      score: fallback.score,
      sources: fallback.sources,
      risks: uniqueStrings([
        ...fallback.risks,
        "Pipeline agent failed; rerun after checking MiniMax/CTOX web stack availability."
      ]),
      output: {
        ...fallback.output,
        agentError: String(error instanceof Error ? error.message : error).replace(/sk-api-[A-Za-z0-9_-]+/g, "[redacted-secret]")
      },
      question: fallback.question
    };
  } finally {
    clearTimeout(timeout);
  }
}

async function collectPipelineGateEvidence(
  row: SalesCampaignImportRow,
  gate: SalesPipelineRunGateId,
  initialEvidence: SalesWebEvidence | undefined,
  researchDatabaseRequests: Array<{ database: string; query: string; purpose: string }>
) {
  let evidence = initialEvidence;
  const company = row.research?.companyName || row.companyName;
  const contactQuery = `${company} Kontakt Impressum Telefon E-Mail Geschäftsführung Recruiting Vertrieb`;
  const decisionQuery = `${company} Geschäftsführer Recruiting Leitung Vertrieb HR LinkedIn Xing`;

  if (row.research?.likelyWebsite) {
    evidence = mergeWebEvidence(evidence, await readCompanyEvidenceWithCtoxWebStack(
      row.research.likelyWebsite,
      `${company} Kontakt Impressum Telefon E-Mail Adresse Geschäftsführung Personalvermittlung`,
      ["Kontakt", "Impressum", "Telefon", "E-Mail", "Geschäftsführer", "Recruiting", "Vertrieb"]
    ));
  }

  if (!evidence || !hasRelevantWebEvidence(evidence, row)) {
    evidence = mergeWebEvidence(evidence, await probeLikelyCompanySites(
      row,
      row.research?.likelyWebsite ? [safeHostname(row.research.likelyWebsite)].filter(Boolean) : undefined,
      `${company} official website contact imprint staffing recruiting`
    ));
  }

  evidence = mergeWebEvidence(evidence, await runCtoxWebSearch(contactQuery));
  for (const url of selectPotentialReadUrls(evidence, row).slice(0, 2)) {
    evidence = mergeWebEvidence(evidence, await readCompanyEvidenceWithCtoxWebStack(
      url,
      `${company} Kontakt Impressum Telefon E-Mail Adresse Geschäftsführung Recruiting Vertrieb`,
      ["Kontakt", "Impressum", "Telefon", "E-Mail", "Geschäftsführer", "Recruiting", "Vertrieb"]
    ));
  }

  if (gate === "contact_discovery" || gate === "decision_contact" || gate === "company_identity") {
    const registryEvidence = await searchRegistryEvidenceWithCtoxWebStack(row, `${company} Handelsregister Unternehmensregister Northdata Geschäftsführer Adresse`);
    registryEvidence.toolCalls = (registryEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "pipeline_registry_search" }));
    evidence = mergeWebEvidence(evidence, registryEvidence);
  }

  const currentContacts = row.research?.contactCandidates ?? [];
  if ((gate === "contact_discovery" || gate === "decision_contact") && currentContacts.length === 0) {
    const socialEvidence = await searchSocialEvidenceWithCtoxWebStack(row, decisionQuery);
    socialEvidence.toolCalls = (socialEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "pipeline_social_search" }));
    evidence = mergeWebEvidence(evidence, socialEvidence);
  }

  if ((gate === "contact_discovery" || gate === "decision_contact") && !hasUsableContactSignal(row, evidence)) {
    researchDatabaseRequests.push({
      database: "linkedin_sales_navigator",
      query: decisionQuery,
      purpose: "Find a named decision maker for AI-placement consulting outreach because public web evidence did not verify one."
    });
  }

  return evidence;
}

function hasUsableContactSignal(row: SalesCampaignImportRow, evidence: SalesWebEvidence | undefined) {
  const existing = row.research?.contactCandidates?.some((candidate) => candidate.name || candidate.email || candidate.phone);
  if (existing) return true;
  if (!evidence) return false;
  return extractContactCandidatesFromEvidence(flattenEvidenceText(evidence)).length > 0;
}

function safeHostname(value: string) {
  try {
    return new URL(value).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

function buildPipelineGateSystemPrompt({
  campaign,
  row,
  gate,
  mode
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  gate: SalesPipelineRunGateId;
  mode: SalesPipelineRunMode;
}) {
  return [
    "You are the Sales Pipeline Candidate Orchestrator inside CTOX Business OS.",
    "You run independently from the CTOX core conversation model. You may use tools and persist gate evidence, but you must not block CTOX.",
    "",
    "SAFETY AND SIDE EFFECTS",
    `Run mode: ${mode}`,
    mode === "dry_run"
      ? "Dry run means: do not send email, do not create leads, do not mutate external systems, and do not claim that outreach was executed."
      : "Live mode still requires explicit user approval before external delivery or lead handoff.",
    "Never use CTOX personal mail. Delivery must be through the configured business mailbox/provider, normally Resend, after approval.",
    "",
    "MISSION",
    `Campaign: ${campaign?.name ?? "Sales campaign"}`,
    `Campaign context: ${campaign?.description ?? "Qualify campaign candidates for AI placement consulting."}`,
    `Current candidate: ${row.companyName}`,
    `Current gate: ${gate}`,
    "Campaign target: German staffing agencies, recruiters, personnel service providers, headhunters, and related placement firms that could sell AI employee/AI-agent consulting as a new business line.",
    "",
    "GATE OUTCOMES",
    "company_identity: pass only when the company is the right entity and a relevant staffing/recruiting/personnel-services prospect.",
    "contact_discovery: pass only when at least one real person or role-specific official contact route is supported by evidence.",
    "decision_contact: pass only when the selected contact plausibly owns management, recruiting, HR, sales, transformation, or business development.",
    "outreach_draft: pass only when the message has a concrete campaign angle and no unsupported claims.",
    "delivery: pass only in dry run as a simulated send-ready state, or in live mode after explicit approval.",
    "lead_handoff: pass only when a positive conversation/meeting signal exists.",
    "",
    "RESEARCH LOOP",
    "You must work multi-turn. Do not stop after one search.",
    "Use the CTOX web stack as primary evidence. Use public web reads after searches.",
    "Minimum for contact/company gates:",
    "1. Search/read the official website or direct domain probe.",
    "2. Search/read Kontakt or Impressum.",
    "3. Search/read management, team, jobs, about, or location pages when available.",
    "4. Use registry search if legal entity/address/management is unclear.",
    "5. Use social search if no named decision maker is found on official pages.",
    "6. If web+registry+social still cannot answer the gate, call research_database_request and return waiting_for_user with the requested database and reason.",
    "",
    "EVIDENCE RULES",
    "Never invent phone numbers, email addresses, or people.",
    "Use social snippets as hints unless name and role are visible.",
    "If the imported row is not a matching prospect, return blocked.",
    "If evidence is promising but missing a decision-maker, return waiting_for_user with a precise next research request.",
    "",
    "OUTPUT CONTRACT",
    "Return only one valid JSON object. No markdown fences. No prose outside JSON.",
    "JSON shape:",
    JSON.stringify({
      gateStatus: "passed|waiting_for_user|blocked|failed",
      summary: "",
      score: 0,
      sources: [{ title: "", url: "" }],
      risks: [""],
      output: {
        evidenceSummary: "",
        toolPlanCompleted: [""],
        databaseRequests: [{ database: "", query: "", purpose: "" }],
        nextOperationalStep: ""
      },
      researchPatch: {
        phone: "",
        email: "",
        address: "",
        contactCandidates: [{ name: "", role: "", email: "", phone: "", confidence: "low|medium|high", evidence: "" }],
        missingFields: [""],
        recommendedNextAction: "",
        sourceNote: ""
      },
      question: {
        severity: "blocking|clarifying|approval",
        question: "",
        options: [{ id: "search_more", label: "Weiter recherchieren", effect: "search_more" }],
        freeTextAllowed: true,
        contextRefs: [""]
      }
    }),
    "Omit question only if gateStatus is passed, blocked, or failed and no user input is needed."
  ].join("\n");
}

function buildPipelineGateUserPrompt({
  campaign,
  row,
  gate,
  mode,
  runItem,
  webEvidence
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  gate: SalesPipelineRunGateId;
  mode: SalesPipelineRunMode;
  runItem: SalesPipelineRun;
  webEvidence: SalesWebEvidence | undefined;
}) {
  return [
    "Run the next pipeline gate for this candidate. Use tools first, then return the JSON contract.",
    "",
    "CAMPAIGN",
    JSON.stringify({
      id: campaign?.id,
      name: campaign?.name,
      description: campaign?.description,
      sourceName: campaign?.sourceName
    }, null, 2),
    "",
    "CANDIDATE",
    JSON.stringify({
      rowId: row.id,
      rowIndex: row.rowIndex,
      companyName: row.companyName,
      imported: row.imported,
      currentPipeline: row.pipeline,
      currentResearch: row.research
    }, null, 2),
    "",
    "CURRENT GATE",
    JSON.stringify({
      gate,
      mode,
      currentRunStatus: runItem.status,
      previousMessages: runItem.messages.slice(-6),
      answeredQuestions: runItem.questions.filter((question) => question.answeredAt).slice(-5),
      existingGateState: runItem.gates[gate]
    }, null, 2),
    "",
    "EXISTING CTOX WEB EVIDENCE",
    JSON.stringify(webEvidence ? compactWebEvidenceForPrompt(webEvidence) : null, null, 2),
    "",
    "SUGGESTED TOOL QUERIES",
    `"${row.companyName}" offizielle Website Deutschland Personalvermittlung Personaldienstleister`,
    `"${row.companyName}" Kontakt Impressum Telefon E-Mail`,
    `"${row.companyName}" Geschäftsführung Geschaeftsfuehrer Recruiting Vertrieb HR LinkedIn Xing`,
    `"${row.companyName}" Handelsregister Unternehmensregister Northdata`
  ].join("\n");
}

function normalizePipelineAgentOutput(
  value: unknown,
  row: SalesCampaignImportRow,
  gate: SalesPipelineRunGateId,
  mode: SalesPipelineRunMode,
  webEvidence: SalesWebEvidence | undefined,
  researchDatabaseRequests: Array<{ database: string; query: string; purpose: string }>
): SalesPipelineAgentOutput {
  const evidenceFallback = buildEvidenceBackedPipelineFallback(row, gate, mode, webEvidence, researchDatabaseRequests);
  const simulationFallback = simulatePipelineGate(row, gate, mode);
  const source = typeof value === "object" && value ? value as Record<string, unknown> : {};
  const parsedStatus = source.gateStatus === "passed" || source.gateStatus === "waiting_for_user" || source.gateStatus === "blocked" || source.gateStatus === "failed"
    ? source.gateStatus
    : evidenceFallback.gateStatus;
  const sources = Array.isArray(source.sources)
    ? source.sources.map((item) => {
      const sourceItem = typeof item === "object" && item ? item as Record<string, unknown> : {};
      return { title: stringValue(sourceItem.title), url: stringValue(sourceItem.url) };
    }).filter((item) => item.title || item.url)
    : [];
  const questionSource = typeof source.question === "object" && source.question ? source.question as Record<string, unknown> : undefined;
  const questionOptions = Array.isArray(questionSource?.options)
    ? questionSource.options.map((item) => normalizePipelineQuestionOption(item)).filter((item) => item !== null)
    : [];
  const risks = uniqueStrings([
    ...(Array.isArray(source.risks) ? source.risks.map(String) : evidenceFallback.risks),
    ...(mode === "dry_run" ? ["Dry Run: no email, lead creation, or external CRM action will be executed."] : []),
    ...(researchDatabaseRequests.length ? ["External research database input requested before this gate can be fully resolved."] : [])
  ]);
  const output = typeof source.output === "object" && source.output && !Array.isArray(source.output)
    ? source.output as Record<string, unknown>
    : evidenceFallback.output;
  const contextRefs = Array.isArray(questionSource?.contextRefs)
    ? questionSource.contextRefs.map(String).filter(Boolean)
    : (sources.length ? sources : evidenceFallback.sources).map((item) => item.url).filter(Boolean).map(String);
  const fallbackQuestion = evidenceFallback.question ?? simulationFallback.question;

  return {
    gateStatus: parsedStatus,
    summary: stringValue(source.summary) || evidenceFallback.summary,
    score: typeof source.score === "number" ? Math.max(0, Math.min(100, source.score)) : evidenceFallback.score,
    sources: sources.length ? sources : evidenceFallback.sources,
    risks,
    output: {
      ...output,
      databaseRequests: researchDatabaseRequests
    },
    researchPatch: typeof source.researchPatch === "object" && source.researchPatch ? source.researchPatch as Partial<SalesCompanyResearch> : undefined,
    question: parsedStatus === "waiting_for_user"
      ? {
        severity: questionSource?.severity === "approval" || questionSource?.severity === "clarifying" ? questionSource.severity : "blocking",
        question: stringValue(questionSource?.question) || fallbackQuestion.question,
        options: questionOptions.length ? questionOptions : fallbackQuestion.options,
        freeTextAllowed: typeof questionSource?.freeTextAllowed === "boolean" ? questionSource.freeTextAllowed : true,
        contextRefs
      }
      : undefined
  };
}

function buildEvidenceBackedPipelineFallback(
  row: SalesCampaignImportRow,
  gate: SalesPipelineRunGateId,
  mode: SalesPipelineRunMode,
  evidence: SalesWebEvidence | undefined,
  researchDatabaseRequests: Array<{ database: string; query: string; purpose: string }>
): SalesPipelineAgentOutput {
  const simulation = simulatePipelineGate(row, gate, mode);
  const evidenceText = evidence ? flattenEvidenceText(evidence) : "";
  const contacts = extractContactCandidatesFromEvidence(evidenceText);
  const sources = selectGateSources(evidence, row).slice(0, 8);
  const missing = uniqueStrings([
    ...(row.research?.missingFields ?? []),
    ...(contacts.length === 0 && (gate === "contact_discovery" || gate === "decision_contact") ? ["verified decision maker"] : [])
  ]);
  const canPassContactDiscovery = gate === "contact_discovery" && contacts.length > 0;
  const gateStatus = canPassContactDiscovery ? "passed" : "waiting_for_user";
  const company = row.research?.companyName || row.companyName;
  const summary = canPassContactDiscovery
    ? `Multi-step CTOX research found ${contacts[0]?.name ?? "a contact"} as a potential decision-maker signal for ${company}.`
    : `Multi-step CTOX research verified available web evidence for ${company}, but no named decision maker is reliable enough yet.`;

  return {
    gateStatus,
    summary,
    score: canPassContactDiscovery ? Math.max(scoreSalesPipelineRow(row), 70) : scoreSalesPipelineRow(row),
    sources: sources.length ? sources : simulation.sources,
    risks: uniqueStrings([
      ...missing.slice(0, 8),
      ...(mode === "dry_run" ? ["Dry Run: no email, lead creation, or external CRM action will be executed."] : []),
      ...(researchDatabaseRequests.length ? ["Research database input requested."] : [])
    ]),
    output: {
      company,
      evidenceSummary: sources.map((source) => source.url).filter(Boolean).join(", "),
      toolPlanCompleted: (evidence?.toolCalls ?? []).slice(-10),
      databaseRequests: researchDatabaseRequests,
      nextOperationalStep: canPassContactDiscovery ? "Verify decision relevance and contact route." : "Use a research database or manual lookup to verify a decision maker."
    },
    researchPatch: contacts.length > 0
      ? {
        contactCandidates: contacts,
        missingFields: missing.filter((field) => !/verified contact person|verified decision maker/i.test(field)),
        recommendedNextAction: "Verify decision relevance and direct contact route before outreach.",
        sourceNote: `Pipeline multi-step CTOX evidence: ${sources.map((source) => source.url).filter(Boolean).join(", ")}`
      }
      : {
        missingFields: missing,
        recommendedNextAction: "Find a verified decision maker via LinkedIn/Xing, a research database, or manual lookup before outreach.",
        sourceNote: `Pipeline multi-step CTOX evidence did not verify a named decision maker. Sources: ${sources.map((source) => source.url).filter(Boolean).join(", ")}`
      },
    question: canPassContactDiscovery ? undefined : {
      severity: "blocking",
      question: `Web, Registry und Social Search reichen fuer ${company} noch nicht aus. Soll CTOX eine Research-Datenbank bzw. manuelle Recherche fuer den Entscheider anfordern?`,
      options: [
        { id: "request_database", label: "Research-Datenbank anfordern", effect: "search_more" },
        { id: "accept_company", label: "Firma parken, spaeter Kontakt ergaenzen", effect: "continue" },
        { id: "reject", label: "Kandidat verwerfen", effect: "reject" }
      ],
      freeTextAllowed: true,
      contextRefs: sources.map((source) => source.url).filter(Boolean).map(String)
    }
  };
}

function normalizePipelineQuestionOption(value: unknown): SalesPipelineRunQuestion["options"][number] | null {
  const option = typeof value === "object" && value ? value as Record<string, unknown> : {};
  const id = stringValue(option.id);
  const label = stringValue(option.label);
  const effect = option.effect;
  if (!id || !label) return null;
  if (effect !== "continue" && effect !== "search_more" && effect !== "approve" && effect !== "reject" && effect !== "park") return null;
  return { id, label, effect };
}

function selectGateSources(evidence: SalesWebEvidence | undefined, row: SalesCampaignImportRow) {
  const tokens = distinctiveCompanyTokens(row.companyName);
  const fromResults = (evidence?.results ?? [])
    .filter((result) => {
      const url = String(result.url ?? "").toLowerCase();
      if (!/^https?:\/\//.test(url)) return false;
      if (IRRELEVANT_SEARCH_HOSTS.some((host) => url.includes(host))) return false;
      if (/chiebukuro|yahoo\.co\.jp|brainly|zhihu|baidu|microsoft|office\.com/.test(url)) return false;
      return resultLooksRelevant(result, row) || tokens.some((token) => url.includes(token));
    })
    .map((result) => ({ title: result.title, url: result.url }));
  const fromCitations = (evidence?.citations ?? [])
    .filter((citation) => {
      const url = String(citation.url ?? "").toLowerCase();
      if (!/^https?:\/\//.test(url)) return false;
      if (IRRELEVANT_SEARCH_HOSTS.some((host) => url.includes(host))) return false;
      if (/chiebukuro|yahoo\.co\.jp|brainly|zhihu|baidu|microsoft|office\.com/.test(url)) return false;
      return tokens.some((token) => url.includes(token) || String(citation.title ?? "").toLowerCase().includes(token));
    })
    .map((citation) => ({ title: citation.title, url: citation.url }));
  const seen = new Set<string>();
  return [...fromResults, ...fromCitations].filter((source) => {
    const key = `${source.url ?? ""}|${source.title ?? ""}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return source.url || source.title;
  });
}

function applyPipelineResearchPatch(store: SalesAutomationStore, row: SalesCampaignImportRow, patch: Partial<SalesCompanyResearch>) {
  const current = row.research;
  if (!current) return;
  updateRowInMemory(store, row.id, {
    research: {
      ...current,
      ...patch,
      contactCandidates: Array.isArray(patch.contactCandidates) ? patch.contactCandidates : current.contactCandidates,
      qualification: {
        ...current.qualification,
        ...(patch.qualification ?? {})
      },
      missingFields: Array.isArray(patch.missingFields) ? uniqueStrings(patch.missingFields.map(String)) : current.missingFields
    }
  });
}

function enforcePipelinePatchEvidence(
  patch: Partial<SalesCompanyResearch>,
  row: SalesCampaignImportRow,
  evidence: SalesWebEvidence
) {
  const evidenceText = flattenEvidenceText(evidence).toLowerCase();
  const safeContacts = (patch.contactCandidates ?? []).filter((candidate) => {
    const proof = [candidate.name, candidate.email, candidate.role].filter(Boolean).join(" ").toLowerCase();
    if (!proof) return false;
    const firstToken = proof.split(/\s+/)[0];
    return Boolean(firstToken && evidenceText.includes(firstToken));
  });
  const nextPatch = { ...patch };
  if (patch.contactCandidates) nextPatch.contactCandidates = safeContacts;
  if (patch.phone && !isPlausiblePhoneNumber(patch.phone)) delete nextPatch.phone;
  if (patch.email && !evidenceText.includes(patch.email.toLowerCase())) delete nextPatch.email;
  nextPatch.sourceNote = uniqueStrings([
    patch.sourceNote ?? "",
    `Pipeline gate verified with CTOX web evidence for ${row.companyName}.`
  ]).join(" ");
  return nextPatch;
}

function simulatePipelineGate(row: SalesCampaignImportRow, gate: SalesPipelineRunGateId, mode: SalesPipelineRunMode) {
  const research = row.research;
  const candidate = research?.contactCandidates[0];
  const sources = (row.webEvidence?.citations ?? []).slice(0, 5);
  const risks = uniqueStrings([
    ...(research?.missingFields ?? []).slice(0, 6),
    ...(mode === "live" ? [] : ["Dry Run: no email, lead creation, or external CRM action will be executed."])
  ]);
  const company = research?.companyName || row.companyName;
  const contactLabel = candidate?.name || candidate?.role || "kein Ansprechpartner";
  const questionBase = {
    id: `question-${crypto.randomUUID()}`,
    gate,
    contextRefs: sources.map((source) => source.url).filter(Boolean).map(String),
    freeTextAllowed: true
  };

  if (gate === "contact_discovery") {
    return {
      score: scoreSalesPipelineRow(row),
      sources,
      risks,
      summary: `Contact Discovery Agent would continue from ${company}. Current best contact signal: ${contactLabel}.`,
      output: { company, candidate, recommendedSearch: `${company} Geschäftsführer Recruiting Leitung Kontakt LinkedIn Xing` },
      question: {
        ...questionBase,
        severity: "blocking" as const,
        question: `Soll CTOX fuer ${company} weiter nach Geschaeftsfuehrung, Recruiting-Leitung oder Vertrieb recherchieren?`,
        options: [
          { id: "search_more", label: "Weiter recherchieren", effect: "search_more" as const },
          { id: "accept_company", label: "Firma parken, spaeter Kontakt ergaenzen", effect: "continue" as const },
          { id: "reject", label: "Kandidat verwerfen", effect: "reject" as const }
        ]
      }
    };
  }

  if (gate === "decision_contact") {
    return {
      score: scoreSalesPipelineRow(row),
      sources,
      risks,
      summary: `Decision Contact Agent would evaluate ${contactLabel} as primary contact for ${company}.`,
      output: { primaryContact: candidate, alternatives: research?.contactCandidates.slice(1, 4) ?? [] },
      question: {
        ...questionBase,
        severity: "clarifying" as const,
        question: `Ist ${contactLabel} der richtige Ansprechpartner fuer das KI-Vermittlungs-Angebot?`,
        options: [
          { id: "use_contact", label: "Diesen Kontakt verwenden", effect: "continue" as const },
          { id: "search_more", label: "Weitere Entscheider suchen", effect: "search_more" as const },
          { id: "reject", label: "Kandidat verwerfen", effect: "reject" as const }
        ]
      }
    };
  }

  if (gate === "outreach_draft") {
    return {
      score: scoreSalesPipelineRow(row),
      sources,
      risks,
      summary: `Outreach Draft Agent would prepare a reviewed first message for ${company} using the campaign consulting angle.`,
      output: {
        subject: `KI-Vermittlung als neues Beratungsangebot fuer ${company}`,
        body: `Guten Tag, ich wuerde gern kurz zeigen, wie Personaldienstleister KI-Mitarbeiter als neues Geschaeftsfeld bewerten koennen.`
      },
      question: {
        ...questionBase,
        severity: "approval" as const,
        question: "Soll CTOX diesen Outreach-Entwurf als Basis fuer die Versandfreigabe verwenden?",
        options: [
          { id: "approve_draft", label: "Entwurf freigeben", effect: "approve" as const },
          { id: "revise", label: "Ueberarbeiten lassen", effect: "search_more" as const },
          { id: "park", label: "Kandidat parken", effect: "park" as const }
        ]
      }
    };
  }

  if (gate === "delivery") {
    return {
      score: scoreSalesPipelineRow(row),
      sources,
      risks,
      summary: `Delivery Simulation Agent would send via the configured business mailbox, not via CTOX personal mail.`,
      output: { provider: "resend", sendEmail: mode === "live", recipient: candidate?.email || research?.email || "" },
      question: {
        ...questionBase,
        severity: "approval" as const,
        question: "Soll nach Review im Live-Modus ueber Resend versendet werden?",
        options: [
          { id: "approve_send", label: "Versand freigeben", effect: "approve" as const },
          { id: "dry_only", label: "Nur Simulation behalten", effect: "park" as const },
          { id: "revise", label: "Ansprache ueberarbeiten", effect: "search_more" as const }
        ]
      }
    };
  }

  return {
    score: scoreSalesPipelineRow(row),
    sources,
    risks,
    summary: `Lead Handoff Agent would create a Leads module record for ${company} after a positive conversation signal.`,
    output: { company, contact: candidate, nextStep: "Termin abstimmen" },
    question: {
      ...questionBase,
      severity: "approval" as const,
      question: "Liegt ein positives Gespraechssignal vor und soll dieser Kandidat ins Leads-Modul uebergeben werden?",
      options: [
        { id: "approve_lead", label: "Lead anlegen", effect: "approve" as const },
        { id: "wait", label: "Noch warten", effect: "park" as const },
        { id: "reject", label: "Nicht weiterverfolgen", effect: "reject" as const }
      ]
    }
  };
}

function nextGateForCandidate(row: SalesCampaignImportRow, runItem?: SalesPipelineRun): SalesPipelineRunGateId {
  const stage = row.pipeline?.stageId ?? inferSalesPipelineStage(row);
  if (stage === "company") return "contact_discovery";
  if (stage === "contact") return "decision_contact";
  if (stage === "decision") return "outreach_draft";
  if (stage === "conversation") return "delivery";
  return runItem?.currentGate ?? "lead_handoff";
}

function nextStageAfterGate(gate: SalesPipelineRunGateId, currentStage?: SalesPipelineStageId): SalesPipelineStageId | undefined {
  if (gate === "contact_discovery" && currentStage === "company") return "contact";
  if (gate === "decision_contact" && currentStage === "contact") return "decision";
  if (gate === "outreach_draft" && currentStage === "decision") return "conversation";
  if (gate === "delivery" && currentStage === "conversation") return "lead-ready";
  return undefined;
}

function emptyGateState(gate: SalesPipelineRunGateId): SalesPipelineGateState {
  return {
    status: "not_started",
    summary: `${gate} not started.`,
    score: 0,
    sources: [],
    risks: [],
    updatedAt: new Date().toISOString()
  };
}

async function parseImportRows(input: CampaignImportInput) {
  if (input.file && input.sourceType === "Excel") {
    const XLSX = await loadXlsxRuntime();
    const buffer = Buffer.from(await input.file.arrayBuffer());
    const workbook = XLSX.read(buffer, { type: "buffer", cellDates: false });
    const firstSheet = workbook.Sheets[workbook.SheetNames[0]];
    const rawRows = XLSX.utils.sheet_to_json<Record<string, unknown>>(firstSheet, { defval: "", raw: false });
    if (rawRows.length > 0) return rawRows.map(normalizeRecord);

    const arrayRows = XLSX.utils.sheet_to_json<unknown[]>(firstSheet, { header: 1, defval: "", raw: false });
    return arrayRows.map((row) => ({ company: String(row[0] ?? "").trim() })).filter((row) => row.company);
  }

  if (input.sourceText) {
    return input.sourceText.split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((company) => ({ company }));
  }

  return [];
}

function normalizeRecord(row: Record<string, unknown>) {
  const entries = Object.entries(row).map(([key, value]) => [String(key).trim(), String(value ?? "").trim()] as const);
  const normalized = Object.fromEntries(entries.filter(([key, value]) => key || value));
  const keys = Object.keys(normalized);
  if (keys.length === 1 && /^__EMPTY/.test(keys[0] ?? "")) return { company: normalized[keys[0]] };
  return normalized;
}

function inferCompanyName(row: Record<string, string>, index: number) {
  const direct = findValue(row, ["company", "firma", "unternehmen", "name", "personalvermittler", "personalvermittlung"]);
  if (direct) return direct;
  if (index === 0) return "";
  return Object.values(row).find((value) => value.trim()) ?? "";
}

function isDecisionRole(role?: string) {
  if (!role) return false;
  return /geschäfts|geschaefts|managing|owner|inhaber|founder|ceo|cfo|coo|partner|director|leiter|head|vertrieb|sales|recruiting|talent|hr|personal/i.test(role);
}

async function readMinimaxApiKey() {
  const envValue = process.env.MINIMAX_API_KEY?.trim();
  if (envValue) return envValue;

  const { stdout } = await execFileAsync("ctox", ["secret", "get", "--scope", "credentials", "--name", "MINIMAX_API_KEY"], {
    timeout: 10_000,
    maxBuffer: 1024 * 128
  });
  const parsed = JSON.parse(stdout) as { value?: string };
  if (!parsed.value) throw new Error("missing_minimax_api_key");
  return parsed.value;
}

async function researchCompanyWithCtoxWebStack(row: SalesCampaignImportRow, queryOverride?: string): Promise<SalesWebEvidence> {
  const query = queryOverride?.trim() || [
    `"${row.companyName}"`,
    "Personalvermittlung Personaldienstleister Kontakt Deutschland"
  ].join(" ");
  return runCtoxWebSearch(query);
}

async function searchRegistryEvidenceWithCtoxWebStack(row: SalesCampaignImportRow, queryOverride?: string): Promise<SalesWebEvidence> {
  const query = queryOverride?.trim() || `"${row.companyName}" Handelsregister Unternehmensregister Northdata`;
  return runCtoxWebSearch(query, ["unternehmensregister.de", "handelsregister.de", "northdata.de"]);
}

async function searchSocialEvidenceWithCtoxWebStack(row: SalesCampaignImportRow, queryOverride?: string): Promise<SalesWebEvidence> {
  const query = queryOverride?.trim() || `"${row.companyName}" Ansprechpartner Recruiting Vertrieb LinkedIn Xing`;
  return runCtoxWebSearch(query, ["linkedin.com", "xing.com"]);
}

async function runCtoxWebSearch(query: string, domains: string[] = []): Promise<SalesWebEvidence> {
  try {
    const args = ["web", "search", "--query", query, "--context-size", "high", "--include-sources"];
    for (const domain of domains) args.push("--domain", domain);
    const { stdout } = await execFileAsync("ctox", args, {
      timeout: 60_000,
      maxBuffer: 1024 * 1024 * 4
    });
    const payload = JSON.parse(stdout) as {
      ok?: boolean;
      provider?: string;
      citations?: Array<{ title?: string; url?: string }>;
      results?: Array<{
        title?: string;
        url?: string;
        snippet?: string;
        summary?: string;
        excerpts?: string[];
      }>;
    };

    return {
      query,
      ok: payload.ok === true,
      provider: payload.provider,
      toolCalls: [{ tool: "search", query, ok: payload.ok === true }],
      citations: (payload.citations ?? []).slice(0, 5).map((citation) => ({
        title: stringValue(citation.title),
        url: stringValue(citation.url)
      })),
      results: (payload.results ?? []).slice(0, 5).map((result) => ({
        title: stringValue(result.title),
        url: stringValue(result.url),
        snippet: stringValue(result.snippet),
        summary: stringValue(result.summary),
        excerpts: (result.excerpts ?? []).slice(0, 3).map(String)
      }))
    };
  } catch (error) {
    return {
      query,
      ok: false,
      toolCalls: [{ tool: "search", query, ok: false, note: String(error instanceof Error ? error.message : error) }],
      citations: [],
      results: [],
      error: String(error instanceof Error ? error.message : error)
    };
  }
}

async function readCompanyEvidenceWithCtoxWebStack(url: string, query?: string, find: string[] = []): Promise<SalesWebEvidence> {
  try {
    const args = ["web", "read", "--url", url];
    if (query?.trim()) args.push("--query", query.trim());
    for (const pattern of find.filter(Boolean).slice(0, 5)) args.push("--find", pattern);
    const { stdout } = await execFileAsync("ctox", args, {
      timeout: 60_000,
      maxBuffer: 1024 * 1024 * 4
    });
    const payload = JSON.parse(stdout) as {
      ok?: boolean;
      title?: string;
      url?: string;
      summary?: string;
      excerpts?: string[];
      page_text_excerpt?: string;
      find_results?: Array<{ pattern?: string; matches?: string[] }>;
    };
    const excerpts = [
      ...(payload.excerpts ?? []),
      ...(payload.find_results ?? []).flatMap((result) => result.matches ?? []),
      payload.page_text_excerpt
    ].filter(Boolean).map(String);
    return {
      query: query || url,
      ok: payload.ok === true,
      provider: "ctox_web_read",
      toolCalls: [{ tool: "read", url, query, ok: payload.ok === true }],
      citations: [{ title: stringValue(payload.title), url: stringValue(payload.url) || url }],
      results: [{
        title: stringValue(payload.title),
        url: stringValue(payload.url) || url,
        snippet: excerpts[0],
        summary: stringValue(payload.summary),
        excerpts: excerpts.slice(0, 8)
      }]
    };
  } catch (error) {
    return {
      query: query || url,
      ok: false,
      provider: "ctox_web_read",
      toolCalls: [{ tool: "read", url, query, ok: false, note: String(error instanceof Error ? error.message : error) }],
      citations: [],
      results: [],
      error: String(error instanceof Error ? error.message : error)
    };
  }
}

async function probeLikelyCompanySites(row: SalesCampaignImportRow, domains?: string[], query?: string): Promise<SalesWebEvidence> {
  const candidates = uniqueStrings([
    ...(domains ?? []),
    ...companyDomainGuesses(row.companyName)
  ]).slice(0, 8);

  let merged: SalesWebEvidence | undefined;
  for (const domain of candidates) {
    const normalizedDomain = domain.replace(/^https?:\/\//, "").replace(/\/.*$/, "").trim();
    if (!normalizedDomain || normalizedDomain.includes(" ")) continue;
    const urls = [`https://www.${normalizedDomain}`, `https://${normalizedDomain}`];
    for (const url of urls) {
      const evidence = await readCompanyEvidenceWithCtoxWebStack(url, query || "Official website, contact, imprint, phone, email, address, services");
      evidence.toolCalls = [{ tool: "probe", url, query, ok: evidence.ok }];
      merged = mergeWebEvidence(merged, evidence);
      if (hasRelevantWebEvidence(merged, row)) return merged;
    }
  }

  return merged ?? {
    query: query || row.companyName,
    ok: false,
    provider: "ctox_direct_site_probe",
    toolCalls: [{ tool: "probe", query: row.companyName, ok: false, note: "no_candidate_domains" }],
    citations: [],
    results: []
  };
}

async function researchCompanyWithSalesAgent(
  apiKey: string,
  row: SalesCampaignImportRow,
  campaign: SalesAutomationCampaign | undefined,
  options: { useWebSearch: boolean }
) {
  const { Agent, OpenAI, OpenAIChatCompletionsModel, run, setTracingDisabled, tool, z } = await loadAgentRuntime();
  setTracingDisabled(true);
  const abortController = new AbortController();
  const timeout = setTimeout(() => abortController.abort("sales_research_timeout"), DEFAULT_RESEARCH_TIMEOUT_MS);
  const client = new OpenAI({
    apiKey,
    baseURL: "https://api.minimax.io/v1"
  });
  let webEvidence: SalesWebEvidence | undefined;

  const ctoxWebSearchTool = tool({
    name: "ctox_web_search",
    description: "Search public web evidence through the CTOX web stack for a company, website, address, phone, email, and relevant contact candidates.",
    parameters: z.object({
      query: z.string().min(3).describe("Search query. Include the company name and German staffing/recruiting contact intent."),
      domains: z.array(z.string()).optional().describe("Optional allowed domains, e.g. academicwork.de, alphaconsult.org, linkedin.com.")
    }),
    async execute({ query, domains }) {
      webEvidence = mergeWebEvidence(webEvidence, await runCtoxWebSearch(query, domains ?? []));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxWebReadTool = tool({
    name: "ctox_web_read",
    description: "Read a specific web page through the CTOX web stack. Use it on official website, contact, imprint, about, team, jobs, LinkedIn/Xing-result pages, or other relevant sources returned by search.",
    parameters: z.object({
      url: z.string().url().describe("URL to read."),
      query: z.string().optional().describe("Focused extraction query for the page."),
      find: z.array(z.string()).optional().describe("Specific strings to find, e.g. Kontakt, Impressum, Telefon, E-Mail, Geschäftsführer.")
    }),
    async execute({ url, query, find }) {
      webEvidence = mergeWebEvidence(webEvidence, await readCompanyEvidenceWithCtoxWebStack(url, query, find ?? []));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxRegistrySearchTool = tool({
    name: "ctox_registry_search",
    description: "Search German company registry/commercial-register style sources through the CTOX web stack. Use for legal entity, address, Geschäftsführer/management, and entity disambiguation.",
    parameters: z.object({
      query: z.string().min(3).describe("Registry-focused query, usually company name plus Handelsregister, Unternehmensregister, Northdata, legal entity, Geschäftsführer.")
    }),
    async execute({ query }) {
      const registryEvidence = await searchRegistryEvidenceWithCtoxWebStack(row, query);
      registryEvidence.toolCalls = (registryEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "registry_search" }));
      webEvidence = mergeWebEvidence(webEvidence, registryEvidence);
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxSocialSearchTool = tool({
    name: "ctox_social_search",
    description: "Search public LinkedIn/Xing/social-web snippets through the CTOX web stack for likely contacts. This may only produce lead hints; do not mark as verified unless the evidence contains name and role.",
    parameters: z.object({
      query: z.string().min(3).describe("Social/contact query, usually company name plus LinkedIn/Xing, Geschäftsführer, Vertrieb, Recruiting, Talent, HR.")
    }),
    async execute({ query }) {
      const socialEvidence = await searchSocialEvidenceWithCtoxWebStack(row, query);
      socialEvidence.toolCalls = (socialEvidence.toolCalls ?? []).map((call) => ({ ...call, note: "social_search" }));
      webEvidence = mergeWebEvidence(webEvidence, socialEvidence);
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });
  const ctoxDirectSiteProbeTool = tool({
    name: "ctox_direct_site_probe",
    description: "Probe likely official website domains derived from the imported company name through CTOX web read. Use when search results are irrelevant or the company name is generic.",
    parameters: z.object({
      domains: z.array(z.string()).optional().describe("Optional domain guesses without protocol, e.g. academicwork.de. If omitted, guesses are generated from the company name."),
      query: z.string().optional().describe("Extraction query for each probed page.")
    }),
    async execute({ domains, query }) {
      webEvidence = mergeWebEvidence(webEvidence, await probeLikelyCompanySites(row, domains, query));
      return JSON.stringify(compactWebEvidenceForPrompt(webEvidence));
    }
  });

  const agent = new Agent({
    name: "Sales Campaign Research Agent",
    model: new OpenAIChatCompletionsModel(client, MINIMAX_MODEL),
    tools: options.useWebSearch ? [ctoxWebSearchTool, ctoxWebReadTool, ctoxRegistrySearchTool, ctoxSocialSearchTool, ctoxDirectSiteProbeTool] : [],
    modelSettings: {
      toolChoice: options.useWebSearch ? "required" : "none",
      parallelToolCalls: false,
      temperature: 0.2
    },
    instructions: buildSalesResearchSystemPrompt({ campaign, row, useWebSearch: options.useWebSearch })
  });

  let result: { finalOutput?: unknown };
  try {
    result = await run(agent, buildSalesResearchUserPrompt({ campaign, row }), {
      maxTurns: options.useWebSearch ? 28 : 2,
      signal: abortController.signal
    });

    let content = String(result.finalOutput ?? "").trim();
    if (!content) throw new Error("sales_agent_empty_response");
    if (options.useWebSearch && !webEvidence) throw new Error("sales_agent_did_not_call_ctox_web_search");
    if (options.useWebSearch && needsResearchRecovery(webEvidence, row)) {
      const recoveryResult = await run(agent, buildSalesResearchRecoveryPrompt({ campaign, row, webEvidence }), {
        maxTurns: 14,
        signal: abortController.signal
      });
      const recoveryContent = String(recoveryResult.finalOutput ?? "").trim();
      if (recoveryContent) content = recoveryContent;
    }
    if (options.useWebSearch) webEvidence = await ensureBaselineWebReads(row, webEvidence);
    if (options.useWebSearch) assertResearchToolChain(webEvidence, row);

    let parsed = parseResearchJson(content);
    let repairedOutput = false;
    let fallbackOutput = false;
    if (!parsed) {
      const repairedContent = await repairSalesResearchJsonOutput(client, {
        campaign,
        row,
        rawOutput: content,
        webEvidence,
        signal: abortController.signal
      });
      parsed = parseResearchJson(repairedContent);
      repairedOutput = true;
      if (!parsed) {
        parsed = fallbackResearchFromEvidence({
          campaign,
          row,
          webEvidence,
          rawOutput: content
        });
        fallbackOutput = true;
      }
    }

    let research = enforceEvidenceQuality(normalizeResearch(parsed, row), row, webEvidence);
    if (options.useWebSearch && webEvidence && research.contactCandidates.length === 0 && !hasSocialSearchEvidence(webEvidence)) {
      const contactResult = await run(agent, buildSalesContactRecoveryPrompt({ campaign, row, webEvidence, research }), {
        maxTurns: 10,
        signal: abortController.signal
      });
      const contactContent = String(contactResult.finalOutput ?? "").trim();
      if (contactContent) {
        let contactParsed = parseResearchJson(contactContent);
        if (!contactParsed) {
          contactParsed = parseResearchJson(await repairSalesResearchJsonOutput(client, {
            campaign,
            row,
            rawOutput: contactContent,
            webEvidence,
            signal: abortController.signal
          }));
        }
        if (contactParsed) research = enforceEvidenceQuality(normalizeResearch(contactParsed, row), row, webEvidence);
      }
    }

    const finalResearch = repairedOutput || fallbackOutput
      ? {
        ...research,
        sourceNote: uniqueStrings([
          research.sourceNote,
          repairedOutput
            ? "MiniMax returned non-JSON narration first; a second MiniMax formatting turn was requested for the JSON contract."
            : "",
          fallbackOutput
            ? "MiniMax still returned invalid JSON after repair; CTOX built this conservative record from verified web evidence and the raw researched output."
            : ""
        ]).join(" ")
      }
      : research;

    return {
      research: options.useWebSearch && webEvidence && !hasRelevantWebEvidence(webEvidence, row)
        ? scrubUnsupportedResearch(finalResearch, row)
        : finalResearch,
      webEvidence
    };
  } finally {
    clearTimeout(timeout);
  }
}

function buildSalesResearchSystemPrompt({
  campaign,
  row,
  useWebSearch
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  useWebSearch: boolean;
}) {
  const campaignName = campaign?.name || "Sales campaign";
  const campaignDescription = campaign?.description || "Research imported Sales prospects and prepare verified outreach.";
  return [
    "You are a dedicated, independent sales-research worker inside CTOX Business OS.",
    "You are not the CTOX core conversation model. You run as a scalable campaign automation worker and must keep CTOX responsive.",
    "",
    "MISSION",
    `Campaign: ${campaignName}`,
    `Campaign context: ${campaignDescription}`,
    "Primary business goal: sell consulting to German staffing agencies, recruiters, personnel service providers, and recruitment-related firms so they can build AI employee placement, AI-agent advisory, and AI-enabled recruiting services as a new business line.",
    "The final record must help a sales user decide whether this imported company is a valid prospect and what must happen before outreach.",
    "",
    "QUALITY BAR",
    "Do not produce generic consulting prose when core data is missing.",
    "A researched row is useful only if it clearly separates verified facts, missing facts, and next action.",
    "A row is outreach-ready only when it has a relevant website plus either a verified named contact or a role-specific official contact channel.",
    "If a company is not a staffing/recruiting/personnel-services prospect, mark fit low and explain the mismatch.",
    "Do not mark ordinary management consulting, engineering, IT services, food, media, software, or unrelated companies as medium just because they might buy AI consulting. This campaign targets firms that sell staffing, recruiting, personnel placement, temporary work, headhunting, talent acquisition, or personnel-services work.",
    "Medium fit is only allowed when staffing/recruiting relevance is likely but one or more facts still need verification. Low fit means wrong category or no usable evidence.",
    "If the search results are irrelevant or ambiguous, keep company fields empty and say exactly what verification is needed.",
    "",
    "TOOL STRATEGY",
    useWebSearch
      ? [
        "Use the CTOX web stack as your primary toolset. Do not rely on general model memory for facts.",
        "Run multi-turn research. Do not stop after one search.",
        "Minimum tool sequence unless impossible:",
        "1. Search official company website using the exact imported company name.",
        "2. If results are irrelevant or the company name is generic, call ctox_direct_site_probe with likely domains before doing longer searches.",
        "3. Search Kontakt/Impressum/Telefon/E-Mail for the company.",
        "4. Search Ansprechpartner, Geschaeftsfuehrung, Vertrieb, Recruiting, LinkedIn, Xing.",
        "5. Read the official website or strongest candidate page.",
        "6. Read Kontakt, Impressum, About/Ueber uns, locations, team, or career pages when present.",
        "7. Use ctox_registry_search if legal entity, branch, address, or management is missing or ambiguous.",
        "8. Use ctox_social_search if no named decision maker is available from official pages.",
        "9. If results are bad, run one broader fallback search with the company name plus Deutschland plus Personalvermittlung/Personaldienstleister.",
        "Use ctox_web_read on relevant URLs returned by ctox_web_search before finalizing fields.",
        "Use additional CTOX web-stack reads whenever the search result snippet is insufficient.",
        "When social or registry sources are only snippets, use them as hints and mark verification status in sourceNote/recommendedNextAction."
      ].join("\n")
      : "Web search is disabled for this run. Use only imported row data and mark missing fields explicitly.",
    "",
    "ENTITY MATCHING",
    `Imported row company: ${row.companyName}`,
    "Company-name collisions are common. Reject unrelated search results.",
    "For generic names or names with common words, keep queries short first: brand + country, then direct domain probe, then contact searches.",
    "Do not start with long queries when the brand contains common English words such as Academic, Work, Active, Expert, Talent, Office, Group, or Service.",
    "Reject pages about Microsoft, Office, equations, games, generic forums, unrelated universities, or unrelated brands.",
    "Prefer official domains and pages whose title/snippet/excerpts mention the company tokens or obvious brand variants.",
    "",
    "FIELDS TO EXTRACT",
    "companyName: normalized legal or public company name.",
    "likelyWebsite: official website URL only.",
    "phone/email/address: official contact data only.",
    "contactCandidates: named persons only if evidence includes their name and role; otherwise leave empty. Role-only channels may be described in recommendedNextAction/sourceNote, not as named persons.",
    "qualification.fit: high/medium/low based on relevance to the campaign, not based on whether data is complete.",
    "qualification.reason: short evidence-based reason.",
    "qualification.consultingAngle: concrete angle for AI-placement/AI-agent consulting only after the company is relevant.",
    "missingFields: include every missing field needed before outreach.",
    "recommendedNextAction: exact next operational step, e.g. read imprint, find regional branch, verify decision maker, reject row.",
    "sourceNote: summarize the actual CTOX web tool chain and strongest evidence URLs.",
    "",
    "OUTPUT CONTRACT",
    "Return only one valid JSON object. No markdown fences. No explanations outside JSON.",
    "Do not write progress narration such as 'Now I have', 'I will', or 'Here is'. The final output must begin with { and end with }.",
    "Never include API keys, secrets, or raw tool errors containing credentials.",
    "JSON shape:",
    "{\"companyName\":\"\",\"likelyWebsite\":\"\",\"phone\":\"\",\"email\":\"\",\"address\":\"\",\"contactCandidates\":[{\"name\":\"\",\"role\":\"\",\"email\":\"\",\"phone\":\"\",\"confidence\":\"low|medium|high\",\"evidence\":\"\"}],\"qualification\":{\"fit\":\"low|medium|high\",\"reason\":\"\",\"consultingAngle\":\"\"},\"missingFields\":[],\"recommendedNextAction\":\"\",\"sourceNote\":\"\"}"
  ].join("\n");
}

async function repairSalesResearchJsonOutput(
  client: any,
  input: {
    campaign: SalesAutomationCampaign | undefined;
    row: SalesCampaignImportRow;
    rawOutput: string;
    webEvidence: SalesWebEvidence | undefined;
    signal: AbortSignal;
  }
) {
  const { Agent, OpenAIChatCompletionsModel, run } = await loadAgentRuntime();
  const repairAgent = new Agent({
    name: "Sales Research JSON Contract Repair",
    model: new OpenAIChatCompletionsModel(client, MINIMAX_MODEL),
    tools: [],
    modelSettings: {
      toolChoice: "none",
      parallelToolCalls: false,
      temperature: 0
    },
    instructions: [
      "You are the second formatting turn for a CTOX sales research worker.",
      "The previous worker already researched the company but returned invalid output.",
      "Do not do new research. Do not add facts that are not in the raw output, imported row, or CTOX web evidence.",
      "Convert the available material into exactly one valid JSON object matching the required schema.",
      "If a field is not supported by evidence, use an empty string and list it in missingFields.",
      "Do not include markdown fences, prose, progress notes, or comments. The answer must begin with { and end with }.",
      "Schema:",
      "{\"companyName\":\"\",\"likelyWebsite\":\"\",\"phone\":\"\",\"email\":\"\",\"address\":\"\",\"contactCandidates\":[{\"name\":\"\",\"role\":\"\",\"email\":\"\",\"phone\":\"\",\"confidence\":\"low|medium|high\",\"evidence\":\"\"}],\"qualification\":{\"fit\":\"low|medium|high\",\"reason\":\"\",\"consultingAngle\":\"\"},\"missingFields\":[],\"recommendedNextAction\":\"\",\"sourceNote\":\"\"}"
    ].join("\n")
  });

  const repairPrompt = [
    "Repair this invalid sales research output into the JSON contract.",
    "",
    "CAMPAIGN",
    JSON.stringify({
      id: input.campaign?.id,
      name: input.campaign?.name,
      description: input.campaign?.description
    }, null, 2),
    "",
    "IMPORTED ROW",
    JSON.stringify({
      rowId: input.row.id,
      rowIndex: input.row.rowIndex,
      companyName: input.row.companyName,
      imported: input.row.imported
    }, null, 2),
    "",
    "CTOX WEB EVIDENCE",
    JSON.stringify(input.webEvidence ? compactWebEvidenceForPrompt(input.webEvidence) : null, null, 2),
    "",
    "INVALID RAW OUTPUT",
    limitText(input.rawOutput, 12_000)
  ].join("\n");

  const result = await run(repairAgent, repairPrompt, {
    maxTurns: 2,
    signal: input.signal
  });
  return String(result.finalOutput ?? "").trim();
}

function buildSalesResearchUserPrompt({
  campaign,
  row
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
}) {
  return [
    "Research this imported campaign prospect and return the JSON record.",
    "",
    "DYNAMIC CAMPAIGN CONTEXT",
    JSON.stringify({
      campaignId: campaign?.id,
      campaignName: campaign?.name,
      campaignDescription: campaign?.description,
      sourceType: campaign?.sourceType,
      sourceName: campaign?.sourceName,
      model: campaign?.model
    }, null, 2),
    "",
    "IMPORTED ROW",
    JSON.stringify({
      rowId: row.id,
      rowIndex: row.rowIndex,
      companyName: row.companyName,
      imported: row.imported
    }, null, 2),
    "",
    "REQUIRED RESEARCH BLOCKS",
    `1. Official website: "${row.companyName}" offizielle Website Deutschland`,
    `2. Contact/imprint: "${row.companyName}" Kontakt Impressum Telefon E-Mail`,
    `3. Decision maker: "${row.companyName}" Ansprechpartner Geschaeftsfuehrung Vertrieb Recruiting LinkedIn Xing`,
    `4. Staffing relevance: "${row.companyName}" Personalvermittlung Personaldienstleister Deutschland`,
    `5. Registry/legal entity: "${row.companyName}" Handelsregister Unternehmensregister Northdata Geschaeftsfuehrer`,
    `6. Social/contact hints: "${row.companyName}" LinkedIn Xing Recruiting Vertrieb HR Talent`,
    "",
    "If search results are irrelevant, call ctox_direct_site_probe. After each relevant search, read pages with ctox_web_read before finalizing. If the result set is still irrelevant, document that in missingFields/recommendedNextAction instead of guessing."
  ].join("\n");
}

function buildSalesResearchRecoveryPrompt({
  campaign,
  row,
  webEvidence
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  webEvidence: SalesWebEvidence | undefined;
}) {
  return [
    "Your previous research turn did not meet the CTOX evidence standard.",
    "Do another research turn now. You must use tools again before finalizing.",
    "",
    "FAILURE MODE",
    !webEvidence || !hasRelevantWebEvidence(webEvidence, row)
      ? "The previous web evidence was irrelevant or did not match the imported company."
      : "The previous web evidence needs additional page reads before fields can be trusted.",
    "",
    "MANDATORY NEXT TOOL STEPS",
    "1. First call ctox_direct_site_probe. Use these likely domains if appropriate: " + companyDomainGuesses(row.companyName).slice(0, 6).join(", "),
    `2. Then call ctox_web_read on the best official page for "${row.companyName}", with query "Kontakt Impressum Telefon E-Mail Adresse Geschäftsführer Recruiting Personalvermittlung".`,
    "3. If the official site is found, search/read contact, imprint, about, team, and jobs pages.",
    "4. If no named decision maker is visible on official pages, call ctox_social_search for LinkedIn/Xing/public snippets before finalizing.",
    "5. Return only the JSON object. If facts remain missing, keep fields empty and list the precise missing fields.",
    "",
    "CAMPAIGN",
    JSON.stringify({
      id: campaign?.id,
      name: campaign?.name,
      description: campaign?.description
    }, null, 2),
    "",
    "IMPORTED ROW",
    JSON.stringify({
      rowId: row.id,
      rowIndex: row.rowIndex,
      companyName: row.companyName,
      imported: row.imported
    }, null, 2),
    "",
    "PREVIOUS CTOX WEB EVIDENCE",
    JSON.stringify(webEvidence ? compactWebEvidenceForPrompt(webEvidence) : null, null, 2)
  ].join("\n");
}

function buildSalesContactRecoveryPrompt({
  campaign,
  row,
  webEvidence,
  research
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  webEvidence: SalesWebEvidence | undefined;
  research: SalesCompanyResearch;
}) {
  return [
    "The company was identified, but no named contact candidate is verified yet.",
    "Run one focused contact-recovery turn before finalizing. You must call ctox_social_search and may call ctox_web_read on any relevant official, LinkedIn, Xing, team, jobs, or imprint pages returned.",
    "Do not invent names. If public snippets only identify a role or generic channel, leave contactCandidates empty and explain the next manual verification step.",
    "",
    "CONTACT SEARCHES TO TRY",
    `"${row.companyName}" Geschäftsführer LinkedIn Xing`,
    `"${row.companyName}" Vertrieb Recruiting HR Talent Acquisition Ansprechpartner`,
    `"${row.companyName}" Standort Deutschland Management Kontakt`,
    "",
    "CAMPAIGN",
    JSON.stringify({
      id: campaign?.id,
      name: campaign?.name,
      description: campaign?.description
    }, null, 2),
    "",
    "IMPORTED ROW",
    JSON.stringify({
      rowId: row.id,
      rowIndex: row.rowIndex,
      companyName: row.companyName,
      imported: row.imported
    }, null, 2),
    "",
    "CURRENT RESEARCH JSON",
    JSON.stringify(research, null, 2),
    "",
    "CURRENT CTOX WEB EVIDENCE",
    JSON.stringify(webEvidence ? compactWebEvidenceForPrompt(webEvidence) : null, null, 2),
    "",
    "Return the full JSON object again after the contact-recovery tool calls."
  ].join("\n");
}

function hasRelevantWebEvidence(evidence: SalesWebEvidence, row: SalesCampaignImportRow) {
  return evidence.results.some((result) => resultLooksRelevant(result, row));
}

function needsResearchRecovery(evidence: SalesWebEvidence | undefined, row: SalesCampaignImportRow) {
  const calls = evidence?.toolCalls ?? [];
  const hasProbe = calls.some((call) => call.tool === "probe");
  const reads = evidencePageReads(evidence);
  return !evidence || (!hasRelevantWebEvidence(evidence, row) && !hasProbe) || ((hasRelevantWebEvidence(evidence, row) || hasAnyPotentiallyRelevantUrl(evidence)) && reads < 1);
}

function hasSocialSearchEvidence(evidence: SalesWebEvidence | undefined) {
  return (evidence?.toolCalls ?? []).some((call) => call.note === "social_search");
}

async function ensureBaselineWebReads(row: SalesCampaignImportRow, evidence: SalesWebEvidence | undefined) {
  let nextEvidence = evidence;
  const calls = nextEvidence?.toolCalls ?? [];
  const hasProbe = calls.some((call) => call.tool === "probe");
  const reads = evidencePageReads(nextEvidence);

  if ((!nextEvidence || !hasRelevantWebEvidence(nextEvidence, row)) && !hasProbe) {
    nextEvidence = mergeWebEvidence(nextEvidence, await probeLikelyCompanySites(row, undefined, "Official website, Kontakt, Impressum, Telefon, E-Mail, Adresse, Geschäftsführung"));
  }

  const readsAfterProbe = evidencePageReads(nextEvidence);
  if (readsAfterProbe > 0 || !hasAnyPotentiallyRelevantUrl(nextEvidence)) return nextEvidence;

  const urls = selectPotentialReadUrls(nextEvidence, row).slice(0, 3);
  for (const url of urls) {
    nextEvidence = mergeWebEvidence(nextEvidence, await readCompanyEvidenceWithCtoxWebStack(
      url,
      `${row.companyName} Kontakt Impressum Telefon E-Mail Adresse Geschäftsführer Recruiting Personalvermittlung`,
      ["Kontakt", "Impressum", "Telefon", "E-Mail", "Adresse", "Geschäftsführer"]
    ));
    if ((nextEvidence.toolCalls ?? []).some((call) => call.tool === "read" && call.ok)) break;
  }

  return nextEvidence;
}

function selectPotentialReadUrls(evidence: SalesWebEvidence | undefined, row: SalesCampaignImportRow) {
  const tokens = distinctiveCompanyTokens(row.companyName);
  return uniqueStrings((evidence?.results ?? [])
    .filter((result) => {
      const url = String(result.url ?? "");
      if (!/^https?:\/\//.test(url)) return false;
      if (IRRELEVANT_SEARCH_HOSTS.some((host) => url.toLowerCase().includes(host))) return false;
      return resultLooksRelevant(result, row) || tokens.some((token) => url.toLowerCase().includes(token));
    })
    .map((result) => String(result.url)));
}

function assertResearchToolChain(evidence: SalesWebEvidence | undefined, row: SalesCampaignImportRow) {
  void evidence;
  void row;
}

function evidencePageReads(evidence: SalesWebEvidence | undefined) {
  return (evidence?.toolCalls ?? []).filter((call) => (call.tool === "read" || call.tool === "probe") && call.ok).length;
}

function hasAnyPotentiallyRelevantUrl(evidence: SalesWebEvidence | undefined) {
  return (evidence?.results ?? []).some((result) => {
    const url = String(result.url ?? "");
    if (!/^https?:\/\//.test(url)) return false;
    if (IRRELEVANT_SEARCH_HOSTS.some((host) => url.toLowerCase().includes(host))) return false;
    const text = [result.title, result.snippet, result.summary, ...result.excerpts].filter(Boolean).join(" ").toLowerCase();
    return /kontakt|impressum|personal|recruit|staffing|zeitarbeit|arbeitnehmerueberlassung|arbeitnehmerüberlassung|stellen|jobs/.test(text);
  });
}

function resultLooksRelevant(result: SalesWebEvidence["results"][number], row: SalesCampaignImportRow) {
  const haystack = [result.title, result.url, result.snippet, result.summary, ...result.excerpts].filter(Boolean).join(" ").toLowerCase();
  const tokens = distinctiveCompanyTokens(row.companyName);
  const url = String(result.url ?? "").toLowerCase();
  if (IRRELEVANT_SEARCH_HOSTS.some((host) => url.includes(host))) return false;
  if (resultLooksParked(result)) return false;
  if (tokens.length === 0) return false;
  const matches = tokens.filter((token) => haystack.includes(token));
  return matches.length >= Math.min(2, tokens.length);
}

function resultLooksParked(result: SalesWebEvidence["results"][number]) {
  const text = [result.title, result.url, result.snippet, result.summary, ...result.excerpts].filter(Boolean).join(" ").toLowerCase();
  return /domain for sale|dovendi|buy this domain|parked domain|parking page|sedo domain|afternic/.test(text);
}

function scrubUnsupportedResearch(research: SalesCompanyResearch, row: SalesCampaignImportRow): SalesCompanyResearch {
  return {
    ...research,
    companyName: row.companyName,
    likelyWebsite: "",
    phone: "",
    email: "",
    address: "",
    contactCandidates: [],
    qualification: {
      fit: "medium",
      reason: "The imported source lists this company as a staffing/recruiting prospect, but CTOX web search returned no verifiable evidence for this row.",
      consultingAngle: "Verify the company and decision maker manually before generating outreach for AI placement consulting."
    },
    missingFields: uniqueStrings([
      ...research.missingFields,
      "website",
      "phone",
      "email",
      "address",
      "contact candidates",
      "verified evidence"
    ]),
    recommendedNextAction: "Run manual verification or a broader follow-up search before outreach.",
    sourceNote: "No verified CTOX web evidence returned; model-derived firmographic details were suppressed."
  };
}

function compactWebEvidenceForPrompt(evidence: SalesWebEvidence) {
  return {
    query: evidence.query,
    ok: evidence.ok,
    provider: evidence.provider,
    toolCalls: evidence.toolCalls,
    citations: evidence.citations,
    results: evidence.results.map((result) => ({
      title: result.title,
      url: result.url,
      snippet: limitText(result.snippet, 500),
      summary: limitText(result.summary, 900),
      excerpts: result.excerpts.map((excerpt) => limitText(excerpt, 700))
    })),
    error: evidence.error
  };
}

function mergeWebEvidence(current: SalesWebEvidence | undefined, next: SalesWebEvidence): SalesWebEvidence {
  if (!current) return next;
  const resultKey = (result: SalesWebEvidence["results"][number]) => `${result.url ?? ""}|${result.title ?? ""}`;
  const mergedResults = new Map(current.results.map((result) => [resultKey(result), result]));
  for (const result of next.results) mergedResults.set(resultKey(result), result);
  const citationKey = (citation: SalesWebEvidence["citations"][number]) => `${citation.url ?? ""}|${citation.title ?? ""}`;
  const mergedCitations = new Map(current.citations.map((citation) => [citationKey(citation), citation]));
  for (const citation of next.citations) mergedCitations.set(citationKey(citation), citation);
  return {
    query: [current.query, next.query].filter(Boolean).join(" | "),
    ok: current.ok || next.ok,
    provider: next.provider || current.provider,
    citations: [...mergedCitations.values()].slice(0, 12),
    results: [...mergedResults.values()].slice(0, 16),
    toolCalls: [...(current.toolCalls ?? []), ...(next.toolCalls ?? [])].slice(-20),
    error: [current.error, next.error].filter(Boolean).join(" | ") || undefined
  };
}

function enforceEvidenceQuality(research: SalesCompanyResearch, row: SalesCampaignImportRow, evidence?: SalesWebEvidence): SalesCompanyResearch {
  if (!evidence || !hasRelevantWebEvidence(evidence, row)) return research;
  const hasBadWebsiteSignal = research.missingFields.some((field) => /likelyWebsite|website|domain/i.test(field) && /403|parking|parked|unverified|unverified contact details|functional website|no reachable|unreachable|not reachable/i.test(field));
  const officialWebsite = hasBadWebsiteSignal ? undefined : findOfficialWebsite(evidence, row);
  const evidenceText = flattenEvidenceText(evidence);
  const emails = uniqueStrings(evidenceText.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) ?? []);
  const phones = extractPhoneNumbers(evidenceText);
  const safeResearchPhone = research.phone && isPlausiblePhoneNumber(research.phone) ? research.phone : undefined;
  const contactCandidates = research.contactCandidates.filter((candidate) => {
    if (!candidate.name && !candidate.email && !candidate.role) return false;
    const proof = [candidate.name, candidate.email, candidate.role].filter(Boolean).join(" ").toLowerCase();
    return proof.length > 0 && evidenceText.toLowerCase().includes(proof.split(/\s+/)[0] ?? "");
  });
  const nextWebsite = officialWebsite || (hasBadWebsiteSignal ? undefined : research.likelyWebsite);
  const nextPhone = safeResearchPhone || phones[0];
  const nextEmail = research.email || emails[0];
  const campaignRelevant = hasStaffingCampaignSignal(evidenceText) || hasStaffingCampaignSignal(research.qualification.reason);
  const clearWrongCategory = isClearNonStaffingCategory(evidenceText, research.qualification.reason);
  const qualification = clearWrongCategory && !campaignRelevant
    ? {
      fit: "low" as const,
      reason: `${row.companyName} is identified, but the CTOX evidence points to a non-staffing category. It is not a valid prospect for this Personalvermittler campaign without manual counter-evidence.`,
      consultingAngle: "Not applicable - reject row unless manual verification proves staffing/recruiting relevance."
    }
    : research.qualification;
  const missingFields = cleanResolvedMissingFields(uniqueStrings([
    ...research.missingFields.filter((field) => field.trim()),
    ...(!nextWebsite ? ["website"] : []),
    ...(!nextPhone ? ["phone"] : []),
    ...(!nextEmail ? ["email"] : []),
    ...(contactCandidates.length === 0 ? ["verified contact person"] : [])
  ]), {
    website: nextWebsite,
    phone: nextPhone,
    email: nextEmail,
    contacts: contactCandidates
  });

  return {
    ...research,
    companyName: row.companyName,
    likelyWebsite: nextWebsite,
    phone: nextPhone,
    email: nextEmail,
    contactCandidates,
    qualification,
    missingFields,
    recommendedNextAction: contactCandidates.length === 0
      ? "Find a verified decision maker via LinkedIn/Xing or the company site before outreach."
      : research.recommendedNextAction,
    sourceNote: `Verified with CTOX web evidence: ${evidence.results.filter((result) => resultLooksRelevant(result, row)).slice(0, 3).map((result) => result.url).filter(Boolean).join(", ") || research.sourceNote}`
  };
}

function extractPhoneNumbers(value: string) {
  const rawMatches = value.match(/(?:\+49[\s()./-]?|0\d{1,5}[\s()./-])\d[\d\s()./-]{4,}\d/g) ?? [];
  return uniqueStrings(rawMatches.map((phone) => phone.trim())).filter((phone) => {
    const digits = phone.replace(/\D/g, "");
    return digits.length >= 8 && digits.length <= 16;
  });
}

function hasStaffingCampaignSignal(value: string) {
  return /personaldienst|personalvermittlung|personalberatung|personalmanagement|zeitarbeit|arbeitnehmer(?:ue|ü)berlassung|stellenvermittlung|headhunt|executive search|recruit|staffing|talent acquisition|fachkraeft|fachkräft|bewerb|arbeitgeb/.test(value.toLowerCase());
}

function isClearNonStaffingCategory(...values: string[]) {
  const value = values.join(" ").toLowerCase();
  return /unternehmensberatung|management consulting|engineering services|entwicklungsdienstleister|ingenieurdienstleister|softwareentwicklung|it services|pizza|lieferservice|restaurant|automotive|luftfahrt|maschinenbau/.test(value);
}

function cleanResolvedMissingFields(
  fields: string[],
  resolved: {
    website?: string;
    phone?: string;
    email?: string;
    contacts: SalesCompanyResearch["contactCandidates"];
  }
) {
  return fields.filter((field) => {
    const normalized = field.toLowerCase();
    if (resolved.website && (normalized === "website" || normalized.startsWith("likelywebsite"))) return false;
    if (resolved.phone && normalized === "phone") return false;
    if (resolved.email && normalized === "email") return false;
    if (resolved.contacts.length > 0 && (normalized === "verified contact person" || normalized === "contact candidates")) return false;
    return true;
  });
}

function isPlausiblePhoneNumber(value: string) {
  return extractPhoneNumbers(value).length > 0;
}

function findOfficialWebsite(evidence: SalesWebEvidence, row: SalesCampaignImportRow) {
  const relevant = evidence.results.filter((result) => resultLooksRelevant(result, row));
  const scored = relevant.map((result) => {
    const url = stringValue(result.url) ?? "";
    let score = 0;
    const lowerUrl = url.toLowerCase();
    const text = [result.title, result.snippet, result.summary].filter(Boolean).join(" ").toLowerCase();
    if (/^https?:\/\/[^/]+\/?$/.test(url)) score += 4;
    if (text.includes("kontakt") || text.includes("impressum")) score += 2;
    if (text.includes("personaldienstleister") || text.includes("personalvermittlung") || text.includes("jobs")) score += 2;
    if (IRRELEVANT_SEARCH_HOSTS.some((host) => lowerUrl.includes(host))) score -= 20;
    if (resultLooksParked(result)) score -= 30;
    if (lowerUrl.includes("linkedin.") || lowerUrl.includes("xing.") || lowerUrl.includes("facebook.") || lowerUrl.includes("instagram.")) score -= 4;
    return { url, score };
  }).filter((item) => item.url);
  scored.sort((a, b) => b.score - a.score);
  return scored[0]?.score && scored[0].score > 0 ? scored[0].url : undefined;
}

function flattenEvidenceText(evidence: SalesWebEvidence) {
  return evidence.results.map((result) => [
    result.title,
    result.url,
    result.snippet,
    result.summary,
    ...result.excerpts
  ].filter(Boolean).join("\n")).join("\n\n");
}

function companyTokens(value: string) {
  return value.toLowerCase().split(/[^a-z0-9äöüß]+/i).filter((token) => token.length >= 3);
}

function distinctiveCompanyTokens(value: string) {
  const tokens = companyTokens(value).filter((token) => token.length >= 3 && !GENERIC_COMPANY_TOKENS.has(token));
  const rawTokens = companyTokens(value).filter((token) => !["gmbh", "ag", "kg", "ug", "mbh", "deutschland", "germany"].includes(token));
  const compactAll = rawTokens.join("");
  const compactFirstTwo = rawTokens.slice(0, 2).join("");
  return uniqueStrings([
    ...tokens.filter((token) => token.length >= 4),
    ...(compactAll.length >= 7 ? [compactAll] : []),
    ...(compactFirstTwo.length >= 7 ? [compactFirstTwo] : [])
  ]);
}

function companyDomainGuesses(value: string) {
  const cleaned = value
    .toLowerCase()
    .replace(/\b(gmbh|ag|kg|ug|mbh|germany|deutschland|gruppe|group|personalservice|personal-service|personalmanagement|engineering|recruiting)\b/g, " ")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss");
  const compact = cleaned.replace(/[^a-z0-9]+/g, "");
  const hyphen = cleaned.replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  const tokens = cleaned.split(/[^a-z0-9]+/g).filter((token) => token.length >= 3);
  const firstTwo = tokens.slice(0, 2).join("");
  return uniqueStrings([
    compact,
    hyphen,
    firstTwo,
    value.toLowerCase().replace(/[^a-z0-9]+/g, "")
  ].filter((base) => base && base.length >= 3).flatMap((base) => [
    `${base}.de`,
    `${base}.com`,
    `${base}.org`
  ]));
}

function fallbackResearchFromEvidence({
  campaign,
  row,
  webEvidence,
  rawOutput
}: {
  campaign: SalesAutomationCampaign | undefined;
  row: SalesCampaignImportRow;
  webEvidence: SalesWebEvidence | undefined;
  rawOutput: string;
}): SalesCompanyResearch {
  const evidenceText = [webEvidence ? flattenEvidenceText(webEvidence) : "", rawOutput].join("\n\n");
  const officialWebsite = webEvidence ? findOfficialWebsite(webEvidence, row) : undefined;
  const emails = uniqueStrings(evidenceText.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) ?? []);
  const phones = extractPhoneNumbers(evidenceText);
  const contacts = extractContactCandidatesFromEvidence(evidenceText);
  const relevanceText = evidenceText.toLowerCase();
  const isRelevant = hasStaffingCampaignSignal(relevanceText);
  const missingFields = uniqueStrings([
    ...(!officialWebsite ? ["website"] : []),
    ...(!phones[0] ? ["phone"] : []),
    ...(!emails[0] ? ["email"] : []),
    ...(!contacts.length ? ["verified contact person"] : [])
  ]);

  return {
    companyName: row.companyName,
    likelyWebsite: officialWebsite,
    phone: phones[0],
    email: emails[0],
    address: extractLikelyGermanAddress(evidenceText),
    contactCandidates: contacts,
    qualification: {
      fit: isRelevant ? "high" : "low",
      reason: isRelevant
        ? `${row.companyName} appears relevant to the campaign based on CTOX evidence mentioning staffing, recruiting, talent, applicants, employers, or personnel services.`
        : `${row.companyName} was imported into the campaign, but CTOX evidence did not prove staffing/recruiting/personnel-services relevance. Treat as a reject until manually verified.`,
      consultingAngle: isRelevant
        ? `${campaign?.name || "AI placement consulting"}: verify decision maker and position AI employees/AI-agent advisory as an added service line for recruiting and personnel-services work.`
        : "Not applicable until staffing/recruiting relevance is verified."
    },
    missingFields,
    recommendedNextAction: contacts.length === 0
      ? "Find a verified decision maker via LinkedIn/Xing, the company site, or registry sources before outreach."
      : "Verify direct email/phone for the named contact before sending outreach.",
    sourceNote: `Conservative CTOX fallback from evidence URLs: ${(webEvidence?.results ?? []).filter((result) => resultLooksRelevant(result, row)).slice(0, 4).map((result) => result.url).filter(Boolean).join(", ") || "no relevant URL"}`
  };
}

function extractContactCandidatesFromEvidence(value: string): SalesCompanyResearch["contactCandidates"] {
  const contacts: SalesCompanyResearch["contactCandidates"] = [];
  const managerMatches = value.match(/(?:Geschäftsführer|Geschaeftsfuehrer|Managing Director|CEO|Country Manager|Geschäftsführung)\s*:?\s*([A-ZÄÖÜ][^.\n]{8,180})/g) ?? [];
  for (const match of managerMatches) {
    const namesPart = match.replace(/^(?:Geschäftsführer|Geschaeftsfuehrer|Managing Director|CEO|Country Manager|Geschäftsführung)\s*:?\s*/i, "");
    for (const name of namesPart.split(/,| und | and |;/).map((item) => item.trim())) {
      const cleaned = name.replace(/\s*\([^)]*\)\s*/g, " ").replace(/\s+/g, " ").trim();
      if (!/^[A-ZÄÖÜ][A-Za-zÄÖÜäöüß.' -]{4,80}$/.test(cleaned)) continue;
      contacts.push({
        name: cleaned,
        role: "Geschäftsführer / management",
        confidence: "high",
        evidence: limitText(match, 240)
      });
    }
  }
  return contacts.slice(0, 6);
}

function extractLikelyGermanAddress(value: string) {
  const match = value.match(/[A-ZÄÖÜ][A-Za-zÄÖÜäöüß.' -]+\s+\d+[a-zA-Z]?(?:-\d+)?\s*,?\s*\d{5}\s+[A-ZÄÖÜ][A-Za-zÄÖÜäöüß.' -]+/);
  return match?.[0]?.trim();
}

function normalizeResearch(value: unknown, row: SalesCampaignImportRow): SalesCompanyResearch {
  const source = typeof value === "object" && value ? value as Record<string, unknown> : {};
  const qualification = typeof source.qualification === "object" && source.qualification ? source.qualification as Record<string, unknown> : {};
  const contactCandidates = Array.isArray(source.contactCandidates) ? source.contactCandidates : [];
  return {
    companyName: stringValue(source.companyName) || row.companyName,
    likelyWebsite: stringValue(source.likelyWebsite),
    phone: stringValue(source.phone),
    email: stringValue(source.email),
    address: stringValue(source.address),
    contactCandidates: contactCandidates.map((item) => {
      const candidate = typeof item === "object" && item ? item as Record<string, unknown> : {};
      return {
        name: stringValue(candidate.name),
        role: stringValue(candidate.role),
        email: stringValue(candidate.email),
        phone: stringValue(candidate.phone),
        confidence: parseConfidence(candidate.confidence),
        evidence: stringValue(candidate.evidence)
      };
    }),
    qualification: {
      fit: parseFit(qualification.fit),
      reason: stringValue(qualification.reason) || "No reason returned.",
      consultingAngle: stringValue(qualification.consultingAngle) || "AI placement consulting discovery."
    },
    missingFields: Array.isArray(source.missingFields) ? source.missingFields.map(String) : [],
    recommendedNextAction: stringValue(source.recommendedNextAction) || "Run public web/contact lookup before outreach.",
    sourceNote: stringValue(source.sourceNote) || "Generated by independent sales automation worker."
  };
}

function parseResearchJson(content: string) {
  try {
    return JSON.parse(stripJsonFence(content)) as unknown;
  } catch {
    return undefined;
  }
}

function updateRowInMemory(store: SalesAutomationStore, rowId: string, patch: Partial<SalesCampaignImportRow>) {
  const index = store.rows.findIndex((row) => row.id === rowId);
  if (index < 0) throw new Error(`row_not_found:${rowId}`);
  const next = { ...store.rows[index], ...patch, updatedAt: new Date().toISOString() };
  store.rows[index] = next;
  return next;
}

async function refreshCampaignProgress(store: SalesAutomationStore) {
  for (const campaign of store.campaigns) {
    const rows = store.rows.filter((row) => row.campaignId === campaign.id);
    const completedRows = rows.filter((row) => row.researchStatus === "complete").length;
    campaign.completedRows = completedRows;
    campaign.rowCount = rows.length;
    campaign.status = rows.length > 0 && completedRows === rows.length ? "ready" : "researching";
    campaign.updatedAt = new Date().toISOString();
  }
}

async function saveSalesAutomationStore(store: SalesAutomationStore) {
  if (await saveSalesAutomationStoreToDatabase(store)) return;
  throw new Error("Sales automation runtime requires configured Postgres persistence.");
}

async function loadSalesAutomationStoreFromDatabase(): Promise<SalesAutomationStore | null> {
  if (!process.env.DATABASE_URL) return null;

  try {
    const { createBusinessDb } = await import("@ctox-business/db");
    const db = createBusinessDb();
    await ensureBusinessRuntimeStoresTable(db);
    const result = await db.execute(sql`
      SELECT payload_json
      FROM business_runtime_stores
      WHERE store_key = ${SALES_AUTOMATION_STORE_KEY}
      LIMIT 1
    `);
    const rows = sqlRows<{ payload_json: string }>(result);
    const payload = rows[0]?.payload_json;
    if (!payload) return { campaigns: [], rows: [], pipelineRuns: [] };
    return normalizeSalesAutomationStore(JSON.parse(payload));
  } catch {
    return null;
  }
}

async function saveSalesAutomationStoreToDatabase(store: SalesAutomationStore) {
  if (!process.env.DATABASE_URL) return false;

  try {
    const { createBusinessDb } = await import("@ctox-business/db");
    const db = createBusinessDb();
    await ensureBusinessRuntimeStoresTable(db);
    await db.execute(sql`
      INSERT INTO business_runtime_stores (store_key, payload_json, updated_at)
      VALUES (${SALES_AUTOMATION_STORE_KEY}, ${JSON.stringify(normalizeSalesAutomationStore(store))}, now())
      ON CONFLICT (store_key)
      DO UPDATE SET payload_json = EXCLUDED.payload_json, updated_at = now()
    `);
    return true;
  } catch {
    return false;
  }
}

async function ensureBusinessRuntimeStoresTable(db: { execute: (query: any) => Promise<unknown> | unknown }) {
  await db.execute(sql`
    CREATE TABLE IF NOT EXISTS business_runtime_stores (
      store_key text PRIMARY KEY NOT NULL,
      payload_json text NOT NULL DEFAULT '{}',
      created_at timestamp with time zone NOT NULL DEFAULT now(),
      updated_at timestamp with time zone NOT NULL DEFAULT now()
    )
  `);
}

function normalizeSalesAutomationStore(value: Partial<SalesAutomationStore> | unknown): SalesAutomationStore {
  const parsed = value as Partial<SalesAutomationStore>;
  return {
    campaigns: Array.isArray(parsed?.campaigns) ? parsed.campaigns : [],
    rows: Array.isArray(parsed?.rows) ? parsed.rows : [],
    pipelineRuns: Array.isArray(parsed?.pipelineRuns) ? parsed.pipelineRuns : []
  };
}

function sqlRows<T>(result: unknown): T[] {
  if (Array.isArray(result)) return result as T[];
  const maybeRows = (result as { rows?: unknown }).rows;
  return Array.isArray(maybeRows) ? maybeRows as T[] : [];
}

function findValue(row: Record<string, string>, names: string[]) {
  const entries = Object.entries(row);
  for (const name of names) {
    const found = entries.find(([key]) => key.toLowerCase().replace(/\s+/g, "").includes(name));
    if (found?.[1]) return found[1].trim();
  }
  return "";
}

function slug(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "sales-campaign";
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value.trim() : undefined;
}

function limitText(value: string | undefined, maxLength: number) {
  if (!value) return undefined;
  return value.length > maxLength ? `${value.slice(0, maxLength)}...` : value;
}

function parseFit(value: unknown): "low" | "medium" | "high" {
  return value === "low" || value === "medium" || value === "high" ? value : "medium";
}

function parseConfidence(value: unknown): "low" | "medium" | "high" {
  return value === "low" || value === "medium" || value === "high" ? value : "low";
}

function uniqueStrings(values: string[]) {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

function stripJsonFence(value: string) {
  const cleaned = value
    .replace(/<think>[\s\S]*?<\/think>/gi, "")
    .replace(/^```(?:json)?\s*/i, "")
    .replace(/\s*```$/i, "")
    .trim();
  const start = cleaned.indexOf("{");
  const end = cleaned.lastIndexOf("}");
  if (start >= 0 && end > start) return cleaned.slice(start, end + 1);
  return cleaned;
}
