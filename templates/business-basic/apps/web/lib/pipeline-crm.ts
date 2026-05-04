export type PipelineLocale = "de" | "en";
export type TransitionStatus = "idle" | "blocked" | "ready" | "running" | "completed" | "failed";

export type PipelineUser = {
  id: string;
  name: string;
  email: string;
  avatarInitials: string;
};

export type PipelineAccount = {
  id: string;
  name: string;
  industry: string;
  city: string;
  country: string;
  health: "strong" | "steady" | "watch";
};

export type PipelineContact = {
  id: string;
  accountId: string;
  firstName: string;
  lastName: string;
  email: string;
  title: string;
};

export type PipelineStage = {
  id: string;
  name: string;
  sortOrder: number;
  probability: number;
  color: string;
  wipLimit: number;
  exitCriteria: string[];
  transitionStartCriteria: string;
  transitionAgentTodos: string[];
  transitionAgentPrompt: string;
};

export type PipelineOpportunity = {
  id: string;
  accountId: string;
  primaryContactId: string;
  stageId: string;
  name: string;
  amount: number;
  currency: "EUR";
  closeDate: string;
  status: "open" | "won" | "lost";
  source: string;
  forecastCategory: "pipeline" | "best_case" | "commit" | "closed";
  nextStep: string;
  nextStepDueAt: string;
  lastActivityAt: string;
  expectedDecisionDate: string;
  products: string[];
  tags: string[];
  ownerId: string;
  transitionReadiness: TransitionStatus;
  transitionBlockers: string[];
  activeRun?: PipelineTransitionRun;
};

export type PipelineTransitionRun = {
  id: string;
  recordId: string;
  fromStageId: string;
  toStageId: string;
  status: TransitionStatus;
  progress: number;
  criteriaSnapshot: string;
  agentTodoSnapshot: string[];
  agentPromptSnapshot: string;
  log: Array<{ at: string; level: "info" | "warn" | "error"; message: string }>;
};

export type PipelineTransitionMessage = {
  id: string;
  runId: string;
  role: "user" | "agent" | "system";
  body: string;
};

export type PipelineTask = {
  id: string;
  subject: string;
  status: "todo" | "in_progress" | "done" | "blocked";
  dueAt: string;
  relatedType: "opportunity" | "lead" | "contact";
  relatedId: string;
  priority: "low" | "medium" | "high";
};

export type PipelineDataset = {
  users: PipelineUser[];
  accounts: PipelineAccount[];
  contacts: PipelineContact[];
  stages: PipelineStage[];
  opportunities: PipelineOpportunity[];
  tasks: PipelineTask[];
  transitionRuns: PipelineTransitionRun[];
  transitionMessages: PipelineTransitionMessage[];
};

export const pipelineUsers: PipelineUser[] = [
  { id: "usr-mara", name: "Mara König", email: "mara@example.com", avatarInitials: "MK" },
  { id: "usr-jonas", name: "Jonas Weber", email: "jonas@example.com", avatarInitials: "JW" },
  { id: "usr-lea", name: "Lea Brandt", email: "lea@example.com", avatarInitials: "LB" }
];

export const pipelineAccounts: PipelineAccount[] = [
  { id: "acc-helio", name: "Helio Systems", industry: "Energy Software", city: "Hamburg", country: "DE", health: "strong" },
  { id: "acc-lumen", name: "Lumen Works", industry: "Manufacturing", city: "Berlin", country: "DE", health: "steady" },
  { id: "acc-nova", name: "Nova Retail Group", industry: "Retail", city: "München", country: "DE", health: "watch" }
];

export const pipelineContacts: PipelineContact[] = [
  { id: "con-amira", accountId: "acc-helio", firstName: "Amira", lastName: "Sayed", email: "amira.sayed@helio.example", title: "VP Operations" },
  { id: "con-tom", accountId: "acc-lumen", firstName: "Tom", lastName: "Keller", email: "tom.keller@lumen.example", title: "Head of Sales" },
  { id: "con-elena", accountId: "acc-nova", firstName: "Elena", lastName: "Voss", email: "elena.voss@nova.example", title: "Procurement Lead" }
];

export const pipelineStages: PipelineStage[] = [
  {
    id: "stage-discovery",
    name: "Discovery",
    sortOrder: 1,
    probability: 20,
    color: "#0e6a71",
    wipLimit: 8,
    exitCriteria: ["Pain confirmed", "Decision process known"],
    transitionStartCriteria: "Pain and buying process are documented. Primary contact has agreed to a next meeting.",
    transitionAgentTodos: [
      "Vollständige Kontaktdaten recherchieren: E-Mail, Telefon, Rolle, LinkedIn und Unternehmenskontext.",
      "Buying Trigger, Pain, Dringlichkeit und aktuellen Workaround identifizieren.",
      "Stärksten Touchpoint für die Ansprache bestimmen und Outreach-Winkel skizzieren.",
      "Entscheider bestätigen und nächsten Termin sichern."
    ],
    transitionAgentPrompt: "Qualify the lead, summarize the pain, identify the buying process, and prepare the record for Qualified."
  },
  {
    id: "stage-qualified",
    name: "Qualified",
    sortOrder: 2,
    probability: 45,
    color: "#0a4c58",
    wipLimit: 6,
    exitCriteria: ["Budget range captured", "Primary contact mapped"],
    transitionStartCriteria: "Budget range, buying committee, and value hypothesis are present.",
    transitionAgentTodos: [
      "Budgetrahmen, Buying Committee und kommerziellen Fit validieren.",
      "Use Case, Erfolgsmetrik und messbaren Geschäftswert qualifizieren.",
      "Technische, Security-, Legal- und Procurement-Blocker mappen.",
      "Proposal-Scope und konkrete Next-Step-Empfehlung vorbereiten."
    ],
    transitionAgentPrompt: "Validate the business case, draft the proposal outline, and collect missing pricing or procurement details."
  },
  {
    id: "stage-proposal",
    name: "Proposal",
    sortOrder: 3,
    probability: 70,
    color: "#b17619",
    wipLimit: 5,
    exitCriteria: ["Proposal sent", "Procurement owner known"],
    transitionStartCriteria: "Proposal was sent and legal/procurement owner is known.",
    transitionAgentTodos: [
      "Versand des Angebots an alle relevanten Stakeholder bestätigen.",
      "Legal-, Procurement- und Security-Fragen mit Owner und Deadline nachhalten.",
      "Einwandbehandlung, ROI-Zusammenfassung und Closing-Plan vorbereiten.",
      "Entscheidungsdatum, Unterzeichner und finale kommerzielle Konditionen bestätigen."
    ],
    transitionAgentPrompt: "Follow up on proposal risks, resolve blockers, and prepare close plan for Won."
  },
  {
    id: "stage-won",
    name: "Won",
    sortOrder: 4,
    probability: 100,
    color: "#297a56",
    wipLimit: 99,
    exitCriteria: ["Contract signed", "Kickoff scheduled"],
    transitionStartCriteria: "Contract is signed and kickoff is scheduled.",
    transitionAgentTodos: [
      "Unterschriebenen Vertrag, Billing-Daten und Kickoff-Owner prüfen.",
      "Customer-Success-Handoff mit Zusagen und Risiken erstellen.",
      "Kickoff terminieren und Onboarding-Agenda vorbereiten.",
      "Erste Implementierungsaufgaben und internes Owner-Handoff anlegen."
    ],
    transitionAgentPrompt: "Create customer-success handoff, summarize commitments, and schedule onboarding."
  }
];

export const pipelineTransitionRuns: PipelineTransitionRun[] = [
  {
    id: "run-orbis",
    recordId: "opp-orbis",
    fromStageId: "stage-qualified",
    toStageId: "stage-proposal",
    status: "running",
    progress: 42,
    criteriaSnapshot: pipelineStages[1].transitionStartCriteria,
    agentTodoSnapshot: pipelineStages[1].transitionAgentTodos,
    agentPromptSnapshot: pipelineStages[1].transitionAgentPrompt,
    log: [
      { at: "2026-04-29T08:05:00.000Z", level: "info", message: "Transition Qualified -> Proposal started." },
      { at: "2026-04-29T08:08:00.000Z", level: "info", message: "Stakeholder map confirmed: Elena owns procurement, CTO reviews security." },
      { at: "2026-04-29T08:14:00.000Z", level: "warn", message: "Waiting for final security questionnaire attachment." }
    ]
  },
  {
    id: "run-fieldops",
    recordId: "opp-fieldops",
    fromStageId: "stage-proposal",
    toStageId: "stage-won",
    status: "running",
    progress: 18,
    criteriaSnapshot: pipelineStages[2].transitionStartCriteria,
    agentTodoSnapshot: pipelineStages[2].transitionAgentTodos,
    agentPromptSnapshot: pipelineStages[2].transitionAgentPrompt,
    log: [
      { at: "2026-05-04T09:35:00.000Z", level: "info", message: "Won handoff started after procurement confirmation." }
    ]
  }
];

export const pipelineOpportunities: PipelineOpportunity[] = [
  {
    id: "opp-nova",
    accountId: "acc-nova",
    primaryContactId: "con-elena",
    stageId: "stage-discovery",
    name: "Nova supplier portal intake",
    amount: 19000,
    currency: "EUR",
    closeDate: "2026-06-28",
    status: "open",
    source: "Event",
    forecastCategory: "pipeline",
    nextStep: "Confirm procurement owner and buying process",
    nextStepDueAt: "2026-05-02T10:00:00.000Z",
    lastActivityAt: "2026-04-20T10:15:00.000Z",
    expectedDecisionDate: "2026-06-14",
    products: ["Contact Management"],
    tags: ["stale", "procurement"],
    ownerId: "usr-jonas",
    transitionReadiness: "blocked",
    transitionBlockers: ["Decision process unknown"]
  },
  {
    id: "opp-atlas",
    accountId: "acc-lumen",
    primaryContactId: "con-tom",
    stageId: "stage-qualified",
    name: "Atlas mobility partner CRM",
    amount: 42000,
    currency: "EUR",
    closeDate: "2026-07-12",
    status: "open",
    source: "Outbound",
    forecastCategory: "pipeline",
    nextStep: "Qualify integration requirements",
    nextStepDueAt: "2026-05-01T14:30:00.000Z",
    lastActivityAt: "2026-04-26T09:20:00.000Z",
    expectedDecisionDate: "2026-06-24",
    products: ["API", "Sales CRM"],
    tags: ["integration"],
    ownerId: "usr-lea",
    transitionReadiness: "running",
    transitionBlockers: []
  },
  {
    id: "opp-helio",
    accountId: "acc-helio",
    primaryContactId: "con-amira",
    stageId: "stage-proposal",
    name: "Helio field rollout",
    amount: 84000,
    currency: "EUR",
    closeDate: "2026-05-21",
    status: "open",
    source: "Expansion",
    forecastCategory: "commit",
    nextStep: "Send revised data-processing appendix",
    nextStepDueAt: "2026-04-30T09:00:00.000Z",
    lastActivityAt: "2026-04-28T13:45:00.000Z",
    expectedDecisionDate: "2026-05-10",
    products: ["Sales CRM", "Automation"],
    tags: ["legal-review", "expansion"],
    ownerId: "usr-lea",
    transitionReadiness: "blocked",
    transitionBlockers: ["Legal owner not confirmed"]
  },
  {
    id: "opp-orbis",
    accountId: "acc-nova",
    primaryContactId: "con-elena",
    stageId: "stage-proposal",
    name: "Orbis analytics sales desk",
    amount: 57000,
    currency: "EUR",
    closeDate: "2026-05-30",
    status: "open",
    source: "Partner",
    forecastCategory: "best_case",
    nextStep: "Send security questionnaire answers",
    nextStepDueAt: "2026-04-30T16:00:00.000Z",
    lastActivityAt: "2026-04-29T08:00:00.000Z",
    expectedDecisionDate: "2026-05-19",
    products: ["Sales CRM", "Email Automation"],
    tags: ["security-review"],
    ownerId: "usr-jonas",
    transitionReadiness: "running",
    transitionBlockers: [],
    activeRun: pipelineTransitionRuns[0]
  },
  {
    id: "opp-lumen",
    accountId: "acc-lumen",
    primaryContactId: "con-tom",
    stageId: "stage-proposal",
    name: "Lumen sales workspace",
    amount: 36000,
    currency: "EUR",
    closeDate: "2026-06-04",
    status: "open",
    source: "Website",
    forecastCategory: "best_case",
    nextStep: "Run admin-controls demo with Tom",
    nextStepDueAt: "2026-05-03T11:00:00.000Z",
    lastActivityAt: "2026-04-27T16:00:00.000Z",
    expectedDecisionDate: "2026-05-22",
    products: ["Sales CRM"],
    tags: ["demo", "admin-controls"],
    ownerId: "usr-jonas",
    transitionReadiness: "running",
    transitionBlockers: []
  },
  {
    id: "opp-fieldops",
    accountId: "acc-helio",
    primaryContactId: "con-amira",
    stageId: "stage-won",
    name: "FieldOps lead routing",
    amount: 31000,
    currency: "EUR",
    closeDate: "2026-05-18",
    status: "open",
    source: "Referral",
    forecastCategory: "commit",
    nextStep: "Confirm rollout owner and start date",
    nextStepDueAt: "2026-05-04T09:30:00.000Z",
    lastActivityAt: "2026-04-29T09:05:00.000Z",
    expectedDecisionDate: "2026-05-12",
    products: ["Lead Automation"],
    tags: ["routing", "implementation"],
    ownerId: "usr-jonas",
    transitionReadiness: "running",
    transitionBlockers: [],
    activeRun: pipelineTransitionRuns[1]
  }
];

export const pipelineTasks: PipelineTask[] = [
  { id: "task-helio-redlines", subject: "Follow up on Helio proposal redlines", status: "todo", dueAt: "2026-04-30T09:00:00.000Z", relatedType: "opportunity", relatedId: "opp-helio", priority: "high" },
  { id: "task-fieldops-import", subject: "Review imported FieldOps lead fields", status: "todo", dueAt: "2026-04-29T15:00:00.000Z", relatedType: "lead", relatedId: "lead-fieldops", priority: "medium" },
  { id: "task-nova-procurement", subject: "Verify Nova alternate procurement contact", status: "done", dueAt: "2026-05-02T10:00:00.000Z", relatedType: "contact", relatedId: "con-elena", priority: "medium" }
];

export const pipelineTransitionMessages: PipelineTransitionMessage[] = [
  {
    id: "msg-orbis-system",
    runId: "run-orbis",
    role: "system",
    body: "Gate passed: budget, stakeholder and success metric are present. Transition run created from persisted stage configuration."
  },
  {
    id: "msg-orbis-agent",
    runId: "run-orbis",
    role: "agent",
    body: "I am preparing the Proposal checklist. Current blocker: one security questionnaire attachment is missing."
  }
];

export const pipelineCrmDataset: PipelineDataset = {
  users: pipelineUsers,
  accounts: pipelineAccounts,
  contacts: pipelineContacts,
  stages: pipelineStages,
  opportunities: pipelineOpportunities,
  tasks: pipelineTasks,
  transitionRuns: pipelineTransitionRuns,
  transitionMessages: pipelineTransitionMessages
};

export function pipelineMoney(value: number, currency = "EUR", locale: PipelineLocale = "de") {
  return new Intl.NumberFormat(locale === "en" ? "en-US" : "de-DE", {
    currency,
    maximumFractionDigits: 0,
    style: "currency"
  }).format(value);
}

export function pipelineDateTime(value: string, locale: PipelineLocale = "de") {
  return new Intl.DateTimeFormat(locale === "en" ? "en-US" : "de-DE", {
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    timeZone: "UTC"
  }).format(new Date(value));
}

export function pipelineDate(value: string, locale: PipelineLocale = "de") {
  return new Intl.DateTimeFormat(locale === "en" ? "en-US" : "de-DE", {
    day: "2-digit",
    month: "2-digit",
    timeZone: "UTC",
    year: "numeric"
  }).format(new Date(value));
}

export function pipelineStatus(locale: PipelineLocale, value: string) {
  const labels: Record<PipelineLocale, Record<string, string>> = {
    de: {
      blocked: "blockiert",
      completed: "erledigt",
      failed: "fehlgeschlagen",
      idle: "idle",
      ready: "bereit",
      running: "läuft"
    },
    en: {
      blocked: "blocked",
      completed: "done",
      failed: "failed",
      idle: "idle",
      ready: "ready",
      running: "running"
    }
  };

  return labels[locale][value] ?? value;
}

export function localizePipelineStageName(locale: PipelineLocale, name: string) {
  if (locale !== "de") return name;
  if (name === "Qualified") return "Qualifiziert";
  if (name === "Proposal") return "Angebot";
  if (name === "Won") return "Gewonnen";
  return name;
}

export function localizePipelineStageLines(locale: PipelineLocale, stage: PipelineStage, field: "exitCriteria" | "transitionAgentTodos") {
  if (locale !== "de") return stage[field];
  const defaults: Record<string, Partial<Record<typeof field, string[]>>> = {
    Discovery: {
      exitCriteria: ["Pain bestätigt", "Entscheidungsprozess bekannt"],
      transitionAgentTodos: pipelineStages[0].transitionAgentTodos
    },
    Qualified: {
      exitCriteria: ["Budgetrahmen erfasst", "Hauptkontakt zugeordnet"],
      transitionAgentTodos: pipelineStages[1].transitionAgentTodos
    },
    Proposal: {
      exitCriteria: ["Angebot versendet", "Procurement-Owner bekannt"],
      transitionAgentTodos: pipelineStages[2].transitionAgentTodos
    },
    Won: {
      exitCriteria: ["Vertrag unterschrieben", "Kickoff terminiert"],
      transitionAgentTodos: pipelineStages[3].transitionAgentTodos
    }
  };

  return defaults[stage.name]?.[field] ?? stage[field];
}

export function localizePipelineStageText(locale: PipelineLocale, stage: PipelineStage, field: "transitionStartCriteria" | "transitionAgentPrompt") {
  if (locale !== "de") return stage[field];
  const defaults: Record<string, Partial<Record<typeof field, string>>> = {
    Discovery: {
      transitionAgentPrompt: "Qualifiziere den Lead, fasse den Pain zusammen, identifiziere den Buying-Prozess und bereite den Datensatz fuer Qualifiziert vor.",
      transitionStartCriteria: "Transition starten, wenn Pain, Buying Trigger, Decision Owner und naechster Termin dokumentiert sind. Blockieren, wenn Company Fit oder Kontaktdaten unvollstaendig sind."
    },
    Proposal: {
      transitionAgentPrompt: "Halte Angebotsrisiken nach, loese Blocker und bereite den Closing-Plan fuer Gewonnen vor.",
      transitionStartCriteria: "Transition starten, wenn das Angebot versendet ist, Procurement Owner bekannt ist, Legal-Blocker gelistet sind und ein Entscheidungsdatum bestaetigt ist."
    },
    Qualified: {
      transitionAgentPrompt: "Validiere den Business Case, skizziere das Angebot und sammle fehlende Pricing- oder Procurement-Details.",
      transitionStartCriteria: "Transition starten, wenn Budgetrahmen, primaerer Stakeholder, Erfolgsmetrik und Angebotsumfang bekannt sind. Blockieren, wenn Security- oder Legal-Owner unbekannt sind."
    },
    Won: {
      transitionAgentPrompt: "Erstelle das Customer-Success-Handoff, fasse Zusagen zusammen und terminiere das Onboarding.",
      transitionStartCriteria: "Transition starten, wenn Vertrag unterschrieben ist und Kickoff Owner, Kickoff-Datum und Billing-Handoff dokumentiert sind."
    }
  };

  return defaults[stage.name]?.[field] ?? stage[field];
}
