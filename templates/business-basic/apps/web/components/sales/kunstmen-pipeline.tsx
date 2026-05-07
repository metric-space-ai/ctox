import {
  inferSalesPipelineStage,
  scoreSalesPipelineRow,
  type SalesCampaignImportRow,
  type SalesPipelineRun,
  type SalesPipelineRunQuestion,
  type SalesPipelineStageId
} from "../../lib/sales-automation-runtime";
import { SalesQueueButton } from "./actions";

type QueryState = {
  locale?: string;
  mode?: string;
  selectedId?: string;
  theme?: string;
};

type PipelineStage = {
  id: SalesPipelineStageId;
  de: string;
  en: string;
  descriptionDe: string;
  descriptionEn: string;
  exitDe: string[];
  exitEn: string[];
};

type PipelineCard = {
  id: string;
  row: SalesCampaignImportRow;
  stageId: SalesPipelineStageId;
  companyName: string;
  website: string;
  contactName: string;
  contactRole: string;
  contactConfidence: string;
  fit: "low" | "medium" | "high";
  evidenceCount: number;
  missingFields: string[];
  nextStep: string;
  reason: string;
  sourceNote: string;
  score: number;
  run?: SalesPipelineRun;
  openQuestion?: SalesPipelineRunQuestion;
};

const pipelineStages: PipelineStage[] = [
  {
    id: "company",
    de: "Unternehmen identifiziert",
    en: "Company identified",
    descriptionDe: "Fit und Stammdaten sind belastbar genug, aber es fehlt noch ein Ansprechpartner.",
    descriptionEn: "Fit and company data are usable, but no contact is known yet.",
    exitDe: ["passender Personaldienstleister", "Website oder belastbare Quelle", "Ansprechpartner gefunden"],
    exitEn: ["relevant staffing company", "website or reliable source", "contact found"]
  },
  {
    id: "contact",
    de: "Ansprechpartner vorhanden",
    en: "Contact found",
    descriptionDe: "Ein Kontakt ist bekannt, seine Entscheidungsnaehe ist aber noch nicht bestaetigt.",
    descriptionEn: "A contact is known, but decision relevance is not confirmed yet.",
    exitDe: ["Name und Rolle vorhanden", "Entscheidungsnaehe pruefen", "Kontaktweg pruefen"],
    exitEn: ["name and role present", "verify decision relevance", "verify contact route"]
  },
  {
    id: "decision",
    de: "Richtiger Ansprechpartner",
    en: "Right contact",
    descriptionDe: "Der richtige Ansprechpartner ist plausibel, Kontaktweg oder Kontext fehlen noch.",
    descriptionEn: "The right contact is plausible, but contact route or context is still missing.",
    exitDe: ["Entscheider bestaetigt", "Telefon oder E-Mail vorhanden", "Gesprächswinkel klar"],
    exitEn: ["decision maker confirmed", "phone or email present", "conversation angle clear"]
  },
  {
    id: "conversation",
    de: "Gesprächsbereitschaft klaeren",
    en: "Check willingness",
    descriptionDe: "Der naechste Schritt ist die echte Antwort: will die Person ueber das Anliegen sprechen?",
    descriptionEn: "The next step is the real response: will this person discuss the topic?",
    exitDe: ["positives Signal", "Termininteresse", "Einwand dokumentiert"],
    exitEn: ["positive signal", "meeting interest", "objection documented"]
  },
  {
    id: "lead-ready",
    de: "Lead ready",
    en: "Lead ready",
    descriptionDe: "Alle Vorqualifizierungs-Gates sind erfuellt. Uebergabe ins Leads-Modul.",
    descriptionEn: "All pre-qualification gates are fulfilled. Handoff to Leads.",
    exitDe: ["Unternehmen validiert", "richtige Person", "Gespraech zugesagt"],
    exitEn: ["company validated", "right person", "conversation agreed"]
  }
];

export async function KunstmenPipelineView({ query }: { query: QueryState }) {
  const locale = query.locale === "en" ? "en" : "de";
  const store = fallbackPipelineStore;
  const rows = store.rows.length ? store.rows : fallbackPipelineRows;
  const campaign = store.campaigns[0] ?? fallbackPipelineCampaign;
  const cards = buildPipelineCards(rows, store.pipelineRuns ?? []);
  const selected = query.selectedId ? cards.find((card) => card.id === query.selectedId) : undefined;
  const rejected = store.rows.filter((row) => !row.pipeline && !isPipelineCandidate(row)).length;
  const ready = cards.filter((card) => card.stageId === "lead-ready").length;
  const needsContact = cards.filter((card) => card.stageId === "company").length;
  const waiting = cards.filter((card) => card.run?.status === "waiting_for_user").length;
  const running = cards.filter((card) => card.run?.status === "running").length;

  return (
    <section className="kunstmen-pipeline lead-pipeline" data-context-module="sales" data-context-submodule="pipeline">
      <header className="kunstmen-work-header">
        <div className="kunstmen-work-title">
          <h1>{locale === "en" ? "Pipeline" : "Pipeline"}</h1>
          <p>{locale === "en" ? "Pre-qualify campaign candidates before they become leads." : "Kampagnen-Kandidaten vorqualifizieren, bevor sie Leads werden."}</p>
        </div>
        <section className="kunstmen-work-summary" aria-label="Pipeline summary">
          <span><strong>{cards.length}</strong> {locale === "en" ? "active" : "aktiv"}</span>
          <span><strong>{needsContact}</strong> {locale === "en" ? "need contacts" : "ohne Ansprechpartner"}</span>
          <span><strong>{waiting}</strong> {locale === "en" ? "questions" : "Rueckfragen"}</span>
          <span><strong>{running}</strong> {locale === "en" ? "running" : "laufend"}</span>
          <span><strong>{ready}</strong> Lead-ready</span>
          <span><strong>{rejected}</strong> {locale === "en" ? "not qualified" : "nicht qualifiziert"}</span>
          {campaign ? <span>{campaign.sourceName} · {campaign.completedRows}/{campaign.rowCount}</span> : null}
        </section>
        {ready > 0 ? (
          <div className="kunstmen-workspace-actions">
            <SalesQueueButton
              action="sync"
              className="kunstmen-drawer-button"
              instruction="Review all lead-ready Sales pipeline candidates and transfer them into the Leads module with full campaign, research, contact and qualification context."
              payload={{
                campaign,
                rows: cards.filter((card) => card.stageId === "lead-ready").map((card) => ({
                  id: card.id,
                  companyName: card.companyName,
                  website: card.website,
                  contactName: card.contactName,
                  contactRole: card.contactRole,
                  fit: card.fit,
                  nextStep: card.nextStep,
                  score: card.score
                }))
              }}
              recordId="sales-pipeline-ready-handoff"
              resource="leads"
              title="Transfer lead-ready pipeline candidates"
            >
              {locale === "en" ? `${ready} to Leads` : `${ready} an Leads`}
            </SalesQueueButton>
          </div>
        ) : null}
      </header>

      <main className="lead-pipeline-layout">
        <div className="kanban-wrap">
          <p className="kanban-status" aria-live="polite">
            {locale === "en"
              ? "This board is the qualification layer between campaign research and the Leads module."
              : "Dieses Board ist die Qualifizierungsstufe zwischen Kampagnen-Recherche und Leads-Modul."}
          </p>
          <div className="sales-board lead-sales-board" aria-label="Sales lead qualification pipeline">
            {pipelineStages.map((stage) => {
              const stageCards = cards.filter((card) => card.stageId === stage.id);
              return (
                <section className="sales-column" data-stage-id={stage.id} key={stage.id}>
                  <header className="column-head">
                    <div>
                      <h2>{stageLabel(stage, locale)}</h2>
                      <p>{stageDescription(stage, locale)}</p>
                    </div>
                    <span className="column-count">{stageCards.length}</span>
                  </header>
                  <div className="exit-criteria">
                    {stageExit(stage, locale).map((criterion) => <span key={criterion}>{criterion}</span>)}
                  </div>
                  <div className="card-stack">
                    {stageCards.map((card) => <PipelineCardLink card={card} locale={locale} query={query} key={card.id} />)}
                  </div>
                </section>
              );
            })}
          </div>
        </div>
        {selected ? <PipelineInspector card={selected} campaignName={campaign?.name ?? "Campaign"} locale={locale} /> : null}
      </main>
      <script dangerouslySetInnerHTML={{ __html: pipelineRunScript(locale) }} />
    </section>
  );
}

function PipelineCardLink({ card, query, locale }: { card: PipelineCard; query: QueryState; locale: "de" | "en" }) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const isSelected = query.selectedId === card.id;
  if (!isSelected) params.set("selectedId", card.id);
  return (
    <a
      className={`sales-card lead-card ${isSelected ? "selected" : ""}`}
      data-context-item
      data-context-label={card.companyName}
      data-context-module="sales"
      data-context-record-id={card.id}
      data-context-record-type="pipeline-candidate"
      data-context-submodule="pipeline"
      href={`/app/sales/pipeline?${params.toString()}`}
    >
      <span className="card-topline">
        <strong>{card.companyName}</strong>
        <span>{card.fit.toUpperCase()}</span>
      </span>
      <span className="deal-name">{card.website || (locale === "en" ? "Website missing" : "Website fehlt")}</span>
      <span className="card-context">{card.contactName || (locale === "en" ? "No contact yet" : "Noch kein Ansprechpartner")}</span>
      {card.run ? (
        <span className={`pipeline-run-chip status-${card.run.status}`}>
          {card.run.status === "waiting_for_user" ? (locale === "en" ? "needs answer" : "Rueckfrage") : card.run.status}
        </span>
      ) : null}
      <span className="card-next">
        <small>{locale === "en" ? "Next qualification step" : "Naechster Qualifizierungsschritt"}</small>
        {card.nextStep}
      </span>
      <span className={`transition-badge ${card.score >= 75 ? "ready" : card.score >= 45 ? "running" : "blocked"}`}>{card.score}</span>
    </a>
  );
}

function PipelineInspector({ card, campaignName, locale }: { card: PipelineCard; campaignName: string; locale: "de" | "en" }) {
  const missing = card.missingFields.slice(0, 8);
  const candidate = card.row.research?.contactCandidates[0];
  const run = card.run;
  const question = card.openQuestion;
  const activeGate = run?.gates[run.currentGate];
  const toolSteps = pipelineToolSteps(activeGate?.output);
  const databaseRequests = pipelineDatabaseRequests(activeGate?.output);
  const ctoxPayload = {
    candidateId: card.id,
    companyName: card.companyName,
    campaignName,
    stageId: card.stageId,
    website: card.website,
    fit: card.fit,
    contact: candidate,
    missingFields: card.missingFields,
    nextStep: card.nextStep,
    run: run ? {
      id: run.id,
      mode: run.mode,
      status: run.status,
      currentGate: run.currentGate,
      gate: activeGate,
      openQuestion: question,
      toolSteps,
      databaseRequests
    } : undefined
  };
  return (
    <aside className="bottom-drawer open lead-inspector" aria-label={`${card.companyName} details`}>
      <div className="drawer-head">
        <strong>{card.companyName}</strong>
        <a href={`/app/sales/pipeline?locale=${locale}`}>{locale === "en" ? "Close" : "Schliessen"}</a>
      </div>
      <section className="inspector">
        <header className="panel-head">
          <div>
            <h2>{stageLabel(pipelineStages.find((stage) => stage.id === card.stageId) ?? pipelineStages[0], locale)}</h2>
            <p>{campaignName} · #{card.row.rowIndex}</p>
          </div>
          <SalesQueueButton
            action="sync"
            className="button"
            instruction={`Update this Sales pipeline candidate. Use the selected company, campaign research, web evidence, missing fields and current gate to decide the next best action. If the person has agreed to talk, transfer the candidate into the Leads module.`}
            payload={ctoxPayload}
            recordId={card.id}
            resource="pipeline"
            title={`Pipeline candidate: ${card.companyName}`}
          >
            {locale === "en" ? "Ask CTOX" : "CTOX fragen"}
          </SalesQueueButton>
        </header>
        <dl className="compact-defs">
          <dt>{locale === "en" ? "Website" : "Website"}</dt><dd>{card.website || "-"}</dd>
          <dt>{locale === "en" ? "Fit" : "Fit"}</dt><dd>{card.fit}</dd>
          <dt>{locale === "en" ? "Evidence" : "Evidenz"}</dt><dd>{card.evidenceCount} {locale === "en" ? "sources" : "Quellen"}</dd>
          <dt>{locale === "en" ? "Contact" : "Ansprechpartner"}</dt><dd>{card.contactName || "-"}</dd>
          <dt>{locale === "en" ? "Role" : "Rolle"}</dt><dd>{card.contactRole || "-"}</dd>
          <dt>{locale === "en" ? "Confidence" : "Sicherheit"}</dt><dd>{card.contactConfidence || "-"}</dd>
        </dl>
        <section className="automation-panel">
          <header className="section-head">
            <div><h3>{locale === "en" ? "Qualification gate" : "Qualifizierungs-Gate"}</h3><p>{card.nextStep}</p></div>
          </header>
          <p className="gate-ok">{card.reason}</p>
          <p>{card.sourceNote}</p>
          {missing.length > 0 ? <ul className="blocker-list">{missing.map((item) => <li key={item}>{item}</li>)}</ul> : null}
        </section>
        {candidate ? (
          <section className="automation-panel">
            <header className="section-head"><h3>{locale === "en" ? "Best contact candidate" : "Bester Kontaktkandidat"}</h3></header>
            <dl className="compact-defs">
              <dt>Name</dt><dd>{candidate.name || "-"}</dd>
              <dt>{locale === "en" ? "Role" : "Rolle"}</dt><dd>{candidate.role || "-"}</dd>
              <dt>Email</dt><dd>{candidate.email || "-"}</dd>
              <dt>Telefon</dt><dd>{candidate.phone || "-"}</dd>
              <dt>{locale === "en" ? "Evidence" : "Evidenz"}</dt><dd>{candidate.evidence || "-"}</dd>
            </dl>
          </section>
        ) : null}
        <section className="automation-panel pipeline-run-panel">
          <header className="section-head">
            <div>
              <h3>{locale === "en" ? "Candidate automation" : "Kandidaten-Automation"}</h3>
              <p>{run ? `${run.mode} · ${run.currentGate} · ${run.status}` : (locale === "en" ? "No run yet" : "Noch kein Run")}</p>
            </div>
          </header>
          <form data-pipeline-run-start data-candidate-id={card.id}>
            <input name="mode" type="hidden" value="dry_run" />
            <button className="button" type="submit">{run ? (locale === "en" ? "Continue dry run" : "Dry Run fortsetzen") : (locale === "en" ? "Start dry run" : "Dry Run starten")}</button>
          </form>
          {run?.messages.at(-1) ? <p className="gate-ok">{run.messages.at(-1)?.body}</p> : null}
          {toolSteps.length > 0 ? (
            <section className="pipeline-run-evidence" aria-label="Pipeline research steps">
              <strong>{locale === "en" ? "Research blocks" : "Research-Bloecke"}</strong>
              <ol>{toolSteps.slice(0, 8).map((step, index) => <li key={`${step.tool}-${index}`}>{step.tool}{step.note ? ` · ${step.note}` : ""}{step.query ? ` · ${step.query}` : step.url ? ` · ${step.url}` : ""}</li>)}</ol>
            </section>
          ) : null}
          {databaseRequests.length > 0 ? (
            <section className="pipeline-run-evidence warning" aria-label="Pipeline research database requests">
              <strong>{locale === "en" ? "Database request" : "Datenbank-Anfrage"}</strong>
              {databaseRequests.map((request, index) => <p key={`${request.database}-${index}`}>{request.database}: {request.purpose}</p>)}
            </section>
          ) : null}
          {question ? (
            <form className="pipeline-question" data-pipeline-run-answer data-question-id={question.id} data-run-id={run?.id}>
              <strong>{question.question}</strong>
              <div className="pipeline-question-options">
                {question.options.map((option) => (
                  <label key={option.id}>
                    <input name="choiceId" type="radio" value={option.id} />
                    <span>{option.label}</span>
                  </label>
                ))}
              </div>
              {question.freeTextAllowed ? <textarea name="text" placeholder={locale === "en" ? "Optional instruction for CTOX" : "Optionale Anweisung an CTOX"} /> : null}
              <button className="button secondary" type="submit">{locale === "en" ? "Answer" : "Antwort senden"}</button>
            </form>
          ) : null}
          {run?.gates[run.currentGate]?.risks?.length ? (
            <ul className="blocker-list">{run.gates[run.currentGate]?.risks.slice(0, 5).map((risk) => <li key={risk}>{risk}</li>)}</ul>
          ) : null}
        </section>
      </section>
    </aside>
  );
}

function pipelineToolSteps(output: Record<string, unknown> | undefined) {
  const steps = Array.isArray(output?.toolPlanCompleted) ? output.toolPlanCompleted : [];
  return steps.map((item) => {
    const step = typeof item === "object" && item ? item as Record<string, unknown> : {};
    return {
      tool: typeof step.tool === "string" ? step.tool : "tool",
      query: typeof step.query === "string" ? step.query : undefined,
      url: typeof step.url === "string" ? step.url : undefined,
      note: typeof step.note === "string" ? step.note : undefined
    };
  }).filter((step) => step.query || step.url || step.note);
}

function pipelineDatabaseRequests(output: Record<string, unknown> | undefined) {
  const requests = Array.isArray(output?.databaseRequests) ? output.databaseRequests : [];
  return requests.map((item) => {
    const request = typeof item === "object" && item ? item as Record<string, unknown> : {};
    return {
      database: typeof request.database === "string" ? request.database : "research_database",
      query: typeof request.query === "string" ? request.query : "",
      purpose: typeof request.purpose === "string" ? request.purpose : ""
    };
  }).filter((request) => request.query || request.purpose);
}

function buildPipelineCards(rows: SalesCampaignImportRow[], runs: SalesPipelineRun[]): PipelineCard[] {
  return rows
    .filter((row) => row.pipeline?.status === "active" || row.pipeline?.status === "lead-ready")
    .map((row) => {
      const research = row.research;
      const candidate = research?.contactCandidates[0];
      const fit = research?.qualification.fit ?? "low";
      const stageId = row.pipeline?.stageId ?? inferSalesPipelineStage(row);
      const score = row.pipeline?.score ?? scoreSalesPipelineRow(row);
      const run = latestRunForCandidate(runs, row.id);
      const openQuestion = run?.questions.find((question) => !question.answeredAt);
      return {
        id: row.id,
        row,
        stageId,
        companyName: research?.companyName || row.companyName,
        website: research?.likelyWebsite || "",
        contactName: candidate?.name || "",
        contactRole: candidate?.role || "",
        contactConfidence: candidate?.confidence || "",
        fit,
        evidenceCount: row.webEvidence?.citations?.length ?? 0,
        missingFields: research?.missingFields ?? [],
        nextStep: nextStepForStage(stageId, research?.recommendedNextAction),
        reason: research?.qualification.reason || "",
        sourceNote: research?.sourceNote || "",
        score,
        run,
        openQuestion
      };
    })
    .sort((left, right) => stageOrder(left.stageId) - stageOrder(right.stageId) || right.score - left.score || left.companyName.localeCompare(right.companyName));
}

const fallbackPipelineCampaign = {
  id: "campaign-business-os-starter",
  name: "AI service line starter campaign",
  description: "Starter campaign for qualifying business-service prospects before they become Sales leads.",
  sourceType: "Excel" as const,
  sourceName: "starter-prospects.xlsx",
  model: "MiniMax-M2.7" as const,
  status: "ready" as const,
  rowCount: 6,
  completedRows: 6,
  createdAt: "2026-05-01T08:00:00.000Z",
  updatedAt: "2026-05-06T08:00:00.000Z"
};

const fallbackPipelineRows: SalesCampaignImportRow[] = [
  fallbackPipelineRow({
    id: "starter-row-talentbridge",
    rowIndex: 1,
    companyName: "TalentBridge Consulting",
    website: "https://talentbridge.example",
    fit: "high",
    stageId: "company",
    score: 62,
    reason: "Business-service positioning fits the campaign, but the decision maker still needs verification.",
    consultingAngle: "Position CTOX as a way to package AI-assisted recruiting workflows into a new consulting offer.",
    missingFields: ["verified decision maker", "direct email", "phone"],
    nextAction: "Find the owner for digital recruiting products and verify a direct contact route.",
    sourceNote: "Imported as a recruiting consultancy with clear AI-service adjacency."
  }),
  fallbackPipelineRow({
    id: "starter-row-rheinrecruit",
    rowIndex: 2,
    companyName: "RheinRecruit GmbH",
    website: "https://rheinrecruit.example",
    fit: "medium",
    stageId: "contact",
    score: 70,
    contact: { name: "Lena Hartmann", role: "Head of Recruiting Operations", confidence: "medium", evidence: "Role inferred from public team profile." },
    reason: "Relevant staffing company with an operations owner, decision relevance is not fully confirmed.",
    consultingAngle: "Use AI employee demos to show how recruiters can add a repeatable advisory line.",
    missingFields: ["decision authority", "direct phone"],
    nextAction: "Confirm whether Lena owns service innovation or should route to management.",
    sourceNote: "Company identified and contact candidate found."
  }),
  fallbackPipelineRow({
    id: "starter-row-novastaff",
    rowIndex: 3,
    companyName: "NovaStaff Partners",
    website: "https://novastaff.example",
    fit: "high",
    stageId: "decision",
    score: 81,
    contact: { name: "Marcel Vogt", role: "Managing Partner", email: "marcel.vogt@novastaff.example", confidence: "high", evidence: "Managing partner listed on company imprint and team page." },
    reason: "Correct senior contact is plausible, but the specific conversation angle needs tightening.",
    consultingAngle: "Lead with a concrete AI-placement workshop for staffing firms.",
    missingFields: ["preferred outreach channel"],
    nextAction: "Prepare a concise opener and confirm best channel for a first conversation.",
    sourceNote: "Decision contact and company fit are strong."
  }),
  fallbackPipelineRow({
    id: "starter-row-atlaspersonal",
    rowIndex: 4,
    companyName: "Atlas Personalservice",
    website: "https://atlas-personal.example",
    fit: "high",
    stageId: "conversation",
    score: 88,
    contact: { name: "Nora Stein", role: "Geschäftsführerin", email: "nora.stein@atlas-personal.example", phone: "+49 30 000000", confidence: "high", evidence: "Executive role and direct route verified in starter data." },
    reason: "Company, fit, decision maker and contact route are available. Next step is willingness to talk.",
    consultingAngle: "Show how AI recruiting assistants can become a consulting product for their existing customers.",
    missingFields: [],
    nextAction: "Send first message and capture whether there is interest in a 20-minute discussion.",
    sourceNote: "Ready for controlled outreach."
  }),
  fallbackPipelineRow({
    id: "starter-row-brighthire",
    rowIndex: 5,
    companyName: "BrightHire Services",
    website: "https://brighthire.example",
    fit: "medium",
    stageId: "conversation",
    score: 76,
    contact: { name: "Samuel Krüger", role: "Innovation Lead", email: "samuel.krueger@brighthire.example", confidence: "medium", evidence: "Innovation ownership visible, budget role unclear." },
    reason: "Good topic fit, but budget and actual willingness still need proof.",
    consultingAngle: "Test a small AI advisory offer before operationalizing a full service line.",
    missingFields: ["budget owner", "meeting interest"],
    nextAction: "Ask whether they want to review concrete AI-worker examples from German companies.",
    sourceNote: "Contact exists, conversation gate open."
  }),
  fallbackPipelineRow({
    id: "starter-row-fieldops",
    rowIndex: 6,
    companyName: "FieldOps Recruiting",
    website: "https://fieldops-recruiting.example",
    fit: "high",
    stageId: "lead-ready",
    score: 94,
    contact: { name: "Amira Sayed", role: "Managing Director", email: "amira.sayed@fieldops-recruiting.example", phone: "+49 40 000000", confidence: "high", evidence: "Starter record marks direct decision route and positive reply." },
    reason: "The prospect is qualified, the right person is known, and there is a positive conversation signal.",
    consultingAngle: "Convert into a lead for appointment coordination and proposal scoping.",
    missingFields: [],
    nextAction: "Transfer to Leads with campaign source, contact route, fit reason and next meeting task.",
    sourceNote: "Lead-ready starter example."
  })
];

const fallbackPipelineStore = {
  campaigns: [fallbackPipelineCampaign],
  rows: fallbackPipelineRows,
  pipelineRuns: []
};

function fallbackPipelineRow({
  companyName,
  consultingAngle,
  contact,
  fit,
  id,
  missingFields,
  nextAction,
  reason,
  rowIndex,
  score,
  sourceNote,
  stageId,
  website
}: {
  companyName: string;
  consultingAngle: string;
  contact?: { name?: string; role?: string; email?: string; phone?: string; confidence: "low" | "medium" | "high"; evidence?: string };
  fit: "low" | "medium" | "high";
  id: string;
  missingFields: string[];
  nextAction: string;
  reason: string;
  rowIndex: number;
  score: number;
  sourceNote: string;
  stageId: SalesPipelineStageId;
  website: string;
}) {
  return {
    id,
    campaignId: fallbackPipelineCampaign.id,
    rowIndex,
    companyName,
    imported: { companyName, source: fallbackPipelineCampaign.sourceName },
    researchStatus: "complete",
    webEvidence: {
      query: `${companyName} staffing recruiting AI consulting`,
      ok: true,
      provider: "starter",
      toolCalls: [
        { tool: "search", query: `${companyName} recruiting services`, ok: true, note: "starter evidence" },
        { tool: "read", url: website, ok: true, note: "starter website profile" }
      ],
      citations: [{ title: `${companyName} website`, url: website }],
      results: [{ title: `${companyName} website`, url: website, snippet: "Starter evidence for the vanilla Business OS pipeline.", excerpts: ["Business-service prospect for AI consulting campaign."] }]
    },
    research: {
      companyName,
      likelyWebsite: website,
      contactCandidates: contact ? [contact] : [],
      qualification: { fit, reason, consultingAngle },
      missingFields,
      recommendedNextAction: nextAction,
      sourceNote
    },
    pipeline: {
      status: stageId === "lead-ready" ? "lead-ready" : "active",
      stageId,
      transferredAt: "2026-05-06T08:00:00.000Z",
      transferredBy: "campaign-gate",
      gateReasons: [reason],
      score
    },
    updatedAt: "2026-05-06T08:00:00.000Z"
  } satisfies SalesCampaignImportRow;
}

function latestRunForCandidate(runs: SalesPipelineRun[], candidateId: string) {
  return runs
    .filter((run) => run.candidateId === candidateId)
    .sort((left, right) => Date.parse(right.updatedAt) - Date.parse(left.updatedAt))[0];
}

function isPipelineCandidate(row: SalesCampaignImportRow) {
  if (row.researchStatus !== "complete" || !row.research) return false;
  const fit = row.research.qualification.fit;
  const hasCompanyEvidence = Boolean(row.research.likelyWebsite || row.webEvidence?.citations?.some((citation) => citation.url));
  return fit !== "low" && hasCompanyEvidence;
}

function nextStepForStage(stageId: SalesPipelineStageId, fallback?: string) {
  if (stageId === "company") return "Ansprechpartner recherchieren und Kontaktweg verifizieren.";
  if (stageId === "contact") return "Rolle pruefen und richtigen Entscheider bestaetigen.";
  if (stageId === "decision") return "Direkten Kontaktweg und passenden Gespraechsanlass klaeren.";
  if (stageId === "conversation") return "Ansprechen und klaeren, ob Interesse an einem Termin besteht.";
  return fallback || "In das Leads-Modul uebergeben.";
}

function stageOrder(stageId: SalesPipelineStageId) {
  return pipelineStages.findIndex((stage) => stage.id === stageId);
}

function stageLabel(stage: PipelineStage, locale: "de" | "en") {
  return locale === "en" ? stage.en : stage.de;
}

function stageDescription(stage: PipelineStage, locale: "de" | "en") {
  return locale === "en" ? stage.descriptionEn : stage.descriptionDe;
}

function stageExit(stage: PipelineStage, locale: "de" | "en") {
  return locale === "en" ? stage.exitEn : stage.exitDe;
}

function pipelineRunScript(locale: "de" | "en") {
  const messages = locale === "en"
    ? { running: "Pipeline run is running ...", saved: "Pipeline run updated.", failed: "Pipeline run failed." }
    : { running: "Pipeline Run laeuft ...", saved: "Pipeline Run aktualisiert.", failed: "Pipeline Run fehlgeschlagen." };
  return `(() => {
  if (window.__ctoxSalesPipelineRuns) return;
  window.__ctoxSalesPipelineRuns = true;
  const messages = ${JSON.stringify(messages)};
  const postJson = async (url, body) => {
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body)
    });
    if (!response.ok) throw new Error(await response.text());
    return response.json();
  };
  document.addEventListener("submit", async (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    if (form.matches("[data-pipeline-run-start]")) {
      event.preventDefault();
      const button = form.querySelector("button");
      if (button) button.textContent = messages.running;
      try {
        await postJson("/api/sales/pipeline-runs", {
          candidateId: form.dataset.candidateId,
          mode: form.elements.mode?.value || "dry_run",
          gate: "next"
        });
        window.location.reload();
      } catch {
        if (button) button.textContent = messages.failed;
      }
    }
    if (form.matches("[data-pipeline-run-answer]")) {
      event.preventDefault();
      const data = new FormData(form);
      const button = form.querySelector("button");
      if (button) button.textContent = messages.running;
      try {
        await postJson("/api/sales/pipeline-runs/" + form.dataset.runId + "/answer", {
          questionId: form.dataset.questionId,
          choiceId: String(data.get("choiceId") || ""),
          text: String(data.get("text") || "")
        });
        window.location.reload();
      } catch {
        if (button) button.textContent = messages.failed;
      }
    }
  });
})();`;
}
