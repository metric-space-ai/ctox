import { getMarketingResearchRuns, upsertMarketingResearchRun } from "./marketing-research-store";
import { marketingSeed, type ResearchRun, type ResearchSource, type ResearchSourceScore } from "./marketing-seed";
import { runCtoxSourceReviewDiscovery } from "./ctox-core-bridge";

const MINIMAX_MODEL = "MiniMax-M2.7";

type SearchHit = {
  title: string;
  url: string;
  snippet: string;
  query: string;
};

type ReviewedHit = {
  accept: boolean;
  title?: string;
  group?: string;
  type?: string;
  score?: number;
  access?: string;
  contribution?: string;
  fields?: string;
  use?: string;
  missing?: string;
  tags?: string[];
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

  const discoveryDir = `/home/ubuntu/.local/state/ctox/business-os/source-review/${run.id}`;
  const queries = await generateSourceReviewQueries(topic, target, run.sources.length > 0);
  const payload = await runCtoxSourceReviewDiscovery({
    topic,
    runId: run.id,
    title: run.title,
    queries,
    targetAdditionalSources: target,
    workspace: discoveryDir,
    existingDiscoveryDir: run.sources.length > 0 ? discoveryDir : undefined,
    databaseUrl: process.env.DATABASE_URL,
    openaiApiKey: process.env.OPENAI_API_KEY,
    storeKey: "marketing/research/runs"
  });
  const refreshed = await getMarketingResearchRuns(marketingSeed.researchRuns, { includeArchived: true });
  const nextRun = refreshed.find((item) => item.id === run.id);
  if (nextRun) return nextRun;

  const fallbackRun = {
    ...run,
    researchProgress: {
      status: "error" as const,
      currentStep: "Source-Review lieferte keinen gespeicherten Run",
      currentQuery: String(payload.summary ?? topic),
      targetAdditionalSources: target,
      identifiedDelta: numberField(payload.screened_unique_sources),
      readDelta: numberField(payload.unique_sources),
      usedDelta: 0,
      updatedAt: new Date().toISOString()
    }
  };
  await upsertMarketingResearchRun(fallbackRun, marketingSeed.researchRuns);
  return fallbackRun;
}

async function generateSourceReviewQueries(topic: string, target: number, continuation: boolean) {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) throw new Error("source_review_llm_query_planner_not_configured");
  const { default: OpenAI } = await import("openai");
  const client = new OpenAI({ apiKey });
  const response = await client.chat.completions.create({
    model: process.env.CTOX_SOURCE_REVIEW_LLM_MODEL ?? "gpt-5.4-mini",
    response_format: { type: "json_object" },
    messages: [
      {
        role: "system",
        content: "You write strict source-review web search query plans. Return only valid JSON."
      },
      {
        role: "user",
        content: JSON.stringify({
          topic,
          targetAdditionalSources: target,
          continuation,
          instructions: [
            "Return natural raw search queries a careful human researcher would actually type.",
            "Use obvious short seed queries first.",
            "Keep the core object and core data need together.",
            "Add source-container queries for datasets, databases, technical reports, standards, manuals, documentation, repositories and official/public data portals.",
            "Do not use hidden benchmark answers, topic-specific shortcuts, regex-like keyword salad, or overly compressed prompt fragments.",
            "For continuation, add queries that broaden source families without restarting the same exact search."
          ],
          outputSchema: {
            queries: [
              { focus: "seed", query: "plain search query" }
            ]
          }
        })
      }
    ]
  });
  const parsed = JSON.parse(response.choices[0]?.message?.content ?? "{}") as { queries?: Array<{ focus?: unknown; query?: unknown }> };
  const queries = (parsed.queries ?? [])
    .map((item) => ({
      focus: typeof item.focus === "string" && item.focus.trim() ? item.focus.trim().slice(0, 80) : "llm",
      query: typeof item.query === "string" ? item.query.trim() : ""
    }))
    .filter((item) => item.query.length >= 3)
    .slice(0, Math.max(8, Math.min(24, Math.ceil(target / 10) + 8)));
  if (queries.length === 0) throw new Error("source_review_llm_query_planner_returned_no_queries");
  return queries;
}

async function persistPartialResults(
  run: ResearchRun,
  accepted: ResearchSource[],
  screenedDelta: number,
  readDelta: number,
  currentQuery: string,
  target: number
) {
  const nextRun = mergeResearchResults(run, accepted, screenedDelta);
  nextRun.status = "collecting";
  nextRun.researchProgress = {
    status: "running",
    currentStep: "Quellen werden bewertet",
    currentQuery,
    targetAdditionalSources: target,
    identifiedDelta: screenedDelta,
    readDelta,
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

function mapDeepResearchSources(payload: Record<string, unknown>, topic: string): ResearchSource[] {
  const rawSources = Array.isArray(payload.sources) ? payload.sources as Array<Record<string, unknown>> : [];
  return rawSources.map((source, index) => {
    const title = stringField(source.title) || stringField(source.url) || `Research source ${index + 1}`;
    const url = stringField(source.url);
    const host = safeHostname(url);
    const sourceType = stringField(source.source_type) || stringField(source.source) || "source";
    const read = objectField(source.read);
    const readOk = read?.ok === true;
    const scoreValue = scoreDeepResearchSource(source, topic, readOk);
    const score: ResearchSourceScore = scoreValue >= 78 ? "A" : scoreValue >= 60 ? "B" : scoreValue >= 42 ? "C" : "D";
    const group = groupDeepResearchSource(source);
    const snippet = stringField(source.summary) || stringField(source.snippet);
    return {
      id: `src-${slugify(host || sourceType)}-${slugify(title).slice(0, 42)}`,
      title: title.slice(0, 120),
      group,
      type: labelSourceType(sourceType),
      publisher: host || stringField(source.source) || "ctox deep research",
      year: String(source.year ?? inferYear(`${title} ${snippet}`)),
      score,
      scoreValue,
      contribution: snippet || "Verified by CTOX deep research source discovery.",
      access: readOk ? "readable public source" : source.metadata_only ? "metadata only" : "public source",
      url,
      tags: [group, labelSourceType(sourceType), stringField(source.search_label)].filter(Boolean).slice(0, 5),
      fields: snippet || "Source metadata and available extracted content.",
      use: readOk ? "Use as a checked source from the deep-research reading pass." : "Use as a discovered source; read status is limited.",
      missing: readOk ? "Validate exact tables/measurements before final engineering use." : "Full source extraction was not completed or failed.",
      fit: {
        primary: readOk ? 4 : 2,
        structured: source.metadata_only ? 2 : 3,
        coverage: Math.max(1, Math.min(5, Math.round(scoreValue / 20))),
        specificity: Math.max(1, Math.min(5, Math.round(scoreValue / 20))),
        reuse: readOk ? 4 : 2
      },
      links: url ? [{ label: host || "Source", url }] : []
    };
  }).filter((source) => source.url);
}

function deepResearchCounts(payload: Record<string, unknown>, usedCount: number) {
  const callCounts = objectField(payload.research_call_counts);
  const searchRuns = Array.isArray(payload.search_runs) ? payload.search_runs as Array<Record<string, unknown>> : [];
  const identifiedFromRuns = searchRuns.reduce((sum, run) => sum + numberField(run.result_count), 0);
  return {
    identified: numberField(callCounts?.deduplicated_sources) || identifiedFromRuns || usedCount,
    read: numberField(callCounts?.sources_with_page_read_attempts) || usedCount,
    used: usedCount
  };
}

function scoreDeepResearchSource(source: Record<string, unknown>, topic: string, readOk: boolean) {
  const text = `${stringField(source.title)} ${stringField(source.snippet)} ${stringField(source.summary)} ${stringField(source.url)}`.toLowerCase();
  let score = scoreHit(text, topic);
  if (readOk) score += 12;
  if (source.metadata_only) score -= 12;
  if (source.scholarly) score += 6;
  return clampScore(score);
}

function groupDeepResearchSource(source: Record<string, unknown>) {
  const text = `${stringField(source.title)} ${stringField(source.snippet)} ${stringField(source.url)}`.toLowerCase();
  const type = stringField(source.source_type) || stringField(source.source);
  return inferGroup(text, labelSourceType(type));
}

function labelSourceType(type: string) {
  const normalized = type.replace(/_/g, " ").trim();
  if (!normalized) return "source";
  if (normalized === "paper metadata") return "technical report";
  return normalized;
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

async function reviewSearchHits(topic: string, hits: SearchHit[]) {
  const apiKey = process.env.MINIMAX_API_KEY?.trim();
  if (!apiKey || hits.length === 0) return fallbackReviewedHits(topic, hits);

  const prompt = [
    "Return only JSON with a property results.",
    "results must be an array with one item per candidate URL.",
    "Evaluate search results for a research task. This is source screening, not search-query generation.",
    "Accept only sources that are plausibly useful research inputs for the task: datasets, databases, technical documentation, official reports, papers, repositories, standards, or primary/credible domain sources.",
    "Reject shopping pages, generic SEO articles, news without data, product category pages, irrelevant broad pages, and weak results that do not help the research question.",
    "Do not accept every result. Be selective. If the title/snippet is not enough to justify use, reject it.",
    "For accepted sources, provide score 0-100, group, type, access, contribution, fields, use, missing, and tags.",
    `Research task:\n${topic}`,
    `Candidates:\n${JSON.stringify(hits, null, 2)}`
  ].join("\n\n");

  const response = await fetch("https://api.minimax.io/v1/chat/completions", {
    method: "POST",
    headers: {
      "authorization": `Bearer ${apiKey}`,
      "content-type": "application/json"
    },
    body: JSON.stringify({
      model: MINIMAX_MODEL,
      messages: [
        { role: "system", content: "You are a strict research source screener. You reject weak search results and only keep useful sources." },
        { role: "user", content: prompt }
      ],
      temperature: 0.1
    })
  });
  if (!response.ok) return fallbackReviewedHits(topic, hits);
  const payload = await response.json() as { choices?: Array<{ message?: { content?: string } }> };
  const content = payload.choices?.[0]?.message?.content ?? "";
  const match = content.match(/\{[\s\S]*\}/);
  if (!match) return fallbackReviewedHits(topic, hits);
  const parsed = JSON.parse(match[0]) as { results?: Array<ReviewedHit & { url?: string }> };
  const reviews = new Map<string, ReviewedHit>();
  for (const result of parsed.results ?? []) {
    if (!result.url) continue;
    reviews.set(normalizeUrl(result.url), result);
  }
  return reviews;
}

async function searchWeb(query: string): Promise<SearchHit[]> {
  const providers = [
    searchWithSerper,
    searchWithBrave,
    searchWithDuckDuckGoHtml,
    searchWithBingHtml
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

function buildResearchSource(hit: SearchHit, topic: string, review?: ReviewedHit): ResearchSource {
  const host = safeHostname(hit.url);
  const text = `${hit.title} ${hit.snippet} ${hit.url}`.toLowerCase();
  const scoreValue = clampScore(review?.score ?? scoreHit(text, topic));
  const score: ResearchSourceScore = scoreValue >= 78 ? "A" : scoreValue >= 60 ? "B" : scoreValue >= 42 ? "C" : "D";
  const type = cleanLabel(review?.type) ?? inferType(text, host);
  const group = cleanLabel(review?.group) ?? inferGroup(text, type);
  return {
    id: `src-${slugify(host)}-${slugify(hit.title).slice(0, 42)}`,
    title: (review?.title || hit.title).slice(0, 120),
    group,
    type,
    publisher: host || "public web",
    year: inferYear(text),
    score,
    scoreValue,
    contribution: review?.contribution || hit.snippet || `Search result found for: ${hit.query}`,
    access: review?.access || "public web",
    url: hit.url,
    tags: [...(review?.tags ?? []), hit.query, group, type].filter(Boolean).slice(0, 5),
    fields: review?.fields || hit.snippet || "Public source metadata and linked content.",
    use: review?.use || "Candidate source for the research question; inspect source content before final use.",
    missing: review?.missing || "Needs source-level reading and validation before relying on it."
  };
}

function fallbackReviewedHits(topic: string, hits: SearchHit[]) {
  const topicTerms = uniqueStrings(topic.toLowerCase().split(/[^a-z0-9äöüß]+/).filter((term) => term.length > 3));
  const reviews = new Map<string, ReviewedHit>();
  for (const hit of hits) {
    const text = `${hit.title} ${hit.snippet} ${hit.url}`.toLowerCase();
    const host = safeHostname(hit.url);
    const topicMatches = topicTerms.filter((term) => text.includes(term)).length;
    const hasResearchSignal = /\b(dataset|database|data|download|csv|repository|github|zenodo|figshare|mendeley|technical report|report|paper|publication|documentation|docs|manual|pdf|standard|specification)\b/.test(text);
    const weakSource = /\b(shop|shopping|buy|price|amazon|forum|reddit|hacker news|steam|scribd|pinterest|youtube|school district|news|blog)\b/.test(text);
    const accept = hasResearchSignal && topicMatches >= 2 && !weakSource;
    reviews.set(normalizeUrl(hit.url), {
      accept,
      score: accept ? scoreHit(text, topic) : 0,
      group: inferGroup(text, inferType(text, host)),
      type: inferType(text, host),
      access: "public web",
      contribution: hit.snippet,
      fields: hit.snippet,
      use: accept ? "Potentially useful source for the research task." : "",
      missing: "Needs source-level validation.",
      tags: [hit.query]
    });
  }
  return reviews;
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

function stringField(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function numberField(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function objectField(value: unknown) {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

function clampScore(value: number) {
  return Math.max(20, Math.min(92, Math.round(value)));
}

function cleanLabel(value?: string) {
  const label = value?.replace(/\s+/g, " ").trim();
  return label || undefined;
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
