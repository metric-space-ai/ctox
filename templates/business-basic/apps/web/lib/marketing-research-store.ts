import { loadRuntimeJsonStore, saveRuntimeJsonStore } from "./runtime-json-store";
import type { ResearchRun } from "./marketing-seed";

const RESEARCH_RUNS_STORE_KEY = "marketing/research/runs";

export async function getMarketingResearchRuns(fallback: ResearchRun[], options: { includeArchived?: boolean } = {}): Promise<ResearchRun[]> {
  const stored = await loadRuntimeJsonStore<ResearchRun[]>(RESEARCH_RUNS_STORE_KEY);
  const runs = !Array.isArray(stored) || stored.length === 0 ? fallback : mergeStoredResearchRuns(stored, fallback);
  return options.includeArchived ? runs : runs.filter((run) => !run.archivedAt);
}

export async function saveMarketingResearchRuns(runs: ResearchRun[]) {
  return saveRuntimeJsonStore(RESEARCH_RUNS_STORE_KEY, runs);
}

export async function upsertMarketingResearchRun(run: ResearchRun, fallback: ResearchRun[]) {
  const runs = await getMarketingResearchRuns(fallback, { includeArchived: true });
  const nextRuns = [run, ...runs.filter((item) => item.id !== run.id)];
  const persisted = await saveMarketingResearchRuns(nextRuns);
  return { persisted, runs: nextRuns };
}

export async function archiveMarketingResearchRun(runId: string, fallback: ResearchRun[]) {
  const runs = await getMarketingResearchRuns(fallback, { includeArchived: true });
  const now = new Date().toISOString();
  const nextRuns = runs.map((run) => run.id === runId ? { ...run, archivedAt: run.archivedAt ?? now } : run);
  const persisted = await saveMarketingResearchRuns(nextRuns);
  return { persisted, runs: nextRuns.filter((run) => !run.archivedAt) };
}

function mergeStoredResearchRuns(stored: ResearchRun[], fallback: ResearchRun[]) {
  const fallbackById = new Map(fallback.map((run) => [run.id, run]));
  const merged = stored.map((run) => mergeResearchRun(run, fallbackById.get(run.id)));
  const storedIds = new Set(stored.map((run) => run.id));
  return [...merged, ...fallback.filter((run) => !storedIds.has(run.id))];
}

function mergeResearchRun(stored: ResearchRun, fallback?: ResearchRun): ResearchRun {
  if (!fallback) return stored;
  return {
    ...fallback,
    ...stored,
    queryCount: stored.queryCount,
    screenedCount: stored.screenedCount,
    acceptedCount: stored.acceptedCount,
    sources: stored.sources,
    graph: {
      nodes: stored.graph.nodes,
      edges: stored.graph.edges
    },
    expansionRequests: stored.expansionRequests ?? fallback.expansionRequests,
    criteriaItems: stored.criteriaItems ?? fallback.criteriaItems,
    sourceGroupLabels: stored.sourceGroupLabels ?? fallback.sourceGroupLabels,
    hiddenSourceGroups: stored.hiddenSourceGroups ?? fallback.hiddenSourceGroups,
    customSourceGroups: stored.customSourceGroups ?? fallback.customSourceGroups,
    researchProgress: stored.researchProgress ?? fallback.researchProgress
  };
}

function mergeById<T extends { id: string }>(fallback: T[], stored: T[]) {
  const out = new Map(fallback.map((item) => [item.id, item]));
  for (const item of stored) out.set(item.id, { ...out.get(item.id), ...item });
  return [...out.values()];
}

function mergeGraphEdges<T extends { source: string; target: string; relation: string }>(fallback: T[], stored: T[]) {
  const out = new Map(fallback.map((item) => [`${item.source}:${item.target}:${item.relation}`, item]));
  for (const item of stored) out.set(`${item.source}:${item.target}:${item.relation}`, item);
  return [...out.values()];
}
