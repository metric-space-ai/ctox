import {
  operationsActionItems,
  operationsCustomers,
  operationsDecisions,
  operationsDocuments,
  operationsDocumentTemplates,
  operationsKnowledgeItems,
  operationsMeetings,
  operationsMilestones,
  operationsPeople,
  operationsProjects,
  operationsWorkItems,
  type Localized,
  type OperationsActionItem,
  type OperationsDecision,
  type OperationsKnowledgeItem,
  type OperationsMilestone,
  type OperationsMeeting,
  type OperationsProject,
  type OperationsWorkItem
} from "./operations-seed";
import { getCtoxKnowledgeStore, type CtoxKnowledgeStore } from "./ctox-knowledge-store";

export type OperationsBundle = {
  source: "seed" | "postgres";
  company: {
    name: string;
    segment: string;
    products: string[];
  };
  people: typeof operationsPeople;
  customers: typeof operationsCustomers;
  projects: typeof operationsProjects;
  workItems: typeof operationsWorkItems;
  milestones: typeof operationsMilestones;
  knowledgeItems: typeof operationsKnowledgeItems;
  meetings: typeof operationsMeetings;
  decisions: typeof operationsDecisions;
  actionItems: typeof operationsActionItems;
  documents: typeof operationsDocuments;
  documentTemplates: typeof operationsDocumentTemplates;
  ctoxKnowledge: CtoxKnowledgeStore;
};

export type OperationsResource = Exclude<keyof OperationsBundle, "source" | "company">;

const seedCtoxKnowledge: CtoxKnowledgeStore = {
  source: "seed",
  skills: [],
  mainSkills: [],
  skillbooks: [],
  runbooks: [],
  runbookItems: [],
  sourceBindings: [],
  counts: {
    systemSkills: 0,
    skills: 0,
    mainSkills: 0,
    skillbooks: 0,
    runbooks: 0,
    runbookItems: 0
  }
};

const seedBundle: OperationsBundle = {
  source: "seed",
  company: {
    name: "Acme Operations",
    segment: "B2B SaaS / Service Delivery",
    products: ["Core Platform", "Implementation Service", "Managed Support", "Reporting Add-on"]
  },
  people: operationsPeople,
  customers: operationsCustomers,
  projects: operationsProjects,
  workItems: operationsWorkItems,
  milestones: operationsMilestones,
  knowledgeItems: operationsKnowledgeItems,
  meetings: operationsMeetings,
  decisions: operationsDecisions,
  actionItems: operationsActionItems,
  documents: operationsDocuments,
  documentTemplates: operationsDocumentTemplates,
  ctoxKnowledge: seedCtoxKnowledge
};

export async function getOperationsBundle(): Promise<OperationsBundle> {
  const ctoxKnowledge = await getCtoxKnowledgeStore();
  const withCtoxKnowledge = { ...seedBundle, ctoxKnowledge };
  if (!shouldUsePostgres()) return withCtoxKnowledge;

  try {
    const db = await import("@ctox-business/db/operations");
    const [
      projectRows,
      workItemRows,
      milestoneRows,
      knowledgeRows,
      meetingRows,
      decisionRows,
      actionRows
    ] = await Promise.all([
      db.listOperationsProjects(),
      db.listOperationsWorkItems(),
      db.listOperationsMilestones(),
      db.listOperationsKnowledgeItems(),
      db.listOperationsMeetings(),
      db.listOperationsDecisions(),
      db.listOperationsActionItems()
    ]);
    const shouldSeed = projectRows.length === 0 && workItemRows.length === 0 && shouldAutoSeedPostgres();
    if (shouldSeed) {
      await db.seedOperationsData(seedBundle);
      return getOperationsBundle();
    }

    return {
      ...withCtoxKnowledge,
      source: "postgres",
      projects: projectRows.map(rowToProject),
      workItems: workItemRows.map(rowToWorkItem),
      milestones: milestoneRows.map(rowToMilestone),
      knowledgeItems: knowledgeRows.map(rowToKnowledgeItem),
      meetings: meetingRows.map(rowToMeeting),
      decisions: decisionRows.map(rowToDecision),
      actionItems: actionRows.map(rowToActionItem)
    };
  } catch (error) {
    console.warn("Falling back to Operations seed data.", error);
    return withCtoxKnowledge;
  }
}

export async function getOperationsResource(resource: string) {
  const bundle = await getOperationsBundle();
  const resourceMap = {
    "action-items": bundle.actionItems,
    customers: bundle.customers,
    decisions: bundle.decisions,
    documents: bundle.documents,
    "document-templates": bundle.documentTemplates,
    knowledge: bundle.knowledgeItems,
    meetings: bundle.meetings,
    milestones: bundle.milestones,
    people: bundle.people,
    projects: bundle.projects,
    "work-items": bundle.workItems
  };

  return resourceMap[resource as keyof typeof resourceMap] ?? null;
}

function shouldUsePostgres() {
  const value = process.env.DATABASE_URL;
  return Boolean(value && !value.includes("user:password@localhost"));
}

function shouldAutoSeedPostgres() {
  return process.env.CTOX_BUSINESS_AUTO_SEED !== "false";
}

function rowToProject(row: {
  externalId: string;
  code: string;
  name: string;
  ownerId: string;
  customerId: string | null;
  health: string;
  progress: number;
  startDate: string;
  endDate: string;
  nextMilestone: string;
  summaryJson: string;
  linkedModulesJson: string;
}): OperationsProject {
  return {
    id: row.externalId,
    code: row.code,
    name: row.name,
    ownerId: row.ownerId,
    customerId: row.customerId ?? undefined,
    health: parseEnum(row.health, ["Green", "Amber", "Red"], "Green"),
    progress: row.progress,
    activeItems: 0,
    nextMilestone: row.nextMilestone,
    start: row.startDate,
    end: row.endDate,
    summary: parseLocalized(row.summaryJson),
    linkedModules: parseJson(row.linkedModulesJson, [])
  };
}

function rowToWorkItem(row: {
  externalId: string;
  projectExternalId: string;
  subject: string;
  type: string;
  status: string;
  priority: string;
  assigneeId: string;
  dueDate: string;
  estimate: number;
  descriptionJson: string;
  linkedKnowledgeIdsJson: string;
}): OperationsWorkItem {
  return {
    id: row.externalId,
    projectId: row.projectExternalId,
    subject: row.subject,
    type: parseEnum(row.type, ["Feature", "Bug", "Task", "Decision", "Checklist", "Document"], "Task"),
    status: parseEnum(row.status, ["Backlog", "Ready", "In progress", "Review", "Done"], "Backlog"),
    priority: parseEnum(row.priority, ["Low", "Normal", "High", "Urgent"], "Normal"),
    assigneeId: row.assigneeId,
    due: row.dueDate,
    estimate: row.estimate,
    description: parseLocalized(row.descriptionJson),
    linkedKnowledgeIds: parseJson(row.linkedKnowledgeIdsJson, [])
  };
}

function rowToKnowledgeItem(row: {
  externalId: string;
  projectExternalId: string;
  title: string;
  kind: string;
  updatedOn: string;
  ownerId: string;
  linkedWorkItemIdsJson: string;
  sectionsJson: string;
}): OperationsKnowledgeItem {
  return {
    id: row.externalId,
    title: row.title,
    projectId: row.projectExternalId,
    kind: parseEnum(row.kind === "Runbook" ? "Runbook" : "Skillbook", ["Skillbook", "Runbook"], "Skillbook"),
    updated: row.updatedOn,
    ownerId: row.ownerId,
    linkedItems: parseJson(row.linkedWorkItemIdsJson, []),
    sections: parseJson(row.sectionsJson, [])
  };
}

function rowToMilestone(row: {
  externalId: string;
  projectExternalId: string;
  title: string;
  dueDate: string;
  status: string;
}): OperationsMilestone {
  return {
    id: row.externalId,
    projectId: row.projectExternalId,
    title: row.title,
    date: row.dueDate,
    status: parseEnum(row.status, ["Upcoming", "At risk", "Complete"], "Upcoming")
  };
}

function rowToMeeting(row: {
  externalId: string;
  projectExternalId: string;
  title: string;
  startsAt: string;
  facilitatorId: string;
  agendaJson: string;
  decisionIdsJson: string;
  actionItemIdsJson: string;
}): OperationsMeeting {
  return {
    id: row.externalId,
    title: row.title,
    projectId: row.projectExternalId,
    date: row.startsAt,
    facilitatorId: row.facilitatorId,
    agenda: parseJson(row.agendaJson, []),
    decisions: parseJson(row.decisionIdsJson, []),
    actionItems: parseJson(row.actionItemIdsJson, [])
  };
}

function rowToDecision(row: {
  externalId: string;
  meetingExternalId: string;
  projectExternalId: string;
  textJson: string;
  linkedWorkItemIdsJson: string;
}): OperationsDecision {
  return {
    id: row.externalId,
    meetingId: row.meetingExternalId,
    projectId: row.projectExternalId,
    text: parseLocalized(row.textJson),
    linkedWorkItemIds: parseJson(row.linkedWorkItemIdsJson, [])
  };
}

function rowToActionItem(row: {
  externalId: string;
  ownerId: string;
  dueDate: string;
  textJson: string;
  workItemExternalId: string | null;
}): OperationsActionItem {
  return {
    id: row.externalId,
    ownerId: row.ownerId,
    due: row.dueDate,
    text: parseLocalized(row.textJson),
    workItemId: row.workItemExternalId ?? undefined
  };
}

function parseLocalized(value: string): Localized {
  return parseJson(value, { en: value, de: value });
}

function parseJson<T>(value: string, fallback: T): T {
  try {
    return JSON.parse(value) as T;
  } catch {
    return fallback;
  }
}

function parseEnum<T extends string>(value: string, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? value as T : fallback;
}
