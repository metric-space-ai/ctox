import { getMarketingResearchRuns } from "./marketing-research-store";

export type SupportedLocale = "en" | "de";
export type Localized = Record<SupportedLocale, string>;

export type MarketingPerson = {
  id: string;
  name: string;
  role: string;
};

export type WebsitePage = {
  id: string;
  title: string;
  path: string;
  status: "draft" | "review" | "published";
  ownerId: string;
  updated: string;
  intent: Localized;
  nextAction: Localized;
};

export type MarketingAsset = {
  id: string;
  name: string;
  kind: "deck" | "one-pager" | "case-study" | "visual" | "email";
  status: "draft" | "review" | "ready";
  ownerId: string;
  updated: string;
  audience: Localized;
  usage: Localized;
};

export type Campaign = {
  id: string;
  name: string;
  channel: "email" | "linkedin" | "webinar" | "search" | "partner";
  status: "planned" | "active" | "paused";
  ownerId: string;
  launch: string;
  target: Localized;
  nextAction: Localized;
};

export type ResearchItem = {
  id: string;
  title: string;
  kind: "market-note" | "persona" | "interview" | "pricing" | "search";
  status: "queued" | "collecting" | "synthesized";
  ownerId: string;
  updated: string;
  insight: Localized;
  linkedCampaignIds: string[];
};

export type ResearchSourceScore = "A" | "B" | "C" | "D";

export type ResearchSource = {
  id: string;
  title: string;
  group: string;
  type: string;
  publisher: string;
  year: string;
  score: ResearchSourceScore;
  scoreValue: number;
  contribution: string;
  access: string;
  url: string;
  tags?: string[];
  fields?: string;
  use?: string;
  missing?: string;
  fit?: Record<string, number>;
  links?: Array<{ label: string; url: string }>;
};

export type ResearchGraphNode = {
  id: string;
  label: string;
  kind: "query" | "source" | "group";
  score?: ResearchSourceScore;
};

export type ResearchGraphEdge = {
  source: string;
  target: string;
  relation: string;
};

export type ResearchExpansionRequest = {
  id: string;
  createdAt: string;
  query: string;
  criteria: string;
  targetAdditionalSources?: number;
  status: "queued" | "running" | "done";
};

export type ResearchCriterion = {
  id: string;
  label: string;
  description: string;
  active: boolean;
  createdAt: string;
  updatedAt: string;
};

export type ResearchSourceGroup = {
  id: string;
  label: string;
  createdAt: string;
  updatedAt: string;
};

export type ResearchProgress = {
  status: "queued" | "running" | "done" | "error";
  currentStep: string;
  currentQuery?: string;
  targetAdditionalSources?: number;
  identifiedDelta: number;
  readDelta: number;
  usedDelta: number;
  updatedAt: string;
  taskId?: string;
};

export type ResearchRun = {
  id: string;
  title: string;
  status: "draft" | "collecting" | "synthesized";
  updated: string;
  prompt?: string;
  criteria?: string;
  archivedAt?: string;
  queryCount: number;
  screenedCount: number;
  acceptedCount: number;
  summary: Localized;
  sources: ResearchSource[];
  graph: {
    nodes: ResearchGraphNode[];
    edges: ResearchGraphEdge[];
  };
  expansionRequests?: ResearchExpansionRequest[];
  criteriaItems?: ResearchCriterion[];
  sourceGroupLabels?: Record<string, string>;
  hiddenSourceGroups?: string[];
  customSourceGroups?: ResearchSourceGroup[];
  researchProgress?: ResearchProgress;
};

export type CommerceItem = {
  id: string;
  name: string;
  kind: "service" | "subscription" | "workshop" | "audit";
  status: "draft" | "listed" | "review";
  price: string;
  ownerId: string;
  nextAction: Localized;
};

export type MarketingBundle = {
  people: MarketingPerson[];
  websitePages: WebsitePage[];
  assets: MarketingAsset[];
  campaigns: Campaign[];
  researchItems: ResearchItem[];
  researchRuns: ResearchRun[];
  commerceItems: CommerceItem[];
};

export const marketingSeed: MarketingBundle = {
  people: [
    { id: "maria", name: "Maria Chen", role: "Marketing lead" },
    { id: "noah", name: "Noah Weber", role: "Demand generation" },
    { id: "elena", name: "Elena Rossi", role: "Content systems" },
    { id: "sam", name: "Sam Patel", role: "Product marketing" }
  ],
  websitePages: [
    {
      id: "home",
      title: "Public home",
      path: "/",
      status: "draft",
      ownerId: "sam",
      updated: "2026-05-02",
      intent: { en: "Empty public surface with top-right login preserved.", de: "Leere öffentliche Fläche mit erhaltenem Login rechts oben." },
      nextAction: { en: "Add customer-specific first viewport once positioning is approved.", de: "Kundenspezifischen ersten Viewport ergänzen, sobald Positionierung freigegeben ist." }
    },
    {
      id: "product",
      title: "Product overview",
      path: "/product",
      status: "draft",
      ownerId: "sam",
      updated: "2026-05-01",
      intent: { en: "Explain the operating-system app without replacing the vanilla template.", de: "Business-OS-App erklären, ohne das Vanilla-Template zu ersetzen." },
      nextAction: { en: "Connect approved screenshots from app modules.", de: "Freigegebene Screenshots aus den App-Modulen verbinden." }
    },
    {
      id: "security",
      title: "Security and hosting",
      path: "/security",
      status: "review",
      ownerId: "elena",
      updated: "2026-04-30",
      intent: { en: "Answer self-hosting, Vercel, Neon, and data-boundary questions.", de: "Self-Hosting-, Vercel-, Neon- und Datengrenzen-Fragen beantworten." },
      nextAction: { en: "Sync proof points with Business and Operations docs.", de: "Proof Points mit Business- und Operations-Dokumenten synchronisieren." }
    }
  ],
  assets: [
    {
      id: "intro-deck",
      name: "Business OS intro deck",
      kind: "deck",
      status: "review",
      ownerId: "maria",
      updated: "2026-05-01",
      audience: { en: "Founder and operator buyers", de: "Founder und operative Käufer" },
      usage: { en: "Sales discovery and first demo follow-up.", de: "Sales Discovery und Follow-up nach der ersten Demo." }
    },
    {
      id: "self-hosting-sheet",
      name: "Self-hosting one-pager",
      kind: "one-pager",
      status: "ready",
      ownerId: "elena",
      updated: "2026-04-29",
      audience: { en: "Technical evaluators", de: "Technische Evaluatoren" },
      usage: { en: "Attach to security and procurement conversations.", de: "An Security- und Procurement-Gespräche anhängen." }
    },
    {
      id: "launch-email",
      name: "Launch announcement email",
      kind: "email",
      status: "draft",
      ownerId: "noah",
      updated: "2026-05-02",
      audience: { en: "Early customer list", de: "Frühe Kundenliste" },
      usage: { en: "Queue after website copy is approved.", de: "Nach Freigabe des Website-Texts einplanen." }
    }
  ],
  campaigns: [
    {
      id: "founder-demo",
      name: "Founder demo sequence",
      channel: "email",
      status: "active",
      ownerId: "noah",
      launch: "2026-05-06",
      target: { en: "Founder-led teams replacing spreadsheets with CTOX-managed software.", de: "Founder-geführte Teams, die Tabellen durch CTOX-verwaltete Software ersetzen." },
      nextAction: { en: "Insert Sales pipeline feedback after the next five demos.", de: "Sales-Pipeline-Feedback nach den nächsten fünf Demos einarbeiten." }
    },
    {
      id: "ops-webinar",
      name: "Operations workspace webinar",
      channel: "webinar",
      status: "planned",
      ownerId: "maria",
      launch: "2026-05-21",
      target: { en: "Service businesses that need projects, wiki, and tickets in one app.", de: "Service-Unternehmen, die Projekte, Wiki und Tickets in einer App brauchen." },
      nextAction: { en: "Use Operations module starter data as the demo narrative.", de: "Operations-Starterdaten als Demo-Erzählung verwenden." }
    },
    {
      id: "self-host-search",
      name: "Self-hosted business stack search",
      channel: "search",
      status: "planned",
      ownerId: "elena",
      launch: "2026-05-14",
      target: { en: "Buyers comparing Next.js, Postgres, Vercel, and Neon business software.", de: "Käufer, die Next.js-, Postgres-, Vercel- und Neon-Business-Software vergleichen." },
      nextAction: { en: "Connect competitive-analysis keywords to page briefs.", de: "Keywords aus Wettbewerbsanalyse mit Page-Briefs verbinden." }
    }
  ],
  researchItems: [
    {
      id: "operator-persona",
      title: "Operator buyer persona",
      kind: "persona",
      status: "synthesized",
      ownerId: "sam",
      updated: "2026-05-01",
      insight: { en: "Buyers want customization without losing a stable operating system shell.", de: "Käufer wollen Customizing, ohne die stabile Business-OS-Shell zu verlieren." },
      linkedCampaignIds: ["founder-demo", "ops-webinar"]
    },
    {
      id: "self-host-intent",
      title: "Self-hosting search intent",
      kind: "search",
      status: "collecting",
      ownerId: "elena",
      updated: "2026-04-30",
      insight: { en: "Intent clusters around Postgres portability, Vercel deployment, and data ownership.", de: "Intent clustert um Postgres-Portabilität, Vercel-Deployment und Dateneigentum." },
      linkedCampaignIds: ["self-host-search"]
    },
    {
      id: "pricing-anchors",
      title: "Pricing anchor notes",
      kind: "pricing",
      status: "queued",
      ownerId: "maria",
      updated: "2026-04-28",
      insight: { en: "Starter stack should price against custom software setup plus ongoing CTOX changes.", de: "Starter-Stack sollte gegen Custom-Software-Setup plus laufende CTOX-Änderungen gepreist werden." },
      linkedCampaignIds: ["founder-demo"]
    }
  ],
  researchRuns: [],
  commerceItems: [
    {
      id: "starter-stack",
      name: "Business Basic setup",
      kind: "service",
      status: "listed",
      price: "€4,900",
      ownerId: "maria",
      nextAction: { en: "Sync offer terms with Business products and invoice templates.", de: "Angebotsbedingungen mit Business-Produkten und Rechnungsvorlagen synchronisieren." }
    },
    {
      id: "monthly-customizing",
      name: "Monthly CTOX customization",
      kind: "subscription",
      status: "draft",
      price: "€1,500/mo",
      ownerId: "sam",
      nextAction: { en: "Define included agent-run capacity and change limits.", de: "Enthaltene Agent-Run-Kapazität und Änderungsgrenzen definieren." }
    },
    {
      id: "operations-audit",
      name: "Operations module audit",
      kind: "audit",
      status: "review",
      price: "€1,200",
      ownerId: "elena",
      nextAction: { en: "Connect audit output to Operations knowledge pages.", de: "Audit-Ergebnis mit Operations-Knowledge-Seiten verbinden." }
    }
  ]
};

export async function getMarketingBundle(): Promise<MarketingBundle> {
  if (!shouldUsePostgres()) return marketingSeed;

  try {
    const db = await import("@ctox-business/db/modules");
    const [websiteRows, assetRows, campaignRows, researchRows, commerceRows] = await Promise.all([
      db.listModuleRecords("marketing", "website"),
      db.listModuleRecords("marketing", "assets"),
      db.listModuleRecords("marketing", "campaigns"),
      db.listModuleRecords("marketing", "research"),
      db.listModuleRecords("marketing", "commerce")
    ]);

    const shouldSeed = (websiteRows?.length ?? 0) === 0 && (campaignRows?.length ?? 0) === 0 && shouldAutoSeedPostgres();
    if (shouldSeed) {
      await db.seedModuleRecords("marketing", marketingSeedRecords());
      return getMarketingBundle();
    }

    return {
      people: marketingSeed.people,
      websitePages: rowsToPayload(websiteRows, marketingSeed.websitePages),
      assets: rowsToPayload(assetRows, marketingSeed.assets),
      campaigns: rowsToPayload(campaignRows, marketingSeed.campaigns),
      researchItems: rowsToPayload(researchRows, marketingSeed.researchItems),
      researchRuns: await getMarketingResearchRuns(marketingSeed.researchRuns),
      commerceItems: rowsToPayload(commerceRows, marketingSeed.commerceItems)
    };
  } catch (error) {
    console.warn("Falling back to Marketing seed data.", error);
    return marketingSeed;
  }
}

export async function getMarketingResource(resource: string) {
  const data = await getMarketingBundle();
  if (resource === "website") return data.websitePages;
  if (resource === "assets") return data.assets;
  if (resource === "campaigns") return data.campaigns;
  if (resource === "research") return data.researchItems;
  if (resource === "research-runs") return data.researchRuns;
  if (resource === "commerce") return data.commerceItems;
  if (resource === "people") return data.people;
  return null;
}

export function text(value: Localized, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}

function marketingSeedRecords() {
  return {
    assets: marketingSeed.assets.map((asset) => ({
      id: asset.id,
      label: asset.name,
      status: asset.status,
      ownerId: asset.ownerId,
      payload: asset
    })),
    campaigns: marketingSeed.campaigns.map((campaign) => ({
      id: campaign.id,
      label: campaign.name,
      status: campaign.status,
      ownerId: campaign.ownerId,
      payload: campaign
    })),
    commerce: marketingSeed.commerceItems.map((item) => ({
      id: item.id,
      label: item.name,
      status: item.status,
      ownerId: item.ownerId,
      payload: item
    })),
    research: marketingSeed.researchItems.map((item) => ({
      id: item.id,
      label: item.title,
      status: item.status,
      ownerId: item.ownerId,
      payload: item
    })),
    website: marketingSeed.websitePages.map((page) => ({
      id: page.id,
      label: page.title,
      status: page.status,
      ownerId: page.ownerId,
      payload: page
    }))
  };
}

function rowsToPayload<T>(rows: Array<{ payloadJson: string }> | null | undefined, fallback: T[]): T[] {
  if (!rows || rows.length === 0) return fallback;
  return rows.map((row) => parseJson(row.payloadJson)).filter(Boolean) as T[];
}

function parseJson(value: string) {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

function shouldUsePostgres() {
  const value = process.env.DATABASE_URL;
  return Boolean(value && !value.includes("user:password@localhost"));
}

function shouldAutoSeedPostgres() {
  return process.env.CTOX_BUSINESS_AUTO_SEED !== "false";
}
