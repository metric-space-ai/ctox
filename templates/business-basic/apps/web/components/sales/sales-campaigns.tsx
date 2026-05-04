"use client";

import { useMemo, useState } from "react";
import { SalesQueueButton } from "./actions";

type QueryState = {
  locale?: string;
  theme?: string;
};

type SupportedLocale = "en" | "de";

type SalesCampaign = {
  id: string;
  name: string;
  status: "Draft" | "Research" | "Ready" | "Active";
  sourceTypes: Array<"Excel" | "URL" | "PDF" | "Text">;
  importedRecords: number;
  enrichedRecords: number;
  assignedRecords: number;
  ownerId: string;
  assignmentPrompt: LocalizedText;
  nextStep: LocalizedText;
};

type SalesBundle = {
  campaigns: SalesCampaign[];
};

type CampaignDialog = "create" | "import" | "details" | null;
type CampaignColumnConfig = "" | "company" | "contact" | "touchpoint" | "outreach" | "send";

type CampaignCreateDraft = {
  campaignType: "Outbound" | "Inbound";
  campaignName: string;
  assignmentPrompt: string;
  status: SalesCampaign["status"];
};

type LocalizedText = {
  en: string;
  de: string;
};

function text(value: LocalizedText, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}

const sourceImports = [
  {
    id: "src-e-world-url",
    name: "E-world exhibitor URL",
    type: "URL",
    records: 96,
    status: "Research",
    enrichment: "42 enriched",
    note: "Website, segment and buying-trigger extraction prepared."
  },
  {
    id: "src-association-pdf",
    name: "Association member PDF",
    type: "PDF",
    records: 48,
    status: "Parsed",
    enrichment: "Needs research",
    note: "Company names found; contact and website fields are incomplete."
  },
  {
    id: "src-partner-xlsx",
    name: "Partner account workbook",
    type: "Excel",
    records: 62,
    status: "Ready",
    enrichment: "62 enriched",
    note: "CRM overlap and expansion campaign candidates detected."
  }
];

const campaignMailAccounts = [
  {
    id: "mail-sales-primary",
    label: "sales@ctox.example",
    provider: "SMTP",
    sender: "sales@ctox.example",
    replyTo: "reply@ctox.example",
    dailyLimit: 120,
    hourlyLimit: 18
  },
  {
    id: "resend-vercel",
    label: "Resend / Vercel",
    provider: "Resend",
    sender: "hello@ctox.example",
    replyTo: "sales@ctox.example",
    dailyLimit: 500,
    hourlyLimit: 60
  }
];

const campaignSendPolicy = {
  accountId: "mail-sales-primary",
  minDelayMinutes: 6,
  maxDelayMinutes: 18,
  quietHours: "18:00-08:00",
  requireApproval: true,
  unsubscribeRequired: true,
  bounceStopAfter: 2
};

const assignmentRules = [
  "ICP fit and product relevance before geography",
  "Buying trigger, recent signal and source confidence",
  "Existing account relationship and open opportunity context",
  "Postal code may support routing, but does not decide alone"
];

const NEW_IMPORT_CAMPAIGN_ID = "__new_import_campaign__";

const outreachRows = [
  {
    id: "outreach-voltware-keller",
    campaignId: "campaign-energy-market-import",
    company: "Voltware GmbH",
    domain: "voltware.example",
    person: "Mara Keller",
    email: "mara.keller@voltware.example",
    role: "Head of Operations",
    department: "Operations",
    location: "44135 Dortmund",
    status: "Entwurf",
    tags: ["Energy market source import", "operations", "Minimax V2.7"],
    messageType: "E-Mail",
    subject: "Operations-Trigger aus Ihrem Netzservice-Aufbau",
    body: "Hallo Frau Keller,\nauf Ihrer Website betonen Sie Netzservice und Prozesssicherheit, gleichzeitig suchen Sie nach Operations-Verstaerkung. CTOX kann solche Uebergaben aus Vertrieb, Service und Betrieb in einer Pipeline buendeln, ohne dass Teams parallel in Tabellen arbeiten.\nSoll ich Ihnen ein kurzes Beispiel zeigen, wie ein Energie-Team seine naechsten Schritte damit strukturiert?",
    followup1: "Hallo Frau Keller,\nich wollte den Gedanken zu Netzservice-Uebergaben noch einmal nachhalten. Wenn Sie moechten, schicke ich Ihnen ein kurzes Beispiel fuer eine Operations-Pipeline mit automatischen naechsten Aktionen.",
    followup2: "Hallo Frau Keller,\nich lege das Thema sonst zur Seite. Falls Prozessuebergaben zwischen Vertrieb, Service und Betrieb bei Ihnen gerade relevant sind, kann ich Ihnen eine knappe Vorlage schicken.",
    note: "Touchpoints: Website Netzservice, offene Operations-Rolle, Energie-Segment."
  },
  {
    id: "outreach-nordgrid-weber",
    campaignId: "campaign-energy-market-import",
    company: "NordGrid Services",
    domain: "nordgrid.example",
    person: "Jonas Weber",
    email: "jonas.weber@nordgrid.example",
    role: "Commercial Lead",
    department: "Sales",
    location: "20457 Hamburg",
    status: "Bereit",
    tags: ["Energy market source import", "grid modernization"],
    messageType: "Betreff",
    subject: "Ihre Modernisierungsprojekte als Sales-Pipeline",
    body: "Hallo Herr Weber,\nin Ihrer Projektkommunikation geht es stark um Netzmodernisierung und Partnerkoordination. Genau dort entstehen oft viele lose Follow-ups. CTOX fuehrt solche Signale in Kampagnen, Pipeline und naechste Aktionen zusammen, damit kein interessanter Kontakt nach dem ersten Touchpoint liegen bleibt.\nSoll ich Ihnen eine Beispiel-Liste mit Touchpoints und Anschreiben zeigen?",
    followup1: "Hallo Herr Weber,\nkurzer Nachtrag: Die Idee waere keine Massenmail-Strecke, sondern eine pruefbare Liste mit Touchpoint, Ansprechpartner, Entwurf und naechstem Schritt.",
    followup2: "Hallo Herr Weber,\nfalls die Modernisierungspipeline aktuell nicht Ihr Thema ist, hake ich nicht weiter nach. Einen Beispielaufbau kann ich bei Bedarf gern senden.",
    note: "Touchpoints: PDF member list, modernization wording, commercial role."
  },
  {
    id: "outreach-brightfield-schulz",
    campaignId: "campaign-account-expansion",
    company: "Brightfield GmbH",
    domain: "brightfield.example",
    person: "Lea Schulz",
    email: "lea.schulz@brightfield.example",
    role: "Partner Consulting",
    department: "Consulting",
    location: "10115 Berlin",
    status: "Antwort",
    tags: ["Existing account expansion", "consulting delivery"],
    messageType: "Follow-up 1",
    subject: "Aus Ihrem Consulting-Rollout eine wiederholbare Delivery-Pipeline machen",
    body: "Hallo Frau Schulz,\nbei Brightfield laufen Consulting Delivery und Angebotsarbeit sichtbar eng zusammen. CTOX kann aus angenommenen Angeboten direkt Delivery-Handoffs, Verantwortliche und naechste Aktionen erzeugen. So wird aus dem Sales-Kontext kein manueller Projektstart.\nSoll ich Ihnen den Handoff an einem Beispiel zeigen?",
    followup1: "Hallo Frau Schulz,\nSie hatten geantwortet, dass der Handoff spannend ist. Ich wuerde daraus direkt eine Opportunity mit Kampagnenkontext anlegen und den naechsten Schritt vorbereiten.",
    followup2: "Hallo Frau Schulz,\nich halte den Handoff-Kontext weiter offen und kann Ihnen alternativ nur die Checkliste senden.",
    note: "Antwort erkannt: in Leads ueberfuehren, Tag Existing account expansion setzen."
  },
  {
    id: "outreach-helio-sayed",
    campaignId: "campaign-account-expansion",
    company: "Helio Systems",
    domain: "helio.example",
    person: "Amira Sayed",
    email: "amira.sayed@helio.example",
    role: "VP Operations",
    department: "Operations",
    location: "20095 Hamburg",
    status: "Wartet",
    tags: ["Existing account expansion", "field rollout"],
    messageType: "E-Mail",
    subject: "Field rollout ohne verlorene Follow-ups",
    body: "Hallo Frau Sayed,\nIhr FieldOps-Rollout zeigt, dass operative Zustaendigkeiten und Commercial Handoffs zusammenlaufen. CTOX kann Kampagnenantworten direkt in einen Sales-Lead mit Quelle, Touchpoints und naechstem Schritt ueberfuehren.\nSoll ich Ihnen zeigen, wie der Antwort-Handoff aussieht?",
    followup1: "Hallo Frau Sayed,\nkurzer Nachfass: Der Wert liegt vor allem darin, Antworten nicht nur als Mail, sondern als Lead-Kontext mit Kampagnen-Tag zu behandeln.",
    followup2: "Hallo Frau Sayed,\nich schliesse den Loop hier. Wenn Reply-to-Lead fuer FieldOps relevant wird, kann ich das Beispiel nachreichen.",
    note: "Touchpoints: active opportunity, rollout, operations owner."
  }
];

const messageKeys = [
  ["subject", "Betreff"],
  ["body", "E-Mail"],
  ["followup1", "Follow-up 1"],
  ["followup2", "Follow-up 2"]
] as const;

const inboundCampaigns = [
  {
    id: "inbound-fieldops-readiness",
    name: "FieldOps Readiness Check",
    status: "Draft",
    tag: "Inbound: FieldOps readiness",
    offer: "15-Minuten Pipeline-Check fuer Operations- und Rollout-Teams",
    landingPath: "/lp/fieldops-readiness",
    budget: 4200,
    leads: 18,
    cpl: 233,
    target: "Operations-Leads aus Energie, Field Service und B2B Services",
    variants: [
      {
        id: "variant-a",
        name: "A · Problem first",
        headline: "Keine Antwort aus Kampagnen darf im Postfach enden",
        cta: "Readiness Check anfragen",
        status: "Ready"
      },
      {
        id: "variant-b",
        name: "B · Pipeline first",
        headline: "Aus Anzeigen-Leads direkt eine qualifizierte Sales-Pipeline bauen",
        cta: "Pipeline-Beispiel sehen",
        status: "Draft"
      }
    ],
    channels: [
      { name: "Google Ads / Ad Sense", budget: 2600, goal: "High-intent search and retargeting", status: "Plan" },
      { name: "LinkedIn", budget: 1600, goal: "Ops and Commercial roles", status: "Preview" }
    ],
    fields: ["Name", "E-Mail", "Firma", "Rolle", "Dringlichkeit", "Nachricht"]
  },
  {
    id: "inbound-crm-import",
    name: "CRM Import Automation",
    status: "Planned",
    tag: "Inbound: CRM import automation",
    offer: "Import- und Research-Audit fuer bestehende Leadlisten",
    landingPath: "/lp/crm-import-automation",
    budget: 2800,
    leads: 9,
    cpl: 311,
    target: "Teams mit Excel-, PDF- oder URL-Quellen im Vertrieb",
    variants: [
      {
        id: "variant-a",
        name: "A · Source import",
        headline: "Aus Rohlisten werden recherchierte Kampagnen",
        cta: "Import-Audit starten",
        status: "Ready"
      },
      {
        id: "variant-b",
        name: "B · Research promise",
        headline: "Kontakte, Signale und naechste Schritte automatisch vervollstaendigen",
        cta: "Beispiel anfragen",
        status: "Paused"
      }
    ],
    channels: [
      { name: "Google Ads / Ad Sense", budget: 1800, goal: "Import automation keywords", status: "Plan" },
      { name: "LinkedIn", budget: 1000, goal: "Sales Ops and RevOps feed", status: "Draft" }
    ],
    fields: ["Name", "E-Mail", "Firma", "Quellentyp", "Volumen", "Nachricht"]
  }
];

export function SalesCampaignsView({
  data,
  locale,
  query
}: {
  data: SalesBundle;
  locale: SupportedLocale;
  query: QueryState;
}) {
  const [campaigns, setCampaigns] = useState<SalesCampaign[]>(data.campaigns);
  const [inboundCampaignState, setInboundCampaignState] = useState(inboundCampaigns);
  const [activeDialog, setActiveDialog] = useState<CampaignDialog>(null);
  const [selectedCampaignId, setSelectedCampaignId] = useState(data.campaigns[0]?.id ?? "");
  const [campaignStatus, setCampaignStatus] = useState("");
  const [importStatus, setImportStatus] = useState("");
  const [importSourceType, setImportSourceType] = useState<SalesCampaign["sourceTypes"][number]>("Excel");
  const [importTargetId, setImportTargetId] = useState(data.campaigns[0]?.id ?? NEW_IMPORT_CAMPAIGN_ID);
  const [importNewCampaignName, setImportNewCampaignName] = useState("");
  const [importNewCampaignPrompt, setImportNewCampaignPrompt] = useState(assignmentRules.join("\n"));
  const [columnConfig, setColumnConfig] = useState<CampaignColumnConfig>("");
  const [showCreateRow, setShowCreateRow] = useState(false);
  const [createDraft, setCreateDraft] = useState<CampaignCreateDraft>({
    campaignType: "Outbound",
    campaignName: "",
    assignmentPrompt: assignmentRules.join("\n"),
    status: "Research"
  });
  const selectedOutboundCampaign = campaigns.find((campaign) => campaign.id === selectedCampaignId);
  const selectedInboundCampaign = inboundCampaignState.find((campaign) => campaign.id === selectedCampaignId);
  const selectedCampaign = selectedOutboundCampaign ?? campaigns[0];
  const selectedCampaignLabel = selectedOutboundCampaign?.name ?? selectedInboundCampaign?.name ?? selectedCampaign?.name ?? (locale === "de" ? "Laufende Kampagne" : "Running campaign");
  const selectedOutreachRows = selectedOutboundCampaign ? outreachRows.filter((row) => row.campaignId === selectedOutboundCampaign.id) : [];
  const selectedSourceImports = selectedOutboundCampaign?.id === "campaign-energy-market-import"
    ? sourceImports.filter((source) => source.id !== "src-partner-xlsx")
    : selectedOutboundCampaign?.id === "campaign-account-expansion"
      ? sourceImports.filter((source) => source.id === "src-partner-xlsx")
      : [];
  const assigned = campaigns.reduce((sum, campaign) => sum + campaign.assignedRecords, 0);
  const outboundCampaignRows = campaigns.map((campaign) => {
    const rows = outreachRows.filter((row) => row.campaignId === campaign.id);
    const replies = rows.filter((row) => row.status === "Antwort").length;
    const ready = rows.filter((row) => row.status === "Bereit" || row.status === "Entwurf").length;
    return { campaign, rows, replies, ready };
  });
  const campaignOptions = useMemo(
    () => campaigns.map((campaign) => ({ id: campaign.id, name: campaign.name })),
    [campaigns]
  );
  const toggleCampaignDetails = (campaignId: string) => {
    if (activeDialog === "details" && selectedCampaignId === campaignId) {
      setActiveDialog(null);
      return;
    }
    setSelectedCampaignId(campaignId);
    setActiveDialog("details");
  };
  const updateOutboundCampaign = (campaignId: string, patch: Partial<SalesCampaign>) => {
    setCampaigns((current) => current.map((campaign) => campaign.id === campaignId ? { ...campaign, ...patch } : campaign));
  };
  const updateOutboundText = (campaignId: string, field: "assignmentPrompt" | "nextStep", value: string) => {
    setCampaigns((current) => current.map((campaign) => campaign.id === campaignId ? { ...campaign, [field]: { en: value, de: value } } : campaign));
  };
  const updateInboundCampaign = (campaignId: string, patch: Partial<(typeof inboundCampaigns)[number]>) => {
    setInboundCampaignState((current) => current.map((campaign) => campaign.id === campaignId ? { ...campaign, ...patch } : campaign));
  };
  const createCampaignFromRow = async () => {
    setCampaignStatus(locale === "de" ? "Kampagne wird angelegt." : "Campaign is being created.");
    const campaignName = createDraft.campaignName.trim() || (locale === "de" ? "Neue Kampagne" : "New campaign");
    const id = `campaign-${slugify(campaignName) || crypto.randomUUID()}`;
    if (createDraft.campaignType === "Inbound") {
      const landingPath = `/lp/${slugify(campaignName) || "new-campaign"}`;
      const nextInbound = {
        budget: 0,
        channels: [],
        cpl: 0,
        fields: ["Name", "E-Mail", "Firma", "Nachricht"],
        id,
        landingPath,
        leads: 0,
        name: campaignName,
        offer: createDraft.assignmentPrompt.split("\n").find(Boolean) ?? (locale === "de" ? "Neues Inbound-Angebot" : "New inbound offer"),
        status: "Draft",
        tag: `Inbound: ${campaignName}`,
        target: createDraft.assignmentPrompt,
        variants: [
          {
            cta: locale === "de" ? "Anfrage senden" : "Send request",
            headline: createDraft.assignmentPrompt.split("\n").find(Boolean) ?? campaignName,
            id: "variant-a",
            name: "A · Draft",
            status: "Draft"
          }
        ]
      };
      const result = await postCampaignMutation(query, {
        action: "create",
        instruction: "Create an inbound Sales campaign with landing page planning, contact form handoff into Sales/Leads, campaign tag, budget shell, and editable variants.",
        payload: nextInbound,
        recordId: id,
        title: `Create inbound Sales campaign: ${campaignName}`
      });
      if (result.ok) {
        setInboundCampaignState((current) => [nextInbound, ...current.filter((campaign) => campaign.id !== id)]);
        setSelectedCampaignId(id);
        setShowCreateRow(false);
        setCampaignStatus(locale === "de" ? "Inbound-Kampagne wurde angelegt." : "Inbound campaign created.");
      } else {
        setCampaignStatus(result.error ?? (locale === "de" ? "Aktion fehlgeschlagen." : "Action failed."));
      }
      return;
    }

    const nextCampaign: SalesCampaign = {
      id,
      name: campaignName,
      status: createDraft.status,
      sourceTypes: [],
      importedRecords: 0,
      enrichedRecords: 0,
      assignedRecords: 0,
      ownerId: "sales-lead",
      assignmentPrompt: { en: createDraft.assignmentPrompt, de: createDraft.assignmentPrompt },
      nextStep: {
        en: "Import contact lists or sources, then run research enrichment and prompt-based assignment.",
        de: "Kontaktlisten oder Quellen importieren, danach Research und promptbasierte Zuordnung starten."
      }
    };
    const result = await postCampaignMutation(query, {
      action: "create",
      recordId: id,
      title: `Create outbound Sales campaign: ${campaignName}`,
      instruction: "Create an outbound Sales campaign as a standalone campaign container. It must work before any pipeline, lead, offer, or customer record exists. Use the prompt as the campaign routing and assignment policy for imported contact lists.",
      payload: nextCampaign
    });
    if (result.ok) {
      setCampaigns((current) => [nextCampaign, ...current.filter((campaign) => campaign.id !== id)]);
      setSelectedCampaignId(id);
      setShowCreateRow(false);
      setCampaignStatus(locale === "de" ? "Outbound-Kampagne wurde angelegt." : "Outbound campaign created.");
    } else {
      setCampaignStatus(result.error ?? (locale === "de" ? "Aktion fehlgeschlagen." : "Action failed."));
    }
  };

  return (
    <section className="sales-campaign-workspace" data-context-module="sales" data-context-submodule="campaigns">
      <header className="campaign-hero">
        <div>
          <h1>{locale === "de" ? "Kampagnen" : "Campaigns"}</h1>
          <p>{locale === "de" ? "Laufende Kampagnen zentral verwalten. Import oeffnet links; Settings und Automation liegen im unteren Kampagnendetail." : "Manage running campaigns centrally. Import opens left; settings and automation live in the bottom campaign detail."}</p>
        </div>
        <div className="campaign-toolbar">
          <button
            className="campaign-primary"
            onClick={() => {
              setCampaignStatus("");
              setShowCreateRow((current) => !current);
              setActiveDialog(null);
            }}
            type="button"
          >
            {locale === "de" ? "Kampagne anlegen" : "Create campaign"}
          </button>
          <button
            className="campaign-secondary"
            onClick={() => {
              setImportStatus("");
              setImportTargetId(selectedOutboundCampaign?.id ?? campaigns[0]?.id ?? NEW_IMPORT_CAMPAIGN_ID);
              setActiveDialog("import");
            }}
            type="button"
          >
            {locale === "de" ? "Kontaktliste importieren" : "Import contact list"}
          </button>
        </div>
      </header>

      <section className="campaign-command-center" aria-label={locale === "de" ? "Laufende Kampagnen" : "Running campaigns"}>
        <div className="campaign-pane-head">
          <div>
            <h2>{locale === "de" ? "Laufende Kampagnen" : "Running campaigns"}</h2>
            <p>{locale === "de" ? "Diese Liste ist die Hauptansicht. Lange Kampagnenlisten bleiben hier scrollbar, ohne die Werkzeuge darunter zu stapeln." : "This list is the main view. Long campaign lists stay scrollable here without stacking tools below."}</p>
          </div>
          <div className="campaign-kpis" aria-label="Campaign summary">
            <span><strong>{campaigns.length + inboundCampaignState.length}</strong>{locale === "de" ? "laufend" : "running"}</span>
            <span><strong>{outreachRows.length}</strong>{locale === "de" ? "Ansprachen" : "outreach"}</span>
            <span><strong>{inboundCampaignState.reduce((sum, campaign) => sum + campaign.leads, 0)}</strong>{locale === "de" ? "Inbound-Leads" : "inbound leads"}</span>
            <span><strong>{assigned}</strong>{locale === "de" ? "zugeordnet" : "assigned"}</span>
          </div>
        </div>

        <div className="campaign-hub-table">
          <div className="campaign-hub-row campaign-hub-head">
            <span>{locale === "de" ? "Kampagne" : "Campaign"}</span>
            <span>Status</span>
            <span>{locale === "de" ? "Typ" : "Type"}</span>
            <span>Outbound</span>
            <span>Inbound</span>
            <span>{locale === "de" ? "Naechster Schritt" : "Next step"}</span>
            <span>{locale === "de" ? "Verwalten" : "Manage"}</span>
          </div>
          {showCreateRow ? (
            <form
              className="campaign-hub-create-row"
              onSubmit={(event) => {
                event.preventDefault();
                void createCampaignFromRow();
              }}
            >
              <label>
                <span>{locale === "de" ? "Name" : "Name"}</span>
                <input onChange={(event) => setCreateDraft((current) => ({ ...current, campaignName: event.target.value }))} placeholder={locale === "de" ? "Neue Kampagne" : "New campaign"} value={createDraft.campaignName} />
              </label>
              <label>
                <span>{locale === "de" ? "Typ" : "Type"}</span>
                <select onChange={(event) => setCreateDraft((current) => ({ ...current, campaignType: event.target.value as CampaignCreateDraft["campaignType"] }))} value={createDraft.campaignType}>
                  <option value="Outbound">Outbound</option>
                  <option value="Inbound">Inbound</option>
                </select>
              </label>
              <label>
                <span>Status</span>
                <select onChange={(event) => setCreateDraft((current) => ({ ...current, status: event.target.value as SalesCampaign["status"] }))} value={createDraft.status}>
                  {["Draft", "Research", "Ready", "Active"].map((status) => <option key={status} value={status}>{status}</option>)}
                </select>
              </label>
              <label className="campaign-create-prompt">
                <span>{createDraft.campaignType === "Inbound" ? (locale === "de" ? "Zielgruppe / Angebot" : "Audience / offer") : (locale === "de" ? "Zuordnungskriterien" : "Assignment criteria")}</span>
                <textarea onChange={(event) => setCreateDraft((current) => ({ ...current, assignmentPrompt: event.target.value }))} value={createDraft.assignmentPrompt} />
              </label>
              <span className="campaign-row-actions compact-actions">
                <button className="campaign-primary" type="submit">{locale === "de" ? "Anlegen" : "Create"}</button>
                <button className="campaign-secondary" onClick={() => setShowCreateRow(false)} type="button">{locale === "de" ? "Abbrechen" : "Cancel"}</button>
              </span>
              {campaignStatus ? <small className="campaign-inline-status">{campaignStatus}</small> : null}
            </form>
          ) : null}
          {outboundCampaignRows.map(({ campaign, rows, replies, ready }) => (
            <article
              className={`campaign-hub-row ${activeDialog === "details" && selectedCampaignId === campaign.id ? "is-selected" : ""}`}
              data-campaign-name={campaign.name}
              data-context-item
              data-context-label={campaign.name}
              data-context-module="sales"
              data-context-record-id={campaign.id}
              data-context-record-type="outbound_campaign"
              data-context-submodule="campaigns"
              key={campaign.id}
              onClick={() => toggleCampaignDetails(campaign.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  toggleCampaignDetails(campaign.id);
                }
              }}
              role="button"
              tabIndex={0}
            >
              <span>
                <strong>{campaign.name}</strong>
                <small>{campaign.importedRecords} importiert · {campaign.enrichedRecords} recherchiert · {campaign.assignedRecords} zugeordnet</small>
              </span>
              <span><em>{campaign.status}</em><small>{locale === "de" ? "Outbound aktiv" : "outbound active"}</small></span>
              <span>Outbound</span>
              <span><strong>{rows.length}</strong><small>{ready} bereit · {replies} Antworten</small></span>
              <span><strong>-</strong><small>{locale === "de" ? "keine Landingpage" : "no landing page"}</small></span>
              <span>{text(campaign.nextStep, locale)}</span>
              <span className="campaign-row-actions compact-actions">
                <button onClick={(event) => {
                  event.stopPropagation();
                  setSelectedCampaignId(campaign.id);
                  toggleCampaignDetails(campaign.id);
                }} type="button">{locale === "de" ? "Details" : "Details"}</button>
                <button onClick={(event) => {
                  event.stopPropagation();
                  setSelectedCampaignId(campaign.id);
                  setActiveDialog("details");
                }} type="button">{locale === "de" ? "Settings" : "Settings"}</button>
              </span>
            </article>
          ))}
          {inboundCampaignState.map((campaign) => (
            <article
              className={`campaign-hub-row inbound ${activeDialog === "details" && selectedCampaignId === campaign.id ? "is-selected" : ""}`}
              data-campaign-name={campaign.name}
              data-context-item
              data-context-label={campaign.name}
              data-context-module="sales"
              data-context-record-id={campaign.id}
              data-context-record-type="inbound_campaign"
              data-context-submodule="campaigns"
              key={campaign.id}
              onClick={() => toggleCampaignDetails(campaign.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  toggleCampaignDetails(campaign.id);
                }
              }}
              role="button"
              tabIndex={0}
            >
              <span>
                <strong>{campaign.name}</strong>
                <small>{campaign.tag} · {campaign.landingPath}</small>
              </span>
              <span><em>{campaign.status}</em><small>{locale === "de" ? "Landingpage geplant" : "landing page planned"}</small></span>
              <span>Inbound</span>
              <span><strong>-</strong><small>{locale === "de" ? "kein Versandlauf" : "no send run"}</small></span>
              <span><strong>{campaign.leads}</strong><small>{campaign.budget.toLocaleString("de-DE")} € Budget · {campaign.cpl} € CPL</small></span>
              <span>{campaign.target}</span>
              <span className="campaign-row-actions compact-actions">
                <button onClick={(event) => {
                  event.stopPropagation();
                  toggleCampaignDetails(campaign.id);
                }} type="button">{locale === "de" ? "Details" : "Details"}</button>
                <button onClick={(event) => {
                  event.stopPropagation();
                  setSelectedCampaignId(campaign.id);
                  setActiveDialog("details");
                }} type="button">{locale === "de" ? "Settings" : "Settings"}</button>
              </span>
            </article>
          ))}
        </div>
      </section>

      {activeDialog === "create" ? (
        <dialog className="campaign-create-dialog" open>
          <form
            className="campaign-import-form"
            onSubmit={async (event) => {
              event.preventDefault();
              setCampaignStatus(locale === "de" ? "Kampagne wird angelegt." : "Campaign is being created.");
              const campaignName = createDraft.campaignName.trim() || (locale === "de" ? "Neue Kampagne" : "New campaign");
              const id = `campaign-${slugify(campaignName) || crypto.randomUUID()}`;
              const nextCampaign: SalesCampaign = {
                id,
                name: campaignName,
                status: createDraft.status,
                sourceTypes: [],
                importedRecords: 0,
                enrichedRecords: 0,
                assignedRecords: 0,
                ownerId: "sales-lead",
                assignmentPrompt: { en: createDraft.assignmentPrompt, de: createDraft.assignmentPrompt },
                nextStep: {
                  en: "Import contact lists or sources, then run research enrichment and prompt-based assignment.",
                  de: "Kontaktlisten oder Quellen importieren, danach Research und promptbasierte Zuordnung starten."
                }
              };
              const result = await postCampaignMutation(query, {
                action: "create",
                recordId: id,
                title: `Create Sales campaign: ${campaignName}`,
                instruction: "Create a Sales campaign as a standalone campaign container. It must work before any pipeline, lead, offer, or customer record exists. Use the prompt as the campaign routing and assignment policy for imported contact lists.",
                payload: nextCampaign
              });
              if (result.ok) {
                setCampaigns((current) => [nextCampaign, ...current.filter((campaign) => campaign.id !== id)]);
                setSelectedCampaignId(id);
                setCampaignStatus(locale === "de" ? "Kampagne wurde angelegt." : "Campaign created.");
              } else {
                setCampaignStatus(result.error ?? (locale === "de" ? "Aktion fehlgeschlagen." : "Action failed."));
              }
            }}
          >
            <div className="campaign-dialog-head">
              <div>
                <h2>{locale === "de" ? "Kampagne anlegen" : "Create campaign"}</h2>
                <p>{locale === "de" ? "Eine Kampagne ist der Container fuer Kontaktlisten, Research, Zuordnung und Ansprachen." : "A campaign is the container for contact lists, research, assignment, and outreach."}</p>
              </div>
              <button onClick={() => setActiveDialog(null)} type="button">{locale === "de" ? "Schliessen" : "Close"}</button>
            </div>
            <label>{locale === "de" ? "Kampagnenname" : "Campaign name"}<input onChange={(event) => setCreateDraft((current) => ({ ...current, campaignName: event.target.value }))} placeholder={locale === "de" ? "z.B. Stadtwerke Operations Trigger" : "e.g. Utility operations trigger"} value={createDraft.campaignName} /></label>
            <label>Status<select onChange={(event) => setCreateDraft((current) => ({ ...current, status: event.target.value as SalesCampaign["status"] }))} value={createDraft.status}>
              {["Draft", "Research", "Ready", "Active"].map((status) => <option key={status} value={status}>{status}</option>)}
            </select></label>
            <label>{locale === "de" ? "Zuordnungskriterien per Prompt" : "Assignment criteria prompt"}<textarea onChange={(event) => setCreateDraft((current) => ({ ...current, assignmentPrompt: event.target.value }))} value={createDraft.assignmentPrompt} /></label>
            <button className="campaign-primary" type="submit">{locale === "de" ? "Kampagne anlegen" : "Create campaign"}</button>
            {campaignStatus ? <small>{campaignStatus}</small> : null}
          </form>
        </dialog>
      ) : null}

      {activeDialog === "import" ? (
        <dialog className="campaign-import-dialog" open>
        <form
          className="campaign-import-form"
          onSubmit={async (event) => {
            event.preventDefault();
            const form = event.currentTarget;
            const formData = new FormData(form);
            const file = formData.get("sourceFile");
            const sourceType = importSourceType;
            const sourceUrl = String(formData.get("sourceUrl") || "").trim();
            const sourceText = String(formData.get("sourceText") || "").trim();
            const sourceHint = String(formData.get("sourceHint") || "").trim();
            const createsCampaign = importTargetId === NEW_IMPORT_CAMPAIGN_ID;
            const campaignName = importNewCampaignName.trim() || sourceHint.split("\n").find(Boolean)?.trim() || (locale === "de" ? "Neue Import-Kampagne" : "New import campaign");
            const newCampaignId = `campaign-${slugify(campaignName) || crypto.randomUUID()}`;
            const targetCampaignId = createsCampaign ? newCampaignId : importTargetId;
            const newCampaign: SalesCampaign | null = createsCampaign ? {
              id: newCampaignId,
              name: campaignName,
              status: "Research",
              sourceTypes: [],
              importedRecords: 0,
              enrichedRecords: 0,
              assignedRecords: 0,
              ownerId: "sales-lead",
              assignmentPrompt: { en: importNewCampaignPrompt, de: importNewCampaignPrompt },
              nextStep: {
                en: "Parse the imported contact list, enrich records, and move the campaign through the preparation steps.",
                de: "Importierte Kontaktliste parsen, Datensaetze anreichern und die Kampagne durch die Vorbereitungsschritte fuehren."
              }
            } : null;
            const targetCampaign = newCampaign ?? campaigns.find((campaign) => campaign.id === targetCampaignId);
            setImportStatus(locale === "de" ? "Import wird gestartet." : "Import is starting.");
            const canUseFile = sourceType === "Excel" || sourceType === "PDF";
            const importedRecords = canUseFile && file instanceof File && file.name ? estimateImportedRecords(file, sourceType) : sourceUrl ? 25 : sourceText ? estimateTextRecords(sourceText) : 0;
            const result = await postCampaignMutation(query, {
              action: "create",
              recordId: `source-import-${crypto.randomUUID()}`,
              title: createsCampaign ? `Create campaign and import contact list: ${targetCampaign?.name ?? targetCampaignId}` : `Import contact list into campaign: ${targetCampaign?.name ?? targetCampaignId}`,
              instruction: createsCampaign
                ? "Create a new outbound Sales campaign from this import, then import the contact/source list into it. Parse contacts from Excel, CSV, PDF, URL, or text; normalize company, person, role, email, URL, postal code, source evidence, and consent/status fields; then start the campaign preparation stages."
                : "Import this contact/source list into the selected Sales campaign. Parse contacts from Excel, CSV, PDF, URL, or text; normalize company, person, role, email, URL, postal code, source evidence, and consent/status fields; then start LLM research enrichment and assign every usable contact to this campaign by the campaign prompt.",
              payload: {
                campaignId: targetCampaignId,
                campaignName: targetCampaign?.name,
                createCampaign: createsCampaign,
                newCampaign,
                assignmentPrompt: targetCampaign ? text(targetCampaign.assignmentPrompt, locale) : "",
                sourceType,
                sourceUrl,
                sourceText,
                sourceHint,
                fileName: canUseFile && file instanceof File && file.name ? file.name : "",
                fileType: canUseFile && file instanceof File && file.type ? file.type : "",
                fileSize: canUseFile && file instanceof File && file.size ? file.size : 0,
                importedRecordsEstimate: importedRecords,
                importMode: "campaign_contact_list"
              }
            });
            if (result.ok) {
              setCampaigns((current) => {
                const next = current.map((campaign) => campaign.id === targetCampaignId
                  ? {
                      ...campaign,
                      importedRecords: campaign.importedRecords + importedRecords,
                      sourceTypes: campaign.sourceTypes.includes(sourceType) ? campaign.sourceTypes : [...campaign.sourceTypes, sourceType],
                      status: campaign.status === "Draft" ? "Research" : campaign.status
                    }
                  : campaign);
                if (!newCampaign) return next;
                return [
                  {
                    ...newCampaign,
                    importedRecords,
                    sourceTypes: [sourceType]
                  },
                  ...next.filter((campaign) => campaign.id !== newCampaign.id)
                ];
              });
              setSelectedCampaignId(targetCampaignId);
              setImportTargetId(targetCampaignId);
              setImportStatus(createsCampaign ? (locale === "de" ? "Kampagne wurde angelegt und Import gestartet." : "Campaign created and import started.") : (locale === "de" ? "Kontaktliste wurde importiert." : "Contact list imported."));
            } else {
              setImportStatus(result.error ?? (locale === "de" ? "Aktion fehlgeschlagen." : "Action failed."));
            }
          }}
        >
          <div className="campaign-dialog-head">
            <div>
              <h2>{locale === "de" ? "Kontaktliste importieren" : "Import contact list"}</h2>
              <p>{locale === "de" ? "Excel, URL, PDF oder Rohtext in eine Kampagne importieren und danach Research starten." : "Import Excel, URL, PDF, or raw text into a campaign and then start research."}</p>
            </div>
            <button onClick={() => setActiveDialog(null)} type="button">{locale === "de" ? "Schliessen" : "Close"}</button>
          </div>
          <label>{locale === "de" ? "Zielkampagne" : "Target campaign"}<select name="campaignId" onChange={(event) => setImportTargetId(event.target.value)} value={importTargetId}>
            <option value={NEW_IMPORT_CAMPAIGN_ID}>{locale === "de" ? "Neue Kampagne aus diesem Import anlegen" : "Create new campaign from this import"}</option>
            {campaignOptions.map((campaign) => <option key={campaign.id} value={campaign.id}>{campaign.name}</option>)}
          </select></label>
          {importTargetId === NEW_IMPORT_CAMPAIGN_ID ? (
            <div className="campaign-import-new-campaign">
              <label>{locale === "de" ? "Neue Kampagne" : "New campaign"}<input onChange={(event) => setImportNewCampaignName(event.target.value)} placeholder={locale === "de" ? "z.B. Energie Aussteller Mai" : "e.g. Energy exhibitors May"} value={importNewCampaignName} /></label>
              <label>{locale === "de" ? "Zuordnungskriterien" : "Assignment criteria"}<textarea onChange={(event) => setImportNewCampaignPrompt(event.target.value)} value={importNewCampaignPrompt} /></label>
            </div>
          ) : null}
          <div className="campaign-source-tabs" role="group" aria-label="Source type">
            <label><input checked={importSourceType === "Excel"} name="sourceType" onChange={() => setImportSourceType("Excel")} type="radio" value="Excel" />Excel</label>
            <label><input checked={importSourceType === "URL"} name="sourceType" onChange={() => setImportSourceType("URL")} type="radio" value="URL" />URL</label>
            <label><input checked={importSourceType === "PDF"} name="sourceType" onChange={() => setImportSourceType("PDF")} type="radio" value="PDF" />PDF</label>
            <label><input checked={importSourceType === "Text"} name="sourceType" onChange={() => setImportSourceType("Text")} type="radio" value="Text" />Text</label>
          </div>
          {importSourceType === "Excel" || importSourceType === "PDF" ? (
            <label>{locale === "de" ? "Datei" : "File"}<input accept={importSourceType === "PDF" ? ".pdf,application/pdf" : ".xlsx,.xlsm,.xls,.csv,.tsv"} name="sourceFile" type="file" /></label>
          ) : null}
          {importSourceType === "URL" ? (
            <label>URL<input name="sourceUrl" placeholder="https://example.com/exhibitors" type="url" /></label>
          ) : null}
          {importSourceType === "Text" ? (
            <label>{locale === "de" ? "Copy/Paste Text" : "Copy/paste text"}<textarea className="campaign-source-textarea" name="sourceText" placeholder={locale === "de" ? "Firmen, Personen, Rollen, URLs oder Notizen hier einfuegen ..." : "Paste companies, people, roles, URLs, or notes here ..."} /></label>
          ) : null}
          <label>{locale === "de" ? "Hinweis zur Quelle" : "Source hint"}<textarea name="sourceHint" placeholder={locale === "de" ? "z.B. nur Aussteller mit Operations-, Energie- oder Finance-Bezug" : "e.g. only exhibitors with operations, energy, or finance relevance"} /></label>
          <button className="campaign-primary" type="submit">{locale === "de" ? "Import starten" : "Start import"}</button>
          {importStatus ? <small>{importStatus}</small> : null}
        </form>
        </dialog>
      ) : null}

      {activeDialog === "details" ? (
      <dialog className="campaign-details-sheet" open>
        <div className="campaign-sheet-head">
          <div>
            <h2>{locale === "de" ? "Kampagnendetails" : "Campaign details"}</h2>
            <p>{selectedCampaignLabel}</p>
          </div>
          <button onClick={() => setActiveDialog(null)} type="button">{locale === "de" ? "Schliessen" : "Close"}</button>
        </div>
        <div className={selectedOutboundCampaign ? "campaign-details-board" : "campaign-details-grid campaign-slot-grid"}>
          {selectedOutboundCampaign ? (
            <>
              <section className="campaign-config-strip">
                <form className="campaign-edit-form compact">
                  <label>
                    <span>{locale === "de" ? "Kampagne" : "Campaign"}</span>
                    <input onChange={(event) => updateOutboundCampaign(selectedOutboundCampaign.id, { name: event.target.value })} value={selectedOutboundCampaign.name} />
                  </label>
                  <label>
                    <span>Status</span>
                    <select onChange={(event) => updateOutboundCampaign(selectedOutboundCampaign.id, { status: event.target.value as SalesCampaign["status"] })} value={selectedOutboundCampaign.status}>
                      {["Draft", "Research", "Ready", "Active"].map((status) => <option key={status} value={status}>{status}</option>)}
                    </select>
                  </label>
                  <label>
                    <span>{locale === "de" ? "Zuordnungskriterien" : "Assignment criteria"}</span>
                    <textarea onChange={(event) => updateOutboundText(selectedOutboundCampaign.id, "assignmentPrompt", event.target.value)} value={text(selectedOutboundCampaign.assignmentPrompt, locale)} />
                  </label>
                  <label>
                    <span>{locale === "de" ? "Naechster Schritt" : "Next step"}</span>
                    <textarea onChange={(event) => updateOutboundText(selectedOutboundCampaign.id, "nextStep", event.target.value)} value={text(selectedOutboundCampaign.nextStep, locale)} />
                  </label>
                  <SalesQueueButton
                    action="update"
                    className="campaign-secondary"
                    instruction="Persist the edited outbound Sales campaign settings, including name, status, assignment prompt, next step, imported contact context, and campaign routing policy."
                    payload={{ campaign: selectedOutboundCampaign, type: "Outbound" }}
                    recordId={selectedOutboundCampaign.id}
                    resource="campaigns"
                    title={`Update outbound Sales campaign: ${selectedOutboundCampaign.name}`}
                  >
                    {locale === "de" ? "Speichern" : "Save"}
                  </SalesQueueButton>
                </form>
                <div className="campaign-source-strip">
                  <strong>{selectedOutboundCampaign.importedRecords} importiert · {selectedOutboundCampaign.enrichedRecords} recherchiert · {selectedOutboundCampaign.assignedRecords} zugeordnet</strong>
                  {selectedSourceImports.map((source) => (
                    <span key={source.id}>{source.type}: {source.name} · {source.records}</span>
                  ))}
                  {selectedSourceImports.length ? null : <span>{locale === "de" ? "Noch keine Quelle importiert." : "No source imported yet."}</span>}
                  <SalesQueueButton
                    action="sync"
                    className="campaign-secondary"
                    instruction="Run research enrichment for imported Sales campaign sources. Complete missing website, address, contact role, ICP segment, buying trigger, source confidence, and evidence links."
                    payload={{ campaign: selectedOutboundCampaign, sources: selectedSourceImports, rules: selectedOutboundCampaign.assignmentPrompt }}
                    recordId={`campaign-research-pipeline-${selectedOutboundCampaign.id}`}
                    resource="campaigns"
                    title={`Run campaign source research: ${selectedOutboundCampaign.name}`}
                  >
                    {locale === "de" ? "Research starten" : "Start research"}
                  </SalesQueueButton>
                </div>
              </section>
              <section className="campaign-prep-table" aria-label={locale === "de" ? "Kampagnenvorbereitung" : "Campaign preparation"}>
                <div className="campaign-prep-row campaign-prep-head">
                  <span>{locale === "de" ? "Datensatz" : "Record"}</span>
                  <span>
                    <b>{locale === "de" ? "Firmenstammdaten" : "Company data"}</b>
                    <span className="campaign-column-header-actions">
                      <button aria-label={locale === "de" ? "Firmenstammdaten konfigurieren" : "Configure company data"} onClick={() => setColumnConfig((current) => current === "company" ? "" : "company")} type="button">⚙</button>
                      <SalesQueueButton action="sync" className="campaign-header-play" instruction={`Create company master-data content for all rows in campaign ${selectedOutboundCampaign.name}. Use the configured company data settings, source imports, assignment prompt, and evidence requirements.`} payload={{ campaign: selectedOutboundCampaign, column: "company", prompt: campaignColumnDefaultPrompt("company", locale), rows: selectedOutreachRows }} recordId={`run-company-column-${selectedOutboundCampaign.id}`} resource="campaigns" title={`Run company data column: ${selectedOutboundCampaign.name}`}>▶</SalesQueueButton>
                    </span>
                  </span>
                  <span>
                    <b>{locale === "de" ? "Ansprechpartner" : "Contact"}</b>
                    <span className="campaign-column-header-actions">
                      <button aria-label={locale === "de" ? "Ansprechpartner konfigurieren" : "Configure contact research"} onClick={() => setColumnConfig((current) => current === "contact" ? "" : "contact")} type="button">⚙</button>
                      <SalesQueueButton action="sync" className="campaign-header-play" instruction={`Create contact research content for all rows in campaign ${selectedOutboundCampaign.name}. Use configured contact criteria, verify decision relevance, email, role, source evidence, and alternatives.`} payload={{ campaign: selectedOutboundCampaign, column: "contact", prompt: campaignColumnDefaultPrompt("contact", locale), rows: selectedOutreachRows }} recordId={`run-contact-column-${selectedOutboundCampaign.id}`} resource="campaigns" title={`Run contact research column: ${selectedOutboundCampaign.name}`}>▶</SalesQueueButton>
                    </span>
                  </span>
                  <span>
                    <b>Touchpoint</b>
                    <span className="campaign-column-header-actions">
                      <button aria-label={locale === "de" ? "Touchpoint Research konfigurieren" : "Configure touchpoint research"} onClick={() => setColumnConfig((current) => current === "touchpoint" ? "" : "touchpoint")} type="button">⚙</button>
                      <SalesQueueButton action="sync" className="campaign-header-play" instruction={`Create touchpoint research content for all rows in campaign ${selectedOutboundCampaign.name}. Use configured touchpoint criteria, find triggers, evidence, hypotheses, objections, and source links.`} payload={{ campaign: selectedOutboundCampaign, column: "touchpoint", prompt: campaignColumnDefaultPrompt("touchpoint", locale), rows: selectedOutreachRows }} recordId={`run-touchpoint-column-${selectedOutboundCampaign.id}`} resource="campaigns" title={`Run touchpoint column: ${selectedOutboundCampaign.name}`}>▶</SalesQueueButton>
                    </span>
                  </span>
                  <span>
                    <b>{locale === "de" ? "Ansprache" : "Outreach"}</b>
                    <span className="campaign-column-header-actions">
                      <button aria-label={locale === "de" ? "Ansprache konfigurieren" : "Configure outreach writing"} onClick={() => setColumnConfig((current) => current === "outreach" ? "" : "outreach")} type="button">⚙</button>
                      <SalesQueueButton action="sync" className="campaign-header-play" instruction={`Create outreach drafts for all prepared rows in campaign ${selectedOutboundCampaign.name}. Use company data, contact research, touchpoints, campaign prompt, and configured outreach writing rules.`} payload={{ campaign: selectedOutboundCampaign, column: "outreach", prompt: campaignColumnDefaultPrompt("outreach", locale), rows: selectedOutreachRows }} recordId={`run-outreach-column-${selectedOutboundCampaign.id}`} resource="campaigns" title={`Run outreach column: ${selectedOutboundCampaign.name}`}>▶</SalesQueueButton>
                    </span>
                  </span>
                  <span>
                    <b>{locale === "de" ? "Start / Send" : "Start / send"}</b>
                    <span className="campaign-column-header-actions">
                      <button aria-label={locale === "de" ? "Versand konfigurieren" : "Configure sending"} onClick={() => setColumnConfig((current) => current === "send" ? "" : "send")} type="button">⚙</button>
                      <SalesQueueButton
                        action="sync"
                        className="campaign-header-play"
                        instruction={`Start the approved outbound send run for campaign ${selectedOutboundCampaign.name}. Use configured sender account, provider-compliant rate limits, quiet hours, unsubscribe handling, bounce stopping, and approval requirements. Do not bypass provider or recipient protections.`}
                        payload={{ campaign: selectedOutboundCampaign, column: "send", mailAccounts: campaignMailAccounts, sendPolicy: campaignSendPolicy, rows: selectedOutreachRows }}
                        recordId={`run-send-column-${selectedOutboundCampaign.id}`}
                        resource="campaigns"
                        title={`Run campaign send: ${selectedOutboundCampaign.name}`}
                      >
                        ▶
                      </SalesQueueButton>
                    </span>
                  </span>
                  <span>{locale === "de" ? "Ergebnis" : "Result"}</span>
                </div>
                {columnConfig ? (
                  <div className="campaign-column-config">
                    <strong>{campaignColumnConfigTitle(columnConfig, locale)}</strong>
                    {columnConfig === "send" ? (
                      <div className="campaign-send-config-grid">
                        <label>
                          <span>{locale === "de" ? "Mailkonto / Service" : "Mail account / service"}</span>
                          <select defaultValue={campaignSendPolicy.accountId}>
                            {campaignMailAccounts.map((account) => (
                              <option key={account.id} value={account.id}>{account.label} · {account.provider}</option>
                            ))}
                          </select>
                        </label>
                        <label>
                          <span>{locale === "de" ? "From" : "From"}</span>
                          <input defaultValue={campaignMailAccounts[0].sender} />
                        </label>
                        <label>
                          <span>Reply-To</span>
                          <input defaultValue={campaignMailAccounts[0].replyTo} />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Mails / Stunde" : "Mails / hour"}</span>
                          <input defaultValue={String(campaignMailAccounts[0].hourlyLimit)} min="1" type="number" />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Tageslimit" : "Daily cap"}</span>
                          <input defaultValue={String(campaignMailAccounts[0].dailyLimit)} min="1" type="number" />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Delay min" : "Delay min"}</span>
                          <input defaultValue={String(campaignSendPolicy.minDelayMinutes)} min="1" type="number" />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Delay max" : "Delay max"}</span>
                          <input defaultValue={String(campaignSendPolicy.maxDelayMinutes)} min="1" type="number" />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Ruhezeit" : "Quiet hours"}</span>
                          <input defaultValue={campaignSendPolicy.quietHours} />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Stop bei Bounces" : "Stop after bounces"}</span>
                          <input defaultValue={String(campaignSendPolicy.bounceStopAfter)} min="1" type="number" />
                        </label>
                        <label>
                          <span>{locale === "de" ? "Freigabe" : "Approval"}</span>
                          <select defaultValue={campaignSendPolicy.requireApproval ? "required" : "optional"}>
                            <option value="required">{locale === "de" ? "Vor Versand erforderlich" : "Required before send"}</option>
                            <option value="optional">{locale === "de" ? "Optional" : "Optional"}</option>
                          </select>
                        </label>
                      </div>
                    ) : (
                      <label>
                        <span>{locale === "de" ? "Prompt / Kriterien" : "Prompt / criteria"}</span>
                        <textarea defaultValue={campaignColumnDefaultPrompt(columnConfig, locale)} />
                      </label>
                    )}
                    <SalesQueueButton
                      action="sync"
                      className="campaign-secondary"
                      instruction={columnConfig === "send" ? `Configure outbound campaign sending for ${selectedOutboundCampaign.name}. Save the sender account or Resend provider, From and Reply-To addresses, approval gate, provider-compliant rate limits, quiet hours, jitter window, unsubscribe handling, and bounce stop rules.` : `Configure outbound campaign preparation column ${columnConfig} for ${selectedOutboundCampaign.name}. Save the prompt, processing policy, required evidence, and completion criteria for this column.`}
                      payload={columnConfig === "send" ? { campaign: selectedOutboundCampaign, column: columnConfig, mailAccounts: campaignMailAccounts, sendPolicy: campaignSendPolicy } : { campaign: selectedOutboundCampaign, column: columnConfig }}
                      recordId={`campaign-column-config-${selectedOutboundCampaign.id}-${columnConfig}`}
                      resource="campaigns"
                      title={`Configure campaign column ${columnConfig}: ${selectedOutboundCampaign.name}`}
                    >
                      {locale === "de" ? "Einstellungen speichern" : "Save settings"}
                    </SalesQueueButton>
                  </div>
                ) : null}
                {selectedOutreachRows.length ? selectedOutreachRows.map((row) => (
                  <article
                    className={`campaign-prep-row status-${row.status.toLowerCase()}`}
                    data-campaign-id={row.campaignId}
                    data-company={row.company}
                    data-context-item
                    data-context-label={`${row.company}: ${row.person}`}
                    data-context-module="sales"
                    data-context-record-id={row.id}
                    data-context-record-type="campaign_contact"
                    data-context-submodule="campaigns"
                    data-email={row.email}
                    data-outreach-id={row.id}
                    data-person={row.person}
                    data-tags={row.tags.join(", ")}
                    key={row.id}
                  >
                    <span>
                      <strong>{row.company}</strong>
                      <small>{row.domain} · {row.location}</small>
                      <em>{row.tags[0]}</em>
                    </span>
                    <span>
                      <strong>{locale === "de" ? "Stammdaten" : "Master data"}</strong>
                      <small>{row.department} · {row.location}</small>
                      <SalesQueueButton action="sync" className="campaign-secondary" instruction={`Research company master data for ${row.company}: legal name, website, segment, location, size, buying triggers, and evidence.`} payload={{ campaign: selectedOutboundCampaign, row }} recordId={`company-research-${row.id}`} resource="campaigns" title={`Company research: ${row.company}`}>{locale === "de" ? "Research" : "Research"}</SalesQueueButton>
                    </span>
                    <span>
                      <strong>{row.person}</strong>
                      <small>{row.role} · {row.email}</small>
                      <SalesQueueButton action="sync" className="campaign-secondary" instruction={`Research and verify the best campaign contact for ${row.company}. Confirm role, email, decision relevance, and alternate contacts.`} payload={{ campaign: selectedOutboundCampaign, row }} recordId={`contact-research-${row.id}`} resource="campaigns" title={`Contact research: ${row.company}`}>{locale === "de" ? "Pruefen" : "Verify"}</SalesQueueButton>
                    </span>
                    <span>
                      <strong>{locale === "de" ? "Touchpoints" : "Touchpoints"}</strong>
                      <textarea defaultValue={row.note} name={`touchpoint-${row.id}`} />
                      <SalesQueueButton action="sync" className="campaign-secondary" instruction={`Run touchpoint research for ${row.company} and ${row.person}. Find concrete source evidence, trigger, hypothesis, and objection context for outbound.`} payload={{ campaign: selectedOutboundCampaign, row }} recordId={`touchpoint-research-${row.id}`} resource="campaigns" title={`Touchpoint research: ${row.company}`}>Touchpoint</SalesQueueButton>
                    </span>
                    <span className="campaign-message-cell">
                      <small><strong>{row.subject}</strong></small>
                      <textarea defaultValue={row.body} name={`message-${row.id}`} />
                      <SalesQueueButton action="sync" className="campaign-secondary" instruction={`Write or refine the outbound message for ${row.company} and ${row.person} based on company data, contact research, and touchpoints.`} payload={{ campaign: selectedOutboundCampaign, row }} recordId={`write-outreach-${row.id}`} resource="campaigns" title={`Write outreach: ${row.company}`}>{locale === "de" ? "Entwurf" : "Draft"}</SalesQueueButton>
                    </span>
                    <span className="campaign-row-actions">
                      <em>{row.status}</em>
                      <button data-campaign-send={row.id} type="button">{locale === "de" ? "Senden" : "Send"}</button>
                    </span>
                    <span className="campaign-row-actions">
                      <button data-campaign-reply-handoff={row.id} type="button">{locale === "de" ? "Antwort -> Lead" : "Reply -> Lead"}</button>
                      <button type="button">{locale === "de" ? "Abgelehnt" : "Rejected"}</button>
                      <button type="button">Unknown</button>
                    </span>
                  </article>
                )) : (
                  <article className="campaign-slot-card">
                    <span>{locale === "de" ? "Noch leer" : "Empty"}</span>
                    <strong>{locale === "de" ? "Keine Kontakte importiert" : "No contacts imported"}</strong>
                    <p>{locale === "de" ? "Importiere zuerst eine Kontaktliste in diese Kampagne." : "Import a contact list into this campaign first."}</p>
                  </article>
                )}
              </section>
            </>
          ) : (
            <>
              <section>
                <h3>{locale === "de" ? "Slots" : "Slots"}</h3>
                <div className="campaign-slot-list">
              {selectedInboundCampaign ? (
                <form className="campaign-edit-form">
                  <label>
                    <span>{locale === "de" ? "Kampagnenname" : "Campaign name"}</span>
                    <input onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { name: event.target.value })} value={selectedInboundCampaign.name} />
                  </label>
                  <label>
                    <span>Status</span>
                    <select onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { status: event.target.value })} value={selectedInboundCampaign.status}>
                      {["Draft", "Planned", "Ready", "Active", "Paused"].map((status) => <option key={status} value={status}>{status}</option>)}
                    </select>
                  </label>
                  <label>
                    <span>Landingpage</span>
                    <input onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { landingPath: event.target.value })} value={selectedInboundCampaign.landingPath} />
                  </label>
                  <label>
                    <span>{locale === "de" ? "Angebot" : "Offer"}</span>
                    <textarea onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { offer: event.target.value })} value={selectedInboundCampaign.offer} />
                  </label>
                  <label>
                    <span>{locale === "de" ? "Zielgruppe" : "Audience"}</span>
                    <textarea onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { target: event.target.value })} value={selectedInboundCampaign.target} />
                  </label>
                  <div className="campaign-edit-grid">
                    <label>
                      <span>{locale === "de" ? "Budget" : "Budget"}</span>
                      <input min="0" onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { budget: Number(event.target.value) || 0 })} type="number" value={selectedInboundCampaign.budget} />
                    </label>
                    <label>
                      <span>CPL</span>
                      <input min="0" onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { cpl: Number(event.target.value) || 0 })} type="number" value={selectedInboundCampaign.cpl} />
                    </label>
                  </div>
                  <label>
                    <span>{locale === "de" ? "Formularfelder" : "Form fields"}</span>
                    <input onChange={(event) => updateInboundCampaign(selectedInboundCampaign.id, { fields: event.target.value.split(",").map((field) => field.trim()).filter(Boolean) })} value={selectedInboundCampaign.fields.join(", ")} />
                  </label>
                  <SalesQueueButton
                    action="update"
                    className="campaign-secondary"
                    instruction="Persist the edited inbound Sales campaign settings, including landing page path, offer, audience, budget, CPL, form fields, variants, and lead handoff tags."
                    payload={{ campaign: selectedInboundCampaign, type: "Inbound" }}
                    recordId={selectedInboundCampaign.id}
                    resource="campaigns"
                    title={`Update inbound Sales campaign: ${selectedInboundCampaign.name}`}
                  >
                    {locale === "de" ? "Aenderungen speichern" : "Save changes"}
                  </SalesQueueButton>
                </form>
              ) : null}
                </div>
              </section>
              <section>
            <h3>{selectedInboundCampaign ? (locale === "de" ? "Landingpage-Varianten" : "Landing page variants") : (locale === "de" ? "Importierte Kontakte" : "Imported contacts")}</h3>
            {selectedInboundCampaign ? (
              <div className="campaign-slot-list">
                {selectedInboundCampaign.variants.map((variant) => (
                  <article className="campaign-slot-card" key={variant.id}>
                    <span>{variant.status}</span>
                    <strong>{variant.name}</strong>
                    <p>{variant.headline}</p>
                    <small>{variant.cta}</small>
                  </article>
                ))}
                {selectedInboundCampaign.channels.map((channel) => (
                  <article className="campaign-slot-card" key={channel.name}>
                    <span>{channel.status}</span>
                    <strong>{channel.name}</strong>
                    <small>{channel.budget.toLocaleString("de-DE")} €</small>
                    <p>{channel.goal}</p>
                  </article>
                ))}
              </div>
            ) : null}
              </section>
              <section>
            <h3>{selectedInboundCampaign ? (locale === "de" ? "Inbound-Statistiken" : "Inbound statistics") : (locale === "de" ? "Quellen & Research" : "Sources & research")}</h3>
            <div className="campaign-slot-list">
              {selectedInboundCampaign ? (
                <article className="campaign-inbound-stat">
                  <span>{selectedInboundCampaign.status}</span>
                  <strong>{selectedInboundCampaign.name}</strong>
                  <small>{selectedInboundCampaign.leads} Leads · {selectedInboundCampaign.budget.toLocaleString("de-DE")} € Budget · {selectedInboundCampaign.cpl} € CPL</small>
                  <p>{selectedInboundCampaign.offer}</p>
                </article>
              ) : null}
            </div>
              </section>
            </>
          )}
        </div>
      </dialog>
      ) : null}

      <script dangerouslySetInnerHTML={{ __html: campaignOutreachScript(locale, query) }} />
    </section>
  );
}

async function postCampaignMutation(query: QueryState, body: Record<string, unknown>): Promise<{
  ok?: boolean;
  core?: { taskId?: string | null; mode?: string };
  error?: string;
}> {
  const response = await fetch("/api/sales/campaigns", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ locale: query.locale, theme: query.theme, ...body })
  });
  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<{
    ok?: boolean;
    core?: { taskId?: string | null; mode?: string };
    error?: string;
  }>;
}

function slugify(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function estimateImportedRecords(file: File, sourceType: SalesCampaign["sourceTypes"][number]) {
  if (sourceType === "PDF") return Math.max(8, Math.min(180, Math.round(file.size / 18_000)));
  if (sourceType === "Excel") return Math.max(12, Math.min(500, Math.round(file.size / 8_000)));
  if (sourceType === "Text") return Math.max(4, Math.min(160, Math.round(file.size / 4_000)));
  return 25;
}

function estimateTextRecords(value: string) {
  const rows = value.split(/\n+/).map((row) => row.trim()).filter(Boolean);
  return Math.max(1, Math.min(250, rows.length || Math.round(value.length / 120)));
}

function campaignColumnConfigTitle(column: string, locale: SupportedLocale) {
  const labels: Record<string, LocalizedText> = {
    company: { de: "Spaltenkonfiguration: Firmenstammdaten", en: "Column configuration: company data" },
    contact: { de: "Spaltenkonfiguration: Ansprechpartner", en: "Column configuration: contact research" },
    outreach: { de: "Spaltenkonfiguration: Ansprache", en: "Column configuration: outreach writing" },
    send: { de: "Spaltenkonfiguration: Versand", en: "Column configuration: sending" },
    touchpoint: { de: "Spaltenkonfiguration: Touchpoint Research", en: "Column configuration: touchpoint research" }
  };
  return text(labels[column] ?? labels.company, locale);
}

function campaignColumnDefaultPrompt(column: string, locale: SupportedLocale) {
  const prompts: Record<string, LocalizedText> = {
    company: {
      de: "Recherchiere Firmenstammdaten: legaler Name, Website, Segment, Standort, Groesse, relevante Signale und Quellenbelege. Erst abschliessen, wenn Website und Segment belastbar sind.",
      en: "Research company master data: legal name, website, segment, location, size, relevant signals, and source evidence. Complete only when website and segment are reliable."
    },
    contact: {
      de: "Identifiziere den besten Ansprechpartner und Alternativen. Pruefe Rolle, Entscheidungsnaehe, E-Mail, LinkedIn/Website-Beleg und Kontaktconfidence.",
      en: "Identify the best contact and alternates. Verify role, decision relevance, email, LinkedIn/site evidence, and contact confidence."
    },
    touchpoint: {
      de: "Ermittle konkrete Touchpoints: Trigger, Anlass, Hypothese, moeglicher Einwand, Quelle und warum diese Person jetzt sinnvoll angesprochen wird.",
      en: "Find concrete touchpoints: trigger, reason, hypothesis, likely objection, source, and why this person should be contacted now."
    },
    outreach: {
      de: "Schreibe eine knappe Ansprache aus Stammdaten, Ansprechpartner-Research und Touchpoint. Kein generisches Mailing, genau ein naechster Schritt.",
      en: "Write concise outreach from company data, contact research, and touchpoint. No generic mailing, exactly one next step."
    }
  };
  return text(prompts[column] ?? prompts.company, locale);
}

function InboundWorkbench({ locale, query }: { locale: SupportedLocale; query: QueryState }) {
  const totalBudget = inboundCampaigns.reduce((sum, campaign) => sum + campaign.budget, 0);
  const totalLeads = inboundCampaigns.reduce((sum, campaign) => sum + campaign.leads, 0);
  const activeCampaign = inboundCampaigns[0];
  const linkedInPreview = {
    eyebrow: "CTOX Business OS",
    author: "CTOX Business OS",
    text: "Antworten aus Anzeigen sollten nicht in Formularen liegen bleiben. Der FieldOps Readiness Check fuehrt Landingpage-Leads direkt in das Leads-Modul, inklusive Kampagnen-Tag, Quelle und naechstem Schritt.",
    linkTitle: activeCampaign.offer,
    url: `ctox.example${activeCampaign.landingPath}`
  };

  return (
    <section className="campaign-inbound" aria-label={locale === "de" ? "Inbound-Kampagnen" : "Inbound campaigns"}>
      <header className="campaign-inbound-head">
        <div>
          <h2>{locale === "de" ? "Inbound-Kampagnen" : "Inbound campaigns"}</h2>
          <p>{locale === "de" ? "Landingpage-Varianten, Kontaktformular, Anzeigenbudget und Social-Preview fuer bezahlte Leadkampagnen." : "Landing page variants, contact form, ad budget, and social preview for paid lead campaigns."}</p>
        </div>
        <div className="campaign-inbound-summary" aria-label="Inbound summary">
          <span><strong>{inboundCampaigns.length}</strong>{locale === "de" ? "Kampagnen" : "campaigns"}</span>
          <span><strong>{totalBudget.toLocaleString("de-DE")} €</strong>{locale === "de" ? "Budget" : "budget"}</span>
          <span><strong>{totalLeads}</strong>{locale === "de" ? "Leads" : "leads"}</span>
        </div>
      </header>

      <div className="campaign-inbound-grid">
        <section className="campaign-inbound-pane campaign-landing-builder">
          <div className="campaign-pane-head">
            <div>
              <h3>{locale === "de" ? "Landingpages" : "Landing pages"}</h3>
              <p>{locale === "de" ? "Varianten der normalen Website fuer Anzeigen und organische Feeds." : "Variants of the normal website for ads and organic feeds."}</p>
            </div>
            <SalesQueueButton
              action="sync"
              className="campaign-secondary"
              instruction="Generate controlled landing page variants from the normal CTOX web page. Keep page components reusable, include contact form tracking, preserve campaign tags, and prepare preview URLs before publication."
              payload={{ campaigns: inboundCampaigns }}
              recordId="inbound-landing-page-variants"
              resource="campaigns"
              title="Prepare inbound landing page variants"
            >
              {locale === "de" ? "Varianten erzeugen" : "Generate variants"}
            </SalesQueueButton>
          </div>
          <form className="campaign-landing-form" data-inbound-landing-form>
            <label>{locale === "de" ? "Kampagne" : "Campaign"}<input defaultValue={activeCampaign.name} name="campaignName" /></label>
            <label>{locale === "de" ? "Angebot" : "Offer"}<input defaultValue={activeCampaign.offer} name="offer" /></label>
            <label>{locale === "de" ? "Zielgruppe" : "Audience"}<textarea defaultValue={activeCampaign.target} name="audience" /></label>
            <label>{locale === "de" ? "Formularfelder" : "Form fields"}<input defaultValue={activeCampaign.fields.join(", ")} name="formFields" /></label>
            <label>{locale === "de" ? "Lead-Handoff" : "Lead handoff"}<textarea defaultValue="Jeder Formular-Submit erzeugt einen Lead im Sales/Leads-Modul mit Kampagnen-Tag, Landingpage-Variante, Anzeigenquelle, Formularantworten und naechstem Qualifizierungsschritt." name="handoffRule" /></label>
            <button className="campaign-primary" type="submit">{locale === "de" ? "Landingpage anlegen" : "Create landing page"}</button>
            <small data-inbound-landing-status />
          </form>
          <div className="campaign-variant-list">
            {inboundCampaigns.flatMap((campaign) => campaign.variants.map((variant) => (
              <article className="campaign-variant" data-inbound-campaign-id={campaign.id} key={`${campaign.id}-${variant.id}`}>
                <span>{variant.status}</span>
                <strong>{campaign.name}</strong>
                <p>{variant.headline}</p>
                <small>{variant.name} · {campaign.landingPath} · {variant.cta}</small>
              </article>
            )))}
          </div>
        </section>

        <section className="campaign-inbound-pane">
          <div className="campaign-pane-head">
            <div>
              <h3>{locale === "de" ? "Anzeigenbudget" : "Ad budget"}</h3>
              <p>{locale === "de" ? "Planung fuer Paid Search, Display und Social-Ausspielung." : "Planning for paid search, display, and social rollout."}</p>
            </div>
          </div>
          <form className="campaign-budget-form" data-inbound-budget-form>
            <label>{locale === "de" ? "Monatsbudget" : "Monthly budget"}<input defaultValue={String(activeCampaign.budget)} min="0" name="monthlyBudget" type="number" /></label>
            <label>{locale === "de" ? "Ziel-CPL" : "Target CPL"}<input defaultValue={String(activeCampaign.cpl)} min="0" name="targetCpl" type="number" /></label>
            <label>{locale === "de" ? "Kanal-Mix" : "Channel mix"}<textarea defaultValue={activeCampaign.channels.map((channel) => `${channel.name}: ${channel.budget} EUR - ${channel.goal}`).join("\n")} name="channelMix" /></label>
            <button className="campaign-primary" type="submit">{locale === "de" ? "Budgetplan speichern" : "Save budget plan"}</button>
            <small data-inbound-budget-status />
          </form>
          <div className="campaign-channel-list">
            {inboundCampaigns.flatMap((campaign) => campaign.channels.map((channel) => (
              <article className="campaign-channel" key={`${campaign.id}-${channel.name}`}>
                <span>{channel.status}</span>
                <strong>{channel.name}</strong>
                <small>{campaign.name} · {channel.budget.toLocaleString("de-DE")} €</small>
                <p>{channel.goal}</p>
              </article>
            )))}
          </div>
          <button className="campaign-secondary" data-inbound-ads-rollout type="button">{locale === "de" ? "Ad-Ausspielung vorbereiten" : "Prepare ad rollout"}</button>
        </section>

        <section className="campaign-inbound-pane">
          <div className="campaign-pane-head">
            <div>
              <h3>{locale === "de" ? "LinkedIn Preview" : "LinkedIn preview"}</h3>
              <p>{locale === "de" ? "Feed-Beitrag zur Social-Bewerbung vor dem Ausrollen pruefen." : "Review the feed post before social rollout."}</p>
            </div>
          </div>
          <article className="campaign-social-preview" data-inbound-social-preview>
            <div className="campaign-social-author">
              <span>CT</span>
              <div>
                <strong>{linkedInPreview.author}</strong>
                <small>{locale === "de" ? "Gesponsert · B2B Operations" : "Sponsored · B2B operations"}</small>
              </div>
            </div>
            <p>{linkedInPreview.text}</p>
            <div className="campaign-social-creative">
              <strong>{linkedInPreview.linkTitle}</strong>
              <small>{linkedInPreview.url}</small>
            </div>
          </article>
          <div className="campaign-inbound-actions">
            <button className="campaign-primary" data-inbound-linkedin-publish type="button">{locale === "de" ? "Feed-Post vorbereiten" : "Prepare feed post"}</button>
            <button className="campaign-secondary" data-inbound-lead-handoff type="button">{locale === "de" ? "Test-Lead -> Leads" : "Test lead -> Leads"}</button>
            <small data-inbound-action-status />
          </div>
          <aside className="campaign-reply-rule">
            <strong>{locale === "de" ? "Inbound-Regel" : "Inbound rule"}</strong>
            <p>{locale === "de" ? "Jeder Formular-Lead landet automatisch in Sales/Leads. Der Lead traegt Kampagnen-Tag, Landingpage-Variante, Anzeigenquelle und Formularantworten." : "Every form lead is created in Sales/Leads with campaign tag, landing page variant, ad source, and form answers."}</p>
          </aside>
        </section>
      </div>
      <script dangerouslySetInnerHTML={{ __html: inboundCampaignScript(locale, query) }} />
    </section>
  );
}

function OutreachWorkbench({ locale, query }: { locale: SupportedLocale; query: QueryState }) {
  const replyRows = outreachRows.filter((row) => row.status === "Antwort");
  const readyRows = outreachRows.filter((row) => row.status === "Bereit" || row.status === "Entwurf");

  return (
    <section className="campaign-outreach" aria-label={locale === "de" ? "Kampagnen-Versandliste" : "Campaign outreach list"}>
      <header className="campaign-outreach-head">
        <div>
          <h2>{locale === "de" ? "Versandliste" : "Outreach list"}</h2>
          <p>{locale === "de" ? "Touchpoint-Analyse, Anschreiben und Follow-ups pro Kandidat. Antworten erzeugen Leads mit Kampagnen-Tag." : "Touchpoint analysis, emails, and follow-ups per candidate. Replies create leads with campaign tags."}</p>
        </div>
        <div className="campaign-outreach-actions">
          <SalesQueueButton
            action="sync"
            className="campaign-secondary"
            instruction="Use Minimax V2.7 automation to research campaign candidates, find company touchpoints, choose the best contact per company, and generate subject, email body, follow-up 1, follow-up 2, general note, and next-step note."
            payload={{ rows: outreachRows, automation: "minimax-v2.7-touchpoint-analysis" }}
            recordId="campaign-touchpoint-analysis"
            resource="campaigns"
            title="Run campaign touchpoint analysis"
          >
            {locale === "de" ? "Touchpoints aktualisieren" : "Refresh touchpoints"}
          </SalesQueueButton>
          <button className="campaign-primary" data-campaign-batch-send type="button">{locale === "de" ? "Batch versenden" : "Send batch"}</button>
          <span>{readyRows.length} ready · {replyRows.length} replies</span>
        </div>
      </header>

      <div className="campaign-outreach-table">
        <div className="campaign-outreach-row campaign-outreach-table-head">
          <span>Firma</span>
          <span>Name</span>
          <span>Kontakt</span>
          <span>Rolle</span>
          <span>Ort</span>
          <span>Status</span>
          <span>Tags</span>
          <span>Nachricht</span>
          <span>Notiz</span>
          <span>Aktion</span>
        </div>
        {outreachRows.map((row) => (
          <article
            className={`campaign-outreach-row status-${row.status.toLowerCase()}`}
            data-campaign-id={row.campaignId}
            data-company={row.company}
            data-context-item
            data-context-label={`${row.company}: ${row.person}`}
            data-context-module="sales"
            data-context-record-id={row.id}
            data-context-record-type="campaign_outreach"
            data-context-submodule="campaigns"
            data-email={row.email}
            data-outreach-id={row.id}
            data-person={row.person}
            data-tags={row.tags.join(", ")}
            key={row.id}
          >
            <span><strong>{row.company}</strong><a href={`https://${row.domain}`}>{row.domain}</a></span>
            <span><strong>{row.person}</strong><small>{row.department}</small></span>
            <span><a href={`mailto:${row.email}`}>{row.email}</a></span>
            <span>{row.role}</span>
            <span>{row.location}</span>
            <span><em>{row.status}</em></span>
            <span className="campaign-tag-stack">{row.tags.map((tag) => <small key={tag}>{tag}</small>)}</span>
            <span className="campaign-message-cell">
              <span className="campaign-message-tabs">
                {messageKeys.map(([key, label]) => <button className={row.messageType === label ? "active" : ""} data-message-key={key} key={key} type="button">{label}</button>)}
              </span>
              <textarea defaultValue={row.body} name={`message-${row.id}`} />
              <small><strong>{row.subject}</strong></small>
            </span>
            <span><textarea defaultValue={row.note} name={`note-${row.id}`} /></span>
            <span className="campaign-row-actions">
              <button data-campaign-send={row.id} type="button">{locale === "de" ? "Senden" : "Send"}</button>
              <button data-campaign-reply-handoff={row.id} type="button">{locale === "de" ? "Antwort -> Lead" : "Reply -> Lead"}</button>
            </span>
          </article>
        ))}
      </div>

      <aside className="campaign-reply-rule">
        <strong>{locale === "de" ? "Antwort-Regel" : "Reply rule"}</strong>
        <p>{locale === "de" ? "Sobald eine Antwort zur Kampagne erkannt wird, wird automatisch ein Lead in Sales/Leads angelegt. Quelle, Touchpoints und Kampagnen-Tag bleiben am Lead." : "When a campaign reply is detected, a Sales/Leads record is created automatically. Source, touchpoints, and campaign tag stay attached to the lead."}</p>
      </aside>
      <script dangerouslySetInnerHTML={{ __html: campaignOutreachScript(locale, query) }} />
    </section>
  );
}

function inboundCampaignScript(locale: SupportedLocale, query: QueryState) {
  const messages = locale === "de"
    ? { working: "Wird gestartet ...", landing: "Landingpage wurde vorbereitet.", budget: "Budgetplan wurde gespeichert.", ads: "Ad-Ausspielung wurde vorbereitet.", social: "LinkedIn-Preview wurde vorbereitet.", lead: "Test-Lead wurde in Leads angelegt.", failed: "Aktion fehlgeschlagen." }
    : { working: "Working ...", landing: "Landing page prepared.", budget: "Budget plan saved.", ads: "Ad rollout prepared.", social: "LinkedIn preview prepared.", lead: "Test lead created in Leads.", failed: "Action failed." };
  return `(() => {
  const messages = ${JSON.stringify(messages)};
  const basePayload = ${JSON.stringify({ locale: query.locale, theme: query.theme })};
  const inboundCampaigns = ${JSON.stringify(inboundCampaigns)};
  const postJson = async (url, body) => {
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...basePayload, ...body })
    });
    return response.ok;
  };
  const landingPayload = (form) => {
    const data = new FormData(form);
    return {
      campaignName: String(data.get("campaignName") || ""),
      offer: String(data.get("offer") || ""),
      audience: String(data.get("audience") || ""),
      formFields: String(data.get("formFields") || "").split(",").map((field) => field.trim()).filter(Boolean),
      handoffRule: String(data.get("handoffRule") || ""),
      variants: inboundCampaigns[0]?.variants || []
    };
  };
  document.addEventListener("submit", async (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    if (form.matches("[data-inbound-landing-form]")) {
      event.preventDefault();
      const status = form.querySelector("[data-inbound-landing-status]");
      const payload = landingPayload(form);
      const ok = await postJson("/api/sales/campaigns", {
        action: "create",
        recordId: "inbound-landing-" + crypto.randomUUID(),
        title: "Create inbound landing page: " + payload.campaignName,
        instruction: "Create an inbound Sales campaign landing page as a variant of the normal CTOX web page. Include the configured contact form fields, conversion tracking, campaign tags, preview URL, and automatic Sales/Leads handoff rule for every submitted form lead.",
        payload
      });
      if (status) status.textContent = ok ? messages.landing : messages.failed;
    }
    if (form.matches("[data-inbound-budget-form]")) {
      event.preventDefault();
      const status = form.querySelector("[data-inbound-budget-status]");
      const data = new FormData(form);
      const payload = {
        monthlyBudget: Number(data.get("monthlyBudget") || 0),
        targetCpl: Number(data.get("targetCpl") || 0),
        channelMix: String(data.get("channelMix") || "")
      };
      const ok = await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: "inbound-ad-budget-plan",
        title: "Plan inbound ad budget",
        instruction: "Prepare a paid inbound campaign budget plan. Split budget across Google Ads / Ad Sense and LinkedIn, estimate lead volume from target CPL, preserve rollout guardrails, and keep every channel in review before publication.",
        payload
      });
      if (status) status.textContent = ok ? messages.budget : messages.failed;
    }
  });
  document.addEventListener("click", async (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const status = document.querySelector("[data-inbound-action-status]");
    if (target.closest("[data-inbound-ads-rollout]")) {
      const button = target.closest("[data-inbound-ads-rollout]");
      button.textContent = messages.working;
      const ok = await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: "inbound-ads-rollout",
        title: "Prepare inbound ad rollout",
        instruction: "Prepare channel rollout for inbound campaigns. Create reviewable Google Ads / Ad Sense plan, LinkedIn campaign plan, UTM structure, daily budget caps, conversion tracking, and publication checklist.",
        payload: { campaigns: inboundCampaigns }
      });
      button.textContent = ok ? messages.ads : messages.failed;
    }
    if (target.closest("[data-inbound-linkedin-publish]")) {
      const button = target.closest("[data-inbound-linkedin-publish]");
      button.textContent = messages.working;
      const preview = document.querySelector("[data-inbound-social-preview]")?.textContent || "";
      const ok = await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: "inbound-linkedin-feed-preview",
        title: "Prepare LinkedIn feed post preview",
        instruction: "Prepare a LinkedIn feed post preview for inbound campaign promotion. Keep it as a reviewable draft with landing page URL, UTM tags, target audience, and campaign attribution.",
        payload: { preview, campaigns: inboundCampaigns }
      });
      button.textContent = ok ? messages.social : messages.failed;
      if (status) status.textContent = ok ? messages.social : messages.failed;
    }
    if (target.closest("[data-inbound-lead-handoff]")) {
      const button = target.closest("[data-inbound-lead-handoff]");
      button.textContent = messages.working;
      const campaign = inboundCampaigns[0];
      const ok = await postJson("/api/sales/accounts", {
        action: "create",
        recordId: "inbound-lead-" + crypto.randomUUID(),
        title: "Create Sales lead from inbound form lead",
        instruction: "Create a standalone Sales/Leads record from an inbound landing page form submission. Attach campaign tag, landing page variant, ad source, form answers, UTM data, and next sales activity step. Prior pipeline records are optional.",
        payload: {
          source: "inbound_campaign_form",
          campaignId: campaign.id,
          campaignTags: [campaign.tag],
          company: "Inbound demo account",
          contact: "Demo Lead",
          email: "demo.lead@example.com",
          landingPage: campaign.landingPath,
          adSource: "LinkedIn preview",
          formAnswers: {
            urgency: "Rollout in 30 Tagen",
            message: "Wir wollen Anzeigen-Leads direkt in der Pipeline qualifizieren."
          },
          nextStep: "Lead qualifizieren, Formularantworten pruefen und erste Sales-Aktivitaet planen."
        }
      });
      button.textContent = ok ? messages.lead : messages.failed;
      if (status) status.textContent = ok ? messages.lead : messages.failed;
    }
  });
})();`;
}

function campaignImporterScript(locale: SupportedLocale, query: QueryState) {
  const messages = locale === "de"
    ? { campaignCreated: "Kampagne wurde in CTOX angelegt.", importStarted: "Import wurde in CTOX gestartet.", failed: "Aktion fehlgeschlagen." }
    : { campaignCreated: "Campaign created in CTOX.", importStarted: "Import started in CTOX.", failed: "Action failed." };
  return `(() => {
  const messages = ${JSON.stringify(messages)};
  const basePayload = ${JSON.stringify({ locale: query.locale, theme: query.theme })};
  const postCampaign = async (body) => {
    const response = await fetch("/api/sales/campaigns", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...basePayload, ...body })
    });
    return response.ok;
  };
  const openDialog = (selector) => {
    const dialog = document.querySelector(selector);
    if (dialog instanceof HTMLDialogElement && !dialog.open) dialog.showModal();
  };
  const closeDialog = (selector) => {
    const dialog = document.querySelector(selector);
    if (dialog instanceof HTMLDialogElement && dialog.open) dialog.close();
  };
  document.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const row = target.closest("[data-campaign-name]");
    if (target.closest("[data-campaign-import-open]")) openDialog("[data-campaign-import-dialog]");
    if (target.closest("[data-campaign-import-close]")) closeDialog("[data-campaign-import-dialog]");
    if (target.closest("[data-campaign-details-open]")) {
      const title = document.querySelector("[data-campaign-sheet-title]");
      if (title && row instanceof HTMLElement) title.textContent = row.dataset.campaignName || title.textContent;
      openDialog("[data-campaign-details-dialog]");
    }
    if (target.closest("[data-campaign-details-close]")) closeDialog("[data-campaign-details-dialog]");
  });
  document.addEventListener("submit", async (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    if (form.matches("[data-campaign-source-form]")) {
      event.preventDefault();
      const status = form.querySelector("[data-campaign-form-status]");
      const data = new FormData(form);
      const file = data.get("sourceFile");
      const sourceType = String(data.get("sourceType") || "Excel");
      const sourceUrl = String(data.get("sourceUrl") || "");
      const sourceHint = String(data.get("sourceHint") || "");
      const ok = await postCampaign({
        action: "create",
        recordId: "source-import-" + crypto.randomUUID(),
        title: "Import Sales campaign source",
        instruction: "Import this source into the Sales campaigns module, parse records, normalize columns, preserve source evidence, and start automatic research enrichment.",
        payload: {
          sourceType,
          sourceUrl,
          sourceHint,
          fileName: file instanceof File && file.name ? file.name : "",
          fileType: file instanceof File && file.type ? file.type : "",
          fileSize: file instanceof File && file.size ? file.size : 0
        }
      });
      if (status) status.textContent = ok ? messages.importStarted : messages.failed;
    }
    if (form.matches("[data-campaign-rule-form]")) {
      event.preventDefault();
      const status = form.querySelector("[data-campaign-rule-status]");
      const data = new FormData(form);
      const campaignName = String(data.get("campaignName") || "").trim() || "Prompt-defined campaign";
      const assignmentPrompt = String(data.get("assignmentPrompt") || "").trim();
      const ok = await postCampaign({
        action: "create",
        recordId: "campaign-" + campaignName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""),
        title: "Create Sales campaign: " + campaignName,
        instruction: "Create a Sales campaign from the provided assignment prompt. Use the prompt as the routing policy for imported and enriched records, then propose record assignments with confidence and evidence.",
        payload: { campaignName, assignmentPrompt }
      });
      if (status) status.textContent = ok ? messages.campaignCreated : messages.failed;
    }
  });
})();`;
}

function campaignOutreachScript(locale: SupportedLocale, query: QueryState) {
  const messages = locale === "de"
    ? { working: "Wird gestartet ...", sent: "Versand wurde gestartet.", lead: "Antwort-Handoff wurde in Leads angelegt.", failed: "Aktion fehlgeschlagen." }
    : { working: "Working ...", sent: "Send started.", lead: "Reply handoff created in Leads.", failed: "Action failed." };
  return `(() => {
  const messages = ${JSON.stringify(messages)};
  const basePayload = ${JSON.stringify({ locale: query.locale, theme: query.theme })};
  const postJson = async (url, body) => {
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...basePayload, ...body })
    });
    return response.ok;
  };
  const rowPayload = (row) => ({
    outreachId: row.dataset.outreachId,
    campaignId: row.dataset.campaignId,
    company: row.dataset.company,
    person: row.dataset.person,
    email: row.dataset.email,
    tags: String(row.dataset.tags || "").split(",").map((tag) => tag.trim()).filter(Boolean),
    message: row.querySelector(".campaign-message-cell textarea")?.value || "",
    note: row.querySelector("textarea[name^='note-']")?.value || ""
  });
  document.addEventListener("click", async (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const single = target.closest("[data-campaign-send]");
    if (single) {
      const row = single.closest("[data-outreach-id]");
      if (!row) return;
      single.textContent = messages.working;
      const ok = await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: row.dataset.outreachId,
        title: "Send campaign outreach: " + (row.dataset.company || row.dataset.outreachId),
        instruction: "Start an individual campaign email send. Preserve the prepared message, touchpoint analysis, follow-up drafts, status, and campaign tags. Do not send without the configured sender approval policy.",
        payload: rowPayload(row)
      });
      single.textContent = ok ? messages.sent : messages.failed;
    }
    if (target.closest("[data-campaign-batch-send]")) {
      const button = target.closest("[data-campaign-batch-send]");
      button.textContent = messages.working;
      const rows = Array.from(document.querySelectorAll("[data-outreach-id]")).map(rowPayload);
      const ok = await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: "campaign-batch-send",
        title: "Start campaign batch send",
        instruction: "Start a batch send for prepared campaign outreach rows. Keep every email individually reviewable, track follow-up sequence, and preserve campaign tags.",
        payload: { rows }
      });
      button.textContent = ok ? messages.sent : messages.failed;
    }
    const handoff = target.closest("[data-campaign-reply-handoff]");
    if (handoff) {
      const row = handoff.closest("[data-outreach-id]");
      if (!row) return;
      handoff.textContent = messages.working;
      const payload = rowPayload(row);
      await postJson("/api/sales/campaigns", {
        action: "sync",
        recordId: payload.outreachId,
        title: "Campaign reply detected: " + payload.company,
        instruction: "Mark this campaign outreach row as replied and stop further follow-ups.",
        payload: { ...payload, replyDetected: true }
      });
      const ok = await postJson("/api/sales/accounts", {
        action: "create",
        recordId: "reply-" + (payload.outreachId || crypto.randomUUID()),
        title: "Create Sales lead from campaign reply: " + payload.company,
        instruction: "Create a standalone Sales/Leads record because this campaign recipient replied. Add the campaign tag to the lead, preserve source campaign, touchpoints, prepared messages, contact, and next-step context. Prior pipeline records are optional.",
        payload: {
          source: "campaign_reply",
          campaignId: payload.campaignId,
          campaignTags: payload.tags,
          company: payload.company,
          contact: payload.person,
          email: payload.email,
          nextStep: "Review reply, qualify intent, and start the Sales activity path.",
          originalOutreach: payload
        }
      });
      handoff.textContent = ok ? messages.lead : messages.failed;
    }
  });
})();`;
}
