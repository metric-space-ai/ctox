export type SupportedLocale = "en" | "de";
export type Localized = Record<SupportedLocale, string>;

export type OperationsHealth = "Green" | "Amber" | "Red";
export type WorkStatus = "Backlog" | "Ready" | "In progress" | "Review" | "Done";
export type WorkPriority = "Low" | "Normal" | "High" | "Urgent";

export type OperationsPerson = {
  id: string;
  name: string;
  role: string;
};

export type OperationsCustomer = {
  id: string;
  name: string;
  segment: string;
  ownerId: string;
};

export type OperationsProject = {
  id: string;
  name: string;
  code: string;
  parentProjectId?: string;
  ownerId: string;
  memberIds?: string[];
  customerId?: string;
  health: OperationsHealth;
  progress: number;
  activeItems: number;
  budgetHours?: number;
  spentHours?: number;
  storageUsedGb?: number;
  storageQuotaGb?: number;
  nextMilestone: string;
  start: string;
  end: string;
  summary: Localized;
  linkedModules: Array<"sales" | "marketing" | "business" | "ctox">;
};

export type OperationsWorkRelation = {
  type: "blocks" | "relates" | "duplicates" | "follows";
  targetId: string;
};

export type OperationsTimeEntry = {
  personId: string;
  date: string;
  hours: number;
  note: string;
};

export type OperationsReminder = {
  id: string;
  due: string;
  channel: "CTOX" | "Email" | "Calendar";
  note: string;
};

export type OperationsWorkItem = {
  id: string;
  semanticId?: string;
  subject: string;
  projectId: string;
  type: "Feature" | "Bug" | "Task" | "Decision" | "Checklist" | "Document";
  status: WorkStatus;
  priority: WorkPriority;
  assigneeId: string;
  start?: string;
  due: string;
  estimate: number;
  doneRatio?: number;
  description: Localized;
  linkedKnowledgeIds: string[];
  customFields?: Record<string, string>;
  relations?: OperationsWorkRelation[];
  timeEntries?: OperationsTimeEntry[];
  reminders?: OperationsReminder[];
  comments?: Array<{
    personId: string;
    date: string;
    body: string;
  }>;
};

export type OperationsMilestone = {
  id: string;
  title: string;
  projectId: string;
  date: string;
  status: "Upcoming" | "At risk" | "Complete";
};

export type OperationsMeeting = {
  id: string;
  title: string;
  projectId: string;
  date: string;
  facilitatorId: string;
  decisions: string[];
  actionItems: string[];
  agenda: Localized[];
};

export type OperationsDecision = {
  id: string;
  text: Localized;
  projectId: string;
  meetingId: string;
  linkedWorkItemIds: string[];
};

export type OperationsActionItem = {
  id: string;
  text: Localized;
  ownerId: string;
  due: string;
  workItemId?: string;
};

export type OperationsKnowledgeItem = {
  id: string;
  title: string;
  projectId: string;
  kind: "Skillbook" | "Runbook";
  updated: string;
  ownerId: string;
  linkedItems: string[];
  sections: Array<{
    title: Localized;
    body: Localized;
  }>;
};

export type OperationsDocumentTemplate = {
  id: string;
  name: string;
  kind: "Proposal" | "Invoice" | "Report" | "Meeting notes" | "Runbook";
  targetModule: "sales" | "operations" | "business" | "marketing";
  description: Localized;
  variables: string[];
  blocks: Array<{
    id: string;
    title: Localized;
    html: Localized;
  }>;
};

export type OperationsDocumentRecord = {
  id: string;
  title: string;
  kind: "Skillbook" | "Template" | "Export";
  templateId?: string;
  projectId?: string;
  knowledgeId?: string;
  ownerId: string;
  status: "Draft" | "Review" | "Approved" | "Exported";
  updated: string;
  format: "ctox-doc" | "docx" | "pdf" | "html";
  version: number;
  bodyHtml: Localized;
  linkedRecords: Array<{
    module: "sales" | "marketing" | "operations" | "business" | "ctox";
    recordType: string;
    recordId: string;
    label: string;
  }>;
};

export const operationsPeople: OperationsPerson[] = [
  { id: "owner", name: "Owner", role: "Founder / General Manager" },
  { id: "sales-lead", name: "Sales Lead", role: "Pipeline and customer handoff" },
  { id: "marketing-lead", name: "Marketing Lead", role: "Website, launches, market research" },
  { id: "operations-lead", name: "Operations Lead", role: "Delivery, projects, daily work" },
  { id: "finance-lead", name: "Finance Lead", role: "Invoices, reports, bookkeeping exports" },
  { id: "ctox-agent", name: "CTOX Agent", role: "Automation and business-stack customization" }
];

export const operationsCustomers: OperationsCustomer[] = [
  { id: "northstar-labs", name: "Northstar Labs", segment: "B2B SaaS", ownerId: "sales-lead" },
  { id: "brightfield-gmbh", name: "Brightfield GmbH", segment: "Professional services", ownerId: "sales-lead" },
  { id: "urbangrid-solutions", name: "UrbanGrid Solutions", segment: "Infrastructure", ownerId: "owner" }
];

export const operationsProjects: OperationsProject[] = [
  {
    id: "company-setup",
    name: "Company Setup",
    code: "OPS-001",
    ownerId: "operations-lead",
    memberIds: ["owner", "operations-lead", "ctox-agent", "finance-lead"],
    health: "Green",
    progress: 72,
    activeItems: 5,
    budgetHours: 80,
    spentHours: 48,
    storageUsedGb: 1.8,
    storageQuotaGb: 10,
    nextMilestone: "CTOX Business OS Installed",
    start: "2026-05-01",
    end: "2026-05-15",
    linkedModules: ["ctox", "business"],
    summary: {
      en: "Baseline setup for website, CRM, accounting, internal operating cadence, and CTOX queue integration.",
      de: "Grundsetup für Website, CRM, Buchhaltung, interne Abläufe und CTOX-Queue-Integration."
    }
  },
  {
    id: "first-customer-delivery",
    name: "First Customer Delivery",
    code: "OPS-002",
    parentProjectId: "company-setup",
    ownerId: "operations-lead",
    memberIds: ["operations-lead", "sales-lead", "ctox-agent"],
    customerId: "northstar-labs",
    health: "Amber",
    progress: 38,
    activeItems: 4,
    budgetHours: 120,
    spentHours: 34,
    storageUsedGb: 3.2,
    storageQuotaGb: 12,
    nextMilestone: "First Customer Onboarded",
    start: "2026-05-13",
    end: "2026-06-12",
    linkedModules: ["sales", "ctox"],
    summary: {
      en: "Implementation workflow for the first customer: onboarding, scope control, support handoff, and delivery reporting.",
      de: "Umsetzungsworkflow für den ersten Kunden: Onboarding, Scope-Kontrolle, Support-Übergabe und Delivery Reporting."
    }
  },
  {
    id: "product-launch",
    name: "Product Launch",
    code: "MKT-001",
    parentProjectId: "company-setup",
    ownerId: "marketing-lead",
    memberIds: ["marketing-lead", "sales-lead", "owner"],
    health: "Green",
    progress: 56,
    activeItems: 5,
    budgetHours: 90,
    spentHours: 39,
    storageUsedGb: 2.4,
    storageQuotaGb: 10,
    nextMilestone: "Public Website Ready",
    start: "2026-05-08",
    end: "2026-05-29",
    linkedModules: ["marketing", "sales"],
    summary: {
      en: "Product page, positioning, sales materials, pricing story, and competitive-analysis follow-up.",
      de: "Produktseite, Positionierung, Sales-Materialien, Pricing Story und Follow-up der Wettbewerbsanalyse."
    }
  },
  {
    id: "support-operations",
    name: "Support Operations",
    code: "OPS-003",
    parentProjectId: "company-setup",
    ownerId: "operations-lead",
    memberIds: ["operations-lead", "ctox-agent"],
    health: "Amber",
    progress: 44,
    activeItems: 3,
    budgetHours: 70,
    spentHours: 26,
    storageUsedGb: 0.9,
    storageQuotaGb: 8,
    nextMilestone: "Support Desk Live",
    start: "2026-05-22",
    end: "2026-06-12",
    linkedModules: ["ctox"],
    summary: {
      en: "Helpdesk, SLA, bug intake, escalation rules, and knowledge-base handoff for daily support.",
      de: "Helpdesk, SLA, Bug Intake, Eskalationsregeln und Knowledge-Base-Übergabe für den Support-Alltag."
    }
  },
  {
    id: "finance-readiness",
    name: "Finance Readiness",
    code: "BUS-001",
    parentProjectId: "company-setup",
    ownerId: "finance-lead",
    memberIds: ["finance-lead", "owner", "sales-lead"],
    health: "Red",
    progress: 24,
    activeItems: 3,
    budgetHours: 64,
    spentHours: 18,
    storageUsedGb: 1.1,
    storageQuotaGb: 8,
    nextMilestone: "Invoice Flow Ready",
    start: "2026-05-10",
    end: "2026-06-05",
    linkedModules: ["business", "sales"],
    summary: {
      en: "Quote, invoice, cost-center, monthly reporting, and bookkeeping export readiness.",
      de: "Bereitschaft für Angebote, Rechnungen, Kostenstellen, Monatsreporting und Buchhaltungsexporte."
    }
  }
];

export const operationsWorkItems: OperationsWorkItem[] = [
  {
    id: "wp-1001",
    semanticId: "OPS-001-1",
    subject: "Define default operating model",
    projectId: "company-setup",
    type: "Decision",
    status: "Review",
    priority: "High",
    assigneeId: "owner",
    due: "2026-05-04",
    estimate: 3,
    doneRatio: 80,
    linkedKnowledgeIds: ["kb-operating-model"],
    customFields: {
      Acceptance: "Module boundaries, sync rules, customization policy",
      "CTOX owner": "ctox-agent"
    },
    relations: [
      { type: "blocks", targetId: "wp-1003" },
      { type: "relates", targetId: "wp-1008" }
    ],
    timeEntries: [
      { personId: "owner", date: "2026-05-01", hours: 1.5, note: "Operating-model draft" },
      { personId: "ctox-agent", date: "2026-05-02", hours: 0.75, note: "Sync rules review" }
    ],
    reminders: [
      { id: "rem-1001", due: "2026-05-04 08:30", channel: "CTOX", note: "Review before weekly management." }
    ],
    comments: [
      { personId: "operations-lead", date: "2026-05-02", body: "Keep this as the source of truth for module handoffs." }
    ],
    description: {
      en: "Document how Sales, Marketing, Operations, Business, and CTOX work together in the vanilla stack.",
      de: "Dokumentieren, wie Sales, Marketing, Operations, Business und CTOX im Vanilla Stack zusammenarbeiten."
    }
  },
  {
    id: "wp-1002",
    semanticId: "OPS-002-1",
    subject: "Create first customer onboarding checklist",
    projectId: "first-customer-delivery",
    type: "Checklist",
    status: "Review",
    priority: "High",
    assigneeId: "operations-lead",
    due: "2026-05-08",
    estimate: 5,
    doneRatio: 60,
    linkedKnowledgeIds: ["kb-customer-onboarding"],
    customFields: {
      Template: "Kickoff, access, goals, support handoff",
      "Customer segment": "B2B SaaS"
    },
    relations: [
      { type: "follows", targetId: "wp-1001" },
      { type: "relates", targetId: "wp-1009" }
    ],
    timeEntries: [
      { personId: "operations-lead", date: "2026-05-02", hours: 2, note: "Checklist structure" }
    ],
    reminders: [
      { id: "rem-1002", due: "2026-05-07 16:00", channel: "Calendar", note: "Confirm customer-facing version." }
    ],
    description: {
      en: "Create a reusable onboarding checklist that turns a won sales opportunity into delivery work.",
      de: "Eine wiederverwendbare Onboarding-Checkliste erstellen, die eine gewonnene Opportunity in Delivery-Arbeit überführt."
    }
  },
  {
    id: "wp-1003",
    semanticId: "OPS-001-2",
    subject: "Set up sales pipeline stages",
    projectId: "company-setup",
    type: "Task",
    status: "Ready",
    priority: "Normal",
    assigneeId: "sales-lead",
    due: "2026-05-07",
    estimate: 4,
    doneRatio: 30,
    linkedKnowledgeIds: [],
    relations: [{ type: "follows", targetId: "wp-1001" }],
    description: {
      en: "Define default pipeline stages and handoff rules from Sales into Operations.",
      de: "Standard-Pipeline-Stufen und Übergaberegeln von Sales nach Operations definieren."
    }
  },
  {
    id: "wp-1004",
    semanticId: "MKT-001-1",
    subject: "Prepare launch website content",
    projectId: "product-launch",
    type: "Feature",
    status: "In progress",
    priority: "High",
    assigneeId: "marketing-lead",
    due: "2026-05-12",
    estimate: 6,
    doneRatio: 45,
    linkedKnowledgeIds: ["kb-launch-website"],
    relations: [{ type: "relates", targetId: "wp-1007" }],
    description: {
      en: "Prepare modular public website copy blocks without locking the vanilla template to one use case.",
      de: "Modulare Copy-Blöcke für die öffentliche Website vorbereiten, ohne das Vanilla Template auf einen Use Case festzulegen."
    }
  },
  {
    id: "wp-1005",
    semanticId: "OPS-003-1",
    subject: "Connect bug reports to CTOX queue",
    projectId: "support-operations",
    type: "Feature",
    status: "In progress",
    priority: "Urgent",
    assigneeId: "ctox-agent",
    due: "2026-05-06",
    estimate: 5,
    doneRatio: 55,
    linkedKnowledgeIds: ["kb-bug-reporting"],
    customFields: {
      Screenshot: "Area select + pen markup",
      Queue: "ctox bug-report task"
    },
    relations: [{ type: "blocks", targetId: "wp-1009" }],
    description: {
      en: "Every bug report needs module, submodule, drawer, screenshot annotation, and CTOX queue context.",
      de: "Jeder Bug Report braucht Modul-, Submodul-, Drawer-, Screenshot-Annotation- und CTOX-Queue-Kontext."
    }
  },
  {
    id: "wp-1006",
    subject: "Create invoice numbering rules",
    projectId: "finance-readiness",
    type: "Task",
    status: "Ready",
    priority: "Urgent",
    assigneeId: "finance-lead",
    due: "2026-05-09",
    estimate: 3,
    linkedKnowledgeIds: ["kb-invoice-process"],
    description: {
      en: "Define invoice numbering, tax labels, customer references, and export naming for bookkeeping.",
      de: "Rechnungsnummern, Steuerlabels, Kundenreferenzen und Exportnamen für die Buchhaltung definieren."
    }
  },
  {
    id: "wp-1007",
    subject: "Draft standard proposal template",
    projectId: "company-setup",
    type: "Document",
    status: "Backlog",
    priority: "Normal",
    assigneeId: "sales-lead",
    due: "2026-05-14",
    estimate: 4,
    linkedKnowledgeIds: [],
    description: {
      en: "Create a starting proposal structure for service delivery and product subscriptions.",
      de: "Eine Angebotsstruktur für Service Delivery und Produktabonnements erstellen."
    }
  },
  {
    id: "wp-1008",
    subject: "Set up weekly management meeting",
    projectId: "company-setup",
    type: "Task",
    status: "Done",
    priority: "Low",
    assigneeId: "owner",
    due: "2026-05-02",
    estimate: 2,
    linkedKnowledgeIds: ["kb-weekly-management"],
    description: {
      en: "Default weekly management cadence with decisions, actions, and cross-module review.",
      de: "Standard-Wochenrhythmus mit Entscheidungen, Aktionen und modulübergreifendem Review."
    }
  },
  {
    id: "wp-1009",
    subject: "Create support escalation workflow",
    projectId: "support-operations",
    type: "Checklist",
    status: "Backlog",
    priority: "High",
    assigneeId: "operations-lead",
    due: "2026-05-18",
    estimate: 4,
    linkedKnowledgeIds: ["kb-support-escalation"],
    description: {
      en: "Define support labels, severity, ownership, response targets, and escalation paths.",
      de: "Support-Labels, Schweregrade, Zuständigkeiten, Antwortziele und Eskalationspfade definieren."
    }
  },
  {
    id: "wp-1010",
    subject: "Document deployment runbook",
    projectId: "company-setup",
    type: "Task",
    status: "Done",
    priority: "Normal",
    assigneeId: "ctox-agent",
    due: "2026-05-02",
    estimate: 3,
    linkedKnowledgeIds: ["kb-deployment-runbook"],
    description: {
      en: "Next.js, Postgres, Vercel, Neon, and self-hosting deployment steps as a living runbook.",
      de: "Next.js-, Postgres-, Vercel-, Neon- und Self-Hosting-Schritte als lebendes Runbook."
    }
  },
  {
    id: "wp-1011",
    subject: "Review data protection checklist",
    projectId: "company-setup",
    type: "Checklist",
    status: "Backlog",
    priority: "High",
    assigneeId: "owner",
    due: "2026-05-20",
    estimate: 4,
    linkedKnowledgeIds: [],
    description: {
      en: "Check where customer, employee, financial, and telemetry data are stored or transmitted.",
      de: "Prüfen, wo Kunden-, Mitarbeiter-, Finanz- und Telemetriedaten gespeichert oder übertragen werden."
    }
  },
  {
    id: "wp-1012",
    subject: "Prepare monthly business report",
    projectId: "finance-readiness",
    type: "Feature",
    status: "In progress",
    priority: "Normal",
    assigneeId: "finance-lead",
    due: "2026-05-28",
    estimate: 6,
    linkedKnowledgeIds: ["kb-monthly-business-review"],
    description: {
      en: "Create the first monthly business report with pipeline, delivery, support, invoices, and cash signals.",
      de: "Ersten Monatsbericht mit Pipeline-, Delivery-, Support-, Rechnungs- und Cash-Signalen erstellen."
    }
  }
];

export const operationsMilestones: OperationsMilestone[] = [
  { id: "ms-installed", title: "CTOX Business OS Installed", projectId: "company-setup", date: "2026-05-05", status: "Complete" },
  { id: "ms-website", title: "Public Website Ready", projectId: "product-launch", date: "2026-05-16", status: "Upcoming" },
  { id: "ms-customer", title: "First Customer Onboarded", projectId: "first-customer-delivery", date: "2026-05-27", status: "At risk" },
  { id: "ms-invoice", title: "Invoice Flow Ready", projectId: "finance-readiness", date: "2026-05-31", status: "At risk" },
  { id: "ms-support", title: "Support Desk Live", projectId: "support-operations", date: "2026-06-03", status: "Upcoming" },
  { id: "ms-reporting", title: "Monthly Reporting Ready", projectId: "finance-readiness", date: "2026-06-05", status: "Upcoming" }
];

export const operationsKnowledgeItems: OperationsKnowledgeItem[] = [
  {
    id: "kb-operating-model",
    title: "Business Operating Model",
    projectId: "company-setup",
    kind: "Skillbook",
    updated: "2026-05-02",
    ownerId: "ctox-agent",
    linkedItems: ["wp-1001", "wp-1003"],
    sections: [
      {
        title: { en: "Scope CTOX should learn", de: "Scope, den CTOX lernen soll" },
        body: { en: "Sales, Marketing, Operations, Business, and CTOX core share one operating model: every module owns its functional records, while CTOX stores prompts, bug reports, queue tasks, and cross-module knowledge links.", de: "Sales, Marketing, Operations, Business und CTOX Core teilen ein Betriebsmodell: Jedes Modul besitzt seine Fachrecords, CTOX speichert Prompts, Bug Reports, Queue Tasks und modulübergreifende Knowledge Links." }
      },
      {
        title: { en: "Routing rule", de: "Routing-Regel" },
        body: { en: "When a user prompts CTOX from a module item, keep the current module, submodule, drawer, record id, selected text, and linked work item ids as mandatory task context.", de: "Wenn ein User CTOX aus einem Moduleintrag promptet, müssen aktuelles Modul, Submodul, Drawer, Record-ID, markierter Text und verknüpfte Work-Item-IDs Pflichtkontext des Tasks bleiben." }
      },
      {
        title: { en: "Guardrail", de: "Guardrail" },
        body: { en: "Core upgrades may update CTOX runtime and vanilla skills, but must not overwrite customized business module code or customer-owned Postgres data.", de: "Core-Upgrades dürfen CTOX Runtime und Vanilla Skills aktualisieren, aber nie angepassten Business-Modul-Code oder kundeneigene Postgres-Daten überschreiben." }
      }
    ]
  },
  {
    id: "kb-customer-onboarding",
    title: "Customer Onboarding Skillbook",
    projectId: "first-customer-delivery",
    kind: "Skillbook",
    updated: "2026-05-01",
    ownerId: "operations-lead",
    linkedItems: ["wp-1002"],
    sections: [
      {
        title: { en: "Trigger", de: "Trigger" },
        body: { en: "Start this skillbook when a Sales opportunity is marked won or when a customer project is created without onboarding tasks.", de: "Dieses Skillbook starten, wenn eine Sales Opportunity gewonnen ist oder ein Kundenprojekt ohne Onboarding Tasks angelegt wird." }
      },
      {
        title: { en: "Inputs", de: "Inputs" },
        body: { en: "Customer name, buyer contact, signed scope, kickoff date, delivery owner, support channel, invoice contact, and first measurable outcome.", de: "Kundenname, Buyer-Kontakt, unterschriebener Scope, Kickoff-Datum, Delivery Owner, Supportkanal, Rechnungskontakt und erstes messbares Ergebnis." }
      },
      {
        title: { en: "CTOX output", de: "CTOX Output" },
        body: { en: "Create onboarding work items, link the proposal document, draft kickoff agenda, open a support handoff checklist, and queue missing customer data requests.", de: "Onboarding Work Items anlegen, Angebotsdokument verknüpfen, Kickoff Agenda entwerfen, Support-Handoff-Checkliste öffnen und fehlende Kundendatenanfragen queuen." }
      }
    ]
  },
  {
    id: "kb-bug-reporting",
    title: "Bug Reporting Workflow",
    projectId: "support-operations",
    kind: "Runbook",
    updated: "2026-05-02",
    ownerId: "ctox-agent",
    linkedItems: ["wp-1005"],
    sections: [
      {
        title: { en: "Required context", de: "Erforderlicher Kontext" },
        body: { en: "Bug reports must include module, submodule, record id, drawer state, viewport, screenshot area, annotation, expected behavior, and reproduction steps.", de: "Bug Reports müssen Modul, Submodul, Record-ID, Drawer-State, Viewport, Screenshot-Bereich, Annotation, erwartetes Verhalten und Repro-Schritte enthalten." }
      },
      {
        title: { en: "Procedure", de: "Ablauf" },
        body: { en: "Queue the bug in CTOX, attach the current app route, link the affected work item if present, and create a follow-up task in Operations when the fix needs product work.", de: "Bug in CTOX queuen, aktuelle App-Route anhängen, betroffenes Work Item verknüpfen und einen Operations Follow-up Task anlegen, wenn der Fix Produktarbeit braucht." }
      },
      {
        title: { en: "Definition of done", de: "Definition of Done" },
        body: { en: "A bug is complete only after the UI state is retested in the in-app browser and the queue task references the verification route.", de: "Ein Bug ist erst abgeschlossen, wenn der UI-State im In-App-Browser erneut getestet wurde und der Queue Task die Verifikationsroute referenziert." }
      }
    ]
  },
  {
    id: "kb-deployment-runbook",
    title: "Deployment Runbook",
    projectId: "company-setup",
    kind: "Runbook",
    updated: "2026-05-02",
    ownerId: "ctox-agent",
    linkedItems: ["wp-1010"],
    sections: [
      {
        title: { en: "Default path", de: "Standardpfad" },
        body: { en: "Deploy Next.js to Vercel, Postgres to Neon, and keep CTOX core state separate from customized business data.", de: "Next.js nach Vercel deployen, Postgres über Neon nutzen und CTOX-Core-State getrennt von Business-Customizing halten." }
      },
      {
        title: { en: "Upgrade rule", de: "Upgrade-Regel" },
        body: { en: "Install the CTOX Business OS from the CTOX repo as a skill-owned template. Later CTOX core upgrades must not pull template changes into a customized customer app automatically.", de: "Das CTOX Business OS aus dem CTOX Repo als skill-owned Template installieren. Spätere CTOX Core Upgrades dürfen Template-Änderungen nicht automatisch in eine angepasste Kunden-App ziehen." }
      },
      {
        title: { en: "Rollback", de: "Rollback" },
        body: { en: "Keep the previous deploy, database migration id, and CTOX queue snapshot available before schema or module upgrades.", de: "Vor Schema- oder Modul-Upgrades müssen vorheriges Deployment, Datenbank-Migration-ID und CTOX Queue Snapshot verfügbar sein." }
      }
    ]
  },
  {
    id: "kb-invoice-process",
    title: "Invoice Process Skillbook",
    projectId: "finance-readiness",
    kind: "Skillbook",
    updated: "2026-04-30",
    ownerId: "finance-lead",
    linkedItems: ["wp-1006", "wp-1012"],
    sections: [
      {
        title: { en: "Required fields", de: "Pflichtfelder" },
        body: { en: "Invoices need customer reference, project reference, service period, tax note, line items, due date, export status, and linked delivery proof.", de: "Rechnungen brauchen Kundenreferenz, Projektreferenz, Leistungszeitraum, Steuerhinweis, Positionen, Fälligkeitsdatum, Exportstatus und verknüpften Delivery-Nachweis." }
      },
      {
        title: { en: "Validation", de: "Validierung" },
        body: { en: "Before export, CTOX should compare invoice line items with approved scope, logged delivery work, and customer billing contact.", de: "Vor Export soll CTOX Rechnungspositionen mit freigegebenem Scope, geloggter Delivery-Arbeit und Kunden-Rechnungskontakt vergleichen." }
      },
      {
        title: { en: "Business sync", de: "Business Sync" },
        body: { en: "Approved invoices synchronize to Business reporting, cash forecast, and customer account history.", de: "Freigegebene Rechnungen synchronisieren in Business Reporting, Cash Forecast und Kundenhistorie." }
      }
    ]
  },
  {
    id: "kb-weekly-management",
    title: "Weekly Management Cadence",
    projectId: "company-setup",
    kind: "Skillbook",
    updated: "2026-05-02",
    ownerId: "owner",
    linkedItems: ["wp-1008"],
    sections: [
      {
        title: { en: "Standing agenda", de: "Feste Agenda" },
        body: { en: "Pipeline, delivery, support, finance, blockers, CTOX queue, next decisions, and overdue follow-ups.", de: "Pipeline, Delivery, Support, Finance, Blocker, CTOX Queue, nächste Entscheidungen und überfällige Follow-ups." }
      },
      {
        title: { en: "CTOX preparation", de: "CTOX Vorbereitung" },
        body: { en: "Before the meeting, summarize changed records, stale work items, new bugs, blocked projects, and missing owners.", de: "Vor dem Meeting geänderte Records, veraltete Work Items, neue Bugs, blockierte Projekte und fehlende Owner zusammenfassen." }
      },
      {
        title: { en: "After meeting", de: "Nach dem Meeting" },
        body: { en: "Convert decisions into work items, update runbooks when process changed, and queue business reporting deltas.", de: "Entscheidungen in Work Items umwandeln, Runbooks bei Prozessänderungen aktualisieren und Business-Reporting-Deltas queuen." }
      }
    ]
  },
  {
    id: "kb-support-escalation",
    title: "Support Escalation",
    projectId: "support-operations",
    kind: "Runbook",
    updated: "2026-04-29",
    ownerId: "operations-lead",
    linkedItems: ["wp-1009"],
    sections: [
      {
        title: { en: "Severity", de: "Schweregrad" },
        body: { en: "Classify support work by impact, affected customer, workaround, and required owner.", de: "Support-Arbeit nach Auswirkung, betroffenem Kunden, Workaround und Owner klassifizieren." }
      },
      {
        title: { en: "Escalation path", de: "Eskalationspfad" },
        body: { en: "Urgent customer-impacting issues go to Operations owner first, then CTOX bug queue, then Product work item when code or configuration changes are required.", de: "Dringende kundenrelevante Themen gehen zuerst an den Operations Owner, dann in die CTOX Bug Queue und danach als Product Work Item, wenn Code- oder Konfigurationsänderungen nötig sind." }
      },
      {
        title: { en: "Knowledge update", de: "Knowledge Update" },
        body: { en: "Every resolved support case must update either the customer note, the bug runbook, or the relevant product skillbook.", de: "Jeder gelöste Supportfall muss entweder die Kundennotiz, das Bug Runbook oder das relevante Produkt-Skillbook aktualisieren." }
      }
    ]
  },
  {
    id: "kb-monthly-business-review",
    title: "Monthly Business Review Skillbook",
    projectId: "finance-readiness",
    kind: "Skillbook",
    updated: "2026-05-01",
    ownerId: "finance-lead",
    linkedItems: ["wp-1012"],
    sections: [
      {
        title: { en: "Signals", de: "Signale" },
        body: { en: "Revenue, cash, pipeline, delivery risk, support load, product changes, and CTOX automation impact.", de: "Umsatz, Cash, Pipeline, Delivery-Risiko, Supportlast, Produktänderungen und CTOX-Automation Impact." }
      },
      {
        title: { en: "Inputs CTOX should collect", de: "Inputs, die CTOX sammeln soll" },
        body: { en: "Pull Sales pipeline, Operations progress, support bugs, invoice status, cash forecast, and material product changes into one review draft.", de: "Sales Pipeline, Operations Progress, Support Bugs, Rechnungsstatus, Cash Forecast und relevante Produktänderungen in einen Review Draft ziehen." }
      },
      {
        title: { en: "Output", de: "Output" },
        body: { en: "Produce an executive summary, risk list, decisions needed, and next-month operating priorities with linked records.", de: "Executive Summary, Risikoliste, benötigte Entscheidungen und operative Prioritäten für den nächsten Monat mit verknüpften Records erzeugen." }
      }
    ]
  }
];

export const operationsDocumentTemplates: OperationsDocumentTemplate[] = [
  {
    id: "tpl-proposal-standard",
    name: "Standard Proposal",
    kind: "Proposal",
    targetModule: "sales",
    variables: ["customer.name", "proposal.scope", "pricing.total", "project.timeline", "owner.name"],
    description: {
      en: "Reusable offer template for service delivery, subscriptions, and implementation work.",
      de: "Wiederverwendbare Angebotsvorlage für Service Delivery, Subscriptions und Implementierung."
    },
    blocks: [
      {
        id: "proposal-summary",
        title: { en: "Executive summary", de: "Zusammenfassung" },
        html: {
          en: "<h1>Proposal for {{customer.name}}</h1><p>This proposal describes scope, timeline, responsibilities, and commercial terms.</p>",
          de: "<h1>Angebot für {{customer.name}}</h1><p>Dieses Angebot beschreibt Scope, Timeline, Verantwortlichkeiten und kommerzielle Konditionen.</p>"
        }
      },
      {
        id: "proposal-scope",
        title: { en: "Scope", de: "Leistungsumfang" },
        html: {
          en: "<h2>Scope</h2><ul><li>{{proposal.scope}}</li><li>Delivery timeline: {{project.timeline}}</li></ul>",
          de: "<h2>Leistungsumfang</h2><ul><li>{{proposal.scope}}</li><li>Timeline: {{project.timeline}}</li></ul>"
        }
      }
    ]
  },
  {
    id: "tpl-invoice-standard",
    name: "Standard Invoice",
    kind: "Invoice",
    targetModule: "business",
    variables: ["customer.name", "invoice.number", "invoice.lines", "invoice.total", "invoice.tax"],
    description: {
      en: "Invoice template connected to Business customers, products, tax context, and bookkeeping exports.",
      de: "Rechnungsvorlage mit Business-Kunden, Produkten, Steuerkontext und Buchhaltungsexporten."
    },
    blocks: [
      {
        id: "invoice-header",
        title: { en: "Invoice header", de: "Rechnungskopf" },
        html: {
          en: "<h1>Invoice {{invoice.number}}</h1><p>Customer: {{customer.name}}</p>",
          de: "<h1>Rechnung {{invoice.number}}</h1><p>Kunde: {{customer.name}}</p>"
        }
      },
      {
        id: "invoice-lines",
        title: { en: "Line items", de: "Positionen" },
        html: {
          en: "<h2>Line items</h2><p>{{invoice.lines}}</p><p><strong>Total: {{invoice.total}}</strong></p>",
          de: "<h2>Positionen</h2><p>{{invoice.lines}}</p><p><strong>Summe: {{invoice.total}}</strong></p>"
        }
      }
    ]
  },
  {
    id: "tpl-operations-report",
    name: "Operations Report",
    kind: "Report",
    targetModule: "operations",
    variables: ["project.name", "project.health", "work.open", "risk.summary", "ctox.queue"],
    description: {
      en: "Management-ready report template for delivery risk, open work, decisions, and CTOX follow-up.",
      de: "Management-taugliche Vorlage für Delivery-Risiko, offene Arbeit, Entscheidungen und CTOX Follow-up."
    },
    blocks: [
      {
        id: "ops-report-summary",
        title: { en: "Operating summary", de: "Betriebsübersicht" },
        html: {
          en: "<h1>{{project.name}} operating report</h1><p>Status: {{project.health}}</p><p>{{risk.summary}}</p>",
          de: "<h1>{{project.name}} Betriebsreport</h1><p>Status: {{project.health}}</p><p>{{risk.summary}}</p>"
        }
      }
    ]
  }
];

export const operationsDocuments: OperationsDocumentRecord[] = [
  {
    id: "doc-operating-model",
    title: "Operating Model Draft",
    kind: "Skillbook",
    templateId: "tpl-operations-report",
    projectId: "company-setup",
    knowledgeId: "kb-operating-model",
    ownerId: "owner",
    status: "Draft",
    updated: "2026-05-02",
    format: "ctox-doc",
    version: 3,
    bodyHtml: {
      en: "<h1>Operating Model</h1><p>The Business OS combines Sales, Marketing, Operations, Business, and CTOX core through a shared shell, deep links, prompts, bug reports, and synchronized records.</p><h2>Rules</h2><ul><li>One main surface per submodule.</li><li>Details and edits happen in drawers.</li><li>CTOX prompts always carry record context.</li></ul>",
      de: "<h1>Operating Model</h1><p>Das Business OS verbindet Sales, Marketing, Operations, Business und CTOX Core über Shell, Deep Links, Prompts, Bug Reports und synchronisierte Records.</p><h2>Regeln</h2><ul><li>Eine Hauptfläche pro Submodul.</li><li>Details und Edits in Drawern.</li><li>CTOX Prompts tragen immer Record-Kontext.</li></ul>"
    },
    linkedRecords: [
      { module: "operations", recordType: "project", recordId: "company-setup", label: "Company Setup" },
      { module: "ctox", recordType: "knowledge", recordId: "ops-knowledge-map", label: "CTOX Knowledge Map" }
    ]
  },
  {
    id: "doc-customer-proposal",
    title: "Customer Proposal Template",
    kind: "Template",
    templateId: "tpl-proposal-standard",
    projectId: "first-customer-delivery",
    knowledgeId: "kb-customer-onboarding",
    ownerId: "sales-lead",
    status: "Review",
    updated: "2026-05-02",
    format: "ctox-doc",
    version: 1,
    bodyHtml: {
      en: "<h1>Proposal</h1><p>Use this template for the first customer proposal. Replace variables from Sales and Operations before export.</p>",
      de: "<h1>Angebot</h1><p>Diese Vorlage für das erste Kundenangebot nutzen. Variablen aus Sales und Operations vor dem Export ersetzen.</p>"
    },
    linkedRecords: [
      { module: "sales", recordType: "opportunity", recordId: "first-customer", label: "First customer opportunity" },
      { module: "operations", recordType: "work_item", recordId: "wp-1002", label: "Create first customer onboarding checklist" }
    ]
  },
  {
    id: "doc-invoice-template",
    title: "Invoice Template",
    kind: "Template",
    templateId: "tpl-invoice-standard",
    projectId: "finance-readiness",
    knowledgeId: "kb-invoice-process",
    ownerId: "finance-lead",
    status: "Draft",
    updated: "2026-05-01",
    format: "ctox-doc",
    version: 2,
    bodyHtml: {
      en: "<h1>Invoice {{invoice.number}}</h1><p>Customer: {{customer.name}}</p><p>Lines: {{invoice.lines}}</p><p>Total: {{invoice.total}}</p>",
      de: "<h1>Rechnung {{invoice.number}}</h1><p>Kunde: {{customer.name}}</p><p>Positionen: {{invoice.lines}}</p><p>Summe: {{invoice.total}}</p>"
    },
    linkedRecords: [
      { module: "business", recordType: "invoice", recordId: "inv-2026-001", label: "Invoice INV-2026-001" },
      { module: "operations", recordType: "work_item", recordId: "wp-1006", label: "Create invoice numbering rules" }
    ]
  }
];

export const operationsDecisions: OperationsDecision[] = [
  {
    id: "dec-001",
    meetingId: "mtg-weekly-management",
    projectId: "company-setup",
    linkedWorkItemIds: ["wp-1001"],
    text: {
      en: "Use one unified Business OS shell for all modules.",
      de: "Eine einheitliche Business-OS-Shell für alle Module verwenden."
    }
  },
  {
    id: "dec-002",
    meetingId: "mtg-product-launch",
    projectId: "product-launch",
    linkedWorkItemIds: ["wp-1004"],
    text: {
      en: "Keep public website content empty in the vanilla template and wire only the integration points.",
      de: "Öffentliche Website-Inhalte im Vanilla Template leer halten und nur die Integrationspunkte verdrahten."
    }
  },
  {
    id: "dec-003",
    meetingId: "mtg-weekly-management",
    projectId: "company-setup",
    linkedWorkItemIds: ["wp-1005"],
    text: {
      en: "Route all contextual prompts and bug reports through the CTOX queue.",
      de: "Alle kontextuellen Prompts und Bug Reports über die CTOX Queue routen."
    }
  },
  {
    id: "dec-004",
    meetingId: "mtg-finance-reporting",
    projectId: "finance-readiness",
    linkedWorkItemIds: ["wp-1006", "wp-1012"],
    text: {
      en: "Use Postgres for business data and SQLite only for CTOX core state.",
      de: "Postgres für Business-Daten und SQLite nur für CTOX-Core-State verwenden."
    }
  }
];

export const operationsActionItems: OperationsActionItem[] = [
  { id: "act-001", ownerId: "operations-lead", due: "2026-05-08", workItemId: "wp-1002", text: { en: "Create default customer onboarding checklist.", de: "Default Customer-Onboarding-Checkliste erstellen." } },
  { id: "act-002", ownerId: "finance-lead", due: "2026-05-09", workItemId: "wp-1006", text: { en: "Define invoice export format.", de: "Invoice-Exportformat definieren." } },
  { id: "act-003", ownerId: "operations-lead", due: "2026-05-18", workItemId: "wp-1009", text: { en: "Add support intake labels.", de: "Support-Intake-Labels hinzufügen." } },
  { id: "act-004", ownerId: "marketing-lead", due: "2026-05-12", workItemId: "wp-1004", text: { en: "Review launch copy blocks.", de: "Launch-Copy-Blöcke prüfen." } },
  { id: "act-005", ownerId: "ctox-agent", due: "2026-05-15", text: { en: "Connect knowledge pages to CTOX Knowledge Store.", de: "Knowledge-Seiten mit CTOX Knowledge Store verbinden." } },
  { id: "act-006", ownerId: "ctox-agent", due: "2026-05-16", text: { en: "Verify deployment on Vercel with Neon.", de: "Deployment auf Vercel mit Neon verifizieren." } }
];

export const operationsMeetings: OperationsMeeting[] = [
  {
    id: "mtg-weekly-management",
    title: "Weekly Management",
    projectId: "company-setup",
    date: "2026-05-04 09:00",
    facilitatorId: "owner",
    decisions: ["dec-001", "dec-003"],
    actionItems: ["act-001", "act-005"],
    agenda: [
      { en: "Pipeline, delivery, support, finance, CTOX queue.", de: "Pipeline, Delivery, Support, Finance, CTOX Queue." },
      { en: "Confirm cross-module priorities and blockers.", de: "Modulübergreifende Prioritäten und Blocker bestätigen." }
    ]
  },
  {
    id: "mtg-customer-delivery-sync",
    title: "Customer Delivery Sync",
    projectId: "first-customer-delivery",
    date: "2026-05-06 11:00",
    facilitatorId: "operations-lead",
    decisions: [],
    actionItems: ["act-001"],
    agenda: [
      { en: "Review onboarding checklist, first milestone, and customer risks.", de: "Onboarding-Checkliste, erster Meilenstein und Kundenrisiken prüfen." }
    ]
  },
  {
    id: "mtg-product-launch",
    title: "Product Launch Review",
    projectId: "product-launch",
    date: "2026-05-07 15:30",
    facilitatorId: "marketing-lead",
    decisions: ["dec-002"],
    actionItems: ["act-004"],
    agenda: [
      { en: "Website structure, messaging, materials, and competitive-analysis follow-up.", de: "Website-Struktur, Messaging, Materialien und Wettbewerbsanalyse-Follow-up." }
    ]
  },
  {
    id: "mtg-finance-reporting",
    title: "Finance & Reporting",
    projectId: "finance-readiness",
    date: "2026-05-08 14:00",
    facilitatorId: "finance-lead",
    decisions: ["dec-004"],
    actionItems: ["act-002", "act-006"],
    agenda: [
      { en: "Invoices, forecast, costs, monthly report, and bookkeeping exports.", de: "Rechnungen, Forecast, Kosten, Monatsbericht und Buchhaltungsexporte." }
    ]
  },
  {
    id: "mtg-support-review",
    title: "Support Review",
    projectId: "support-operations",
    date: "2026-05-09 10:00",
    facilitatorId: "operations-lead",
    decisions: [],
    actionItems: ["act-003"],
    agenda: [
      { en: "Bugs, SLA, knowledge base, escalation and CTOX bug queue.", de: "Bugs, SLA, Knowledge Base, Eskalation und CTOX Bug Queue." }
    ]
  }
];

export const operationsStatusColumns: WorkStatus[] = ["Backlog", "Ready", "In progress", "Review", "Done"];

export function personById(personId: string) {
  return operationsPeople.find((person) => person.id === personId);
}

export function customerById(customerId?: string) {
  return customerId ? operationsCustomers.find((customer) => customer.id === customerId) : undefined;
}

export function projectById(projectId: string) {
  return operationsProjects.find((project) => project.id === projectId);
}

export function workItemById(workItemId?: string) {
  return workItemId ? operationsWorkItems.find((item) => item.id === workItemId) : undefined;
}

export function knowledgeById(knowledgeId?: string) {
  return knowledgeId ? operationsKnowledgeItems.find((item) => item.id === knowledgeId) : undefined;
}

export function meetingById(meetingId?: string) {
  return meetingId ? operationsMeetings.find((meeting) => meeting.id === meetingId) : undefined;
}

export function decisionById(decisionId: string) {
  return operationsDecisions.find((decision) => decision.id === decisionId);
}

export function actionItemById(actionItemId: string) {
  return operationsActionItems.find((actionItem) => actionItem.id === actionItemId);
}

export function text(value: Localized, locale: SupportedLocale) {
  return value[locale] ?? value.en;
}
