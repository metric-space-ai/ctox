export type SupportedLocale = "en" | "de";
export type Localized = Record<SupportedLocale, string>;

export type SalesOwner = {
  id: string;
  name: string;
  role: string;
};

export type SalesAccount = {
  id: string;
  name: string;
  segment: string;
  region: string;
  ownerId: string;
  health: "Green" | "Amber" | "Red";
  annualValue: number;
  renewalDate: string;
  summary: Localized;
  nextStep: Localized;
};

export type SalesContact = {
  id: string;
  accountId: string;
  name: string;
  role: string;
  email: string;
  phone: string;
  relationship: "Champion" | "Decision maker" | "Evaluator" | "Finance" | "User";
  lastTouch: string;
  nextStep: Localized;
};

export type SalesOpportunityStage = "Qualify" | "Discover" | "Proposal" | "Negotiation" | "Won";
export type SalesOfferStatus = "Draft" | "Sent" | "Accepted" | "Declined" | "Expired";

export type SalesOpportunity = {
  id: string;
  accountId: string;
  contactId: string;
  name: string;
  stage: SalesOpportunityStage;
  value: number;
  probability: number;
  closeDate: string;
  ownerId: string;
  source: string;
  nextStep: Localized;
  risks: Localized[];
};

export type SalesOfferLine = {
  description: string;
  quantity: number;
  unit: "Hour" | "Day" | "Piece" | "Month";
  unitPrice: number;
  taxRate: number;
  discount: number;
};

export type SalesOffer = {
  id: string;
  opportunityId: string;
  accountId: string;
  contactId: string;
  number: string;
  title: string;
  status: SalesOfferStatus;
  issuedAt: string;
  validUntil: string;
  currency: "EUR" | "USD";
  netAmount: number;
  taxAmount: number;
  grossAmount: number;
  probabilityImpact: number;
  paymentTerms: Localized;
  deliveryScope: Localized;
  introText: Localized;
  closingText: Localized;
  lineItems: SalesOfferLine[];
  nextStep: Localized;
};

export type SalesLead = {
  id: string;
  company: string;
  contactName: string;
  title: string;
  email: string;
  source: "Website" | "Referral" | "Outbound" | "Event" | "Partner";
  score: number;
  status: "New" | "Research" | "Qualified" | "Nurture";
  ownerId: string;
  createdAt: string;
  nextStep: Localized;
};

export type SalesTask = {
  id: string;
  subject: string;
  ownerId: string;
  due: string;
  priority: "Low" | "Normal" | "High" | "Urgent";
  status: "Open" | "In progress" | "Waiting" | "Done";
  linkedResource: "opportunity" | "account" | "contact" | "lead";
  linkedRecordId: string;
  nextStep: Localized;
};

export type SalesCampaign = {
  id: string;
  name: string;
  status: "Draft" | "Research" | "Ready" | "Active";
  sourceTypes: Array<"Excel" | "URL" | "PDF" | "Text">;
  importedRecords: number;
  enrichedRecords: number;
  assignedRecords: number;
  ownerId: string;
  assignmentPrompt: Localized;
  nextStep: Localized;
};

export type SalesCustomer = {
  id: string;
  name: string;
  contactName: string;
  email: string;
  segment: string;
  ownerId: string;
  source: "Direct" | "Accepted offer";
  offerId?: string;
  onboardingStatus: "Not started" | "Queued" | "In progress";
  summary: Localized;
  nextStep: Localized;
};

export type SalesBundle = {
  owners: SalesOwner[];
  accounts: SalesAccount[];
  campaigns: SalesCampaign[];
  contacts: SalesContact[];
  customers: SalesCustomer[];
  opportunities: SalesOpportunity[];
  offers: SalesOffer[];
  leads: SalesLead[];
  tasks: SalesTask[];
};

export const salesOwners: SalesOwner[] = [
  { id: "owner", name: "Owner", role: "Founder / closing" },
  { id: "sales-lead", name: "Sales Lead", role: "Pipeline owner" },
  { id: "ctox-agent", name: "CTOX Agent", role: "Research and follow-up automation" },
  { id: "customer-success", name: "Customer Success", role: "Handoff and onboarding" }
];

export const salesAccounts: SalesAccount[] = [
  {
    id: "northstar-labs",
    name: "Northstar Labs",
    segment: "B2B SaaS",
    region: "DACH",
    ownerId: "sales-lead",
    health: "Green",
    annualValue: 48000,
    renewalDate: "2026-11-18",
    summary: {
      en: "Early design partner for a CTOX-managed operating system across sales, delivery, and support.",
      de: "Frueher Design Partner fuer ein CTOX-gesteuertes Betriebssystem ueber Sales, Delivery und Support."
    },
    nextStep: {
      en: "Send final onboarding plan and confirm the pilot success gate.",
      de: "Finalen Onboarding-Plan senden und Pilot-Erfolgskriterium bestaetigen."
    }
  },
  {
    id: "brightfield-gmbh",
    name: "Brightfield GmbH",
    segment: "Professional services",
    region: "DACH",
    ownerId: "owner",
    health: "Amber",
    annualValue: 36000,
    renewalDate: "2026-09-30",
    summary: {
      en: "Consulting team evaluating CTOX for offer creation, project delivery, and recurring reporting.",
      de: "Beratungsteam evaluiert CTOX fuer Angebote, Projektumsetzung und wiederkehrendes Reporting."
    },
    nextStep: {
      en: "Resolve data hosting question and prepare a short security note.",
      de: "Hosting-Frage klaeren und kurze Security-Notiz vorbereiten."
    }
  },
  {
    id: "urbangrid-solutions",
    name: "UrbanGrid Solutions",
    segment: "Infrastructure",
    region: "EU",
    ownerId: "sales-lead",
    health: "Amber",
    annualValue: 72000,
    renewalDate: "2027-01-12",
    summary: {
      en: "Operations-heavy prospect with project management, ticket routing, and field-service knowledge needs.",
      de: "Operations-lastiger Prospect mit Projektmanagement, Ticket-Routing und Field-Service-Wissen."
    },
    nextStep: {
      en: "Map stakeholder workflow and identify first automation use case.",
      de: "Stakeholder-Workflow abbilden und ersten Automatisierungsfall bestimmen."
    }
  },
  {
    id: "atlas-retail",
    name: "Atlas Retail",
    segment: "Commerce",
    region: "UK",
    ownerId: "ctox-agent",
    health: "Red",
    annualValue: 24000,
    renewalDate: "2026-07-20",
    summary: {
      en: "Inbound commerce lead that needs clear ROI before moving beyond discovery.",
      de: "Inbound-Commerce-Lead braucht klare ROI-Story vor dem naechsten Schritt."
    },
    nextStep: {
      en: "Prepare commerce operations benchmark and ask CTOX to draft a tailored follow-up.",
      de: "Commerce-Operations-Benchmark vorbereiten und CTOX Follow-up entwerfen lassen."
    }
  }
];

export const salesContacts: SalesContact[] = [
  {
    id: "mira-northstar",
    accountId: "northstar-labs",
    name: "Mira Hoffmann",
    role: "COO",
    email: "mira@northstar.example",
    phone: "+49 30 0000001",
    relationship: "Decision maker",
    lastTouch: "2026-05-01",
    nextStep: {
      en: "Confirm pilot sponsor and reporting cadence.",
      de: "Pilot-Sponsor und Reporting-Takt bestaetigen."
    }
  },
  {
    id: "jonas-northstar",
    accountId: "northstar-labs",
    name: "Jonas Keller",
    role: "Head of Product",
    email: "jonas@northstar.example",
    phone: "+49 30 0000002",
    relationship: "Champion",
    lastTouch: "2026-04-30",
    nextStep: {
      en: "Review product workspace mock and collect objections.",
      de: "Produkt-Workspace-Mock pruefen und Einwaende sammeln."
    }
  },
  {
    id: "sven-brightfield",
    accountId: "brightfield-gmbh",
    name: "Sven Richter",
    role: "Managing Partner",
    email: "sven@brightfield.example",
    phone: "+49 40 0000003",
    relationship: "Decision maker",
    lastTouch: "2026-04-28",
    nextStep: {
      en: "Send security note and commercial outline.",
      de: "Security-Notiz und kommerziellen Rahmen senden."
    }
  },
  {
    id: "amina-urbangrid",
    accountId: "urbangrid-solutions",
    name: "Amina Rahman",
    role: "VP Operations",
    email: "amina@urbangrid.example",
    phone: "+31 20 0000004",
    relationship: "Evaluator",
    lastTouch: "2026-04-29",
    nextStep: {
      en: "Schedule process mapping call with field-service lead.",
      de: "Process-Mapping-Call mit Field-Service-Leitung planen."
    }
  },
  {
    id: "ellen-atlas",
    accountId: "atlas-retail",
    name: "Ellen Brooks",
    role: "Digital Operations",
    email: "ellen@atlas.example",
    phone: "+44 20 0000005",
    relationship: "User",
    lastTouch: "2026-04-24",
    nextStep: {
      en: "Send commerce example and ask for current tooling map.",
      de: "Commerce-Beispiel senden und aktuelle Tooling-Map anfragen."
    }
  }
];

export const salesOpportunities: SalesOpportunity[] = [
  {
    id: "opp-northstar-pilot",
    accountId: "northstar-labs",
    contactId: "mira-northstar",
    name: "CTOX Business OS Pilot",
    stage: "Negotiation",
    value: 48000,
    probability: 76,
    closeDate: "2026-05-17",
    ownerId: "sales-lead",
    source: "Design partner referral",
    nextStep: {
      en: "Send pilot order form and CTOX onboarding sequence.",
      de: "Pilot-Auftrag und CTOX-Onboarding-Sequenz senden."
    },
    risks: [
      {
        en: "Needs clear ownership between product and operations.",
        de: "Owner zwischen Product und Operations muss klar sein."
      }
    ]
  },
  {
    id: "opp-brightfield-rollout",
    accountId: "brightfield-gmbh",
    contactId: "sven-brightfield",
    name: "Consulting Delivery Rollout",
    stage: "Proposal",
    value: 36000,
    probability: 48,
    closeDate: "2026-05-29",
    ownerId: "owner",
    source: "Inbound website",
    nextStep: {
      en: "Attach security note and define self-hosting boundary.",
      de: "Security-Notiz anhaengen und Self-Hosting-Grenze definieren."
    },
    risks: [
      {
        en: "Budget holder wants fixed onboarding scope.",
        de: "Budgetverantwortlicher will festen Onboarding-Scope."
      }
    ]
  },
  {
    id: "opp-urbangrid-ops",
    accountId: "urbangrid-solutions",
    contactId: "amina-urbangrid",
    name: "Field Operations Workspace",
    stage: "Discover",
    value: 72000,
    probability: 34,
    closeDate: "2026-06-12",
    ownerId: "sales-lead",
    source: "Outbound account research",
    nextStep: {
      en: "Run discovery call focused on ticket flow and knowledge capture.",
      de: "Discovery Call zu Ticketfluss und Wissenssicherung durchfuehren."
    },
    risks: [
      {
        en: "Long stakeholder chain across operations and IT.",
        de: "Lange Stakeholder-Kette ueber Operations und IT."
      }
    ]
  },
  {
    id: "opp-atlas-commerce",
    accountId: "atlas-retail",
    contactId: "ellen-atlas",
    name: "Commerce Ops Benchmark",
    stage: "Qualify",
    value: 24000,
    probability: 22,
    closeDate: "2026-06-05",
    ownerId: "ctox-agent",
    source: "Website demo request",
    nextStep: {
      en: "Qualify urgency and connect benchmark output to measurable cost.",
      de: "Dringlichkeit qualifizieren und Benchmark-Ergebnis mit Kosten verbinden."
    },
    risks: [
      {
        en: "Use case may remain research-only without executive sponsor.",
        de: "Use Case bleibt ohne Sponsor eventuell reine Recherche."
      }
    ]
  },
  {
    id: "opp-northstar-expansion",
    accountId: "northstar-labs",
    contactId: "jonas-northstar",
    name: "Product Workspace Expansion",
    stage: "Won",
    value: 18000,
    probability: 100,
    closeDate: "2026-04-26",
    ownerId: "sales-lead",
    source: "Expansion",
    nextStep: {
      en: "Hand off requirements into Operations and Knowledge.",
      de: "Anforderungen an Operations und Knowledge uebergeben."
    },
    risks: []
  }
];

export const salesLeads: SalesLead[] = [
  {
    id: "lead-orbit-finance",
    company: "Orbit Finance",
    contactName: "Clara Stein",
    title: "Operations Director",
    email: "clara@orbit.example",
    source: "Referral",
    score: 86,
    status: "Qualified",
    ownerId: "sales-lead",
    createdAt: "2026-04-30",
    nextStep: {
      en: "Convert to account and propose finance-ready Business stack pilot.",
      de: "In Account umwandeln und Business-Stack-Pilot fuer Finance vorschlagen."
    }
  },
  {
    id: "lead-meshworks",
    company: "MeshWorks",
    contactName: "Adam Lee",
    title: "Founder",
    email: "adam@meshworks.example",
    source: "Website",
    score: 71,
    status: "Research",
    ownerId: "ctox-agent",
    createdAt: "2026-05-01",
    nextStep: {
      en: "Ask CTOX to research current stack and draft qualification questions.",
      de: "CTOX Stack recherchieren und Qualifizierungsfragen entwerfen lassen."
    }
  },
  {
    id: "lead-helio-care",
    company: "Helio Care",
    contactName: "Nina Park",
    title: "Head of Support",
    email: "nina@helio.example",
    source: "Event",
    score: 63,
    status: "New",
    ownerId: "sales-lead",
    createdAt: "2026-05-02",
    nextStep: {
      en: "Confirm support workflow pain and schedule short demo.",
      de: "Support-Workflow-Pain bestaetigen und kurze Demo planen."
    }
  },
  {
    id: "lead-fjord-ai",
    company: "Fjord AI",
    contactName: "Lars Holm",
    title: "Revenue Lead",
    email: "lars@fjord.example",
    source: "Outbound",
    score: 54,
    status: "Nurture",
    ownerId: "ctox-agent",
    createdAt: "2026-04-23",
    nextStep: {
      en: "Keep in nurture sequence until product operations trigger appears.",
      de: "In Nurture-Sequenz halten, bis Product-Ops-Trigger sichtbar wird."
    }
  }
];

export const salesCampaigns: SalesCampaign[] = [
  {
    id: "campaign-energy-market-import",
    name: "Energy market source import",
    status: "Research",
    sourceTypes: ["URL", "PDF", "Excel"],
    importedRecords: 184,
    enrichedRecords: 121,
    assignedRecords: 74,
    ownerId: "sales-lead",
    assignmentPrompt: {
      en: "Assign records to campaigns by ICP fit, product relevance, region, signal freshness, and buying trigger. Postal code may support a decision, but must not be the primary criterion.",
      de: "Ordne Datensaetze nach ICP-Fit, Produktrelevanz, Region, Signal-Frische und Buying Trigger Kampagnen zu. PLZ darf unterstuetzen, aber nicht primaeres Kriterium sein."
    },
    nextStep: {
      en: "Review enrichment gaps and approve campaign routing rules.",
      de: "Research-Luecken pruefen und Kampagnen-Zuordnungsregeln freigeben."
    }
  },
  {
    id: "campaign-account-expansion",
    name: "Existing account expansion",
    status: "Ready",
    sourceTypes: ["Excel"],
    importedRecords: 62,
    enrichedRecords: 62,
    assignedRecords: 49,
    ownerId: "sales-lead",
    assignmentPrompt: {
      en: "Prioritize accounts with existing CRM relationship, operational pain signals, current project activity, and CTOX handoff potential.",
      de: "Priorisiere Accounts mit bestehender CRM-Beziehung, operativen Pain-Signalen, aktueller Projektaktivitaet und CTOX-Handoff-Potenzial."
    },
    nextStep: {
      en: "Create outreach batches for owner review.",
      de: "Outreach-Batches fuer Owner-Review anlegen."
    }
  }
];

export const salesOffers: SalesOffer[] = [
  {
    id: "offer-northstar-pilot",
    opportunityId: "opp-northstar-pilot",
    accountId: "northstar-labs",
    contactId: "mira-northstar",
    number: "AG-2026-014",
    title: "CTOX Business OS Pilot",
    status: "Sent",
    issuedAt: "2026-05-01",
    validUntil: "2026-05-15",
    currency: "EUR",
    netAmount: 40336,
    taxAmount: 7664,
    grossAmount: 48000,
    probabilityImpact: 76,
    paymentTerms: {
      en: "50% on acceptance, 50% after pilot success gate.",
      de: "50% bei Beauftragung, 50% nach Pilot-Erfolgskriterium."
    },
    deliveryScope: {
      en: "Sales, Operations, Business, and CTOX queue workspace with guided onboarding.",
      de: "Sales-, Operations-, Business- und CTOX-Queue-Workspace mit gefuehrtem Onboarding."
    },
    introText: {
      en: "We offer the CTOX Business OS pilot as a finance-ready operating workspace.",
      de: "Wir bieten den CTOX Business OS Pilot als finance-ready Betriebsworkspace an."
    },
    closingText: {
      en: "We look forward to the pilot start and will align the rollout dates after acceptance.",
      de: "Wir freuen uns auf den Pilotstart und stimmen die Rollout-Termine nach Beauftragung ab."
    },
    lineItems: [
      { description: "Business OS setup", quantity: 6, unit: "Day", unitPrice: 1180, taxRate: 19, discount: 0 },
      { description: "CTOX Core managed pilot", quantity: 6, unit: "Month", unitPrice: 2200, taxRate: 19, discount: 0 },
      { description: "Onboarding and handoff", quantity: 2, unit: "Day", unitPrice: 1180, taxRate: 19, discount: 0 }
    ],
    nextStep: {
      en: "Confirm commercial owner, then convert accepted offer into Business invoice draft.",
      de: "Kommerziellen Owner bestaetigen, dann angenommenes Angebot in Business-Rechnungsentwurf ueberfuehren."
    }
  },
  {
    id: "offer-brightfield-rollout",
    opportunityId: "opp-brightfield-rollout",
    accountId: "brightfield-gmbh",
    contactId: "sven-brightfield",
    number: "AG-2026-015",
    title: "Consulting Delivery Rollout",
    status: "Draft",
    issuedAt: "2026-05-02",
    validUntil: "2026-05-22",
    currency: "EUR",
    netAmount: 30252,
    taxAmount: 5748,
    grossAmount: 36000,
    probabilityImpact: 48,
    paymentTerms: {
      en: "Payment target 10 days, 2% discount if paid within 5 days.",
      de: "Zahlungsziel 10 Tage, 2% Skonto bei Zahlung innerhalb von 5 Tagen."
    },
    deliveryScope: {
      en: "Self-hosted Next.js/Postgres rollout, offer templates, project workflow, and recurring report setup.",
      de: "Self-hosted Next.js/Postgres Rollout, Angebotsvorlagen, Projektworkflow und wiederkehrende Reports."
    },
    introText: {
      en: "We are pleased to offer the rollout of the consulting delivery workspace.",
      de: "Gerne bieten wir den Rollout des Consulting Delivery Workspace an."
    },
    closingText: {
      en: "After approval, CTOX prepares the Operations handoff and invoice draft automatically.",
      de: "Nach Freigabe bereitet CTOX Operations-Handoff und Rechnungsentwurf automatisch vor."
    },
    lineItems: [
      { description: "Workspace implementation", quantity: 8, unit: "Day", unitPrice: 1180, taxRate: 19, discount: 0 },
      { description: "Security and hosting note", quantity: 1, unit: "Piece", unitPrice: 1800, taxRate: 19, discount: 0 },
      { description: "Template setup", quantity: 4, unit: "Day", unitPrice: 980, taxRate: 19, discount: 0 }
    ],
    nextStep: {
      en: "Attach security note and send commercial draft for review.",
      de: "Security-Notiz anhaengen und kommerziellen Entwurf zur Pruefung senden."
    }
  },
  {
    id: "offer-northstar-expansion",
    opportunityId: "opp-northstar-expansion",
    accountId: "northstar-labs",
    contactId: "jonas-northstar",
    number: "AG-2026-011",
    title: "Product Workspace Expansion",
    status: "Accepted",
    issuedAt: "2026-04-18",
    validUntil: "2026-04-30",
    currency: "EUR",
    netAmount: 15126,
    taxAmount: 2874,
    grossAmount: 18000,
    probabilityImpact: 100,
    paymentTerms: {
      en: "Invoice after requirements handoff.",
      de: "Rechnung nach Anforderungen-Handoff."
    },
    deliveryScope: {
      en: "Product workspace extension, Knowledge sync, and Operations intake configuration.",
      de: "Product-Workspace-Erweiterung, Knowledge-Sync und Operations-Intake-Konfiguration."
    },
    introText: {
      en: "We offer the agreed expansion of the CTOX product workspace.",
      de: "Wir bieten die vereinbarte Erweiterung des CTOX Product Workspace an."
    },
    closingText: {
      en: "Accepted offer is ready for Business invoice creation.",
      de: "Angenommenes Angebot ist bereit fuer die Business-Rechnungserstellung."
    },
    lineItems: [
      { description: "Product workspace extension", quantity: 10, unit: "Day", unitPrice: 1180, taxRate: 19, discount: 0 },
      { description: "Knowledge sync setup", quantity: 1, unit: "Piece", unitPrice: 3310, taxRate: 19, discount: 0 }
    ],
    nextStep: {
      en: "Create invoice draft and queue Operations handoff.",
      de: "Rechnungsentwurf erstellen und Operations-Handoff queuen."
    }
  },
  {
    id: "offer-atlas-benchmark",
    opportunityId: "opp-atlas-commerce",
    accountId: "atlas-retail",
    contactId: "ellen-atlas",
    number: "AG-2026-009",
    title: "Commerce Ops Benchmark",
    status: "Expired",
    issuedAt: "2026-04-10",
    validUntil: "2026-04-24",
    currency: "EUR",
    netAmount: 20168,
    taxAmount: 3832,
    grossAmount: 24000,
    probabilityImpact: 22,
    paymentTerms: {
      en: "Due on acceptance.",
      de: "Faellig bei Beauftragung."
    },
    deliveryScope: {
      en: "Competitive benchmark, commerce operations report, and follow-up workshop.",
      de: "Competitive Benchmark, Commerce-Operations-Report und Follow-up Workshop."
    },
    introText: {
      en: "We offer the commerce operations benchmark as a focused assessment.",
      de: "Wir bieten den Commerce Operations Benchmark als fokussierte Analyse an."
    },
    closingText: {
      en: "Offer expired; revive only after sponsor confirmation.",
      de: "Angebot abgelaufen; Reaktivierung erst nach Sponsor-Bestaetigung."
    },
    lineItems: [
      { description: "Competitive benchmark", quantity: 4, unit: "Day", unitPrice: 1180, taxRate: 19, discount: 0 },
      { description: "Commerce workshop", quantity: 1, unit: "Piece", unitPrice: 3200, taxRate: 19, discount: 0 },
      { description: "Executive report", quantity: 2, unit: "Day", unitPrice: 980, taxRate: 19, discount: 0 }
    ],
    nextStep: {
      en: "Ask CTOX to refresh buying trigger before resending.",
      de: "CTOX soll Buying Trigger aktualisieren, bevor das Angebot neu versendet wird."
    }
  }
];

export const salesCustomers: SalesCustomer[] = [
  {
    id: "customer-northstar-labs",
    name: "Northstar Labs",
    contactName: "Jonas Keller",
    email: "jonas@northstar.example",
    segment: "B2B SaaS",
    ownerId: "customer-success",
    source: "Accepted offer",
    offerId: "offer-northstar-expansion",
    onboardingStatus: "Queued",
    summary: {
      en: "Customer created from the accepted Product Workspace Expansion offer.",
      de: "Kunde aus dem angenommenen Angebot Product Workspace Expansion."
    },
    nextStep: {
      en: "Create onboarding project and hand over accepted scope to Operations.",
      de: "Onboarding-Projekt anlegen und angenommenen Scope an Operations uebergeben."
    }
  },
  {
    id: "customer-metricspace-direct",
    name: "MetricSpace GmbH",
    contactName: "Laura Neumann",
    email: "laura@metricspace.example",
    segment: "Existing customer",
    ownerId: "customer-success",
    source: "Direct",
    onboardingStatus: "In progress",
    summary: {
      en: "Existing customer entered directly without a prior campaign, pipeline, lead, or offer record.",
      de: "Bestandskunde direkt angelegt, ohne vorherige Kampagne, Pipeline, Lead oder Angebot."
    },
    nextStep: {
      en: "Review current onboarding state and link Operations project.",
      de: "Aktuellen Onboarding-Stand pruefen und Operations-Projekt verknuepfen."
    }
  }
];

export const salesTasks: SalesTask[] = [
  {
    id: "task-northstar-order",
    subject: "Send Northstar pilot order form",
    ownerId: "sales-lead",
    due: "2026-05-03",
    priority: "Urgent",
    status: "Open",
    linkedResource: "opportunity",
    linkedRecordId: "opp-northstar-pilot",
    nextStep: {
      en: "Include rollout dates, success gate, and CTOX queue sync note.",
      de: "Rollout-Daten, Erfolgskriterium und CTOX-Queue-Sync-Notiz aufnehmen."
    }
  },
  {
    id: "task-brightfield-security",
    subject: "Prepare Brightfield security note",
    ownerId: "owner",
    due: "2026-05-04",
    priority: "High",
    status: "In progress",
    linkedResource: "account",
    linkedRecordId: "brightfield-gmbh",
    nextStep: {
      en: "Explain self-hosted Next.js and Postgres deployment boundary.",
      de: "Self-hosted Next.js und Postgres Deployment-Grenze erklaeren."
    }
  },
  {
    id: "task-urbangrid-discovery",
    subject: "Book UrbanGrid workflow mapping",
    ownerId: "sales-lead",
    due: "2026-05-06",
    priority: "High",
    status: "Open",
    linkedResource: "contact",
    linkedRecordId: "amina-urbangrid",
    nextStep: {
      en: "Invite VP Operations and IT owner; prepare ticket-flow questions.",
      de: "VP Operations und IT Owner einladen; Ticketflow-Fragen vorbereiten."
    }
  },
  {
    id: "task-meshworks-research",
    subject: "Research MeshWorks stack",
    ownerId: "ctox-agent",
    due: "2026-05-05",
    priority: "Normal",
    status: "Waiting",
    linkedResource: "lead",
    linkedRecordId: "lead-meshworks",
    nextStep: {
      en: "Queue CTOX web research and summarize likely operating gaps.",
      de: "CTOX Web Research queuen und wahrscheinliche Operating Gaps zusammenfassen."
    }
  },
  {
    id: "task-atlas-benchmark",
    subject: "Create Atlas commerce benchmark",
    ownerId: "ctox-agent",
    due: "2026-05-07",
    priority: "Normal",
    status: "Open",
    linkedResource: "opportunity",
    linkedRecordId: "opp-atlas-commerce",
    nextStep: {
      en: "Use Marketing competitive analysis as source material for sales follow-up.",
      de: "Marketing Wettbewerbsanalyse als Quelle fuer Sales Follow-up nutzen."
    }
  }
];

const seedSalesBundle: SalesBundle = {
  owners: salesOwners,
  accounts: salesAccounts,
  campaigns: salesCampaigns,
  contacts: salesContacts,
  customers: salesCustomers,
  opportunities: salesOpportunities,
  offers: salesOffers,
  leads: salesLeads,
  tasks: salesTasks
};

export async function getSalesBundle(): Promise<SalesBundle> {
  if (!shouldUsePostgres()) return seedSalesBundle;

  try {
    const db = await import("@ctox-business/db/modules");
    const [accountRows, campaignRows, contactRows, customerRows, opportunityRows, offerRows, leadRows, taskRows] = await Promise.all([
      db.listModuleRecords("sales", "accounts"),
      db.listModuleRecords("sales", "campaigns"),
      db.listModuleRecords("sales", "contacts"),
      db.listModuleRecords("sales", "customers"),
      db.listModuleRecords("sales", "opportunities"),
      db.listModuleRecords("sales", "offers"),
      db.listModuleRecords("sales", "leads"),
      db.listModuleRecords("sales", "tasks")
    ]);

    const shouldSeed = (accountRows?.length ?? 0) === 0 && (opportunityRows?.length ?? 0) === 0 && (offerRows?.length ?? 0) === 0 && shouldAutoSeedPostgres();
    if (shouldSeed) {
      await db.seedModuleRecords("sales", salesSeedRecords());
      return getSalesBundle();
    }

    return {
      owners: salesOwners,
      accounts: rowsToPayload(accountRows, salesAccounts),
      campaigns: rowsToPayload(campaignRows, salesCampaigns),
      contacts: rowsToPayload(contactRows, salesContacts),
      customers: rowsToPayload(customerRows, salesCustomers),
      opportunities: rowsToPayload(opportunityRows, salesOpportunities),
      offers: rowsToPayload(offerRows, salesOffers),
      leads: rowsToPayload(leadRows, salesLeads),
      tasks: rowsToPayload(taskRows, salesTasks)
    };
  } catch (error) {
    console.warn("Falling back to Sales seed data.", error);
    return seedSalesBundle;
  }
}

export async function getSalesResource(resource: string) {
  const data = await getSalesBundle();

  if (resource === "accounts") return data.accounts;
  if (resource === "campaigns") return data.campaigns;
  if (resource === "contacts") return data.contacts;
  if (resource === "customers") return data.customers;
  if (resource === "leads") return data.leads;
  if (resource === "onboarding_projects") {
    return data.customers.map((customer) => ({
      id: `onboarding-${customer.id}`,
      customerId: customer.id,
      offerId: customer.offerId ?? null,
      status: customer.onboardingStatus
    }));
  }
  if (resource === "offers") return data.offers;
  if (resource === "opportunities" || resource === "pipeline") return data.opportunities;
  if (resource === "sales_activity") return { accounts: data.accounts, contacts: data.contacts, tasks: data.tasks, offers: data.offers };
  if (resource === "tasks") return data.tasks;
  if (resource === "owners") return data.owners;

  return null;
}

export function text(value: Localized, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}

function salesSeedRecords() {
  return {
    accounts: salesAccounts.map((account) => ({
      id: account.id,
      label: account.name,
      status: account.health,
      ownerId: account.ownerId,
      payload: account
    })),
    campaigns: salesCampaigns.map((campaign) => ({
      id: campaign.id,
      label: campaign.name,
      status: campaign.status,
      ownerId: campaign.ownerId,
      payload: campaign
    })),
    contacts: salesContacts.map((contact) => ({
      id: contact.id,
      label: contact.name,
      status: contact.relationship,
      ownerId: null,
      payload: contact
    })),
    customers: salesCustomers.map((customer) => ({
      id: customer.id,
      label: customer.name,
      status: customer.onboardingStatus,
      ownerId: customer.ownerId,
      payload: customer
    })),
    opportunities: salesOpportunities.map((opportunity) => ({
      id: opportunity.id,
      label: opportunity.name,
      status: opportunity.stage,
      ownerId: opportunity.ownerId,
      payload: opportunity
    })),
    offers: salesOffers.map((offer) => ({
      id: offer.id,
      label: offer.title,
      status: offer.status,
      ownerId: salesOpportunities.find((opportunity) => opportunity.id === offer.opportunityId)?.ownerId ?? null,
      payload: offer
    })),
    leads: salesLeads.map((lead) => ({
      id: lead.id,
      label: lead.company,
      status: lead.status,
      ownerId: lead.ownerId,
      payload: lead
    })),
    tasks: salesTasks.map((task) => ({
      id: task.id,
      label: task.subject,
      status: task.status,
      ownerId: task.ownerId,
      payload: task
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
