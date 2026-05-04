import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import type { ReactNode } from "react";
import {
  getMarketingBundle,
  text,
  type CommerceItem,
  type MarketingBundle,
  type MarketingAsset,
  type Campaign,
  type ResearchItem,
  type SupportedLocale,
  type WebsitePage
} from "../lib/marketing-seed";
import { MarketingCreateForm, MarketingQueueButton } from "./marketing-actions";

type QueryState = {
  locale?: string;
  theme?: string;
  panel?: string;
  recordId?: string;
  drawer?: string;
};

type RecordType = "website" | "assets" | "campaigns" | "research" | "commerce";

export async function MarketingWorkspace({
  submoduleId,
  query
}: {
  submoduleId: string;
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const resource = resolveResource(submoduleId);
  const data = await getMarketingBundle();

  if (resource === "assets") return <AssetsView data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (resource === "campaigns") return <CampaignsView data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (resource === "research") return <ResearchView data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  if (resource === "commerce") return <CommerceView data={data} locale={locale} query={query} submoduleId={submoduleId} />;

  return <WebsiteView data={data} locale={locale} query={query} submoduleId={submoduleId} />;
}

export async function MarketingPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const resource = resolveResource(submoduleId);
  const recordId = panelState?.recordId;
  const data = await getMarketingBundle();

  if (panelState?.panel === "marketing-set") {
    const marketingSet = resolveMarketingSet(recordId, resource, data, locale);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader query={query} submoduleId={submoduleId} title={marketingSet.title} />
        <p className="drawer-description">{marketingSet.description}</p>
        <dl className="drawer-facts">
          <div><dt>Items</dt><dd>{marketingSet.items.length}</dd></div>
          <div><dt>Resource</dt><dd>{resourceLabel(marketingSet.resource)}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>Selected items</h3>
          <div className="ops-mini-list">
            {marketingSet.items.map((item) => (
              <a
                data-context-item
                data-context-label={item.label}
                data-context-module="marketing"
                data-context-record-id={item.id}
                data-context-record-type={item.type}
                data-context-submodule={submoduleId}
                href={marketingRecordHref(query, submoduleId, item.panel, item.id)}
                key={`${item.type}-${item.id}`}
              >
                {item.label} · {item.meta}
              </a>
            ))}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>CTOX sync</h3>
          <MarketingQueueButton
            className="drawer-primary"
            instruction={`Review and synchronize this Marketing context set: ${marketingSet.title}.`}
            payload={{ filter: recordId, items: marketingSet.items }}
            recordId={recordId ?? marketingSet.resource}
            resource={marketingSet.resource}
            title={`Sync Marketing set: ${marketingSet.title}`}
          >
            Ask CTOX to process this set
          </MarketingQueueButton>
        </section>
      </div>
    );
  }

  if (panelState?.panel === "new") {
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader query={query} submoduleId={submoduleId} title={`New ${resourceLabel(resource)}`} />
        <p className="drawer-description">Create a marketing record and queue CTOX to wire it into the shared Business OS context.</p>
        <MarketingCreateForm
          ownerOptions={data.people.map((person) => ({ label: person.name, value: person.id }))}
          resource={resource}
          resourceLabel={resourceLabel(resource)}
        />
      </div>
    );
  }

  const record = findRecord(data, resource, recordId);
  if (!record) return null;
  const owner = data.people.find((person) => person.id === record.ownerId);

  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader query={query} submoduleId={submoduleId} title={recordTitle(record)} />
      <dl className="drawer-facts">
        <div><dt>Type</dt><dd>{resourceLabel(resource)}</dd></div>
        <div><dt>Status</dt><dd>{record.status}</dd></div>
        <div><dt>Owner</dt><dd>{owner?.name ?? "CTOX"}</dd></div>
        {"updated" in record ? <div><dt>Updated</dt><dd>{record.updated}</dd></div> : null}
        {"launch" in record ? <div><dt>Launch</dt><dd>{record.launch}</dd></div> : null}
        {"price" in record ? <div><dt>Price</dt><dd>{record.price}</dd></div> : null}
      </dl>
      <section className="ops-drawer-section">
        <h3>Working context</h3>
        <div className="ops-mini-list">
          {recordContext(record, locale).map((line) => <span key={line}>{line}</span>)}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>CTOX sync</h3>
        <MarketingQueueButton
          className="drawer-primary"
          instruction={`Synchronize Marketing ${resourceLabel(resource)} ${recordTitle(record)} with CTOX queue context, bug reports, deep links, Sales follow-up, and Business reporting.`}
          payload={{ record }}
          recordId={record.id}
          resource={resource}
          title={`Sync marketing ${resourceLabel(resource)}: ${recordTitle(record)}`}
        >
          Ask CTOX to sync this
        </MarketingQueueButton>
      </section>
    </div>
  );
}

function WebsiteView({ data, locale, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-knowledge-workspace">
      <Pane title="Website pages" description="Public Next.js surfaces stay empty by default but fully wired.">
        <div className="ops-table ops-knowledge-table">
          <div className="ops-table-head"><span>Page</span><span>Status</span><span>Updated</span></div>
          {data.websitePages.map((page) => (
            <ContextRow href={panelHref(query, submoduleId, "page", page.id, "right")} key={page.id} label={page.title} recordId={page.id} recordType="page" submoduleId={submoduleId}>
              <strong>{page.title}</strong>
              <small>{page.path} · {text(page.intent, locale)}</small>
              <span>{page.status}</span>
              <span>{page.updated}</span>
            </ContextRow>
          ))}
        </div>
      </Pane>
      <Pane title="Publishing rail" description="Visible entry points and next content operations.">
        <SignalList
          items={[
            ["Draft pages", String(data.websitePages.filter((page) => page.status === "draft").length), panelHref(query, submoduleId, "marketing-set", "draft-pages", "right"), "draft-pages"],
            ["Review pages", String(data.websitePages.filter((page) => page.status === "review").length), panelHref(query, submoduleId, "marketing-set", "review-pages", "right"), "review-pages"],
            ["Published pages", String(data.websitePages.filter((page) => page.status === "published").length), panelHref(query, submoduleId, "marketing-set", "published-pages", "right"), "published-pages"]
          ]}
          submoduleId={submoduleId}
        />
        <SignalList items={data.websitePages.map((page) => [page.title, text(page.nextAction, locale), panelHref(query, submoduleId, "page", page.id, "right"), page.id])} submoduleId={submoduleId} />
        <ActionDock query={query} resource="website" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function AssetsView({ data, locale, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-project-workspace">
      <Pane title="Asset library" description="Materials ready for Sales, campaigns, and public pages.">
        <Stack>
          {data.assets.map((asset) => (
            <Card href={panelHref(query, submoduleId, "asset", asset.id, "right")} key={asset.id} label={asset.name} recordId={asset.id} recordType="asset" submoduleId={submoduleId}>
              <strong>{asset.name}</strong>
              <small>{asset.kind} · {asset.status}</small>
              <span>{text(asset.audience, locale)}</span>
            </Card>
          ))}
        </Stack>
      </Pane>
      <Pane title="Usage map" description="Where each material should be used next.">
        <SignalList
          items={[
            ["Ready assets", String(data.assets.filter((asset) => asset.status === "ready").length), panelHref(query, submoduleId, "marketing-set", "assets-ready", "right"), "assets-ready"],
            ["Review assets", String(data.assets.filter((asset) => asset.status === "review").length), panelHref(query, submoduleId, "marketing-set", "assets-review", "right"), "assets-review"],
            ["Draft assets", String(data.assets.filter((asset) => asset.status === "draft").length), panelHref(query, submoduleId, "marketing-set", "assets-draft", "right"), "assets-draft"]
          ]}
          submoduleId={submoduleId}
        />
        <SignalList items={data.assets.map((asset) => [asset.name, text(asset.usage, locale), panelHref(query, submoduleId, "asset", asset.id, "right"), asset.id])} submoduleId={submoduleId} />
      </Pane>
      <Pane title="Actions" description="Create, refresh, and sync asset context.">
        <ActionDock query={query} resource="assets" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function CampaignsView({ data, locale, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-board-workspace">
      {["planned", "active", "paused"].map((status) => (
        <Pane description={`${status} campaign work`} key={status} title={status}>
          <Stack>
            {data.campaigns.filter((campaign) => campaign.status === status).map((campaign) => (
              <Card href={panelHref(query, submoduleId, "campaign", campaign.id, "right")} key={campaign.id} label={campaign.name} recordId={campaign.id} recordType="campaign" submoduleId={submoduleId}>
                <strong>{campaign.name}</strong>
                <small>{campaign.channel} · {campaign.launch}</small>
                <span>{text(campaign.target, locale)}</span>
              </Card>
            ))}
          </Stack>
          <ActionDock query={query} resource="campaigns" setRecordId={`campaigns-${status}`} submoduleId={submoduleId} />
        </Pane>
      ))}
    </div>
  );
}

function ResearchView({ data, locale, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-planning-workspace">
      <Pane title="Research queue" description="Market notes, personas, interviews, pricing, and search signals.">
        <div className="ops-note-feed">
          {data.researchItems.map((item) => (
            <a
              data-context-item
              data-context-label={item.title}
              data-context-module="marketing"
              data-context-record-id={item.id}
              data-context-record-type="research_item"
              data-context-submodule={submoduleId}
              href={panelHref(query, submoduleId, "research", item.id, "right")}
              key={item.id}
            >
              <strong>{item.title}</strong>
              <span>{text(item.insight, locale)}</span>
              <small>{item.kind} · {item.status} · {item.updated}</small>
            </a>
          ))}
        </div>
      </Pane>
      <Pane title="Campaign links" description="Research is actionable only when linked to work.">
        <SignalList
          items={[
            ["Queued research", String(data.researchItems.filter((item) => item.status === "queued").length), panelHref(query, submoduleId, "marketing-set", "research-queued", "right"), "research-queued"],
            ["Collecting research", String(data.researchItems.filter((item) => item.status === "collecting").length), panelHref(query, submoduleId, "marketing-set", "research-collecting", "right"), "research-collecting"],
            ["Synthesized research", String(data.researchItems.filter((item) => item.status === "synthesized").length), panelHref(query, submoduleId, "marketing-set", "research-synthesized", "right"), "research-synthesized"]
          ]}
          submoduleId={submoduleId}
        />
        <SignalList items={data.researchItems.map((item) => [item.title, item.linkedCampaignIds.join(", "), panelHref(query, submoduleId, "research", item.id, "right"), item.id])} submoduleId={submoduleId} />
        <ActionDock query={query} resource="research" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function CommerceView({ data, locale, query, submoduleId }: ViewProps) {
  return (
    <div className="ops-workspace ops-knowledge-workspace">
      <Pane title="Offer catalog" description="Marketing-owned offers that sync to Business products and invoices.">
        <div className="ops-table ops-knowledge-table">
          <div className="ops-table-head"><span>Offer</span><span>Status</span><span>Price</span></div>
          {data.commerceItems.map((item) => (
            <ContextRow href={panelHref(query, submoduleId, "commerce", item.id, "right")} key={item.id} label={item.name} recordId={item.id} recordType="shop_item" submoduleId={submoduleId}>
              <strong>{item.name}</strong>
              <small>{item.kind} · {text(item.nextAction, locale)}</small>
              <span>{item.status}</span>
              <span>{item.price}</span>
            </ContextRow>
          ))}
        </div>
      </Pane>
      <Pane title="Business sync" description="Offers should become products, price rules, and invoice lines.">
        <SignalList
          items={[
            ["Listed offers", String(data.commerceItems.filter((item) => item.status === "listed").length), panelHref(query, submoduleId, "marketing-set", "commerce-listed", "right"), "commerce-listed"],
            ["Draft offers", String(data.commerceItems.filter((item) => item.status === "draft").length), panelHref(query, submoduleId, "marketing-set", "commerce-draft", "right"), "commerce-draft"],
            ["Review offers", String(data.commerceItems.filter((item) => item.status === "review").length), panelHref(query, submoduleId, "marketing-set", "commerce-review", "right"), "commerce-review"]
          ]}
          submoduleId={submoduleId}
        />
        <ActionDock query={query} resource="commerce" submoduleId={submoduleId} />
      </Pane>
    </div>
  );
}

function Pane({ children, description, title }: { children: ReactNode; description: string; title: string }) {
  return (
    <section className="ops-pane">
      <div className="ops-pane-head">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </div>
      {children}
    </section>
  );
}

function Stack({ children }: { children: ReactNode }) {
  return <div className="ops-card-stack">{children}</div>;
}

function Card({ children, href, label, recordId, recordType, submoduleId }: ContextProps & { children: ReactNode }) {
  return (
    <a
      className="ops-work-card"
      data-context-item
      data-context-label={label}
      data-context-module="marketing"
      data-context-record-id={recordId}
      data-context-record-type={recordType}
      data-context-submodule={submoduleId}
      href={href}
    >
      {children}
    </a>
  );
}

function ContextRow({ children, href, label, recordId, recordType, submoduleId }: ContextProps & { children: ReactNode }) {
  return (
    <a
      className="ops-table-row"
      data-context-item
      data-context-label={label}
      data-context-module="marketing"
      data-context-record-id={recordId}
      data-context-record-type={recordType}
      data-context-submodule={submoduleId}
      href={href}
    >
      {children}
    </a>
  );
}

function SignalList({ items, submoduleId }: { items: Array<[string, string, string?, string?]>; submoduleId: string }) {
  return (
    <div className="ops-signal-list">
      {items.map(([label, value, href, recordId]) => {
        const content = (
          <>
          <span>{label}</span>
          <small>{value}</small>
          </>
        );
        const contextProps = {
          "data-context-item": true,
          "data-context-label": label,
          "data-context-module": "marketing",
          "data-context-record-id": recordId ?? label.toLowerCase().replaceAll(" ", "-"),
          "data-context-record-type": "signal",
          "data-context-submodule": submoduleId
        };

        return href ? (
          <a className="ops-signal" href={href} key={label} {...contextProps}>{content}</a>
        ) : (
          <div className="ops-signal" key={label} {...contextProps}>{content}</div>
        );
      })}
    </div>
  );
}

function ActionDock({
  query,
  resource,
  setRecordId,
  submoduleId
}: {
  query: QueryState;
  resource: RecordType;
  setRecordId?: string;
  submoduleId: string;
}) {
  return (
    <div className="ops-action-dock">
      <a
        data-context-action="create"
        data-context-item
        data-context-label={`New ${resourceLabel(resource)}`}
        data-context-module="marketing"
        data-context-record-id={resource}
        data-context-record-type={resource}
        data-context-submodule={submoduleId}
        href={panelHref(query, submoduleId, "new", resource, "left-bottom")}
      >
        New
      </a>
      <a
        data-context-action="open-set"
        data-context-item
        data-context-label={`${resourceLabel(resource)} set`}
        data-context-module="marketing"
        data-context-record-id={setRecordId ?? resource}
        data-context-record-type="marketing-set"
        data-context-submodule={submoduleId}
        href={panelHref(query, submoduleId, "marketing-set", setRecordId ?? resource, "right")}
      >
        Set
      </a>
      <MarketingQueueButton
        instruction={`Synchronize Marketing ${resourceLabel(resource)} records with CTOX core, Sales follow-up, Business products, and public website links.`}
        resource={resource}
        title={`Sync marketing ${resourceLabel(resource)}`}
      >
        Sync CTOX
      </MarketingQueueButton>
    </div>
  );
}

function DrawerHeader({ query, submoduleId, title }: { query: QueryState; submoduleId: string; title: string }) {
  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <a href={baseHref(query, submoduleId)}>Close</a>
    </div>
  );
}

function panelHref(
  query: QueryState,
  submoduleId: string,
  panel: string,
  recordId: string,
  drawer: "left-bottom" | "bottom" | "right"
) {
  if (query.panel === panel && query.recordId === recordId) return baseHref(query, submoduleId);
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", drawer);
  return `/app/marketing/${submoduleId}?${params.toString()}`;
}

function baseHref(query: QueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `/app/marketing/${submoduleId}?${queryString}` : `/app/marketing/${submoduleId}`;
}

function marketingRecordHref(query: QueryState, submoduleId: string, panel: string, recordId: string) {
  return panelHref(query, submoduleId, panel, recordId, "right");
}

function resolveResource(submoduleId: string): RecordType {
  if (submoduleId === "assets") return "assets";
  if (submoduleId === "campaigns") return "campaigns";
  if (submoduleId === "research") return "research";
  if (submoduleId === "commerce") return "commerce";
  return "website";
}

function resourceLabel(resource: RecordType) {
  if (resource === "assets") return "asset";
  if (resource === "campaigns") return "campaign";
  if (resource === "research") return "research item";
  if (resource === "commerce") return "commerce item";
  return "website page";
}

function findRecord(data: MarketingBundle, resource: RecordType, recordId?: string) {
  if (!recordId) return null;
  if (resource === "assets") return data.assets.find((item) => item.id === recordId) ?? null;
  if (resource === "campaigns") return data.campaigns.find((item) => item.id === recordId) ?? null;
  if (resource === "research") return data.researchItems.find((item) => item.id === recordId) ?? null;
  if (resource === "commerce") return data.commerceItems.find((item) => item.id === recordId) ?? null;
  return data.websitePages.find((item) => item.id === recordId) ?? null;
}

function recordTitle(record: WebsitePage | MarketingAsset | Campaign | ResearchItem | CommerceItem) {
  if ("title" in record) return record.title;
  return record.name;
}

function recordContext(record: WebsitePage | MarketingAsset | Campaign | ResearchItem | CommerceItem, locale: SupportedLocale) {
  if ("intent" in record) return [record.path, text(record.intent, locale), text(record.nextAction, locale)];
  if ("audience" in record) return [record.kind, text(record.audience, locale), text(record.usage, locale)];
  if ("target" in record) return [record.channel, text(record.target, locale), text(record.nextAction, locale)];
  if ("insight" in record) return [record.kind, text(record.insight, locale), `Campaigns: ${record.linkedCampaignIds.join(", ")}`];
  return [record.kind, record.price, text(record.nextAction, locale)];
}

type MarketingSetItem = {
  id: string;
  label: string;
  meta: string;
  panel: string;
  type: string;
};

function resolveMarketingSet(recordId: string | undefined, resource: RecordType, data: MarketingBundle, locale: SupportedLocale) {
  const key = recordId ?? resource;
  const pageItems = (items: WebsitePage[]): MarketingSetItem[] => items.map((page) => ({
    id: page.id,
    label: page.title,
    meta: `${page.status} · ${page.updated} · ${page.path}`,
    panel: "page",
    type: "page"
  }));
  const assetItems = (items: MarketingAsset[]): MarketingSetItem[] => items.map((asset) => ({
    id: asset.id,
    label: asset.name,
    meta: `${asset.status} · ${asset.kind} · ${asset.updated}`,
    panel: "asset",
    type: "asset"
  }));
  const campaignItems = (items: Campaign[]): MarketingSetItem[] => items.map((campaign) => ({
    id: campaign.id,
    label: campaign.name,
    meta: `${campaign.status} · ${campaign.channel} · ${campaign.launch}`,
    panel: "campaign",
    type: "campaign"
  }));
  const researchItems = (items: ResearchItem[]): MarketingSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.title,
    meta: `${item.status} · ${item.kind} · ${item.updated}`,
    panel: "research",
    type: "research_item"
  }));
  const commerceItems = (items: CommerceItem[]): MarketingSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.name,
    meta: `${item.status} · ${item.kind} · ${item.price}`,
    panel: "commerce",
    type: "shop_item"
  }));

  if (key === "draft-pages") return marketingSet("Draft pages", "Public pages that still need positioning, content, or visual work.", pageItems(data.websitePages.filter((page) => page.status === "draft")), "website");
  if (key === "review-pages") return marketingSet("Review pages", "Public pages ready for CTOX review before publishing.", pageItems(data.websitePages.filter((page) => page.status === "review")), "website");
  if (key === "published-pages") return marketingSet("Published pages", "Public pages currently live or ready to stay stable.", pageItems(data.websitePages.filter((page) => page.status === "published")), "website");

  if (key === "assets-ready") return marketingSet("Ready assets", "Materials that can be attached to Sales, campaigns, and website journeys.", assetItems(data.assets.filter((asset) => asset.status === "ready")), "assets");
  if (key === "assets-review") return marketingSet("Review assets", "Materials that need final CTOX or owner review before use.", assetItems(data.assets.filter((asset) => asset.status === "review")), "assets");
  if (key === "assets-draft") return marketingSet("Draft assets", "Materials still being assembled or waiting on source inputs.", assetItems(data.assets.filter((asset) => asset.status === "draft")), "assets");

  if (key === "campaigns-active") return marketingSet("Active campaigns", "Campaigns currently driving Sales follow-up and reporting signals.", campaignItems(data.campaigns.filter((campaign) => campaign.status === "active")), "campaigns");
  if (key === "campaigns-planned") return marketingSet("Planned campaigns", "Campaigns with upcoming launch work and dependencies.", campaignItems(data.campaigns.filter((campaign) => campaign.status === "planned")), "campaigns");
  if (key === "campaigns-paused") return marketingSet("Paused campaigns", "Campaigns that need a decision before further execution.", campaignItems(data.campaigns.filter((campaign) => campaign.status === "paused")), "campaigns");

  if (key === "research-queued") return marketingSet("Queued research", "Research items waiting for collection, synthesis, or CTOX web search.", researchItems(data.researchItems.filter((item) => item.status === "queued")), "research");
  if (key === "research-collecting") return marketingSet("Collecting research", "Research items actively gathering signals.", researchItems(data.researchItems.filter((item) => item.status === "collecting")), "research");
  if (key === "research-synthesized") return marketingSet("Synthesized research", "Research items ready to inform campaigns, pages, and product work.", researchItems(data.researchItems.filter((item) => item.status === "synthesized")), "research");

  if (key === "commerce-listed") return marketingSet("Listed offers", "Offers that can sync to Business products and invoices.", commerceItems(data.commerceItems.filter((item) => item.status === "listed")), "commerce");
  if (key === "commerce-draft") return marketingSet("Draft offers", "Offers that need terms, pricing, or capacity limits.", commerceItems(data.commerceItems.filter((item) => item.status === "draft")), "commerce");
  if (key === "commerce-review") return marketingSet("Review offers", "Offers ready for commercial and operational review.", commerceItems(data.commerceItems.filter((item) => item.status === "review")), "commerce");

  if (key === "assets") return marketingSet("Asset library", "All Marketing materials available to CTOX and Sales.", assetItems(data.assets), "assets");
  if (key === "campaigns") return marketingSet("Campaigns", "All campaign work connected to Sales, website, and reporting.", campaignItems(data.campaigns), "campaigns");
  if (key === "research") return marketingSet("Research queue", "All research inputs CTOX can use for positioning, pages, and campaigns.", researchItems(data.researchItems), "research");
  if (key === "commerce") return marketingSet("Offer catalog", "All Marketing-owned offers that sync to Business.", commerceItems(data.commerceItems), "commerce");

  return marketingSet("Website pages", "All public Next.js surfaces with their integration state.", pageItems(data.websitePages), "website");
}

function marketingSet(title: string, description: string, items: MarketingSetItem[], resource: RecordType) {
  return { title, description, items, resource };
}

type ViewProps = {
  data: MarketingBundle;
  locale: SupportedLocale;
  query: QueryState;
  submoduleId: string;
};

type ContextProps = {
  href: string;
  label: string;
  recordId: string;
  recordType: string;
  submoduleId: string;
};
