import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import { PortfolioMap } from "./portfolio-map";
import { ScrapeRunButton } from "./scrape-run-button";
import { ScoreModelEditor } from "./score-model-editor";
import { WatchlistManager } from "./watchlist-manager";

type QueryState = {
  locale?: string;
  theme?: string;
  xAxis?: string;
  yAxis?: string;
  panel?: string;
  recordId?: string;
  drawer?: string;
};

type SupportedLocale = "en" | "de";
type Localized = Record<SupportedLocale, string>;
type AxisId =
  | "positioning"
  | "overlap"
  | "buyerClarity"
  | "employeeCatalog"
  | "hiringFlow"
  | "providerApi"
  | "pricingClarity"
  | "trust"
  | "seoVelocity";

const axisOptions: Array<{ id: AxisId; label: Localized }> = [
  { id: "positioning", label: { en: "Positioning", de: "Positionierung" } },
  { id: "overlap", label: { en: "Overlap", de: "Überschneidung" } },
  { id: "buyerClarity", label: { en: "Buyer clarity", de: "Käuferklarheit" } },
  { id: "employeeCatalog", label: { en: "AI employee catalog", de: "KI-Mitarbeiter-Katalog" } },
  { id: "hiringFlow", label: { en: "Hiring flow", de: "Hiring Flow" } },
  { id: "providerApi", label: { en: "Provider API", de: "Provider API" } },
  { id: "pricingClarity", label: { en: "Pricing clarity", de: "Preisklarheit" } },
  { id: "trust", label: { en: "Trust", de: "Vertrauen" } },
  { id: "seoVelocity", label: { en: "SEO velocity", de: "SEO-Geschwindigkeit" } }
];

const competitors = [
  { rank: 0, id: "own-product", name: "Own product", kind: { en: "Target benchmark", de: "Ziel-Benchmark" }, score: 5.88, dimensions: { positioning: 66, overlap: 70, buyerClarity: 58, employeeCatalog: 62, hiringFlow: 55, providerApi: 68, pricingClarity: 45, trust: 54, seoVelocity: 48 }, status: { en: "Own", de: "Eigen" }, signal: { en: "Repository-derived own-product benchmark from web and product GitHub projects.", de: "Aus Web- und Produkt-GitHub-Projekten abgeleiteter Eigenprodukt-Benchmark." }, isOwn: true },
  { rank: 1, id: "11x", name: "11x", kind: { en: "Managed AI workers", de: "Gemanagte KI-Arbeiter" }, score: 6.33, dimensions: { overlap: 78, buyerClarity: 76, trust: 55, seoVelocity: 68 }, status: { en: "Leader", de: "Leader" }, signal: { en: "Strongest overlap with the target operating model.", de: "Stärkste Überschneidung mit dem Ziel-Betriebsmodell." } },
  { rank: 2, id: "kore-ai", name: "Kore.ai", kind: { en: "Enterprise AI agent platform", de: "Enterprise-Plattform für KI-Agenten" }, score: 5.72, dimensions: { overlap: 57, buyerClarity: 70, trust: 72, seoVelocity: 48 }, status: { en: "Enterprise", de: "Enterprise" }, signal: { en: "Mature enterprise platform signal.", de: "Reifes Enterprise-Plattform-Signal." } },
  { rank: 3, id: "artisan", name: "Artisan", kind: { en: "AI employee platform", de: "KI-Mitarbeiter-Plattform" }, score: 5.61, dimensions: { overlap: 64, buyerClarity: 68, trust: 44, seoVelocity: 61 }, status: { en: "Direct", de: "Direkt" }, signal: { en: "Clear AI employee category positioning.", de: "Klare Positionierung in der KI-Mitarbeiter-Kategorie." } },
  { rank: 4, id: "lindy", name: "Lindy", kind: { en: "AI agents for work", de: "KI-Agenten für Arbeit" }, score: 5.59, dimensions: { overlap: 43, buyerClarity: 40, trust: 42, seoVelocity: 73 }, status: { en: "Workflow", de: "Workflow" }, signal: { en: "Strong workflow automation pull.", de: "Starker Zug in Richtung Workflow-Automation." } },
  { rank: 5, id: "upagents", name: "UpAgents", kind: { en: "AI agent marketplace", de: "Marktplatz für KI-Agenten" }, score: 5.4, dimensions: { overlap: 64, buyerClarity: 62, trust: 38, seoVelocity: 52 }, status: { en: "Marketplace", de: "Marktplatz" }, signal: { en: "Marketplace model is strategically adjacent.", de: "Marktplatzmodell ist strategisch angrenzend." } },
  { rank: 6, id: "relevance-ai", name: "Relevance AI", kind: { en: "AI workforce platform", de: "KI-Workforce-Plattform" }, score: 5.24, dimensions: { overlap: 57, buyerClarity: 66, trust: 49, seoVelocity: 58 }, status: { en: "Platform", de: "Plattform" }, signal: { en: "Direct AI workforce vocabulary.", de: "Direkte KI-Workforce-Sprache." } },
  { rank: 7, id: "agentalent-ai", name: "Agentalent.ai", kind: { en: "AI agent hiring marketplace", de: "Hiring-Marktplatz für KI-Agenten" }, score: 5.15, dimensions: { overlap: 57, buyerClarity: 32, trust: 35, seoVelocity: 43 }, status: { en: "Hiring", de: "Hiring" }, signal: { en: "Hiring framing with weaker buyer clarity.", de: "Hiring-Framing mit schwächerer Käuferklarheit." } },
  { rank: 8, id: "ada", name: "Ada", kind: { en: "AI customer service platform", de: "KI-Kundenservice-Plattform" }, score: 5.12, dimensions: { overlap: 50, buyerClarity: 56, trust: 66, seoVelocity: 46 }, status: { en: "Support", de: "Support" }, signal: { en: "Category-adjacent support automation proof.", de: "Angrenzender Nachweis für Support-Automation." } },
  { rank: 9, id: "decagon", name: "Decagon", kind: { en: "Customer support AI agent platform", de: "KI-Agentenplattform für Kundensupport" }, score: 4.72, dimensions: { overlap: 48, buyerClarity: 54, trust: 58, seoVelocity: 44 }, status: { en: "Support", de: "Support" }, signal: { en: "Support-agent category with integration pressure.", de: "Support-Agenten-Kategorie mit Integrationsdruck." } },
  { rank: 10, id: "ema", name: "Ema", kind: { en: "Universal AI employee platform", de: "Universelle KI-Mitarbeiter-Plattform" }, score: 4.68, dimensions: { overlap: 54, buyerClarity: 48, trust: 52, seoVelocity: 42 }, status: { en: "Employee", de: "Mitarbeiter" }, signal: { en: "Broad AI employee framing, less focused buyer path.", de: "Breites KI-Mitarbeiter-Framing mit weniger klarem Käuferpfad." } },
  { rank: 11, id: "sierra", name: "Sierra", kind: { en: "Customer service AI agent platform", de: "KI-Agentenplattform für Kundenservice" }, score: 4.31, dimensions: { overlap: 42, buyerClarity: 52, trust: 61, seoVelocity: 39 }, status: { en: "Support", de: "Support" }, signal: { en: "Strong service automation signal outside the core hiring path.", de: "Starkes Service-Automation-Signal außerhalb des Kern-Hiring-Pfads." } },
  { rank: 12, id: "andela-ai", name: "Andela AI Talent", kind: { en: "Global talent marketplace", de: "Globaler Talent-Marktplatz" }, score: 3.71, dimensions: { overlap: 34, buyerClarity: 44, trust: 63, seoVelocity: 34 }, status: { en: "Talent", de: "Talent" }, signal: { en: "Human-talent reference point for buyer expectations.", de: "Human-Talent-Referenz für Käufererwartungen." } },
  { rank: 13, id: "qualified-piper", name: "Qualified Piper", kind: { en: "AI SDR agent", de: "KI-SDR-Agent" }, score: 3.68, dimensions: { overlap: 46, buyerClarity: 50, trust: 31, seoVelocity: 37 }, status: { en: "SDR", de: "SDR" }, signal: { en: "Narrow sales-agent workflow signal.", de: "Enges Sales-Agent-Workflow-Signal." } },
  { rank: 14, id: "agent-ai", name: "Agent.ai", kind: { en: "AI agent marketplace", de: "KI-Agenten-Marktplatz" }, score: 3.65, dimensions: { overlap: 39, buyerClarity: 43, trust: 34, seoVelocity: 55 }, status: { en: "Marketplace", de: "Marktplatz" }, signal: { en: "Marketplace reference with broad catalog vocabulary.", de: "Marktplatzreferenz mit breiter Katalog-Sprache." } },
  { rank: 15, id: "toptal-ai", name: "Toptal AI Talent", kind: { en: "AI talent hiring network", de: "KI-Talent-Hiring-Netzwerk" }, score: 3.26, dimensions: { overlap: 28, buyerClarity: 38, trust: 69, seoVelocity: 32 }, status: { en: "Talent", de: "Talent" }, signal: { en: "Premium hiring network reference.", de: "Premium-Hiring-Netzwerk als Referenz." } },
  { rank: 16, id: "upwork-ai", name: "Upwork AI Talent", kind: { en: "AI talent hiring marketplace", de: "KI-Talent-Hiring-Marktplatz" }, score: 2.4, dimensions: { overlap: 24, buyerClarity: 35, trust: 58, seoVelocity: 29 }, status: { en: "Talent", de: "Talent" }, signal: { en: "Large marketplace baseline with weak operating-model overlap.", de: "Große Marktplatz-Baseline mit schwacher Betriebsmodell-Überschneidung." } },
  { rank: 17, id: "turing-ai", name: "Turing AI Talent", kind: { en: "AI talent hiring platform", de: "KI-Talent-Hiring-Plattform" }, score: 1.13, dimensions: { overlap: 18, buyerClarity: 28, trust: 51, seoVelocity: 22 }, status: { en: "Talent", de: "Talent" }, signal: { en: "Low-overlap hiring reference.", de: "Hiring-Referenz mit niedriger Überschneidung." } }
];

const competitorMeta: Record<string, { checkedAt: string; sourceUrl: string }> = {
  "11x": { checkedAt: "2026-04-27T18:25:25.024Z", sourceUrl: "https://11x.ai/" },
  "own-product": { checkedAt: "2026-05-02T00:00:00.000Z", sourceUrl: "https://example.com" },
  "kore-ai": { checkedAt: "2026-04-27T18:25:30.353Z", sourceUrl: "https://kore.ai/" },
  "artisan": { checkedAt: "2026-04-27T18:25:24.460Z", sourceUrl: "https://www.artisan.co/" },
  "lindy": { checkedAt: "2026-04-27T18:25:25.924Z", sourceUrl: "https://www.lindy.ai/" },
  "upagents": { checkedAt: "2026-04-27T18:25:34.641Z", sourceUrl: "https://upagents.app/" },
  "relevance-ai": { checkedAt: "2026-04-27T18:25:25.628Z", sourceUrl: "https://relevanceai.com/" },
  "agentalent-ai": { checkedAt: "2026-04-27T18:25:33.281Z", sourceUrl: "https://agentalent.ai/" },
  "ada": { checkedAt: "2026-04-27T18:25:37.622Z", sourceUrl: "https://www.ada.cx/" },
  "decagon": { checkedAt: "2026-04-27T18:25:36.306Z", sourceUrl: "https://decagon.ai/" },
  "ema": { checkedAt: "2026-04-27T18:25:34.865Z", sourceUrl: "https://www.ema.ai/" },
  "sierra": { checkedAt: "2026-04-27T18:25:35.549Z", sourceUrl: "https://sierra.ai/" },
  "andela-ai": { checkedAt: "2026-04-27T18:25:32.817Z", sourceUrl: "https://andela.com/" },
  "qualified-piper": { checkedAt: "2026-04-27T18:25:36.840Z", sourceUrl: "https://www.qualified.com/ai-sdr" },
  "agent-ai": { checkedAt: "2026-04-27T18:25:26.239Z", sourceUrl: "https://agent.ai/" },
  "toptal-ai": { checkedAt: "2026-04-27T18:25:31.433Z", sourceUrl: "https://www.toptal.com/artificial-intelligence" },
  "upwork-ai": { checkedAt: "2026-04-27T18:25:31.168Z", sourceUrl: "https://www.upwork.com/hire/artificial-intelligence-developers/" },
  "turing-ai": { checkedAt: "2026-04-27T18:25:31.888Z", sourceUrl: "https://www.turing.com/services/ai" }
};

const criteria = [
  { id: "positioning", label: { en: "Positioning", de: "Positionierung" }, weight: 1.25 },
  { id: "buyer-clarity", label: { en: "Buyer clarity", de: "Käuferklarheit" }, weight: 1.1 },
  { id: "ai-employee-catalog", label: { en: "AI employee catalog", de: "KI-Mitarbeiter-Katalog" }, weight: 1 },
  { id: "hiring-flow", label: { en: "Hiring flow", de: "Hiring Flow" }, weight: 1 },
  { id: "provider-api", label: { en: "Provider API", de: "Provider API" }, weight: 0.9 },
  { id: "pricing-clarity", label: { en: "Pricing clarity", de: "Preisklarheit" }, weight: 0.85 },
  { id: "trust", label: { en: "Trust", de: "Vertrauen" }, weight: 1 },
  { id: "seo-velocity", label: { en: "SEO velocity", de: "SEO-Geschwindigkeit" }, weight: 0.75 }
] as const;

const criterionLabels: Record<string, Localized> = {
  positioning: { en: "Positioning", de: "Positionierung" },
  buyer_clarity: { en: "Buyer clarity", de: "Käuferklarheit" },
  employee_catalog: { en: "Employee catalog", de: "Mitarbeiter-Katalog" },
  hiring_interview_flow: { en: "Hiring flow", de: "Hiring Flow" },
  provider_api_onboarding: { en: "Provider API", de: "Provider API" },
  pricing_clarity: { en: "Pricing clarity", de: "Preisklarheit" },
  trust_compliance: { en: "Trust / compliance", de: "Trust / Compliance" },
  seo_content_velocity: { en: "SEO velocity", de: "SEO-Geschwindigkeit" },
  product_maturity: { en: "Product maturity", de: "Produktreife" },
  change_velocity: { en: "Change velocity", de: "Änderungsgeschwindigkeit" },
  differentiation_vs_own_product: { en: "Differentiation", de: "Differenzierung" }
};

const artisanReport = {
  checks: [
    ["Homepage", "200 OK"],
    ["News/blog", "200 OK"],
    ["Evidence items", "13"],
    ["Formula", "measured-facts-v2"]
  ],
  criteria: {
    positioning: 6,
    buyer_clarity: 6.8,
    employee_catalog: 4.8,
    hiring_interview_flow: 5,
    provider_api_onboarding: 2.5,
    pricing_clarity: 3.25,
    trust_compliance: 5.5,
    seo_content_velocity: 8.5,
    product_maturity: 7.5,
    change_velocity: 5,
    differentiation_vs_own_product: 6.4
  },
  evidence: {
    title: "Boost Your Outbound Sales with an AI BDR from Artisan",
    description: "Artisan automates your outbound with an all-in-one, AI-first platform powered by AI employees. Get better outbound sales results with an AI BDR.",
    headings: [
      "Hire Ava The autonomous AI BDR",
      "Outbound was hard to scale",
      "With Ava, it's easy"
    ],
    snippets: [
      "Careers and product copy repeatedly use AI employee positioning.",
      "Ava is framed as an autonomous AI BDR for lead discovery, outbound campaigns, CRM work, cross-sell, upsell, and meeting booking."
    ]
  },
  factors: [
    ["AI worker terms", "2"],
    ["Buyer use cases", "3"],
    ["Catalog terms", "2"],
    ["Hiring terms", "3"],
    ["Provider terms", "1"],
    ["Pricing terms", "1"],
    ["Trust terms", "3"],
    ["SEO terms", "2"]
  ],
  rationale: "Measured formula: ai_worker=2, hiring=3, provider=1, pricing=1, trust=3.",
  recommendations: [
    "Treat Artisan as a direct category-positioning reference for AI employee language.",
    "Compare Ava's role-specific outbound workflow against the own product catalog and onboarding UX.",
    "Exploit weaker provider API and pricing clarity signals in outbound positioning."
  ],
  risks: [
    "Clear AI employee category positioning can intercept buyers searching for operational AI staff.",
    "SEO and product maturity signals are strong enough to create repeated buyer exposure.",
    "Outbound-specific focus may win sales teams before broader operating-system framing is evaluated."
  ]
};

const opportunities: Localized[] = [
  { en: "Make the hire-interview-select-onboard path operationally concrete.", de: "Den Pfad Hire-Interview-Auswahl-Onboarding operativ konkret machen." },
  { en: "Publish security, compliance, and customer proof on high-intent pages.", de: "Security, Compliance und Kundenbelege auf High-Intent-Seiten veröffentlichen." },
  { en: "Expose API, integration, and provider onboarding docs for buyer validation.", de: "API-, Integrations- und Provider-Onboarding-Dokumente für Käuferprüfung sichtbar machen." }
];

const risks: Localized[] = [
  { en: "The current leader owns the clearest direct competitive position.", de: "Der aktuelle Leader besitzt die klarste direkte Wettbewerbsposition." },
  { en: "The strongest overlap signal sits in managed AI worker positioning.", de: "Das stärkste Überschneidungssignal liegt bei gemanagten KI-Arbeitern." },
  { en: "Trust signals remain weak across the market and can block enterprise conversion.", de: "Vertrauenssignale bleiben marktweit schwach und können Enterprise-Conversions blockieren." }
];

export function CompetitiveAnalysisDashboard({
  query
}: {
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = dashboardCopy[locale];
  const xAxis = resolveAxis(query.xAxis, "overlap");
  const yAxis = resolveAxis(query.yAxis, "buyerClarity");
  const ranked = rankedCompetitors();

  return (
    <div className="os-competitive-dashboard">
      <div className="os-workbench">
        <section className="os-pane os-ranking-pane" aria-label={copy.competitorRanking}>
          <div className="os-pane-head">
            <div>
              <h2>{copy.ranking}</h2>
              <p>{copy.scoreModel}</p>
            </div>
            <div className="os-pane-actions">
              <span>{copy.score}</span>
              <a
                data-context-action="create"
                data-context-item
                data-context-label={copy.addCompetitor}
                data-context-module="marketing"
                data-context-record-id="new-source"
                data-context-record-type="watchlist"
                data-context-submodule="competitive-analysis"
                href={panelHref(query, "watchlist", "new-source", "left-bottom")}
                aria-label={copy.addCompetitor}
              >
                +
              </a>
            </div>
          </div>
          <div className="os-ranking-table">
            {ranked.map((competitor) => {
              const meta = competitorMeta[competitor.id];

              return (
              <div
                className={`os-ranking-row${competitor.isOwn ? " own-product-row" : ""}`}
                data-context-item
                data-context-module="marketing"
                data-context-submodule="competitive-analysis"
                data-context-record-type="competitor"
                data-context-record-id={competitor.id}
                data-context-label={competitor.name}
                key={competitor.id}
              >
                <span className="rank">#{competitor.rank}</span>
                <a className="os-ranking-main" href={panelHref(query, "competitor", competitor.id, "right")}>
                  <strong>{competitor.name}</strong>
                  <small>{text(competitor.kind, locale)}</small>
                </a>
                <span className="os-ranking-updated">
                  <small>{copy.updated}</small>
                  <time dateTime={meta.checkedAt}>{formatCheckedAt(meta.checkedAt, locale)}</time>
                </span>
                <span className="score">{competitor.score.toFixed(2)}</span>
              </div>
            );
            })}
          </div>
          <div className="os-action-dock" role="toolbar" aria-label={copy.actions}>
            <a
              data-context-action="open"
              data-context-item
              data-context-label={copy.criteria}
              data-context-module="marketing"
              data-context-record-id="score-model"
              data-context-record-type="criteria"
              data-context-submodule="competitive-analysis"
              href={panelHref(query, "criteria", "score-model", "left-bottom")}
            >
              {copy.criteria}
            </a>
            <a
              data-context-action="open"
              data-context-item
              data-context-label={copy.watchlist}
              data-context-module="marketing"
              data-context-record-id="new-source"
              data-context-record-type="watchlist"
              data-context-submodule="competitive-analysis"
              href={panelHref(query, "watchlist", "new-source", "left-bottom")}
            >
              {copy.watchlist}
            </a>
            <ScrapeRunButton label={copy.run} />
            <a
              data-context-action="open"
              data-context-item
              data-context-label={copy.draft}
              data-context-module="marketing"
              data-context-record-id="founder-update"
              data-context-record-type="draft"
              data-context-submodule="competitive-analysis"
              href={panelHref(query, "draft", "founder-update", "right")}
            >
              {copy.draft}
            </a>
          </div>
        </section>

        <PortfolioMap
          addCompetitorLabel={copy.addCompetitor}
          competitors={ranked}
          initialXAxis={xAxis}
          initialYAxis={yAxis}
          labels={{
            horizontalAxis: copy.horizontalAxis,
            mapDescription: copy.mapDescription,
            portfolioMap: copy.portfolioMap,
            verticalAxis: copy.verticalAxis
          }}
          locale={locale}
          query={{
            drawer: query.drawer,
            locale: query.locale,
            panel: query.panel,
            recordId: query.recordId,
            theme: query.theme
          }}
          watchlistHref={panelHref(query, "watchlist", "new-source", "left-bottom")}
        />

        <section className="os-pane os-decision-pane" aria-label={copy.decisionNotes}>
          <div className="os-pane-head">
            <div>
              <h2>{copy.decisionNotes}</h2>
              <p>{copy.signals}</p>
            </div>
            <div className="os-pane-actions">
              <a
                data-context-action="create"
                data-context-item
                data-context-label={copy.addNote}
                data-context-module="marketing"
                data-context-record-id="new-note"
                data-context-record-type="draft"
                data-context-submodule="competitive-analysis"
                href={panelHref(query, "draft", "new-note", "right")}
                aria-label={copy.addNote}
              >
                +
              </a>
            </div>
          </div>
          <div className="os-note-stack">
            <h3>{copy.opportunities}</h3>
            {opportunities.map((item, index) => (
              <a
                data-context-item
                data-context-module="marketing"
                data-context-submodule="competitive-analysis"
                data-context-record-type="market_opportunity"
                data-context-record-id={`opportunity-${index + 1}`}
                data-context-label={text(item, locale)}
                href={panelHref(query, "opportunity", `opportunity-${index + 1}`, "right")}
                key={text(item, locale)}
              >
                {text(item, locale)}
              </a>
            ))}
            <h3>{copy.risks}</h3>
            {risks.map((item, index) => (
              <a
                data-context-item
                data-context-module="marketing"
                data-context-submodule="competitive-analysis"
                data-context-record-type="market_risk"
                data-context-record-id={`risk-${index + 1}`}
                data-context-label={text(item, locale)}
                href={panelHref(query, "risk", `risk-${index + 1}`, "right")}
                key={text(item, locale)}
              >
                {text(item, locale)}
              </a>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

export function CompetitiveAnalysisPanel({
  panelState,
  query
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
}) {
  const panel = panelState?.panel;
  const recordId = panelState?.recordId;
  const ranked = rankedCompetitors();
  const competitor = ranked.find((item) => item.id === recordId) ?? ranked[0];
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = dashboardCopy[locale];
  const report = buildCompetitorReport(competitor);

  if (panel === "criteria") {
    return (
      <div className="drawer-content score-model-drawer">
        <DrawerHeader
          title={copy.scoreModelTitle}
          query={query}
        />
        <p className="drawer-description">{copy.scoreModelDescription}</p>
        <ScoreModelEditor
          addLabel={copy.addCriterion}
          initialCriteria={criteria.map((criterion) => ({
            id: criterion.id,
            name: text(criterion.label, locale),
            weight: criterion.weight
          }))}
          nameLabel={copy.name}
          newCriterionLabel={copy.newCriterion}
          nextScrapeLabel={copy.nextStandardScrape}
          removeLabel={copy.remove}
          rescrapeNoticeLabel={copy.rescrapeNotice}
          rescrapeNowLabel={copy.rescrapeNow}
          scrapeDecisionLabel={copy.scrapeDecision}
          weightLabel={copy.weight}
        />
      </div>
    );
  }

  if (panel === "criterion") {
    return (
      <div className="drawer-content">
        <DrawerHeader title={copy.newCriterion} query={query} />
        <label className="drawer-field">
          {copy.name}
          <input placeholder={copy.signalName} type="text" />
        </label>
        <label className="drawer-field">
          {copy.weight}
          <input placeholder="1.00" type="number" />
        </label>
        <button className="drawer-primary" type="button">{copy.addCriterion}</button>
      </div>
    );
  }

  if (panel === "metric" || panel === "opportunity" || panel === "risk") {
    return (
      <div className="drawer-content">
        <DrawerHeader title={recordId ? titleFromId(recordId) : copy.signal} query={query} />
        <p>{copy.signalDescription}</p>
        <button className="drawer-primary" type="button">{copy.askCtoxSignal}</button>
      </div>
    );
  }

  if (panel === "watchlist") {
    return (
      <div className="drawer-content">
        <DrawerHeader
          actionHref={panelHref(query, "watchlist", "new-source", "left-bottom")}
          actionLabel={copy.addCompetitor}
          title={copy.watchlist}
          query={query}
        />
        <p className="drawer-description">{copy.watchlistDescription}</p>
        <WatchlistManager
          competitorUrlLabel={copy.competitorUrl}
          displayNameLabel={copy.displayName}
          optionalLabel={copy.optional}
          queueNextRunLabel={copy.queueNextRun}
          rescrapeNowLabel={copy.rescrapeNow}
          searchLabel={copy.searchCompanies}
          searchPlaceholder={copy.searchCompaniesPlaceholder}
        />
      </div>
    );
  }

  if (panel === "draft") {
    return (
      <div className="drawer-content">
        <DrawerHeader title={copy.updateDraft} query={query} />
        <p>{copy.updateDraftDescription}</p>
        <button className="drawer-primary" type="button">{copy.createDraft}</button>
      </div>
    );
  }

  return (
    <div className="drawer-content competitor-report">
      <DrawerHeader title={competitor.name} query={query} />
      <p className="drawer-description">{report.summary}</p>
      <div className="report-actions">
        <a className="drawer-primary" href={competitorMeta[competitor.id].sourceUrl} rel="noreferrer" target="_blank">{copy.openWebsite}</a>
        <button className="drawer-primary" type="button">{copy.askCtoxAnalyze}</button>
      </div>
      <dl className="drawer-facts report-facts">
        <div><dt>{copy.rank}</dt><dd>#{competitor.rank}</dd></div>
        <div><dt>{copy.score}</dt><dd>{competitor.score.toFixed(2)}</dd></div>
        <div><dt>{copy.status}</dt><dd>{text(competitor.status, locale)}</dd></div>
        <div><dt>{copy.updated}</dt><dd>{formatCheckedAt(competitorMeta[competitor.id].checkedAt, locale)}</dd></div>
      </dl>
      <section className="report-section">
        <h3>{copy.evidence}</h3>
        <strong>{report.evidence.title}</strong>
        <p>{report.evidence.description}</p>
        <div className="report-list">
          {report.evidence.headings.map((heading) => <span key={heading}>{heading}</span>)}
        </div>
      </section>
      <section className="report-section">
        <h3>{copy.criteriaScores}</h3>
        <div className="report-score-grid">
          {Object.entries(report.criteria).map(([key, value]) => (
            <div key={key}>
              <span>{text(criterionLabels[key] ?? { en: titleFromId(key), de: titleFromId(key) }, locale)}</span>
              <meter max="10" min="0" value={value} />
              <strong>{value.toFixed(1)}</strong>
            </div>
          ))}
        </div>
      </section>
      <section className="report-section">
        <h3>{copy.measuredFacts}</h3>
        <dl className="drawer-facts report-facts">
          {report.factors.map(([label, value]) => (
            <div key={label}><dt>{label}</dt><dd>{value}</dd></div>
          ))}
        </dl>
        <p>{report.rationale}</p>
      </section>
      <section className="report-section">
        <h3>{copy.risks}</h3>
        <ul>
          {report.risks.map((item) => <li key={item}>{item}</li>)}
        </ul>
      </section>
      <section className="report-section">
        <h3>{copy.recommendations}</h3>
        <ul>
          {report.recommendations.map((item) => <li key={item}>{item}</li>)}
        </ul>
      </section>
    </div>
  );
}

function DrawerHeader({
  actionHref,
  actionLabel,
  title,
  query
}: {
  actionHref?: string;
  actionLabel?: string;
  title: string;
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = dashboardCopy[locale];

  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <div className="drawer-head-actions">
        {actionHref ? <a className="drawer-icon-action" href={actionHref} aria-label={actionLabel ?? "Add"}>+</a> : null}
        <a href={baseHref(query)} aria-label={copy.closePanel}>{copy.close}</a>
      </div>
    </div>
  );
}

function panelHref(query: QueryState, panel: string, recordId: string, drawer: "left-bottom" | "bottom" | "right") {
  if (query.panel === panel && query.recordId === recordId) {
    return baseHref(query);
  }

  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.xAxis) params.set("xAxis", query.xAxis);
  if (query.yAxis) params.set("yAxis", query.yAxis);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  return `/app/marketing/competitive-analysis?${params.toString()}`;
}

function baseHref(query: QueryState) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  if (query.xAxis) params.set("xAxis", query.xAxis);
  if (query.yAxis) params.set("yAxis", query.yAxis);
  const queryString = params.toString();
  return queryString ? `/app/marketing/competitive-analysis?${queryString}` : "/app/marketing/competitive-analysis";
}

function resolveAxis(value: string | undefined, fallback: AxisId) {
  return axisOptions.some((option) => option.id === value) ? value as AxisId : fallback;
}

function formatCheckedAt(value: string, locale: SupportedLocale) {
  return new Intl.DateTimeFormat(locale === "de" ? "de-DE" : "en-US", {
    day: "2-digit",
    month: "short",
    year: "numeric"
  }).format(new Date(value));
}

function buildCompetitorReport(competitor: typeof competitors[number]) {
  if (competitor.id === "artisan") {
    return {
      ...artisanReport,
      summary: "Full measured-facts report for Artisan. Artisan is scored as a direct AI employee platform with strong outbound sales positioning, solid buyer clarity, and weaker provider API / pricing clarity evidence."
    };
  }

  return {
    checks: [
      ["Homepage", "200 OK"],
      ["Evidence items", "sample"],
      ["Formula", "measured-facts-v2"]
    ],
    criteria: {
      positioning: Math.round(competitor.dimensions.overlap / 10 * 10) / 10,
      buyer_clarity: Math.round(competitor.dimensions.buyerClarity / 10 * 10) / 10,
      trust_compliance: Math.round(competitor.dimensions.trust / 10 * 10) / 10,
      seo_content_velocity: Math.round(competitor.dimensions.seoVelocity / 10 * 10) / 10,
      differentiation_vs_own_product: Math.round(competitor.score * 10) / 10
    },
    evidence: {
      title: `${competitor.name} competitive evidence`,
      description: text(competitor.kind, "en"),
      headings: [
        text(competitor.signal, "en"),
        "Report assembled from current starter evidence and ready for CTOX research enrichment."
      ],
      snippets: [text(competitor.signal, "en")]
    },
    factors: [
      ["Overlap", `${competitor.dimensions.overlap / 10}/10`],
      ["Buyer clarity", `${competitor.dimensions.buyerClarity / 10}/10`],
      ["Trust", `${competitor.dimensions.trust / 10}/10`],
      ["SEO velocity", `${competitor.dimensions.seoVelocity / 10}/10`]
    ],
    rationale: "Measured formula assembled from the starter competitive-analysis dimensions.",
    recommendations: [
      "Review source evidence and enrich the factors with live CTOX research output.",
      "Ask CTOX to rerun the monitor before making product or positioning changes."
    ],
    risks: [
      text(competitor.signal, "en")
    ],
    summary: `${competitor.name} report generated from the current vanilla measured-facts model. Replace this section with the full CTOX research artifact when the monitor sync is connected.`
  };
}

function titleFromId(value: string) {
  return value.split("-").map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : part).join(" ");
}

function text(value: Localized, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}

function rankedCompetitors() {
  return [...competitors]
    .sort((left, right) => right.score - left.score || left.name.localeCompare(right.name))
    .map((competitor, index) => ({ ...competitor, rank: index + 1 }));
}

const dashboardCopy: Record<SupportedLocale, Record<string, string>> = {
  en: {
    actions: "Competitive analysis actions",
    addCompetitor: "Add competitor",
    addCriterion: "Add criterion",
    addNote: "Add note",
    askCtoxAnalyze: "Ask CTOX to analyze this",
    askCtoxSignal: "Ask CTOX to work on this",
    buyerClarity: "Buyer clarity",
    close: "Close",
    closePanel: "Close panel",
    competitorRanking: "Competitor ranking",
    competitorUrl: "Competitor URL",
    createDraft: "Create draft",
    criteria: "Criteria",
    criteriaScores: "Criteria scores",
    decisionNotes: "Decision notes",
    displayName: "Display name",
    draft: "Draft",
    evidence: "Evidence",
    horizontalAxis: "Horizontal axis",
    mapDescription: "{x} against {y}.",
    measuredFacts: "Measured facts",
    name: "Name",
    newCriterion: "New criterion",
    nextStandardScrape: "Next standard scrape",
    optional: "Optional",
    open: "Open",
    openWebsite: "Open website",
    opportunities: "Opportunities",
    overlap: "Overlap",
    portfolioMap: "Portfolio map",
    queueNextRun: "Queue for next monitor run",
    rank: "Rank",
    ranking: "Ranking",
    recommendations: "Recommendations",
    remove: "Remove",
    rescrapeNotice: "New criterion \"{criterion}\" was added. Scores will only be reliable after CTOX measures this criterion across the watchlist. Run a rescrape now, or include it in the next standard scrape.",
    rescrapeNow: "Run rescrape now",
    risks: "Risks",
    run: "Run",
    score: "Score",
    scoreModel: "Measured-facts score model.",
    scoreModelDescription: "Weighted facts used to calculate the competitor score. Adjust weights to change how CTOX evaluates positioning, clarity, proof, and technical readiness.",
    scoreModelTitle: "Score model",
    scrapeDecision: "Selected: {decision}",
    searchCompanies: "Search companies",
    searchCompaniesPlaceholder: "AI employee platforms in Europe",
    signal: "Signal",
    signalDescription: "This signal is part of the current competitive analysis workspace.",
    signalName: "Signal name",
    signals: "Signals to act on next.",
    status: "Status",
    target: "Target",
    updateDraft: "Update draft",
    updateDraftDescription: "Review gate is passing. The update can summarize ranking changes, open market gaps, and recommended product actions.",
    updated: "Updated",
    verticalAxis: "Vertical axis",
    watchlist: "Watchlist",
    watchlistDescription: "Add a company manually or ask CTOX web search to discover initial competitors. New sources are queued into the same scrape target.",
    weight: "Weight"
  },
  de: {
    actions: "Aktionen der Wettbewerbsanalyse",
    addCompetitor: "Wettbewerber hinzufügen",
    addCriterion: "Kriterium hinzufügen",
    addNote: "Notiz hinzufügen",
    askCtoxAnalyze: "CTOX damit beauftragen",
    askCtoxSignal: "CTOX an diesem Signal arbeiten lassen",
    buyerClarity: "Käuferklarheit",
    close: "Schließen",
    closePanel: "Panel schließen",
    competitorRanking: "Wettbewerber-Ranking",
    competitorUrl: "Wettbewerber-URL",
    createDraft: "Entwurf erstellen",
    criteria: "Kriterien",
    criteriaScores: "Kriterien-Scores",
    decisionNotes: "Entscheidungssignale",
    displayName: "Anzeigename",
    draft: "Entwurf",
    evidence: "Evidenz",
    horizontalAxis: "Horizontale Achse",
    mapDescription: "{x} gegen {y}.",
    measuredFacts: "Gemessene Fakten",
    name: "Name",
    newCriterion: "Neues Kriterium",
    nextStandardScrape: "Nächster Standardscrape",
    optional: "Optional",
    open: "Öffnen",
    openWebsite: "Webseite öffnen",
    opportunities: "Chancen",
    overlap: "Überschneidung",
    portfolioMap: "Portfolio-Karte",
    queueNextRun: "Für nächsten Monitorlauf vormerken",
    rank: "Rang",
    ranking: "Ranking",
    recommendations: "Empfehlungen",
    remove: "Entfernen",
    rescrapeNotice: "Das neue Kriterium \"{criterion}\" wurde hinzugefügt. Die Scores sind erst belastbar, wenn CTOX dieses Kriterium über die Watchlist gemessen hat. Jetzt rescrapen oder beim nächsten Standardscrape mitlaufen lassen.",
    rescrapeNow: "Jetzt rescrapen",
    risks: "Risiken",
    run: "Starten",
    score: "Score",
    scoreModel: "Score-Modell aus gemessenen Fakten.",
    scoreModelDescription: "Gewichtete Fakten zur Berechnung des Wettbewerber-Scores. Passe die Gewichte an, um zu steuern, wie CTOX Positionierung, Klarheit, Nachweise und technische Reife bewertet.",
    scoreModelTitle: "Score-Modell",
    scrapeDecision: "Ausgewählt: {decision}",
    searchCompanies: "Unternehmen suchen",
    searchCompaniesPlaceholder: "KI-Mitarbeiter-Plattformen in Europa",
    signal: "Signal",
    signalDescription: "Dieses Signal gehört zur aktuellen Wettbewerbsanalyse.",
    signalName: "Signalname",
    signals: "Signale für die nächsten Schritte.",
    status: "Status",
    target: "Ziel",
    updateDraft: "Update-Entwurf",
    updateDraftDescription: "Das Review-Gate ist bestanden. Das Update kann Rankingänderungen, offene Marktlücken und empfohlene Produktaktionen zusammenfassen.",
    updated: "Aktualisiert",
    watchlist: "Watchlist",
    watchlistDescription: "Füge ein Unternehmen manuell hinzu oder lasse CTOX Web Search initiale Wettbewerber finden. Neue Quellen laufen in dasselbe Scrape-Target.",
    verticalAxis: "Vertikale Achse",
    weight: "Gewicht"
  }
};
