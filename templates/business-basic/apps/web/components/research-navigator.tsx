"use client";

import * as d3 from "d3";
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import type { ResearchCriterion, ResearchExpansionRequest, ResearchRun, ResearchSource, ResearchSourceGroup, SupportedLocale } from "../lib/marketing-seed";

type ViewMode = "cards" | "scoring" | "graph";
type GraphNodeDatum = d3.SimulationNodeDatum & ResearchRun["graph"]["nodes"][number];
type GraphNode = ResearchRun["graph"]["nodes"][number];
type GraphEdgeDatum = d3.SimulationLinkDatum<GraphNodeDatum> & {
  source: string | GraphNodeDatum;
  target: string | GraphNodeDatum;
  relation: string;
};
type SourceGroupView = ResearchSourceGroup & { count: number; generated?: boolean };

const fitColumns = [
  ["primary", "Primärdaten"],
  ["structured", "Struktur"],
  ["coverage", "Abdeckung"],
  ["specificity", "Passung"],
  ["reuse", "Nutzbarkeit"]
] as const;

export function ResearchNavigator({
  locale,
  runs: initialRuns
}: {
  locale: SupportedLocale;
  runs: ResearchRun[];
}) {
  const [runs, setRuns] = useState<ResearchRun[]>(initialRuns.filter((run) => !run.archivedAt));
  const [activeRunId, setActiveRunId] = useState(initialRuns.find((run) => !run.archivedAt)?.id ?? "");
  const [activeView, setActiveView] = useState<ViewMode>("cards");
  const [activeFilter, setActiveFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [selectedSourceId, setSelectedSourceId] = useState("");
  const [selectedGraphNodeId, setSelectedGraphNodeId] = useState<string | undefined>();
  const [detailSheetOpen, setDetailSheetOpen] = useState(false);
  const [detailSelectionKey, setDetailSelectionKey] = useState<string | undefined>();
  const [expandPanelOpen, setExpandPanelOpen] = useState(false);
  const [expansionQuery, setExpansionQuery] = useState("");
  const [expansionCriteria, setExpansionCriteria] = useState("");
  const [criterionLabel, setCriterionLabel] = useState("");
  const [criterionDescription, setCriterionDescription] = useState("");
  const [editingCriterionId, setEditingCriterionId] = useState<string | undefined>();
  const [sourceGroupLabel, setSourceGroupLabel] = useState("");
  const [editingSourceGroupId, setEditingSourceGroupId] = useState<string | undefined>();
  const [newRunOpen, setNewRunOpen] = useState(false);
  const [newRunTitle, setNewRunTitle] = useState("");
  const [newRunPrompt, setNewRunPrompt] = useState("");
  const [newRunCriteria, setNewRunCriteria] = useState("");
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [quickExpansionFeedback, setQuickExpansionFeedback] = useState("");

  const run = runs.find((item) => item.id === activeRunId) ?? runs[0];
  const sources = useMemo(() => run?.sources ?? [], [run]);
  const requests = run?.expansionRequests ?? [];
  const criteriaItems = run?.criteriaItems ?? [];
  const sourceGroups = useMemo(() => buildSourceGroups(run), [run]);
  const filterOptions = useMemo(() => buildFilterOptions(sourceGroups), [sourceGroups]);
  const filteredSources = useMemo(() => {
    const term = search.trim().toLowerCase();
    return sources
      .filter((source) => {
        const tagMatch = activeFilter === "all" || normalizeFacet(source.group) === activeFilter;
        const haystack = [
          source.title,
          source.group,
          source.type,
          source.publisher,
          source.contribution,
          source.fields,
          source.use,
          source.missing,
          ...(source.tags ?? [])
        ].join(" ").toLowerCase();
        return tagMatch && (!term || haystack.includes(term));
      })
      .sort((left, right) => right.scoreValue - left.scoreValue);
  }, [activeFilter, search, sources]);

  const selectedSource = sources.find((source) => source.id === selectedSourceId) ?? sources[0];
  const selectedGraphNode = selectedGraphNodeId && run ? run.graph.nodes.find((node) => node.id === selectedGraphNodeId) : undefined;
  const graphNodeSources = selectedGraphNode && run ? resolveGraphNodeSources(selectedGraphNode, run, sources) : [];

  useEffect(() => {
    setRuns(initialRuns.filter((item) => !item.archivedAt));
  }, [initialRuns]);

  useEffect(() => {
    const activeProgress = run?.researchProgress?.status;
    if (activeProgress !== "queued" && activeProgress !== "running") return;
    const timer = window.setInterval(async () => {
      const response = await fetch("/api/marketing/research-runs").catch(() => null);
      if (!response?.ok) return;
      const payload = await response.json().catch(() => null) as { data?: ResearchRun[] } | null;
      if (!payload?.data) return;
      setRuns(payload.data.filter((item) => !item.archivedAt));
    }, 4000);
    return () => window.clearInterval(timer);
  }, [run?.researchProgress?.status]);

  useEffect(() => {
    if (!run) return;
    if (run.sources.length === 0) {
      setSelectedSourceId("");
      setSelectedGraphNodeId(undefined);
      setDetailSheetOpen(false);
      setDetailSelectionKey(undefined);
      return;
    }
    if (!run.sources.some((source) => source.id === selectedSourceId)) {
      setSelectedSourceId(run.sources[0]?.id ?? "");
      setSelectedGraphNodeId(undefined);
      setDetailSheetOpen(false);
      setDetailSelectionKey(undefined);
    }
  }, [run, selectedSourceId]);

  function setActiveRun(runId: string) {
    setActiveRunId(runId);
    const nextRun = runs.find((item) => item.id === runId);
    setSelectedSourceId(nextRun?.sources[0]?.id ?? "");
    setSelectedGraphNodeId(undefined);
    setDetailSheetOpen(false);
    setDetailSelectionKey(undefined);
    setSearch("");
    setActiveFilter("all");
  }

  function openSourceDetail(id: string) {
    const key = `source:${id}`;
    setSelectedSourceId(id);
    setSelectedGraphNodeId(undefined);
    setDetailSheetOpen((open) => detailSelectionKey === key ? !open : true);
    setDetailSelectionKey(key);
  }

  function openGraphDetail(node: GraphNode) {
    const key = `graph:${node.id}`;
    setSelectedGraphNodeId(node.id);
    if (node.kind === "source") setSelectedSourceId(node.id);
    setDetailSheetOpen((open) => detailSelectionKey === key ? !open : true);
    setDetailSelectionKey(key);
  }

  async function saveRun(nextRun: ResearchRun) {
    const response = await fetch("/api/marketing/research-runs", {
      body: JSON.stringify({ run: nextRun }),
      headers: { "content-type": "application/json" },
      method: "POST"
    }).catch(() => null);
    if (!response?.ok) return false;
    const payload = await response.json().catch(() => null) as { runs?: ResearchRun[] } | null;
    const nextRuns = payload?.runs?.filter((item) => !item.archivedAt) ?? [nextRun, ...runs.filter((item) => item.id !== nextRun.id)];
    setRuns(nextRuns);
    setActiveRunId(nextRun.id);
    return true;
  }

  async function saveExpansionRequest() {
    if (!run) return;
    if (!expansionQuery.trim() && !expansionCriteria.trim()) return;
    const request: ResearchExpansionRequest = {
      id: `exp-${Date.now()}`,
      createdAt: new Date().toISOString(),
      query: expansionQuery.trim(),
      criteria: expansionCriteria.trim(),
      status: "queued"
    };
    const nextRequests = [request, ...requests];
    setExpansionQuery("");
    setExpansionCriteria("");
    setSaveState("saving");

    const saved = await saveRun({ ...run, expansionRequests: nextRequests, updated: new Date().toISOString().slice(0, 10) });
    setSaveState(saved ? "saved" : "error");
  }

  async function saveQuickExpansion(amount: number) {
    if (!run) return;
    const query = `Suche um ${amount} Kandidaten erweitern`;
    setQuickExpansionFeedback("Recherche wird gestartet ...");
    const request: ResearchExpansionRequest = {
      id: `exp-${Date.now()}`,
      createdAt: new Date().toISOString(),
      query,
      criteria: "Bestehende Quellenkarte erweitern und relevante Kandidaten ergänzen.",
      targetAdditionalSources: amount,
      status: "queued"
    };
    const nextRun: ResearchRun = {
      ...run,
      expansionRequests: [request, ...(run.expansionRequests ?? [])],
      researchProgress: {
        status: "queued",
        currentStep: "Wartet auf Recherche-Start",
        currentQuery: query,
        targetAdditionalSources: amount,
        identifiedDelta: 0,
        readDelta: 0,
        usedDelta: 0,
        updatedAt: new Date().toISOString()
      },
      updated: new Date().toISOString().slice(0, 10)
    };
    setRuns((currentRuns) => [nextRun, ...currentRuns.filter((item) => item.id !== nextRun.id)]);
    const saved = await saveRun(nextRun);
    if (!saved) {
      setQuickExpansionFeedback("Recherche-Status konnte nicht gespeichert werden");
      return;
    }
    setQuickExpansionFeedback("Recherche läuft");
    void startResearchExecution(nextRun.id, amount);
  }

  async function saveCriterion(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!run || !criterionLabel.trim()) return;
    const now = new Date().toISOString();
    const nextCriterion: ResearchCriterion = {
      id: editingCriterionId ?? `criterion-${Date.now()}`,
      label: criterionLabel.trim(),
      description: criterionDescription.trim(),
      active: true,
      createdAt: criteriaItems.find((item) => item.id === editingCriterionId)?.createdAt ?? now,
      updatedAt: now
    };
    const nextCriteria = editingCriterionId
      ? criteriaItems.map((item) => item.id === editingCriterionId ? nextCriterion : item)
      : [nextCriterion, ...criteriaItems];
    const saved = await saveRun({ ...run, criteriaItems: nextCriteria, updated: now.slice(0, 10) });
    if (!saved) return;
    setCriterionLabel("");
    setCriterionDescription("");
    setEditingCriterionId(undefined);
  }

  async function deleteCriterion(id: string) {
    if (!run) return;
    await saveRun({ ...run, criteriaItems: criteriaItems.filter((item) => item.id !== id), updated: new Date().toISOString().slice(0, 10) });
    if (editingCriterionId === id) {
      setCriterionLabel("");
      setCriterionDescription("");
      setEditingCriterionId(undefined);
    }
  }

  function editCriterion(item: ResearchCriterion) {
    setEditingCriterionId(item.id);
    setCriterionLabel(item.label);
    setCriterionDescription(item.description);
  }

  async function saveSourceGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!run || !sourceGroupLabel.trim()) return;
    const now = new Date().toISOString();
    const label = sourceGroupLabel.trim();
    const groupId = editingSourceGroupId ?? slugify(label);
    const currentCustomGroups = run.customSourceGroups ?? [];
    const generatedGroup = sourceGroups.find((group) => group.id === groupId)?.generated;
    const nextRun: ResearchRun = {
      ...run,
      hiddenSourceGroups: (run.hiddenSourceGroups ?? []).filter((id) => id !== groupId),
      sourceGroupLabels: generatedGroup
        ? { ...(run.sourceGroupLabels ?? {}), [groupId]: label }
        : run.sourceGroupLabels,
      customSourceGroups: generatedGroup
        ? currentCustomGroups
        : editingSourceGroupId
          ? currentCustomGroups.map((group) => group.id === groupId ? { ...group, label, updatedAt: now } : group)
          : [{ id: groupId, label, createdAt: now, updatedAt: now }, ...currentCustomGroups],
      updated: now.slice(0, 10)
    };
    const saved = await saveRun(nextRun);
    if (!saved) return;
    setSourceGroupLabel("");
    setEditingSourceGroupId(undefined);
  }

  function editSourceGroup(group: SourceGroupView) {
    setEditingSourceGroupId(group.id);
    setSourceGroupLabel(group.label);
  }

  async function deleteSourceGroup(groupId: string) {
    if (!run) return;
    const nextRun: ResearchRun = {
      ...run,
      hiddenSourceGroups: [...new Set([...(run.hiddenSourceGroups ?? []), groupId])],
      customSourceGroups: (run.customSourceGroups ?? []).filter((group) => group.id !== groupId),
      updated: new Date().toISOString().slice(0, 10)
    };
    const saved = await saveRun(nextRun);
    if (!saved) return;
    if (activeFilter === groupId) setActiveFilter("all");
    if (editingSourceGroupId === groupId) {
      setSourceGroupLabel("");
      setEditingSourceGroupId(undefined);
    }
  }

  async function createResearchRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const title = newRunTitle.trim();
    const prompt = newRunPrompt.trim();
    if (!title || !prompt) return;
    const id = `${slugify(title)}-${Date.now()}`;
    const nextRun: ResearchRun = {
      id,
      title,
      prompt,
      criteria: newRunCriteria.trim(),
      status: "draft",
      updated: new Date().toISOString().slice(0, 10),
      queryCount: 0,
      screenedCount: 0,
      acceptedCount: 0,
      summary: {
        en: prompt,
        de: prompt
      },
      sources: [],
      graph: { nodes: [], edges: [] },
      expansionRequests: []
    };
    const saved = await saveRun(nextRun);
    if (!saved) return;
    setNewRunTitle("");
    setNewRunPrompt("");
    setNewRunCriteria("");
    setNewRunOpen(false);
    setDetailSheetOpen(false);
    void startResearchExecution(nextRun.id, 50);
  }

  async function startResearchExecution(runId: string, amount: number) {
    const response = await fetch("/api/marketing/research-runs/run", {
      body: JSON.stringify({ runId, amount }),
      headers: { "content-type": "application/json" },
      method: "POST"
    }).catch(() => null);
    const payload = await response?.json().catch(() => null) as { ok?: boolean; run?: ResearchRun; error?: string } | null;
    if (!response?.ok || !payload?.ok || !payload.run) {
      setQuickExpansionFeedback(`Recherche fehlgeschlagen${payload?.error ? `: ${payload.error}` : ""}`);
      return;
    }
    setRuns((currentRuns) => [payload.run!, ...currentRuns.filter((item) => item.id !== payload.run!.id)]);
    setQuickExpansionFeedback("Recherche aktualisiert");
  }

  async function archiveResearchRun(runId: string) {
    const response = await fetch(`/api/marketing/research-runs?id=${encodeURIComponent(runId)}`, { method: "DELETE" }).catch(() => null);
    if (!response?.ok) return;
    const payload = await response.json().catch(() => null) as { runs?: ResearchRun[] } | null;
    const nextRuns = payload?.runs ?? runs.filter((item) => item.id !== runId);
    setRuns(nextRuns);
    if (activeRunId === runId) {
      const nextActive = nextRuns[0];
      setActiveRunId(nextActive?.id ?? "");
      setSelectedSourceId(nextActive?.sources[0]?.id ?? "");
      setSelectedGraphNodeId(undefined);
      setDetailSheetOpen(false);
      setDetailSelectionKey(undefined);
    }
  }

  if (!run) {
    return (
      <div className="research-navigator-shell research-navigator-empty">
        <RunSidebar
          activeRunId=""
          archiveRun={archiveResearchRun}
          createResearchRun={createResearchRun}
          newRunCriteria={newRunCriteria}
          newRunOpen={newRunOpen}
          newRunPrompt={newRunPrompt}
          newRunTitle={newRunTitle}
          quickExpansionFeedback={quickExpansionFeedback}
          quickExpand={saveQuickExpansion}
          runs={runs}
          setActiveRun={setActiveRun}
          setNewRunCriteria={setNewRunCriteria}
          setNewRunOpen={setNewRunOpen}
          setNewRunPrompt={setNewRunPrompt}
          setNewRunTitle={setNewRunTitle}
        />
        <main className="research-main-pane">
          <EmptyResearchRun />
        </main>
      </div>
    );
  }

  return (
    <div className="research-navigator-shell">
      <RunSidebar
        activeRunId={run.id}
        archiveRun={archiveResearchRun}
        createResearchRun={createResearchRun}
        newRunCriteria={newRunCriteria}
        newRunOpen={newRunOpen}
        newRunPrompt={newRunPrompt}
        newRunTitle={newRunTitle}
        quickExpansionFeedback={quickExpansionFeedback}
        quickExpand={saveQuickExpansion}
        runs={runs}
        setActiveRun={setActiveRun}
        setNewRunCriteria={setNewRunCriteria}
        setNewRunOpen={setNewRunOpen}
        setNewRunPrompt={setNewRunPrompt}
        setNewRunTitle={setNewRunTitle}
      />
      <main className="research-main-pane">
        <div className="research-view-switch" aria-label="Research views">
          <button className={activeView === "cards" ? "active" : ""} onClick={() => setActiveView("cards")} type="button">Quellenkarten</button>
          <button className={activeView === "scoring" ? "active" : ""} onClick={() => setActiveView("scoring")} type="button">Scoring-Liste</button>
          <button className={activeView === "graph" ? "active" : ""} onClick={() => setActiveView("graph")} type="button">Discovery Graph</button>
          <button onClick={() => setExpandPanelOpen(true)} type="button">Suche erweitern</button>
        </div>

        <div className="research-filter-header">
          <input
            aria-label="Quelle suchen"
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Quelle suchen ..."
            value={search}
          />
            <div>
              {filterOptions.map(([value, label]) => (
                <button className={activeFilter === value ? "active" : ""} key={value} onClick={() => setActiveFilter(value)} type="button">
                  {label}
                </button>
              ))}
              <button onClick={() => setExpandPanelOpen(true)} type="button">Bearbeiten</button>
            </div>
          </div>

        {sources.length === 0 ? <EmptyResearchRun run={run} /> : null}

        {sources.length > 0 && activeView === "cards" ? (
          <section className="research-card-grid">
            {filteredSources.map((source) => (
              <SourceCard
                key={source.id}
                onSelect={openSourceDetail}
                selected={!selectedGraphNodeId && selectedSource?.id === source.id}
                source={source}
              />
            ))}
          </section>
        ) : null}

        {sources.length > 0 && activeView === "scoring" ? (
          <section className="research-scoring-layout research-scoring-layout-single">
            <UnifiedScoringList
              onSelect={openSourceDetail}
              selectedSourceId={!selectedGraphNodeId ? selectedSource?.id : undefined}
              sources={filteredSources}
            />
          </section>
        ) : null}

        {sources.length > 0 && activeView === "graph" ? (
          <section className="research-graph-layout">
            <D3DiscoveryGraph
              onSelectNode={openGraphDetail}
              run={run}
              selectedNodeId={selectedGraphNodeId ?? selectedSource?.id}
            />
          </section>
        ) : null}
      </main>

      {detailSheetOpen ? (
        <section className="research-detail-sheet" aria-label="Research detail">
          <div className="research-detail-sheet-head">
            <div>
              <span>{selectedGraphNode && selectedGraphNode.kind !== "source" ? graphKindLabel(selectedGraphNode.kind) : selectedSource?.score ? `${selectedSource.score} · ${selectedSource.scoreValue}` : "Detail"}</span>
              <strong>{selectedGraphNode && selectedGraphNode.kind !== "source" ? selectedGraphNode.label : selectedSource?.title}</strong>
            </div>
            <button onClick={() => setDetailSheetOpen(false)} type="button">Schließen</button>
          </div>
          <div className="research-detail-sheet-body">
            {selectedGraphNode && selectedGraphNode.kind !== "source" ? (
              <GraphNodeDetail node={selectedGraphNode} sources={graphNodeSources} />
            ) : (
              <SourceDetail source={selectedSource} />
            )}
          </div>
        </section>
      ) : null}

      {expandPanelOpen ? (
        <aside className="research-expand-drawer" aria-label="Suche erweitern">
          <div className="research-detail-sheet-head">
            <div>
              <strong>Suche erweitern</strong>
            </div>
            <button onClick={() => setExpandPanelOpen(false)} type="button">Schließen</button>
          </div>
          <div className="research-expansion-panel">
            <p>Kriterien und Suchrichtungen bleiben an dieser Recherche gespeichert.</p>
            <form className="research-criteria-form" onSubmit={saveSourceGroup}>
              <label>
                Rubrik
                <input onChange={(event) => setSourceGroupLabel(event.target.value)} placeholder="z. B. Primärdaten" value={sourceGroupLabel} />
              </label>
              <button type="submit">{editingSourceGroupId ? "Rubrik speichern" : "Rubrik hinzufügen"}</button>
            </form>
            <div className="research-criteria-list">
              {sourceGroups.map((group) => (
                <div key={group.id}>
                  <strong>{group.label}</strong>
                  <span>{group.count} Quellen</span>
                  <nav>
                    <button onClick={() => editSourceGroup(group)} type="button">Editieren</button>
                    <button onClick={() => deleteSourceGroup(group.id)} type="button">Löschen</button>
                  </nav>
                </div>
              ))}
            </div>
            <form className="research-criteria-form" onSubmit={saveCriterion}>
              <label>
                Kriterium
                <input onChange={(event) => setCriterionLabel(event.target.value)} placeholder="z. B. nur Primärquellen" value={criterionLabel} />
              </label>
              <label>
                Beschreibung
                <textarea onChange={(event) => setCriterionDescription(event.target.value)} placeholder="Was soll ein Treffer erfüllen oder ausschließen?" value={criterionDescription} />
              </label>
              <button type="submit">{editingCriterionId ? "Kriterium speichern" : "Kriterium hinzufügen"}</button>
            </form>
            <div className="research-criteria-list">
              {criteriaItems.map((item) => (
                <div key={item.id}>
                  <strong>{item.label}</strong>
                  <span>{item.description || "Keine Beschreibung"}</span>
                  <nav>
                    <button onClick={() => editCriterion(item)} type="button">Editieren</button>
                    <button onClick={() => deleteCriterion(item.id)} type="button">Löschen</button>
                  </nav>
                </div>
              ))}
            </div>
            <label>
              Neue Suchfrage
              <input onChange={(event) => setExpansionQuery(event.target.value)} placeholder="z. B. weitere Primärquellen, Datensätze oder technische Berichte" value={expansionQuery} />
            </label>
            <label>
              Kriterien / Ausschlüsse
              <textarea onChange={(event) => setExpansionCriteria(event.target.value)} placeholder="z. B. nur Rohdaten, bevorzugt CSV/XLSX, keine reinen Papers ohne Datenlink" value={expansionCriteria} />
            </label>
            <button onClick={saveExpansionRequest} type="button">Kriterium speichern</button>
            <span className={`research-save-state state-${saveState}`}>{saveStateLabel(saveState)}</span>
            <div className="research-request-list">
              {requests.map((request) => (
                <div key={request.id}>
                  <strong>{request.query || "Zusätzliche Suchrichtung"}</strong>
                  <span>{request.criteria || "Keine Zusatzkriterien"}</span>
                </div>
              ))}
            </div>
          </div>
        </aside>
      ) : null}
    </div>
  );
}

function RunSidebar({
  activeRunId,
  archiveRun,
  createResearchRun,
  newRunCriteria,
  newRunOpen,
  newRunPrompt,
  newRunTitle,
  quickExpansionFeedback,
  quickExpand,
  runs,
  setActiveRun,
  setNewRunCriteria,
  setNewRunOpen,
  setNewRunPrompt,
  setNewRunTitle
}: {
  activeRunId: string;
  archiveRun: (runId: string) => void;
  createResearchRun: (event: FormEvent<HTMLFormElement>) => void;
  newRunCriteria: string;
  newRunOpen: boolean;
  newRunPrompt: string;
  newRunTitle: string;
  quickExpansionFeedback: string;
  quickExpand: (amount: number) => void;
  runs: ResearchRun[];
  setActiveRun: (runId: string) => void;
  setNewRunCriteria: (value: string) => void;
  setNewRunOpen: (value: boolean) => void;
  setNewRunPrompt: (value: string) => void;
  setNewRunTitle: (value: string) => void;
}) {
  const activeRun = runs.find((run) => run.id === activeRunId);
  return (
    <aside className="research-run-sidebar">
      <div className="research-run-sidebar-head">
        <div>
          <span>Recherchen</span>
          <strong>{runs.length}</strong>
        </div>
        <button onClick={() => setNewRunOpen(!newRunOpen)} type="button">{newRunOpen ? "Schließen" : "Neu"}</button>
      </div>

      {newRunOpen ? (
        <form className="research-new-run-form" onSubmit={createResearchRun}>
          <label>
            Titel
            <input onChange={(event) => setNewRunTitle(event.target.value)} placeholder="Neue Recherche" value={newRunTitle} />
          </label>
          <label>
            Auftrag
            <textarea onChange={(event) => setNewRunPrompt(event.target.value)} placeholder="Was soll recherchiert werden?" value={newRunPrompt} />
          </label>
          <label>
            Kriterien
            <textarea onChange={(event) => setNewRunCriteria(event.target.value)} placeholder="Scope, Ausschlüsse, gewünschte Quellenarten" value={newRunCriteria} />
          </label>
          <button type="submit">Recherche anlegen</button>
        </form>
      ) : null}

      <div className="research-run-list-panel">
        {runs.map((run) => (
          <div className={`research-run-item ${run.id === activeRunId ? "active" : ""}`} key={run.id}>
            <button onClick={() => setActiveRun(run.id)} type="button">
              <strong>{run.title}</strong>
              <span>{statusLabel(run.status)} · {run.sources.length} Quellen · {run.updated}</span>
            </button>
            <button aria-label={`${run.title} archivieren`} className="research-run-archive" onClick={() => archiveRun(run.id)} type="button">Archiv</button>
          </div>
        ))}
        {runs.length === 0 ? <span className="research-run-empty">Noch keine aktive Recherche.</span> : null}
      </div>

      {activeRun ? (
        <div className="research-sidebar-summary">
          <div className="research-sidebar-metrics">
            <span><strong>{activeRun.screenedCount}</strong> identifiziert</span>
            <span><strong>{activeRun.acceptedCount}</strong> gelesen</span>
            <span><strong>{activeRun.sources.length}</strong> verwendet</span>
          </div>
          <ResearchProgressBar progress={activeRun.researchProgress ?? idleResearchProgress(activeRun)} />
          <div className="research-quick-expand" aria-label="Suche schnell erweitern">
            <span>Suche erweitern</span>
            {[50, 100, 200].map((amount) => (
              <button key={amount} onClick={() => quickExpand(amount)} type="button">+{amount}</button>
            ))}
            {quickExpansionFeedback ? <small>{quickExpansionFeedback}</small> : null}
          </div>
        </div>
      ) : null}
    </aside>
  );
}

function EmptyResearchRun({ run }: { run?: ResearchRun }) {
  return (
    <section className="research-empty-run">
      <span>{run ? statusLabel(run.status) : "Keine Recherche"}</span>
      <h2>{run ? "Recherche-Entwurf" : "Noch keine Recherche"}</h2>
      <p>{run?.prompt ?? "Lege links eine neue Recherche an, um Auftrag und Kriterien zu speichern."}</p>
      {run?.criteria ? <small>{run.criteria}</small> : null}
    </section>
  );
}

function ResearchActivityFeed({ requests }: { requests: ResearchExpansionRequest[] }) {
  return (
    <section className="research-activity-feed" aria-label="Aktivität">
      <strong>Aktivität</strong>
      <div>
        {requests.slice(0, 3).map((request) => (
          <span key={request.id}>
            {activityRequestLabel(request)}
          </span>
        ))}
      </div>
    </section>
  );
}

function ResearchProgressBar({ progress }: { progress: NonNullable<ResearchRun["researchProgress"]> }) {
  const percent = researchProgressPercent(progress);
  return (
    <section className="research-progress-panel" aria-label="Research-Fortschritt">
      <div>
        <strong>{activityStatusLabel(progress.status)}</strong>
        <span>{progress.currentStep}</span>
      </div>
      <div className="research-progress-track" aria-label={`${percent}%`}>
        <i style={{ width: `${percent}%` }} />
      </div>
      <dl>
        <div><dt>Aktuell</dt><dd>{progress.currentQuery ?? "Noch keine Suchrichtung aktiv"}</dd></div>
        <div><dt>Identifiziert</dt><dd>{progress.identifiedDelta}</dd></div>
        <div><dt>Gelesen</dt><dd>{progress.readDelta}</dd></div>
        <div><dt>Verwendet</dt><dd>{progress.usedDelta}</dd></div>
      </dl>
    </section>
  );
}

function SourceCard({
  onSelect,
  selected,
  source
}: {
  onSelect: (id: string) => void;
  selected: boolean;
  source: ResearchSource;
}) {
  const links = source.links?.slice(0, 3) ?? [{ label: "Quelle öffnen", url: source.url }];
  return (
    <article className={`research-source-card ${selected ? "active" : ""}`}>
      <button className="research-source-card-main" onClick={() => onSelect(source.id)} type="button">
        <span className={`research-grade grade-${source.score.toLowerCase()}`}>{gradeLabel(source.score)}</span>
        <strong>{source.title}</strong>
        <small>{source.type}</small>
        <div className="research-chip-row">
          {(source.tags ?? [source.group]).slice(0, 4).map((tag) => <span key={tag}>{tag}</span>)}
        </div>
        <dl>
          <div><dt>Daten</dt><dd>{source.fields ?? source.contribution}</dd></div>
          <div><dt>Nutzen</dt><dd>{source.use ?? source.contribution}</dd></div>
          <div><dt>Lücke</dt><dd>{source.missing ?? "Offene Grenzen im nächsten Review ergänzen."}</dd></div>
        </dl>
      </button>
      <div className="research-card-links">
        {links.map((link) => <a href={link.url} key={link.url} rel="noreferrer" target="_blank">{link.label}</a>)}
      </div>
    </article>
  );
}

function SourceDetail({ source }: { source?: ResearchSource }) {
  if (!source) return null;
  return (
    <div className="research-source-detail">
      <span className={`research-grade grade-${source.score.toLowerCase()}`}>{source.score} · {source.scoreValue}</span>
      <h2>{source.title}</h2>
      <p>{source.contribution}</p>
      <dl>
        <div><dt>Daten</dt><dd>{source.fields ?? source.contribution}</dd></div>
        <div><dt>Nutzen</dt><dd>{source.use ?? source.contribution}</dd></div>
        <div><dt>Lücke</dt><dd>{source.missing ?? "Noch nicht bewertet."}</dd></div>
      </dl>
      <div className="research-detail-links">
        {(source.links ?? [{ label: "Quelle öffnen", url: source.url }]).map((link) => (
          <a href={link.url} key={link.url} rel="noreferrer" target="_blank">{link.label}</a>
        ))}
      </div>
    </div>
  );
}

function GraphNodeDetail({ node, sources }: { node: GraphNode; sources: ResearchSource[] }) {
  return (
    <div className="research-source-detail">
      <span className="research-grade grade-b">{node.kind === "query" ? "Suchrichtung" : "Gruppe"}</span>
      <h2>{node.label}</h2>
      <p>
        {node.kind === "query"
          ? "Dieser Suchknoten zeigt, welche Quellen durch diese Suchrichtung verbunden sind."
          : "Dieser Gruppenknoten bündelt Quellen mit ähnlichem Datencharakter."}
      </p>
      <div className="research-node-source-list">
        {sources.length > 0 ? sources.map((source) => (
          <a href={source.url} key={source.id} rel="noreferrer" target="_blank">
            <strong>{source.title}</strong>
            <span>{source.score} · {source.scoreValue} · {source.type}</span>
          </a>
        )) : <span>Für diesen Knoten sind noch keine Quellen verknüpft.</span>}
      </div>
    </div>
  );
}

function UnifiedScoringList({
  onSelect,
  selectedSourceId,
  sources
}: {
  onSelect: (id: string) => void;
  selectedSourceId?: string;
  sources: ResearchSource[];
}) {
  return (
    <div className="research-score-list research-unified-score-list">
      <div className="research-unified-row research-unified-head">
        <span>Quelle</span>
        <span>Typ</span>
        <span>Score</span>
        <span>Zugriff</span>
        {fitColumns.map(([, label]) => <span key={label}>{label}</span>)}
      </div>
      <div className="research-unified-body">
        {sources.map((source) => (
          <button className={selectedSourceId === source.id ? "active" : ""} key={source.id} onClick={() => onSelect(source.id)} type="button">
            <span><strong>{source.title}</strong><small>{source.publisher} · {source.year}</small></span>
            <span>{source.type}</span>
            <span className={`research-grade grade-${source.score.toLowerCase()}`}>{source.score} · {source.scoreValue}</span>
            <span>{source.access}</span>
            {fitColumns.map(([key]) => <RatingDots key={key} value={fitValue(source, key)} />)}
          </button>
        ))}
      </div>
    </div>
  );
}

function RatingDots({ value }: { value: number }) {
  return (
    <span className="research-rating-dots" title={`${value}/5`}>
      {Array.from({ length: 5 }).map((_, index) => <i className={index < value ? "filled" : ""} key={index} />)}
    </span>
  );
}

function D3DiscoveryGraph({
  onSelectNode,
  run,
  selectedNodeId
}: {
  onSelectNode: (node: GraphNode) => void;
  run: ResearchRun;
  selectedNodeId?: string;
}) {
  const ref = useRef<SVGSVGElement | null>(null);
  const zoomRef = useRef<d3.ZoomBehavior<SVGSVGElement, unknown> | null>(null);
  const graphNodesRef = useRef<GraphNodeDatum[]>([]);

  useEffect(() => {
    if (!ref.current) return;
    const width = 1200;
    const height = 760;
    const nodes: GraphNodeDatum[] = run.graph.nodes.map((node) => ({ ...node }));
    const edges: GraphEdgeDatum[] = run.graph.edges.map((edge) => ({ ...edge }));
    graphNodesRef.current = nodes;
    const svg = d3.select(ref.current);
    svg.selectAll("*").remove();
    svg.attr("viewBox", `0 0 ${width} ${height}`);
    svg.attr("tabIndex", 0);

    const viewport = svg.append("g").attr("class", "research-d3-viewport");
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.18, 4])
      .on("zoom", (event) => {
        viewport.attr("transform", event.transform.toString());
      });
    zoomRef.current = zoom;
    svg.call(zoom);

    const simulation = d3.forceSimulation<GraphNodeDatum>(nodes)
      .force("link", d3.forceLink<GraphNodeDatum, GraphEdgeDatum>(edges).id((node) => node.id).distance(170).strength(0.42))
      .force("charge", d3.forceManyBody().strength(-720))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("x", d3.forceX<GraphNodeDatum>((node) => node.kind === "query" ? 210 : node.kind === "group" ? 600 : 980).strength(0.16))
      .force("y", d3.forceY(height / 2).strength(0.045))
      .force("collision", d3.forceCollide<GraphNodeDatum>().radius((node) => node.kind === "group" ? 78 : 62).strength(0.78));

    const link = viewport.append("g")
      .attr("class", "research-d3-links")
      .selectAll("line")
      .data(edges)
      .join("line");

    const drag = d3.drag<SVGGElement, GraphNodeDatum>()
      .on("start", (event, item) => {
        if (!event.active) simulation.alphaTarget(0.25).restart();
        item.fx = item.x;
        item.fy = item.y;
      })
      .on("drag", (event, item) => {
        item.fx = event.x;
        item.fy = event.y;
      })
      .on("end", (event, item) => {
        if (!event.active) simulation.alphaTarget(0);
        item.fx = null;
        item.fy = null;
      });

    const node = viewport.append("g")
      .attr("class", "research-d3-nodes")
      .selectAll<SVGGElement, GraphNodeDatum>("g")
      .data(nodes)
      .join("g")
      .attr("class", (item: GraphNodeDatum) => `research-d3-node node-${item.kind} ${item.id === selectedNodeId ? "active" : ""}`)
      .call(drag)
      .on("click", (_, item) => {
        onSelectNode(item);
      });

    node.append("circle").attr("r", (item: GraphNodeDatum) => item.kind === "group" ? 19 : 14);
    node.append("text").text((item: GraphNodeDatum) => graphLabel(item.label)).attr("dy", 34);
    node.append("title").text((item: GraphNodeDatum) => item.label);

    simulation.on("tick", () => {
      link
        .attr("x1", (edge: GraphEdgeDatum) => nodePosition(edge.source, "x"))
        .attr("y1", (edge: GraphEdgeDatum) => nodePosition(edge.source, "y"))
        .attr("x2", (edge: GraphEdgeDatum) => nodePosition(edge.target, "x"))
        .attr("y2", (edge: GraphEdgeDatum) => nodePosition(edge.target, "y"));
      node.attr("transform", (item: GraphNodeDatum) => `translate(${item.x ?? 0},${item.y ?? 0})`);
    });

    window.setTimeout(() => fitGraphToViewport(ref.current, zoom, nodes, width, height), 350);

    return () => {
      simulation.stop();
      zoomRef.current = null;
    };
  }, [onSelectNode, run.graph.edges, run.graph.nodes, selectedNodeId]);

  const zoomBy = (factor: number) => {
    if (!ref.current || !zoomRef.current) return;
    d3.select(ref.current).transition().duration(160).call(zoomRef.current.scaleBy, factor);
  };
  const fit = () => {
    if (!ref.current || !zoomRef.current) return;
    const width = 1200;
    const height = 760;
    fitGraphToViewport(ref.current, zoomRef.current, graphNodesRef.current, width, height);
  };

  return (
    <div className="research-d3-frame">
      <div className="research-d3-controls" aria-label="Graph Navigation">
        <button onClick={fit} type="button">Fit</button>
        <button onClick={() => zoomBy(1.25)} type="button">+</button>
        <button onClick={() => zoomBy(0.8)} type="button">-</button>
      </div>
      <svg ref={ref} role="img" aria-label="Discovery Graph" />
    </div>
  );
}

function graphLabel(label: string) {
  return label.length > 72 ? `${label.slice(0, 69)}...` : label;
}

function fitGraphToViewport(
  element: SVGSVGElement | null,
  zoom: d3.ZoomBehavior<SVGSVGElement, unknown>,
  nodes: GraphNodeDatum[],
  width: number,
  height: number
) {
  if (!element || nodes.length === 0) return;
  const xs = nodes.map((node) => node.x ?? width / 2);
  const ys = nodes.map((node) => node.y ?? height / 2);
  const minX = Math.min(...xs) - 120;
  const maxX = Math.max(...xs) + 120;
  const minY = Math.min(...ys) - 90;
  const maxY = Math.max(...ys) + 90;
  const graphWidth = Math.max(1, maxX - minX);
  const graphHeight = Math.max(1, maxY - minY);
  const scale = Math.max(0.18, Math.min(2.2, 0.88 / Math.max(graphWidth / width, graphHeight / height)));
  const transform = d3.zoomIdentity
    .translate((width - graphWidth * scale) / 2 - minX * scale, (height - graphHeight * scale) / 2 - minY * scale)
    .scale(scale);
  d3.select(element).transition().duration(220).call(zoom.transform, transform);
}

function resolveGraphNodeSources(node: GraphNode, run: ResearchRun, sources: ResearchSource[]) {
  if (node.kind === "source") return sources.filter((source) => source.id === node.id);
  const sourceIds = new Set<string>();
  for (const edge of run.graph.edges) {
    if (edge.source === node.id && run.graph.nodes.find((item) => item.id === edge.target)?.kind === "source") sourceIds.add(edge.target);
    if (edge.target === node.id && run.graph.nodes.find((item) => item.id === edge.source)?.kind === "source") sourceIds.add(edge.source);
  }
  if (node.kind === "group" && sourceIds.size === 0) {
    for (const source of sources) {
      if (source.group.toLowerCase() === node.label.toLowerCase()) sourceIds.add(source.id);
    }
  }
  return sources.filter((source) => sourceIds.has(source.id));
}

function nodePosition(value: string | GraphNodeDatum | number | undefined, axis: "x" | "y") {
  if (typeof value === "object" && value) return value[axis] ?? 0;
  return 0;
}

function buildSourceGroups(run?: ResearchRun): SourceGroupView[] {
  if (!run) return [];
  const hidden = new Set(run.hiddenSourceGroups ?? []);
  const counts = new Map<string, { count: number; rawLabel: string }>();
  for (const source of run.sources) {
    const id = normalizeFacet(source.group);
    if (!id || hidden.has(id)) continue;
    const current = counts.get(id);
    counts.set(id, { count: (current?.count ?? 0) + 1, rawLabel: current?.rawLabel ?? source.group });
  }
  const generated = [...counts.entries()].map(([id, item]) => ({
    id,
    label: run.sourceGroupLabels?.[id] ?? facetLabel(item.rawLabel),
    count: item.count,
    createdAt: run.updated,
    updatedAt: run.updated,
    generated: true
  }));
  const custom = (run.customSourceGroups ?? [])
    .filter((group) => !hidden.has(group.id) && !counts.has(group.id))
    .map((group) => ({ ...group, count: 0, generated: false }));
  return [...generated, ...custom].sort((left, right) => right.count - left.count || left.label.localeCompare(right.label));
}

function buildFilterOptions(sourceGroups: SourceGroupView[]) {
  const dynamic = sourceGroups
    .slice(0, 8)
    .map((item) => [item.id, item.label] as const);
  return [["all", "Alle"] as const, ...dynamic];
}

function normalizeFacet(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

function facetLabel(value: string) {
  const normalized = normalizeFacet(value);
  const labels: Record<string, string> = {
    "bench-data": "Prüfstandsdaten",
    "flight-logs": "Fluglogs",
    "propeller-data": "Propellerdaten",
    "technical-reports": "Technische Berichte",
    "wind-tunnel": "Windkanal"
  };
  if (labels[normalized]) return labels[normalized];
  return value.split(/[\s_-]+/).filter(Boolean).map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

function fitValue(source: ResearchSource, key: string) {
  if (source.fit?.[key] !== undefined) return source.fit[key];
  const legacy = source.fit ?? {};
  if (key === "primary") return legacy.direct ?? 0;
  if (key === "structured") return Math.max(legacy.rpm ?? 0, legacy.simulation ?? 0);
  if (key === "coverage") return legacy.duty ?? 0;
  if (key === "specificity") return Math.max(legacy.direct ?? 0, legacy.vibration ?? 0);
  if (key === "reuse") return legacy.simulation ?? 0;
  return 0;
}

function gradeLabel(score: ResearchSource["score"]) {
  if (score === "A") return "A · Direkt";
  if (score === "B") return "B · Gut";
  if (score === "C") return "C · Ergänzend";
  return "D · Modell";
}

function statusLabel(status: ResearchRun["status"]) {
  if (status === "draft") return "Entwurf";
  if (status === "collecting") return "Sammelt";
  return "Synthese";
}

function activityStatusLabel(status: ResearchExpansionRequest["status"] | NonNullable<ResearchRun["researchProgress"]>["status"]) {
  if (status === "running") return "Läuft";
  if (status === "done") return "Fertig";
  if (status === "error") return "Fehler";
  return "Geplant";
}

function activityRequestLabel(request: ResearchExpansionRequest) {
  if (request.status === "done") {
    return request.targetAdditionalSources ? `Fertig · +${request.targetAdditionalSources} Kandidaten geprüft` : `Fertig · ${request.query}`;
  }
  if (request.status === "running") {
    return request.targetAdditionalSources ? `Läuft · +${request.targetAdditionalSources} Kandidaten` : `Läuft · ${request.query}`;
  }
  return request.targetAdditionalSources ? `Angefragt · +${request.targetAdditionalSources} Kandidaten` : `Angefragt · ${request.query}`;
}

function researchProgressPercent(progress: NonNullable<ResearchRun["researchProgress"]>) {
  if (progress.status === "done") return 100;
  if (progress.status === "queued") return 0;
  const target = progress.targetAdditionalSources ?? 0;
  if (target <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((progress.identifiedDelta / target) * 100)));
}

function graphKindLabel(kind: GraphNode["kind"]) {
  if (kind === "query") return "Suchrichtung";
  if (kind === "group") return "Gruppe";
  return "Quelle";
}

function idleResearchProgress(run: ResearchRun): NonNullable<ResearchRun["researchProgress"]> {
  return {
    status: "done",
    currentStep: run.sources.length > 0 ? "Keine laufende Recherche" : "Noch nicht gestartet",
    currentQuery: run.prompt,
    targetAdditionalSources: undefined,
    identifiedDelta: 0,
    readDelta: 0,
    usedDelta: 0,
    updatedAt: run.updated
  };
}

function buildResearchQueueInstruction(run: ResearchRun, amount: number) {
  const existingSources = run.sources.map((source) => ({
    id: source.id,
    title: source.title,
    url: source.url,
    type: source.type,
    group: source.group,
    score: source.score,
    scoreValue: source.scoreValue
  }));
  return [
    `Setze die Marketing-Research-Recherche "${run.title}" fort und erweitere sie um ${amount} neue Quellenkandidaten.`,
    `Research-Run-ID: ${run.id}.`,
    run.prompt ? `Auftrag: ${run.prompt}` : null,
    run.criteria ? `Kriterien: ${run.criteria}` : null,
    `Bestehende Quellen (${existingSources.length}) dürfen nicht ersetzt werden. Nutze sie als Ausgangskorpus und dedupliziere neue Treffer nach URL, DOI oder Titel.`,
    `Bestehende Quellen als JSON:\n${JSON.stringify(existingSources, null, 2)}`,
    "Nutze den allgemeinen deep-research/source-review Prozess: natürliche Suchanfragen, erste Quellen lesen, Quellenfamilien/Kriterien aus den Quellen ableiten, dann Discovery-Graph erweitern.",
    "Fortsetzungssuche: Starte nicht neu. Baue Follow-up-Queries aus bestehenden Quellen, Hosts, Titeln, Quellenfamilien und offenen Lücken. Ergänze nur neue Quellen.",
    "Schreibe Fortschritt in den Research-Run zurück: researchProgress.status, currentStep, currentQuery, identifiedDelta, readDelta, usedDelta.",
    "Füge neue Quellen in sources hinzu, aktualisiere screenedCount/acceptedCount, source groups und graph.nodes/graph.edges. Keine simulierten Counts.",
    "Persistiere die aktualisierten Daten über /api/marketing/research-runs."
  ].filter(Boolean).join("\n");
}

function displaySummary(run: ResearchRun, locale: SupportedLocale) {
  const summary = locale === "de" ? run.summary.de : run.summary.en;
  if (/LLM|Query|query-to-source|Research/i.test(summary)) {
    return locale === "de"
      ? "Kuratierte Quellenkarte mit Scoring, Quellenhinweisen, Kriterien und Discovery-Beziehungen."
      : "Curated source map with scoring, source notes, criteria and discovery relationships.";
  }
  return summary;
}

function slugify(value: string) {
  return value
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/[\s_-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "research";
}

function saveStateLabel(state: "idle" | "saving" | "saved" | "error") {
  if (state === "saving") return "speichert ...";
  if (state === "saved") return "gespeichert";
  if (state === "error") return "nicht gespeichert";
  return "";
}
