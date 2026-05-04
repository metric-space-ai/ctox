import { normalizeCompanyName } from "./company-settings";

export type BootstrapMode = "empty" | "demo" | "guided";
export type BootstrapLocale = "en" | "de";

export type BootstrapDemoInput = {
  companyName?: string | null;
  mission?: string | null;
  vision?: string | null;
  locale?: string | null;
  mode?: string | null;
};

type Localized = Record<BootstrapLocale, string>;

type ResourceRecord = {
  id: string;
  label: string;
  status: string;
  ownerId?: string | null;
  payload: unknown;
};

type BootstrapPlanItem = {
  module: "settings" | "marketing" | "sales" | "operations" | "business" | "ctox";
  resource: string;
  count: number;
  action: string;
};

export type BootstrapDemoResult = {
  ok: boolean;
  mode: BootstrapMode;
  companyName: string;
  slug: string;
  wrote: boolean;
  plan: BootstrapPlanItem[];
};

export function normalizeBootstrapInput(input: BootstrapDemoInput) {
  const companyName = normalizeCompanyName(input.companyName);
  const locale: BootstrapLocale = input.locale === "de" ? "de" : "en";
  const mode: BootstrapMode = input.mode === "empty" || input.mode === "guided" ? input.mode : "demo";
  const mission = normalizeLongText(input.mission) || defaultMission(companyName);
  const vision = normalizeLongText(input.vision) || defaultVision(companyName);
  const slug = slugify(companyName);

  return { companyName, locale, mission, mode, slug, vision };
}

export async function bootstrapDemoTenant(input: BootstrapDemoInput): Promise<BootstrapDemoResult> {
  if (!shouldUsePostgres()) {
    throw new Error("DATABASE_URL is required to bootstrap a Business OS tenant.");
  }

  const normalized = normalizeBootstrapInput(input);
  const plan = buildBootstrapPlan(normalized.mode);

  if (normalized.mode === "guided") {
    return {
      ok: true,
      mode: normalized.mode,
      companyName: normalized.companyName,
      slug: normalized.slug,
      wrote: false,
      plan
    };
  }

  const db = await import("@ctox-business/db/modules");
  await db.upsertOrganization({ name: normalized.companyName, slug: normalized.slug });

  if (normalized.mode === "demo") {
    await Promise.all([
      db.seedModuleRecords("marketing", marketingBootstrapRecords(normalized)),
      db.seedModuleRecords("sales", salesBootstrapRecords(normalized)),
      db.seedModuleRecords("business", businessBootstrapRecords(normalized)),
      seedOperations(normalized),
      seedCtox(normalized)
    ]);
  }

  return {
    ok: true,
    mode: normalized.mode,
    companyName: normalized.companyName,
    slug: normalized.slug,
    wrote: true,
    plan
  };
}

function buildBootstrapPlan(mode: BootstrapMode): BootstrapPlanItem[] {
  const settingsPlan = [{ module: "settings" as const, resource: "organization", count: 1, action: "upsert tenant settings" }];
  if (mode === "empty") return settingsPlan;

  return [
    ...settingsPlan,
    { module: "marketing", resource: "website", count: 3, action: "create mission-shaped page briefs" },
    { module: "marketing", resource: "campaigns", count: 2, action: "create initial inbound and outbound campaign shells" },
    { module: "marketing", resource: "research", count: 2, action: "create market and persona research work" },
    { module: "sales", resource: "campaigns", count: 2, action: "create contact-source and inbound lead campaigns" },
    { module: "sales", resource: "leads", count: 3, action: "create qualified placeholder leads" },
    { module: "sales", resource: "offers", count: 1, action: "create a draft offer handoff" },
    { module: "operations", resource: "projects", count: 3, action: "create onboarding, launch, and operating cadence projects" },
    { module: "operations", resource: "workItems", count: 6, action: "create first work items and knowledge hooks" },
    { module: "business", resource: "products", count: 3, action: "create billable products and services" },
    { module: "business", resource: "invoices", count: 1, action: "create one draft invoice example" },
    { module: "ctox", resource: "bugs", count: 1, action: "create one synthetic setup follow-up report" }
  ];
}

function marketingBootstrapRecords(input: ReturnType<typeof normalizeBootstrapInput>): Record<string, ResourceRecord[]> {
  const prefix = `bootstrap-${input.slug}`;
  return {
    assets: [
      record(`${prefix}-asset-positioning`, `${input.companyName} positioning one-pager`, "draft", "marketing-lead", {
        id: `${prefix}-asset-positioning`,
        name: `${input.companyName} positioning one-pager`,
        kind: "one-pager",
        status: "draft",
        ownerId: "marketing-lead",
        updated: today(),
        audience: localized("Decision makers evaluating the mission.", "Entscheider, die die Mission bewerten."),
        usage: localized("Attach to first discovery and campaign follow-up.", "An Erstgespraeche und Kampagnen-Follow-ups anhaengen."),
        mission: input.mission,
        vision: input.vision
      })
    ],
    campaigns: [
      record(`${prefix}-campaign-inbound`, `${input.companyName} inbound validation`, "planned", "marketing-lead", {
        id: `${prefix}-campaign-inbound`,
        name: `${input.companyName} inbound validation`,
        channel: "search",
        status: "planned",
        ownerId: "marketing-lead",
        launch: "2026-05-15",
        target: localized(
          `Prospects searching for a solution aligned with: ${input.mission}`,
          `Interessenten, die eine Loesung passend zur Mission suchen: ${input.mission}`
        ),
        nextAction: localized("Draft landing page variant and contact form.", "Landing-Page-Variante und Kontaktformular entwerfen.")
      }),
      record(`${prefix}-campaign-outbound`, `${input.companyName} outbound discovery`, "planned", "sales-lead", {
        id: `${prefix}-campaign-outbound`,
        name: `${input.companyName} outbound discovery`,
        channel: "email",
        status: "planned",
        ownerId: "sales-lead",
        launch: "2026-05-18",
        target: localized(
          "Accounts with an urgent trigger and a reachable decision process.",
          "Accounts mit akutem Trigger und erreichbarem Entscheidungsprozess."
        ),
        nextAction: localized("Import first source list and run enrichment.", "Erste Quellenliste importieren und Enrichment starten.")
      })
    ],
    commerce: [
      record(`${prefix}-commerce-starter`, `${input.companyName} starter engagement`, "draft", "business-lead", {
        id: `${prefix}-commerce-starter`,
        name: `${input.companyName} starter engagement`,
        kind: "service",
        status: "draft",
        price: "TBD",
        ownerId: "business-lead",
        nextAction: localized("Turn mission into scoped pilot offer.", "Mission in ein abgegrenztes Pilotangebot ueberfuehren.")
      })
    ],
    research: [
      record(`${prefix}-research-persona`, `${input.companyName} buyer hypothesis`, "queued", "marketing-lead", {
        id: `${prefix}-research-persona`,
        title: `${input.companyName} buyer hypothesis`,
        kind: "persona",
        status: "queued",
        ownerId: "marketing-lead",
        updated: today(),
        insight: localized(
          `CTOX should test who is accountable for achieving: ${input.vision}`,
          `CTOX soll testen, wer verantwortlich ist fuer die Vision: ${input.vision}`
        ),
        linkedCampaignIds: [`${prefix}-campaign-inbound`, `${prefix}-campaign-outbound`]
      }),
      record(`${prefix}-research-market`, `${input.companyName} market trigger map`, "queued", "ctox-agent", {
        id: `${prefix}-research-market`,
        title: `${input.companyName} market trigger map`,
        kind: "market-note",
        status: "queued",
        ownerId: "ctox-agent",
        updated: today(),
        insight: localized("Collect market events that make the mission urgent.", "Marktereignisse sammeln, die die Mission dringlich machen."),
        linkedCampaignIds: [`${prefix}-campaign-outbound`]
      })
    ],
    website: [
      record(`${prefix}-page-home`, "Home", "draft", "marketing-lead", {
        id: `${prefix}-page-home`,
        title: "Home",
        path: "/",
        status: "draft",
        ownerId: "marketing-lead",
        updated: today(),
        intent: localized(input.mission, input.mission),
        nextAction: localized("Create first viewport from mission and proof points.", "Ersten Viewport aus Mission und Proof Points erstellen.")
      }),
      record(`${prefix}-page-solution`, "Solution", "draft", "marketing-lead", {
        id: `${prefix}-page-solution`,
        title: "Solution",
        path: "/solution",
        status: "draft",
        ownerId: "marketing-lead",
        updated: today(),
        intent: localized(input.vision, input.vision),
        nextAction: localized("Connect website claim to sales discovery questions.", "Website-Claim mit Sales-Discovery-Fragen verbinden.")
      }),
      record(`${prefix}-page-contact`, "Contact", "draft", "sales-lead", {
        id: `${prefix}-page-contact`,
        title: "Contact",
        path: "/contact",
        status: "draft",
        ownerId: "sales-lead",
        updated: today(),
        intent: localized("Capture inbound leads for the Sales pipeline.", "Inbound Leads fuer die Sales Pipeline erfassen."),
        nextAction: localized("Add form fields and routing rules.", "Formularfelder und Routing-Regeln ergaenzen.")
      })
    ]
  };
}

function salesBootstrapRecords(input: ReturnType<typeof normalizeBootstrapInput>): Record<string, ResourceRecord[]> {
  const prefix = `bootstrap-${input.slug}`;
  const accountId = `${prefix}-account`;
  const contactId = `${prefix}-contact`;
  const opportunityId = `${prefix}-opportunity`;
  const offerId = `${prefix}-offer`;

  return {
    accounts: [
      record(accountId, `${input.companyName} design account`, "Amber", "sales-lead", {
        id: accountId,
        name: `${input.companyName} design account`,
        segment: "Mission-aligned prospect",
        region: "DACH",
        ownerId: "sales-lead",
        health: "Amber",
        annualValue: 36000,
        renewalDate: "2026-12-31",
        summary: localized("Synthetic account for testing the sales workflow.", "Synthetischer Account zum Testen des Sales Workflows."),
        nextStep: localized("Validate buying trigger and decision owner.", "Buying Trigger und Decision Owner validieren.")
      })
    ],
    campaigns: [
      record(`${prefix}-sales-campaign-source`, `${input.companyName} source import`, "Research", "ctox-agent", {
        id: `${prefix}-sales-campaign-source`,
        name: `${input.companyName} source import`,
        status: "Research",
        sourceTypes: ["Excel", "URL", "PDF", "Text"],
        importedRecords: 24,
        enrichedRecords: 0,
        assignedRecords: 0,
        ownerId: "ctox-agent",
        assignmentPrompt: localized(
          "Assign records by ICP fit, urgency, buying trigger, and reachable contact path.",
          "Datensaetze nach ICP Fit, Dringlichkeit, Buying Trigger und erreichbarem Kontaktpfad zuordnen."
        ),
        nextStep: localized("Run company and contact research.", "Firmen- und Ansprechpartner-Recherche starten.")
      }),
      record(`${prefix}-sales-campaign-inbound`, `${input.companyName} inbound handoff`, "Draft", "sales-lead", {
        id: `${prefix}-sales-campaign-inbound`,
        name: `${input.companyName} inbound handoff`,
        status: "Draft",
        sourceTypes: ["URL"],
        importedRecords: 0,
        enrichedRecords: 0,
        assignedRecords: 0,
        ownerId: "sales-lead",
        assignmentPrompt: localized("Route form replies into Contact, Pre-Qualified, Qualified, or Lead.", "Formularantworten in Contact, Pre-Qualified, Qualified oder Lead routen."),
        nextStep: localized("Connect contact form and reply detection.", "Kontaktformular und Reply-Erkennung verbinden.")
      })
    ],
    contacts: [
      record(contactId, "Demo Decision Owner", "Decision maker", null, {
        id: contactId,
        accountId,
        name: "Demo Decision Owner",
        role: "Decision owner",
        email: "decision.owner@example.com",
        phone: "+49 30 000000",
        relationship: "Decision maker",
        lastTouch: "2026-05-04",
        nextStep: localized("Ask for current priority and preferred next step.", "Nach aktueller Prioritaet und gewuenschtem naechsten Schritt fragen.")
      })
    ],
    customers: [
      record(`${prefix}-customer`, `${input.companyName} future customer`, "Not started", "customer-success", {
        id: `${prefix}-customer`,
        name: `${input.companyName} future customer`,
        contactName: "Demo Decision Owner",
        email: "decision.owner@example.com",
        segment: "Bootstrap placeholder",
        ownerId: "customer-success",
        source: "Accepted offer",
        offerId,
        onboardingStatus: "Not started",
        summary: localized("Placeholder customer created to test the optional direct-start path.", "Platzhalterkunde zum Testen des optionalen Direkteinstiegs."),
        nextStep: localized("Create onboarding project only after offer acceptance.", "Onboarding-Projekt erst nach Angebotsannahme anlegen.")
      })
    ],
    leads: [
      record(`${prefix}-lead-inbound`, `${input.companyName} inbound lead`, "Qualified", "sales-lead", {
        id: `${prefix}-lead-inbound`,
        company: `${input.companyName} inbound lead`,
        contactName: "Demo Decision Owner",
        title: "Mission-fit inquiry",
        email: "decision.owner@example.com",
        source: "Website",
        score: 74,
        status: "Qualified",
        ownerId: "sales-lead",
        createdAt: "2026-05-04",
        nextStep: localized("Plan first meeting and prepare discovery map.", "Erstes Meeting planen und Discovery Map vorbereiten.")
      }),
      record(`${prefix}-lead-outbound`, `${input.companyName} outbound reply`, "Research", "ctox-agent", {
        id: `${prefix}-lead-outbound`,
        company: `${input.companyName} outbound reply`,
        contactName: "Research Contact",
        title: "Trigger-based response",
        email: "research.contact@example.com",
        source: "Outbound",
        score: 61,
        status: "Research",
        ownerId: "ctox-agent",
        createdAt: "2026-05-04",
        nextStep: localized("Research buying center before meeting proposal.", "Buying Center vor Terminvorschlag recherchieren.")
      }),
      record(`${prefix}-lead-partner`, `${input.companyName} partner referral`, "New", "owner", {
        id: `${prefix}-lead-partner`,
        company: `${input.companyName} partner referral`,
        contactName: "Partner Contact",
        title: "Referral from partner ecosystem",
        email: "partner.contact@example.com",
        source: "Partner",
        score: 55,
        status: "New",
        ownerId: "owner",
        createdAt: "2026-05-04",
        nextStep: localized("Clarify referral context and urgency.", "Referral-Kontext und Dringlichkeit klaeren.")
      })
    ],
    offers: [
      record(offerId, `${input.companyName} pilot offer`, "Draft", "sales-lead", {
        id: offerId,
        opportunityId,
        accountId,
        contactId,
        number: "AG-BS-001",
        title: `${input.companyName} pilot offer`,
        status: "Draft",
        issuedAt: "2026-05-04",
        validUntil: "2026-05-25",
        currency: "EUR",
        netAmount: 12000,
        taxAmount: 2280,
        grossAmount: 14280,
        probabilityImpact: 40,
        paymentTerms: localized("50% at start, 50% after pilot acceptance.", "50% zum Start, 50% nach Pilotabnahme."),
        deliveryScope: localized("Pilot scope derived from mission and first success metric.", "Pilotumfang aus Mission und erstem Erfolgskriterium."),
        introText: localized(`This draft offer translates ${input.companyName}'s mission into a first controlled pilot.`, `Dieser Angebotsentwurf uebersetzt die Mission von ${input.companyName} in einen ersten kontrollierten Pilot.`),
        closingText: localized("Next step: approve scope and schedule kickoff.", "Naechster Schritt: Scope freigeben und Kickoff terminieren."),
        lineItems: [
          { description: "Business OS setup", quantity: 1, unit: "Piece", unitPrice: 7200, taxRate: 19, discount: 0 },
          { description: "Mission-to-module bootstrap", quantity: 3, unit: "Day", unitPrice: 1600, taxRate: 19, discount: 0 }
        ],
        nextStep: localized("Review commercial assumptions with decision owner.", "Kommerzielle Annahmen mit Decision Owner pruefen.")
      })
    ],
    opportunities: [
      record(opportunityId, `${input.companyName} mission pilot`, "Qualified", "sales-lead", {
        id: opportunityId,
        accountId,
        contactId,
        name: `${input.companyName} mission pilot`,
        stage: "Qualified",
        value: 12000,
        probability: 45,
        closeDate: "2026-06-14",
        ownerId: "sales-lead",
        source: "Business OS bootstrap",
        nextStep: localized("Move to offer once success metric is confirmed.", "In Angebot ueberfuehren, sobald Erfolgskriterium bestaetigt ist."),
        risks: [
          localized("Decision process still synthetic until real discovery.", "Entscheidungsprozess bleibt synthetisch bis zur echten Discovery.")
        ]
      })
    ],
    tasks: [
      record(`${prefix}-task-discovery`, "Prepare first discovery map", "Open", "sales-lead", {
        id: `${prefix}-task-discovery`,
        subject: "Prepare first discovery map",
        ownerId: "sales-lead",
        due: "2026-05-06T10:00:00.000Z",
        priority: "High",
        status: "Open",
        linkedResource: "lead",
        linkedRecordId: `${prefix}-lead-inbound`,
        nextStep: localized("Use mission and vision as discovery anchors.", "Mission und Vision als Discovery-Anker nutzen.")
      })
    ]
  };
}

function businessBootstrapRecords(input: ReturnType<typeof normalizeBootstrapInput>): Record<string, ResourceRecord[]> {
  const prefix = `bootstrap-${input.slug}`;
  const customerId = `${prefix}-business-customer`;
  const productId = `${prefix}-product-core`;
  const setupId = `${prefix}-product-setup`;
  const invoiceId = `${prefix}-invoice-draft`;

  return {
    bookkeeping: [
      record(`${prefix}-export-open`, "Bootstrap export review", "Needs review", "finance-lead", {
        id: `${prefix}-export-open`,
        period: "May 2026",
        system: "DATEV",
        status: "Needs review",
        invoiceIds: [invoiceId],
        netAmount: 9600,
        taxAmount: 1824,
        generatedAt: today(),
        dueDate: "2026-05-30",
        reviewer: "finance-lead",
        context: localized("Review account mapping before first real export.", "Kontenmapping vor erstem echten Export pruefen.")
      })
    ],
    customers: [
      record(customerId, `${input.companyName} bootstrap customer`, "Review", "finance-lead", {
        id: customerId,
        name: `${input.companyName} bootstrap customer`,
        segment: "Internal bootstrap",
        owner: "finance-lead",
        taxId: "TBD",
        billingEmail: "billing@example.com",
        paymentTerms: "14 days",
        status: "Review",
        country: "DE",
        mrr: 0,
        arBalance: 0,
        lastInvoiceId: invoiceId,
        notes: localized("Replace this synthetic record with real customer master data.", "Diesen synthetischen Datensatz durch echte Kundenstammdaten ersetzen.")
      })
    ],
    invoices: [
      record(invoiceId, "RE-BS-001", "Draft", customerId, {
        id: invoiceId,
        number: "RE-BS-001",
        customerId,
        customerNumber: "10000",
        addressLines: [`${input.companyName} bootstrap customer`, "Finance", "Example Street 1", "10115 Berlin", "Deutschland"],
        documentTitle: "Rechnung",
        issueDate: "2026-05-04",
        dueDate: "2026-05-18",
        serviceDate: "2026-05-04",
        status: "Draft",
        currency: "EUR",
        lines: [
          { productId, quantity: 1, unitPrice: 4800, taxRate: 19 },
          { productId: setupId, quantity: 1, unitPrice: 4800, taxRate: 19 }
        ],
        netAmount: 9600,
        taxAmount: 1824,
        total: 11424,
        balanceDue: 11424,
        collectionStatus: "Clear",
        reminderLevel: 0,
        deliveryChannel: "Draft",
        paymentMethods: ["Bank transfer"],
        payments: [],
        tags: ["bootstrap-demo"],
        notes: localized("Draft invoice for testing the invoice editor and export flow.", "Rechnungsentwurf zum Testen von Editor und Exportfluss.")
      })
    ],
    products: [
      record(productId, `${input.companyName} Core Offering`, "Draft", "business-lead", {
        id: productId,
        sku: "BOOTSTRAP-CORE",
        name: `${input.companyName} Core Offering`,
        type: "Subscription",
        price: 4800,
        taxRate: 19,
        revenueAccount: "8400 SaaS subscriptions",
        status: "Draft",
        margin: 70,
        description: localized(input.mission, input.mission)
      }),
      record(setupId, "Business OS setup", "Draft", "business-lead", {
        id: setupId,
        sku: "BOOTSTRAP-SETUP",
        name: "Business OS setup",
        type: "Service",
        price: 4800,
        taxRate: 19,
        revenueAccount: "8337 Implementation services",
        status: "Draft",
        margin: 62,
        description: localized("Initial module setup from mission and vision.", "Initiales Modulsetup aus Mission und Vision.")
      }),
      record(`${prefix}-product-research`, "Research and campaign automation", "Review", "business-lead", {
        id: `${prefix}-product-research`,
        sku: "BOOTSTRAP-RESEARCH",
        name: "Research and campaign automation",
        type: "Service",
        price: 1800,
        taxRate: 19,
        revenueAccount: "8338 Research services",
        status: "Review",
        margin: 58,
        description: localized("Automated research, touchpoint analysis, and campaign preparation.", "Automatisierte Recherche, Touchpoint-Analyse und Kampagnenvorbereitung.")
      })
    ],
    reports: [
      record(`${prefix}-report-readiness`, "Bootstrap readiness report", "Draft", "finance-lead", {
        id: `${prefix}-report-readiness`,
        title: "Bootstrap readiness report",
        period: "May 2026",
        status: "Draft",
        amount: 9600,
        dueDate: "2026-05-12",
        taxContext: "Synthetic records only",
        exportContext: "Not export ready until real master data is confirmed",
        summary: localized("Shows whether all modules have starter data and handoff paths.", "Zeigt, ob alle Module Startdaten und Uebergabepfade haben."),
        linkedExportIds: [`${prefix}-export-open`]
      })
    ]
  };
}

async function seedOperations(input: ReturnType<typeof normalizeBootstrapInput>) {
  const db = await import("@ctox-business/db/operations");
  const prefix = `bootstrap-${input.slug}`;

  await db.seedOperationsData({
    actionItems: [
      {
        id: `${prefix}-action-approve-mission`,
        due: "2026-05-07",
        ownerId: "owner",
        text: localized("Approve mission-to-module assumptions.", "Annahmen von Mission zu Modulen freigeben."),
        workItemId: `${prefix}-work-mission-map`
      }
    ],
    decisions: [
      {
        id: `${prefix}-decision-demo-data`,
        meetingId: `${prefix}-meeting-kickoff`,
        projectId: `${prefix}-project-setup`,
        text: localized("Bootstrap data is synthetic and may be replaced module by module.", "Bootstrap-Daten sind synthetisch und koennen modulweise ersetzt werden."),
        linkedWorkItemIds: [`${prefix}-work-mission-map`]
      }
    ],
    knowledgeItems: [
      {
        id: `${prefix}-knowledge-operating-model`,
        kind: "Runbook",
        linkedItems: [`${prefix}-work-mission-map`, `${prefix}-work-website`],
        ownerId: "operations-lead",
        projectId: `${prefix}-project-setup`,
        sections: [
          {
            title: localized("Mission", "Mission"),
            body: localized(input.mission, input.mission)
          },
          {
            title: localized("Vision", "Vision"),
            body: localized(input.vision, input.vision)
          }
        ],
        title: `${input.companyName} operating model`,
        updated: today()
      }
    ],
    meetings: [
      {
        id: `${prefix}-meeting-kickoff`,
        actionItems: [`${prefix}-action-approve-mission`],
        agenda: [
          localized("Confirm mission and first module priorities.", "Mission und erste Modulprioritaeten bestaetigen."),
          localized("Decide which placeholders become real records first.", "Entscheiden, welche Platzhalter zuerst echte Datensaetze werden.")
        ],
        date: "2026-05-06T09:30:00.000Z",
        decisions: [`${prefix}-decision-demo-data`],
        facilitatorId: "operations-lead",
        projectId: `${prefix}-project-setup`,
        title: "Business OS bootstrap kickoff"
      }
    ],
    milestones: [
      {
        id: `${prefix}-milestone-initialized`,
        date: "2026-05-08",
        projectId: `${prefix}-project-setup`,
        status: "Upcoming",
        title: "Business OS initialized"
      }
    ],
    projects: [
      {
        code: "BOS-001",
        end: "2026-05-15",
        health: "Green",
        id: `${prefix}-project-setup`,
        linkedModules: ["ctox", "business", "marketing", "sales"],
        name: `${input.companyName} Business OS setup`,
        nextMilestone: "Business OS initialized",
        ownerId: "operations-lead",
        progress: 20,
        start: "2026-05-04",
        summary: localized("Install the tenant, confirm mission/vision, and replace synthetic placeholders with real records.", "Tenant installieren, Mission/Vision bestaetigen und synthetische Platzhalter durch echte Datensaetze ersetzen.")
      },
      {
        code: "MKT-001",
        end: "2026-05-22",
        health: "Amber",
        id: `${prefix}-project-website`,
        linkedModules: ["marketing", "sales"],
        name: `${input.companyName} website and inbound launch`,
        nextMilestone: "Landing page ready",
        ownerId: "marketing-lead",
        progress: 10,
        start: "2026-05-06",
        summary: localized("Create public website content, inbound form routing, and campaign preview.", "Public Website Content, Inbound-Formularrouting und Kampagnenpreview erstellen.")
      },
      {
        code: "SAL-001",
        end: "2026-05-29",
        health: "Amber",
        id: `${prefix}-project-sales-motion`,
        linkedModules: ["sales", "ctox"],
        name: `${input.companyName} first sales motion`,
        nextMilestone: "First qualified lead",
        ownerId: "sales-lead",
        progress: 15,
        start: "2026-05-06",
        summary: localized("Turn research and campaign replies into qualified leads and offer-ready records.", "Recherche und Kampagnenantworten in qualifizierte Leads und angebotsreife Datensaetze ueberfuehren.")
      }
    ],
    workItems: [
      workItem(`${prefix}-work-mission-map`, `${prefix}-project-setup`, "Map mission to module priorities", "Task", "Ready", "High", "owner", "2026-05-07", localized("Turn mission and vision into module-level success criteria.", "Mission und Vision in Erfolgskriterien je Modul uebersetzen."), [`${prefix}-knowledge-operating-model`]),
      workItem(`${prefix}-work-website`, `${prefix}-project-website`, "Draft homepage and contact form", "Feature", "Backlog", "High", "marketing-lead", "2026-05-10", localized("Prepare first public content and inbound lead capture.", "Ersten Public Content und Inbound Lead Capture vorbereiten."), [`${prefix}-knowledge-operating-model`]),
      workItem(`${prefix}-work-campaign-import`, `${prefix}-project-sales-motion`, "Import first contact source", "Task", "Backlog", "Normal", "ctox-agent", "2026-05-11", localized("Import a synthetic source list and define enrichment settings.", "Synthetische Quellenliste importieren und Enrichment Settings definieren."), []),
      workItem(`${prefix}-work-offer-template`, `${prefix}-project-sales-motion`, "Review offer template", "Document", "Backlog", "Normal", "sales-lead", "2026-05-14", localized("Align offer editor with the expected sales handoff.", "Angebotseditor mit erwarteter Sales-Uebergabe abgleichen."), []),
      workItem(`${prefix}-work-invoice-template`, `${prefix}-project-setup`, "Review invoice and export setup", "Task", "Backlog", "Normal", "finance-lead", "2026-05-15", localized("Confirm billing fields, tax assumptions, and export mapping.", "Billing-Felder, Steuerannahmen und Exportmapping bestaetigen."), []),
      workItem(`${prefix}-work-ctox-prompts`, `${prefix}-project-setup`, "Create first CTOX prompt patterns", "Task", "Backlog", "Normal", "ctox-agent", "2026-05-13", localized("Define safe right-click prompts for every module.", "Sichere Rechtsklick-Prompts fuer jedes Modul definieren."), [])
    ]
  });
}

async function seedCtox(input: ReturnType<typeof normalizeBootstrapInput>) {
  const db = await import("@ctox-business/db/modules");
  await db.upsertCtoxBugReport({
    id: `bootstrap-${input.slug}-setup-follow-up`,
    title: "Bootstrap setup follow-up",
    moduleId: "ctox",
    submoduleId: "bugs",
    status: "triaged",
    severity: "normal",
    tags: ["Business OS Bug Report", "bootstrap-demo"],
    coreTaskId: null
  });
}

function workItem(
  id: string,
  projectId: string,
  subject: string,
  type: string,
  status: string,
  priority: string,
  assigneeId: string,
  due: string,
  description: Localized,
  linkedKnowledgeIds: string[]
) {
  return {
    assigneeId,
    description,
    due,
    estimate: 4,
    id,
    linkedKnowledgeIds,
    priority,
    projectId,
    status,
    subject,
    type
  };
}

function record(id: string, label: string, status: string, ownerId: string | null, payload: unknown): ResourceRecord {
  return { id, label, status, ownerId, payload };
}

function localized(en: string, de: string): Localized {
  return { en, de };
}

function normalizeLongText(value?: string | null) {
  return String(value ?? "")
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 1200);
}

function slugify(value: string) {
  return normalizeCompanyName(value)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48) || "tenant";
}

function today() {
  return new Date().toISOString().slice(0, 10);
}

function defaultMission(companyName: string) {
  return `${companyName} uses CTOX Business OS to turn scattered work into connected, accountable operating workflows.`;
}

function defaultVision(companyName: string) {
  return `${companyName} should become a company where every customer touchpoint, project, document, and decision can be understood and improved by CTOX.`;
}

function shouldUsePostgres() {
  const value = process.env.DATABASE_URL;
  return Boolean(value && !value.includes("user:password@localhost"));
}
