import { createCtoxCoreTask } from "./ctox-core-bridge";
import { getMarketingResearchRuns, upsertMarketingResearchRun } from "./marketing-research-store";
import { marketingSeed, type ResearchRun } from "./marketing-seed";

export async function runMarketingResearch(runId: string, amount: number) {
  const runs = await getMarketingResearchRuns(marketingSeed.researchRuns, { includeArchived: true });
  const run = runs.find((item) => item.id === runId);
  if (!run) throw new Error("research_run_not_found");

  const target = Math.max(5, Math.min(amount || 25, 200));
  const topic = [run.prompt, run.criteria, ...(run.criteriaItems ?? []).map((item) => `${item.label}: ${item.description}`)]
    .filter(Boolean)
    .join("\n");

  const task = await createCtoxCoreTask({
    title: `Research: ${run.title}`,
    prompt: buildAgentResearchPrompt(run, topic, target),
    source: "marketing/research",
    context: {
      module: "marketing/research",
      researchRunId: run.id,
      targetAdditionalSources: target
    },
    priority: "high",
    skill: "deep-research",
    threadKey: `marketing-research:${run.id}`,
    workspaceRoot: "/home/ubuntu/.local/lib/ctox/current",
    requireRealQueue: true
  });

  if (!task.taskId) {
    throw new Error("ctox_queue_task_missing");
  }

  const nextRun: ResearchRun = {
    ...run,
    status: "collecting",
    researchProgress: {
      status: "queued",
      currentStep: "CTOX-Agent beauftragt",
      currentQuery: topic,
      targetAdditionalSources: target,
      identifiedDelta: 0,
      readDelta: run.acceptedCount,
      usedDelta: run.sources.length,
      updatedAt: new Date().toISOString(),
      taskId: task.taskId ?? undefined
    },
    expansionRequests: (run.expansionRequests ?? []).map((request, index) => (
      index === 0 && request.status !== "done" ? { ...request, status: "queued" } : request
    )),
    updated: new Date().toISOString().slice(0, 10)
  };

  await upsertMarketingResearchRun(nextRun, marketingSeed.researchRuns);
  return nextRun;
}

function buildAgentResearchPrompt(run: ResearchRun, topic: string, target: number) {
  const databaseUrl = process.env.DATABASE_URL;
  const discoveryDir = `/home/ubuntu/.local/state/ctox/business-os/source-review/${run.id}`;
  return [
    "Fuehre diese Recherche im CTOX-Agent-Loop aus. Keine eingebauten App-Heuristiken und keine Rohquellen als sichtbare Ergebnisse.",
    "",
    "Auftrag:",
    "- Nutze den Deep-Research-Skill als `source_review`.",
    "- Der Agent entscheidet iterativ, wie er weiter recherchiert.",
    "- Starte mit naheliegender Primaerquellen-Suche im Web: Datenbanken, Datensaetze, technische Berichte, Hersteller-/Behoerdenseiten, Dokumentation, Repositories, Standards, Manuals.",
    "- Lies die ersten starken Quellen und leite daraus Suchrichtungen, Source-Familien, Kriterien und Ausschluesse ab.",
    "- Gehe danach tiefer in wissenschaftliche Analyse, Zitations-/Referenz-Snowballing und Discovery-Graph-Ausbau.",
    "- Nutze Suchmaschinen nur als Startpunkt. Der Discovery-Graph soll weitere Quellen unabhaengig von Suchmaschinen-Rankings erschliessen.",
    "- Erzeuge natuerliche Raw-Queries selbst. Keine Wortlisten, Regex-Stunts, Keyword-Salat, Benchmark-Shortcuts oder domain-spezifische Hacks.",
    "- Verifiziere und score Quellen im Agent-Kontext. Nur akzeptierte, gelesene und begruendete Quellen duerfen in den Business-OS-Katalog.",
    "- Rohquellen muessen in screened/rejected Artefakte, nicht in `sources`.",
    "",
    "Persistenz und Live-Update:",
    `- Research Run ID: ${run.id}`,
    "- Store Key: marketing/research/runs",
    `- Discovery-Artefakte: ${discoveryDir}`,
    databaseUrl ? `- DATABASE_URL: ${databaseUrl}` : "- DATABASE_URL ist im Webprozess nicht gesetzt; falls im Agent-Kontext ebenfalls keine DATABASE_URL existiert, den Run mit Fehlerstatus markieren statt Fake-Daten zu schreiben.",
    "- Schreibe Fortschritt laufend in `researchProgress`: status, currentStep, currentQuery, identifiedDelta, readDelta, usedDelta.",
    "- Der Discovery-Runner darf nur Fortschritt/Audit schreiben. Er darf keine sichtbaren Quellen oder Scores in Business OS schreiben.",
    "- Schreibe finale Quellen, Counts und Discovery-Graph erst nach Agent-Review/Agent-Scoring in denselben Research Run.",
    "- Verwende fuer den finalen Business-OS-Writeback `python3 skills/system/research/deep-research/scripts/business_research_writeback.py --database-url \"$DATABASE_URL\" --store-key marketing/research/runs --run-id <RUN_ID> --payload-json <agent_curated_payload.json>`.",
    run.sources.length > 0
      ? `- Fortsetzungssuche: vorhandene Quellen und Artefakte erhalten, ${target} zusaetzliche Kandidaten suchen, nicht von leer neu starten.`
      : `- Neue Suche: initiale Source-Review-Discovery fuer bis zu ${target} zusaetzliche Kandidaten starten.`,
    "",
    "Operator-Topic:",
    topic
  ].join("\n");
}
