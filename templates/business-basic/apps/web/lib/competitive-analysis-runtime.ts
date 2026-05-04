import { execFile } from "node:child_process";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";

const execFileAsync = promisify(execFile);

export const COMPETITIVE_ANALYSIS_TARGET_KEY = "marketing-competitive-analysis";

export type ScrapeTriggerKind = "manual" | "scheduled" | "criterion_added" | "watchlist_added";

export type WatchlistCompany = {
  id: string;
  name: string;
  url: string;
  source: "seed" | "manual" | "search" | "own_benchmark";
};

export type ScrapePlan = {
  enabled: boolean;
  targetKey: string;
  triggerKind: ScrapeTriggerKind;
  scheduledFor?: string;
  commands: string[][];
};

export const seedWatchlist: WatchlistCompany[] = [
  { id: "own-product", name: "Own product", url: process.env.CTOX_OWN_PRODUCT_URL ?? "https://example.com", source: "own_benchmark" },
  { id: "11x", name: "11x", url: "https://11x.ai/", source: "seed" },
  { id: "kore-ai", name: "Kore.ai", url: "https://kore.ai/", source: "seed" },
  { id: "artisan", name: "Artisan", url: "https://www.artisan.co/", source: "seed" },
  { id: "lindy", name: "Lindy", url: "https://www.lindy.ai/", source: "seed" },
  { id: "upagents", name: "UpAgents", url: "https://upagents.app/", source: "seed" },
  { id: "relevance-ai", name: "Relevance AI", url: "https://relevanceai.com/", source: "seed" },
  { id: "agentalent-ai", name: "Agentalent.ai", url: "https://agentalent.ai/", source: "seed" },
  { id: "ada", name: "Ada", url: "https://www.ada.cx/", source: "seed" }
];

export function buildOwnBenchmark() {
  const webRepo = process.env.CTOX_OWN_WEB_REPO ?? process.env.GITHUB_REPOSITORY ?? "local-web-repo";
  const productRepo = process.env.CTOX_OWN_PRODUCT_REPO ?? process.env.GITHUB_REPOSITORY ?? "local-product-repo";

  return {
    id: "own-product",
    name: process.env.CTOX_OWN_PRODUCT_NAME ?? "Own product",
    source: "github_repository",
    url: process.env.CTOX_OWN_PRODUCT_URL ?? "https://example.com",
    repositories: {
      web: webRepo,
      product: productRepo
    },
    method: "Repository-derived benchmark from public web copy, product routes, integration surfaces, pricing/trust pages, and release velocity. Live installs should replace the default repository hints with GitHub connector or CTOX doc/search inputs.",
    dimensions: {
      positioning: 66,
      overlap: 70,
      buyerClarity: 58,
      employeeCatalog: 62,
      hiringFlow: 55,
      providerApi: 68,
      pricingClarity: 45,
      trust: 54,
      seoVelocity: 48
    }
  };
}

export function buildScrapeTargetPayload(watchlist: WatchlistCompany[] = seedWatchlist) {
  return {
    target_key: COMPETITIVE_ANALYSIS_TARGET_KEY,
    display_name: "Marketing Competitive Analysis",
    start_url: watchlist[0]?.url ?? "https://example.com",
    target_kind: "competitive_analysis",
    schedule_hint: "daily",
    output_schema: {
      schema_key: "competitive_analysis.v1",
      record_key_fields: ["id"]
    },
    config: {
      api: {
        filter_fields: [
          "id",
          "name",
          "url",
          "source",
          "is_own_product",
          "classification.category",
          "scores.positioning",
          "scores.buyer_clarity",
          "scores.trust"
        ]
      },
      expected_min_records: Math.max(1, watchlist.length),
      record_key_fields: ["id"],
      sources: watchlist.map((company) => ({
        source_key: company.id,
        display_name: company.name,
        start_url: company.url,
        source_kind: company.source === "own_benchmark" ? "own_benchmark" : "company_website",
        enabled: true,
        merge_strategy: "upsert_by_record_key",
        tags: company.source === "own_benchmark" ? ["own-product", "benchmark"] : ["competitor"],
        config: {
          company_name: company.name,
          company_url: company.url,
          is_own_product: company.source === "own_benchmark",
          scoring_model: "competitive-analysis-v1"
        }
      }))
    }
  };
}

export async function queueCompetitiveScrape(options: {
  triggerKind: ScrapeTriggerKind;
  criterion?: string;
  mode?: "rescrape_now" | "next_standard_scrape";
  scheduledFor?: string;
  watchlist?: WatchlistCompany[];
}) {
  const scheduledFor = options.scheduledFor ?? new Date().toISOString();
  const plan = buildScrapePlan(options.triggerKind, scheduledFor);

  if (!plan.enabled || options.mode === "next_standard_scrape") {
    const task = {
      id: crypto.randomUUID(),
      type: "ctox.scrape",
      status: "queued",
      targetKey: COMPETITIVE_ANALYSIS_TARGET_KEY,
      triggerKind: options.triggerKind,
      mode: options.mode ?? "rescrape_now",
      criterion: options.criterion ?? null,
      scheduledFor
    };
    const core = await createCtoxCoreTask({
      title: `Competitive analysis scrape: ${options.triggerKind}`,
      prompt: `Run or schedule CTOX scrape target ${COMPETITIVE_ANALYSIS_TARGET_KEY} for Marketing / Competitive Analysis.`,
      source: "business-competitive-analysis-scrape",
      context: { task, plan },
      priority: options.triggerKind === "scheduled" ? "normal" : "high",
      skill: "universal-scraping",
      threadKey: "business/marketing/competitive-analysis/scrape"
    });
    await emitCtoxCoreEvent({
      type: "business.scrape_queued",
      module: "marketing",
      recordType: "scrape_target",
      recordId: COMPETITIVE_ANALYSIS_TARGET_KEY,
      payload: { task, plan, core }
    });
    return {
      ok: true,
      queued: true,
      executed: false,
      task: { ...task, coreTaskId: core.taskId },
      core,
      plan
    };
  }

  const tempDir = await mkdtemp(join(tmpdir(), "ctox-business-scrape-"));
  const payloadPath = join(tempDir, "target.json");
  const scriptPath = join(tempDir, "extractor.js");
  await writeFile(payloadPath, JSON.stringify(buildScrapeTargetPayload(options.watchlist), null, 2));
  await writeFile(scriptPath, buildExtractorSource(options.watchlist ?? seedWatchlist));

  const commands = [
    [resolveCtoxBinary(), "scrape", "init"],
    [resolveCtoxBinary(), "scrape", "upsert-target", "--input", payloadPath],
    [resolveCtoxBinary(), "scrape", "register-script", "--target-key", COMPETITIVE_ANALYSIS_TARGET_KEY, "--script-file", scriptPath, "--change-reason", options.triggerKind],
    [resolveCtoxBinary(), "scrape", "execute", "--target-key", COMPETITIVE_ANALYSIS_TARGET_KEY, "--trigger-kind", options.triggerKind === "scheduled" ? "scheduled" : "manual", "--scheduled-for", scheduledFor, "--allow-heal"]
  ];

  const results = [];
  for (const command of commands) {
    const [binary, ...args] = command;
    const { stdout } = await execFileAsync(binary, args, { cwd: process.env.CTOX_ROOT });
    results.push(JSON.parse(stdout));
  }

  await emitCtoxCoreEvent({
    type: "business.scrape_executed",
    module: "marketing",
    recordType: "scrape_target",
    recordId: COMPETITIVE_ANALYSIS_TARGET_KEY,
    payload: { triggerKind: options.triggerKind, scheduledFor, results }
  });

  return { ok: true, queued: false, executed: true, plan: { ...plan, commands }, results };
}

export async function searchCompetitorCompanies(query: string) {
  const cleanedQuery = query.trim();
  if (!cleanedQuery) return { ok: false, error: "query_required" };

  const command = [resolveCtoxBinary(), "web", "search", "--query", cleanedQuery];
  if (shouldExecuteCtox()) {
    const { stdout } = await execFileAsync(command[0], command.slice(1), { cwd: process.env.CTOX_ROOT });
    return { ok: true, executed: true, query: cleanedQuery, result: JSON.parse(stdout) };
  }

  const task = {
    id: crypto.randomUUID(),
    type: "ctox.web_search",
    status: "queued",
    query: cleanedQuery,
    model: process.env.CTOX_COMPETITIVE_MODEL ?? "minimax-m2.7",
    purpose: "Find initial competitor companies for Marketing / Competitive Analysis"
  };
  const core = await createCtoxCoreTask({
    title: `Find competitors: ${cleanedQuery}`,
    prompt: `Use CTOX web search to find initial competitor companies for this competitive-analysis workspace.\n\nQuery: ${cleanedQuery}`,
    source: "business-competitive-analysis-search",
    context: { task, targetKey: COMPETITIVE_ANALYSIS_TARGET_KEY },
    priority: "normal",
    skill: "universal-scraping",
    threadKey: "business/marketing/competitive-analysis/search"
  });

  return {
    ok: true,
    executed: false,
    query: cleanedQuery,
    task: { ...task, coreTaskId: core.taskId },
    core,
    plan: {
      enabled: false,
      commands: [command]
    }
  };
}

export function addManualCompany(input: { name?: string; url?: string }) {
  const name = input.name?.trim();
  const url = input.url?.trim();

  if (!url) return { ok: false, error: "url_required" };

  const parsed = new URL(url);
  const company: WatchlistCompany = {
    id: slug(name || parsed.hostname.replace(/^www\./, "")),
    name: name || titleFromHost(parsed.hostname),
    url: parsed.toString(),
    source: "manual"
  };

  return {
    ok: true,
    company,
    scrape: {
      targetKey: COMPETITIVE_ANALYSIS_TARGET_KEY,
      mode: "watchlist_added",
      nextAction: "queue_scrape"
    }
  };
}

export async function emitManualCompanyAdded(company: WatchlistCompany) {
  const core = await createCtoxCoreTask({
    title: `Add competitor source: ${company.name}`,
    prompt: `Add ${company.name} (${company.url}) to the Marketing / Competitive Analysis watchlist and include it in the CTOX scrape target ${COMPETITIVE_ANALYSIS_TARGET_KEY}.`,
    source: "business-watchlist",
    context: { company, targetKey: COMPETITIVE_ANALYSIS_TARGET_KEY },
    priority: "normal",
    skill: "universal-scraping",
    threadKey: "business/marketing/competitive-analysis/watchlist"
  });
  await emitCtoxCoreEvent({
    type: "business.watchlist_added",
    module: "marketing",
    recordType: "competitor",
    recordId: company.id,
    payload: { company, core }
  });

  return core;
}

function buildScrapePlan(triggerKind: ScrapeTriggerKind, scheduledFor: string): ScrapePlan {
  return {
    enabled: shouldExecuteCtox(),
    targetKey: COMPETITIVE_ANALYSIS_TARGET_KEY,
    triggerKind,
    scheduledFor,
    commands: [
      [resolveCtoxBinary(), "scrape", "init"],
      [resolveCtoxBinary(), "scrape", "upsert-target", "--input", "<generated-target-json>"],
      [resolveCtoxBinary(), "scrape", "register-script", "--target-key", COMPETITIVE_ANALYSIS_TARGET_KEY, "--script-file", "<generated-extractor-js>", "--change-reason", triggerKind],
      [resolveCtoxBinary(), "scrape", "execute", "--target-key", COMPETITIVE_ANALYSIS_TARGET_KEY, "--trigger-kind", triggerKind === "scheduled" ? "scheduled" : "manual", "--scheduled-for", scheduledFor, "--allow-heal"]
    ]
  };
}

function buildExtractorSource(watchlist: WatchlistCompany[]) {
  const records = watchlist.map((company) => ({
    id: company.id,
    name: company.name,
    url: company.url,
    source: company.source,
    is_own_product: company.source === "own_benchmark"
  }));

  return `console.log(JSON.stringify({ records: ${JSON.stringify(records, null, 2)} }));\n`;
}

function resolveCtoxBinary() {
  return process.env.CTOX_WEB_BIN ?? process.env.CTOX_BIN ?? "ctox";
}

function shouldExecuteCtox() {
  return process.env.CTOX_BUSINESS_ENABLE_CTOX_EXECUTION === "true";
}

function slug(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "company";
}

function titleFromHost(hostname: string) {
  return hostname.replace(/^www\./, "").split(".")[0]?.replace(/-/g, " ") ?? hostname;
}
