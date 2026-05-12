import { getMarketingResearchRuns, upsertMarketingResearchRun } from "./marketing-research-store";
import { marketingSeed, type ResearchRun, type ResearchSource, type ResearchSourceScore } from "./marketing-seed";

const MINIMAX_MODEL = "MiniMax-M2.7";

type SearchHit = {
  title: string;
  url: string;
  snippet: string;
  query: string;
};

export async function runMarketingResearch(runId: string, amount: number) {
  const runs = await getMarketingResearchRuns(marketingSeed.researchRuns, { includeArchived: true });
  const run = runs.find((item) => item.id === runId);
  if (!run) throw new Error("research_run_not_found");

  const target = Math.max(5, Math.min(amount || 25, 200));
  const topic = [run.prompt, run.criteria, ...(run.criteriaItems ?? []).map((item) => `${item.label}: ${item.description}`)]
    .filter(Boolean)
    .join("\n");

  await persistRunProgress(run, {
    status: "running",
    currentStep: "Recherche gestartet",
    currentQuery: topic,
    targetAdditionalSources: target,
    identifiedDelta: 0,
    readDelta: 0,
    usedDelta: 0,
    updatedAt: new Date().toISOString()
  });

  const existingUrls = new Set(run.sources.map((source) => normalizeUrl(source.url)));
  const queries = await generateSearchQueries(topic, run);
  const accepted: ResearchSource[] = [];
  const screened: SearchHit[] = [];

  for (let index = 0; index < queries.length && accepted.length < target; index += 1) {
    const query = queries[index];
    await persistRunProgress(run, {
      status: "running",
      currentStep: `Suchrichtung ${index + 1} von ${queries.length}`,
      currentQuery: query,
      targetAdditionalSources: target,
      identifiedDelta: screened.length,
      readDelta: accepted.length,
      usedDelta: run.sources.length + accepted.length,
      updatedAt: new Date().toISOString()
    });

    const hits = await searchWeb(query);
    for (const hit of hits) {
      const key = normalizeUrl(hit.url);
      if (!key || existingUrls.has(key)) continue;
      existingUrls.add(key);
      screened.push(hit);
      const source = buildResearchSource(hit, topic);
      accepted.push(source);
      if (accepted.length >= target) break;
    }

    await persistPartialResults(run, accepted, screened.length, query, target);
  }

  const nextRun = mergeResearchResults(run, accepted, screened.length);
  nextRun.status = nextRun.sources.length > 0 ? "collecting" : "draft";
  nextRun.researchProgress = {
    status: nextRun.sources.length > run.sources.length ? "done" : "error",
    currentStep: nextRun.sources.length > run.sources.length ? "Recherche aktualisiert" : "Keine neuen Quellen gefunden",
    currentQuery: queries[queries.length - 1] ?? topic,
    targetAdditionalSources: target,
    identifiedDelta: screened.length,
    readDelta: accepted.length,
    usedDelta: nextRun.sources.length,
    updatedAt: new Date().toISOString()
  };
  nextRun.expansionRequests = (nextRun.expansionRequests ?? []).map((request, index) => (
    index === 0 && request.status !== "done" ? { ...request, status: nextRun.researchProgress?.status === "done" ? "done" : "running" } : request
  ));
  await upsertMarketingResearchRun(nextRun, marketingSeed.researchRuns);
  return nextRun;
}

async function persistPartialResults(run: ResearchRun, accepted: ResearchSource[], screenedDelta: number, currentQuery: string, target: number) {
  const nextRun = mergeResearchResults(run, accepted, screenedDelta);
  nextRun.status = "collecting";
  nextRun.researchProgress = {
    status: "running",
    currentStep: "Quellen werden bewertet",
    currentQuery,
    targetAdditionalSources: target,
    identifiedDelta: screenedDelta,
    readDelta: accepted.length,
    usedDelta: nextRun.sources.length,
    updatedAt: new Date().toISOString()
  };
  await upsertMarketingResearchRun(nextRun, marketingSeed.researchRuns);
}

async function persistRunProgress(run: ResearchRun, progress: NonNullable<ResearchRun["researchProgress"]>) {
  await upsertMarketingResearchRun({
    ...run,
    status: progress.status === "done" ? "synthesized" : "collecting",
    researchProgress: progress,
    updated: new Date().toISOString().slice(0, 10)
  }, marketingSeed.researchRuns);
}

function mergeResearchResults(run: ResearchRun, accepted: ResearchSource[], screenedDelta: number): ResearchRun {
  const sourcesById = new Map(run.sources.map((source) => [source.id, source]));
  for (const source of accepted) sourcesById.set(source.id, source);
  const sources = [...sourcesById.values()].sort((left, right) => right.scoreValue - left.scoreValue);
  const groups = [...new Set(sources.map((source) => source.group))];
  const queryNodes = [...new Set(accepted.map((source) => source.tags?.[0]).filter(Boolean) as string[])].map((query) => ({
    id: `q-${slugify(query)}`,
    label: query,
    kind: "query" as const
  }));
  const groupNodes = groups.map((group) => ({
    id: `g-${slugify(group)}`,
    label: group,
    kind: "group" as const
  }));
  const sourceNodes = sources.map((source) => ({
    id: source.id,
    label: source.title,
    kind: "source" as const,
    score: source.score
  }));
  const edges = [
    ...sources.map((source) => ({ source: source.id, target: `g-${slugify(source.group)}`, relation: "belongs_to" })),
    ...accepted.map((source) => ({ source: `q-${slugify(source.tags?.[0] ?? source.group)}`, target: source.id, relation: "found" }))
  ];
  return {
    ...run,
    sources,
    queryCount: Math.max(run.queryCount, queryNodes.length),
    screenedCount: Math.max(run.screenedCount + screenedDelta, screenedDelta),
    acceptedCount: Math.max(run.acceptedCount + accepted.length, sources.length),
    graph: {
      nodes: dedupeById([...run.graph.nodes, ...queryNodes, ...groupNodes, ...sourceNodes]),
      edges: dedupeEdges([...run.graph.edges, ...edges])
    },
    updated: new Date().toISOString().slice(0, 10)
  };
}

async function generateSearchQueries(topic: string, run: ResearchRun) {
  const prompt = [
    "Return only JSON with a property queries, an array of natural web search queries.",
    "The queries must be direct, obvious, human search-engine queries. No cryptic boolean syntax, no over-engineered wording.",
    "Make the queries generally useful for the user's research task and source discovery.",
    "Use the current task, criteria, and already found source titles to continue instead of restarting.",
    `Task:\n${topic}`,
    `Existing sources:\n${run.sources.map((source) => `- ${source.title} ${source.url}`).join("\n") || "None yet"}`
  ].join("\n\n");

  const llmQueries = await minimaxJsonQueries(prompt).catch(() => []);
  const fallback = [
    topic,
    `${topic} dataset`,
    `${topic} database`,
    `${topic} technical report`,
    `${topic} public data`,
    `${topic} benchmark`
  ];
  return uniqueStrings([...llmQueries, ...fallback])
    .map((query) => query.replace(/\s+/g, " ").trim())
    .filter((query) => query.length > 5)
    .slice(0, 12);
}

async function minimaxJsonQueries(prompt: string) {
  const apiKey = process.env.MINIMAX_API_KEY?.trim();
  if (!apiKey) return [];
  const response = await fetch("https://api.minimax.io/v1/chat/completions", {
    method: "POST",
    headers: {
      "authorization": `Bearer ${apiKey}`,
      "content-type": "application/json"
    },
    body: JSON.stringify({
      model: MINIMAX_MODEL,
      messages: [
        { role: "system", content: "You write concise, natural web search queries for research discovery." },
        { role: "user", content: prompt }
      ],
      temperature: 0.2
    })
  });
  if (!response.ok) return [];
  const payload = await response.json() as { choices?: Array<{ message?: { content?: string } }> };
  const content = payload.choices?.[0]?.message?.content ?? "";
  const match = content.match(/\{[\s\S]*\}/);
  if (!match) return [];
  const parsed = JSON.parse(match[0]) as { queries?: unknown };
  return Array.isArray(parsed.queries) ? parsed.queries.map(String) : [];
}

async function searchWeb(query: string): Promise<SearchHit[]> {
  const providers = [
    searchWithSerper,
    searchWithBrave,
    searchWithDuckDuckGoHtml,
    searchWithBingHtml,
    searchWithOpenAlex,
    searchWithCrossref
  ];
  const hits: SearchHit[] = [];
  const seen = new Set<string>();

  for (const provider of providers) {
    const providerHits = await provider(query).catch(() => []);
    for (const hit of providerHits) {
      const key = normalizeUrl(hit.url);
      if (!key || seen.has(key)) continue;
      seen.add(key);
      hits.push(hit);
    }
    if (hits.length >= 10) break;
  }

  return hits.slice(0, 12);
}

async function searchWithBrave(query: string): Promise<SearchHit[]> {
  const apiKey = process.env.BRAVE_SEARCH_API_KEY?.trim();
  if (!apiKey) return [];
  const response = await fetch(`https://api.search.brave.com/res/v1/web/search?q=${encodeURIComponent(query)}&count=10`, {
    headers: {
      "accept": "application/json",
      "x-subscription-token": apiKey
    }
  });
  if (!response.ok) return [];
  const payload = await response.json() as { web?: { results?: Array<{ title?: string; url?: string; description?: string }> } };
  return (payload.web?.results ?? []).map((item) => ({
    title: stripHtml(item.title ?? ""),
    url: item.url ?? "",
    snippet: stripHtml(item.description ?? ""),
    query
  })).filter(validHit);
}

async function searchWithSerper(query: string): Promise<SearchHit[]> {
  const apiKey = process.env.SERPER_API_KEY?.trim();
  if (!apiKey) return [];
  const response = await fetch("https://google.serper.dev/search", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": apiKey
    },
    body: JSON.stringify({ q: query, num: 10 })
  });
  if (!response.ok) return [];
  const payload = await response.json() as { organic?: Array<{ title?: string; link?: string; snippet?: string }> };
  return (payload.organic ?? []).map((item) => ({
    title: stripHtml(item.title ?? ""),
    url: item.link ?? "",
    snippet: stripHtml(item.snippet ?? ""),
    query
  })).filter(validHit);
}

async function searchWithOpenAlex(query: string): Promise<SearchHit[]> {
  const response = await fetch(`https://api.openalex.org/works?search=${encodeURIComponent(query)}&per-page=10`, {
    headers: { "accept": "application/json" }
  });
  if (!response.ok) return [];
  const payload = await response.json() as {
    results?: Array<{
      title?: string;
      display_name?: string;
      publication_year?: number;
      doi?: string;
      primary_location?: { landing_page_url?: string; pdf_url?: string; source?: { display_name?: string } };
      abstract_inverted_index?: Record<string, number[]>;
    }>;
  };
  return (payload.results ?? []).map((item) => {
    const title = item.title ?? item.display_name ?? "";
    const landingUrl = item.primary_location?.landing_page_url ?? item.primary_location?.pdf_url ?? item.doi ?? "";
    const source = item.primary_location?.source?.display_name;
    const year = item.publication_year ? String(item.publication_year) : "";
    return {
      title: stripHtml(title),
      url: landingUrl,
      snippet: [source, year, abstractFromInvertedIndex(item.abstract_inverted_index)].filter(Boolean).join(" · "),
      query
    };
  }).filter(validHit);
}

async function searchWithCrossref(query: string): Promise<SearchHit[]> {
  const response = await fetch(`https://api.crossref.org/works?query=${encodeURIComponent(query)}&rows=10`, {
    headers: { "accept": "application/json" }
  });
  if (!response.ok) return [];
  const payload = await response.json() as {
    message?: {
      items?: Array<{
        title?: string[];
        URL?: string;
        DOI?: string;
        publisher?: string;
        type?: string;
        published?: { "date-parts"?: number[][] };
        "container-title"?: string[];
      }>;
    };
  };
  return (payload.message?.items ?? []).map((item) => {
    const title = item.title?.[0] ?? "";
    const year = item.published?.["date-parts"]?.[0]?.[0];
    return {
      title: stripHtml(title),
      url: item.URL ?? (item.DOI ? `https://doi.org/${item.DOI}` : ""),
      snippet: [item.publisher, item.type, item["container-title"]?.[0], year].filter(Boolean).join(" · "),
      query
    };
  }).filter(validHit);
}

async function searchWithBingHtml(query: string): Promise<SearchHit[]> {
  const response = await fetch(`https://www.bing.com/search?q=${encodeURIComponent(query)}`, {
    headers: {
      "accept": "text/html,application/xhtml+xml",
      "accept-language": "en-US,en;q=0.9",
      "user-agent": "Mozilla/5.0 (compatible; research-source-discovery/1.0)"
    }
  });
  if (!response.ok) return [];
  const html = await response.text();
  const results: SearchHit[] = [];
  const blocks = html.split("<li class=\"b_algo").slice(1, 12);
  for (const block of blocks) {
    const link = block.match(/<h2[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/);
    if (!link) continue;
    const snippet = block.match(/<div class="b_caption"[^>]*>\s*<p[^>]*>([\s\S]*?)<\/p>/);
    const title = stripHtml(decodeHtml(link[2] ?? ""));
    const url = unwrapBingUrl(decodeHtml(link[1] ?? ""));
    if (!title || !url.startsWith("http")) continue;
    results.push({
      title,
      url,
      snippet: stripHtml(decodeHtml(snippet?.[1] ?? "")),
      query
    });
  }
  return results;
}

async function searchWithDuckDuckGoHtml(query: string): Promise<SearchHit[]> {
  const response = await fetch(`https://html.duckduckgo.com/html/?q=${encodeURIComponent(query)}`, {
    headers: {
      "accept": "text/html,application/xhtml+xml",
      "accept-language": "en-US,en;q=0.9",
      "user-agent": "Mozilla/5.0 (compatible; research-source-discovery/1.0)"
    }
  });
  if (!response.ok) return [];
  const html = await response.text();
  if (html.includes("anomaly-modal") || html.includes("Unfortunately, bots use DuckDuckGo too")) return [];
  const results: SearchHit[] = [];
  const blocks = html.split(/<div class="result/i).slice(1, 12);
  for (const block of blocks) {
    const link = block.match(/class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/);
    if (!link) continue;
    const snippet = block.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/a>|class="result__snippet"[^>]*>([\s\S]*?)<\/div>/);
    const title = stripHtml(decodeHtml(link[2] ?? ""));
    const url = unwrapDuckDuckGoUrl(decodeHtml(link[1] ?? ""));
    if (!title || !url.startsWith("http")) continue;
    results.push({
      title,
      url,
      snippet: stripHtml(decodeHtml(snippet?.[1] ?? snippet?.[2] ?? "")),
      query
    });
  }
  return results;
}

function buildResearchSource(hit: SearchHit, topic: string): ResearchSource {
  const host = safeHostname(hit.url);
  const text = `${hit.title} ${hit.snippet} ${hit.url}`.toLowerCase();
  const scoreValue = scoreHit(text, topic);
  const score: ResearchSourceScore = scoreValue >= 78 ? "A" : scoreValue >= 60 ? "B" : scoreValue >= 42 ? "C" : "D";
  const type = inferType(text, host);
  const group = inferGroup(text, type);
  return {
    id: `src-${slugify(host)}-${slugify(hit.title).slice(0, 42)}`,
    title: hit.title.slice(0, 120),
    group,
    type,
    publisher: host || "public web",
    year: inferYear(text),
    score,
    scoreValue,
    contribution: hit.snippet || `Search result found for: ${hit.query}`,
    access: "public web",
    url: hit.url,
    tags: [hit.query, group, type].filter(Boolean).slice(0, 4),
    fields: hit.snippet || "Public source metadata and linked content.",
    use: "Candidate source for the research question; inspect source content before final use.",
    missing: "Needs source-level reading and validation before relying on it."
  };
}

function scoreHit(text: string, topic: string) {
  const topicTerms = uniqueStrings(topic.toLowerCase().split(/[^a-z0-9äöüß]+/).filter((term) => term.length > 3));
  const matches = topicTerms.filter((term) => text.includes(term)).length;
  let score = 35 + Math.min(30, matches * 4);
  if (/dataset|database|data|download|csv|xlsx|repository|github|zenodo|figshare|mendeley/.test(text)) score += 15;
  if (/report|paper|publication|technical|nasa|university|doi|pdf/.test(text)) score += 10;
  if (/forum|reddit|pinterest|youtube|advertisement/.test(text)) score -= 15;
  return Math.max(20, Math.min(92, score));
}

function inferType(text: string, host: string) {
  if (/dataset|zenodo|figshare|mendeley|kaggle|repository|github/.test(text)) return "dataset";
  if (/database|data site|catalog/.test(text)) return "database";
  if (/report|technical|pdf|nasa|dtic/.test(text)) return "technical report";
  if (/paper|journal|doi|publication|proceedings/.test(text)) return "publication";
  if (/docs|documentation|manual/.test(text)) return "documentation";
  return host.includes(".edu") ? "academic source" : "web source";
}

function inferGroup(text: string, type: string) {
  if (/flight|telemetry|log|imu/.test(text)) return "Flight logs";
  if (/bench|test stand|thrust|torque|motor|propeller/.test(text)) return "Bench data";
  if (/wind tunnel|rotor|aero/.test(text)) return "Wind tunnel";
  if (/simulation|simulator|gazebo|model/.test(text)) return "Simulation";
  if (/vibration|fault|failure/.test(text)) return "Vibration";
  if (type === "dataset" || type === "database") return "Datasets";
  if (type === "technical report" || type === "publication") return "Technical reports";
  return "Web sources";
}

function inferYear(text: string) {
  return text.match(/\b(20[0-2][0-9]|19[8-9][0-9])\b/)?.[1] ?? "current";
}

function unwrapDuckDuckGoUrl(url: string) {
  try {
    const parsed = new URL(url, "https://duckduckgo.com");
    const uddg = parsed.searchParams.get("uddg");
    return uddg ? decodeURIComponent(uddg) : parsed.href;
  } catch {
    return url;
  }
}

function unwrapBingUrl(url: string) {
  try {
    const parsed = new URL(url, "https://www.bing.com");
    const encoded = parsed.searchParams.get("u");
    if (encoded?.startsWith("a1")) {
      return Buffer.from(encoded.slice(2), "base64url").toString("utf8");
    }
    return parsed.href;
  } catch {
    return url;
  }
}

function abstractFromInvertedIndex(index?: Record<string, number[]>) {
  if (!index) return "";
  return Object.entries(index)
    .sort((left, right) => Math.min(...left[1]) - Math.min(...right[1]))
    .slice(0, 42)
    .map(([word]) => word)
    .join(" ");
}

function validHit(hit: SearchHit) {
  return Boolean(hit.title && hit.url.startsWith("http"));
}

function safeHostname(url: string) {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

function stripHtml(value: string) {
  return value.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
}

function decodeHtml(value: string) {
  return value
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, "\"")
    .replace(/&#x27;/g, "'")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

function normalizeUrl(url: string) {
  return url.replace(/#.*$/, "").replace(/\/$/, "").toLowerCase();
}

function slugify(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 80) || "source";
}

function uniqueStrings(values: string[]) {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

function dedupeById<T extends { id: string }>(items: T[]) {
  return [...new Map(items.map((item) => [item.id, item])).values()];
}

function dedupeEdges<T extends { source: string; target: string; relation: string }>(items: T[]) {
  return [...new Map(items.map((item) => [`${item.source}:${item.target}:${item.relation}`, item])).values()];
}
