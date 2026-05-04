import { createBusinessDb } from "./client";
import {
  operationsActionItems,
  operationsDecisions,
  operationsKnowledgeItems,
  operationsMeetings,
  operationsMilestones,
  operationsProjects,
  operationsWorkItems
} from "./schema";

type Localized = Record<"en" | "de", string>;

type OperationsSeedBundle = {
  projects: Array<{
    code: string;
    customerId?: string;
    end: string;
    health: string;
    id: string;
    linkedModules: string[];
    name: string;
    nextMilestone: string;
    ownerId: string;
    progress: number;
    start: string;
    summary: Localized;
  }>;
  workItems: Array<{
    assigneeId: string;
    description: Localized;
    due: string;
    estimate: number;
    id: string;
    linkedKnowledgeIds: string[];
    priority: string;
    projectId: string;
    status: string;
    subject: string;
    type: string;
  }>;
  milestones: Array<{
    date: string;
    id: string;
    projectId: string;
    status: string;
    title: string;
  }>;
  knowledgeItems: Array<{
    id: string;
    kind: string;
    linkedItems: string[];
    ownerId: string;
    projectId: string;
    sections: Array<{ title: Localized; body: Localized }>;
    title: string;
    updated: string;
  }>;
  meetings: Array<{
    actionItems: string[];
    agenda: Localized[];
    date: string;
    decisions: string[];
    facilitatorId: string;
    id: string;
    projectId: string;
    title: string;
  }>;
  decisions: Array<{
    id: string;
    linkedWorkItemIds: string[];
    meetingId: string;
    projectId: string;
    text: Localized;
  }>;
  actionItems: Array<{
    due: string;
    id: string;
    ownerId: string;
    text: Localized;
    workItemId?: string;
  }>;
};

export async function listOperationsProjects(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsProjects);
}

export async function listOperationsWorkItems(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsWorkItems);
}

export async function listOperationsKnowledgeItems(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsKnowledgeItems);
}

export async function listOperationsMilestones(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsMilestones);
}

export async function listOperationsMeetings(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsMeetings);
}

export async function listOperationsDecisions(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsDecisions);
}

export async function listOperationsActionItems(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(operationsActionItems);
}

export async function seedOperationsData(seed: OperationsSeedBundle, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);

  await db.transaction(async (tx) => {
    for (const project of seed.projects) {
      await tx.insert(operationsProjects).values({
        externalId: project.id,
        code: project.code,
        name: project.name,
        ownerId: project.ownerId,
        customerId: project.customerId ?? null,
        health: project.health,
        progress: project.progress,
        startDate: project.start,
        endDate: project.end,
        nextMilestone: project.nextMilestone,
        summaryJson: stringify(project.summary),
        linkedModulesJson: stringify(project.linkedModules),
        ctoxSyncKey: syncKey("project", project.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsProjects.externalId,
        set: {
          code: project.code,
          name: project.name,
          ownerId: project.ownerId,
          customerId: project.customerId ?? null,
          health: project.health,
          progress: project.progress,
          startDate: project.start,
          endDate: project.end,
          nextMilestone: project.nextMilestone,
          summaryJson: stringify(project.summary),
          linkedModulesJson: stringify(project.linkedModules),
          ctoxSyncKey: syncKey("project", project.id),
          updatedAt: new Date()
        }
      });
    }

    for (const item of seed.workItems) {
      await tx.insert(operationsWorkItems).values({
        externalId: item.id,
        projectExternalId: item.projectId,
        subject: item.subject,
        type: item.type,
        status: item.status,
        priority: item.priority,
        assigneeId: item.assigneeId,
        dueDate: item.due,
        estimate: item.estimate,
        descriptionJson: stringify(item.description),
        linkedKnowledgeIdsJson: stringify(item.linkedKnowledgeIds),
        ctoxSyncKey: syncKey("work-item", item.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsWorkItems.externalId,
        set: {
          projectExternalId: item.projectId,
          subject: item.subject,
          type: item.type,
          status: item.status,
          priority: item.priority,
          assigneeId: item.assigneeId,
          dueDate: item.due,
          estimate: item.estimate,
          descriptionJson: stringify(item.description),
          linkedKnowledgeIdsJson: stringify(item.linkedKnowledgeIds),
          ctoxSyncKey: syncKey("work-item", item.id),
          updatedAt: new Date()
        }
      });
    }

    for (const milestone of seed.milestones) {
      await tx.insert(operationsMilestones).values({
        externalId: milestone.id,
        projectExternalId: milestone.projectId,
        title: milestone.title,
        dueDate: milestone.date,
        status: milestone.status,
        ctoxSyncKey: syncKey("milestone", milestone.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsMilestones.externalId,
        set: {
          projectExternalId: milestone.projectId,
          title: milestone.title,
          dueDate: milestone.date,
          status: milestone.status,
          ctoxSyncKey: syncKey("milestone", milestone.id),
          updatedAt: new Date()
        }
      });
    }

    for (const item of seed.knowledgeItems) {
      await tx.insert(operationsKnowledgeItems).values({
        externalId: item.id,
        projectExternalId: item.projectId,
        title: item.title,
        kind: item.kind,
        ownerId: item.ownerId,
        sectionsJson: stringify(item.sections),
        linkedWorkItemIdsJson: stringify(item.linkedItems),
        updatedOn: item.updated,
        ctoxKnowledgeKey: syncKey("knowledge", item.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsKnowledgeItems.externalId,
        set: {
          projectExternalId: item.projectId,
          title: item.title,
          kind: item.kind,
          ownerId: item.ownerId,
          sectionsJson: stringify(item.sections),
          linkedWorkItemIdsJson: stringify(item.linkedItems),
          updatedOn: item.updated,
          ctoxKnowledgeKey: syncKey("knowledge", item.id),
          updatedAt: new Date()
        }
      });
    }

    for (const meeting of seed.meetings) {
      await tx.insert(operationsMeetings).values({
        externalId: meeting.id,
        projectExternalId: meeting.projectId,
        title: meeting.title,
        startsAt: meeting.date,
        facilitatorId: meeting.facilitatorId,
        agendaJson: stringify(meeting.agenda),
        decisionIdsJson: stringify(meeting.decisions),
        actionItemIdsJson: stringify(meeting.actionItems),
        ctoxSyncKey: syncKey("meeting", meeting.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsMeetings.externalId,
        set: {
          projectExternalId: meeting.projectId,
          title: meeting.title,
          startsAt: meeting.date,
          facilitatorId: meeting.facilitatorId,
          agendaJson: stringify(meeting.agenda),
          decisionIdsJson: stringify(meeting.decisions),
          actionItemIdsJson: stringify(meeting.actionItems),
          ctoxSyncKey: syncKey("meeting", meeting.id),
          updatedAt: new Date()
        }
      });
    }

    for (const decision of seed.decisions) {
      await tx.insert(operationsDecisions).values({
        externalId: decision.id,
        meetingExternalId: decision.meetingId,
        projectExternalId: decision.projectId,
        textJson: stringify(decision.text),
        linkedWorkItemIdsJson: stringify(decision.linkedWorkItemIds),
        ctoxSyncKey: syncKey("decision", decision.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsDecisions.externalId,
        set: {
          meetingExternalId: decision.meetingId,
          projectExternalId: decision.projectId,
          textJson: stringify(decision.text),
          linkedWorkItemIdsJson: stringify(decision.linkedWorkItemIds),
          ctoxSyncKey: syncKey("decision", decision.id),
          updatedAt: new Date()
        }
      });
    }

    for (const actionItem of seed.actionItems) {
      await tx.insert(operationsActionItems).values({
        externalId: actionItem.id,
        ownerId: actionItem.ownerId,
        dueDate: actionItem.due,
        textJson: stringify(actionItem.text),
        workItemExternalId: actionItem.workItemId ?? null,
        ctoxSyncKey: syncKey("action-item", actionItem.id),
        updatedAt: new Date()
      }).onConflictDoUpdate({
        target: operationsActionItems.externalId,
        set: {
          ownerId: actionItem.ownerId,
          dueDate: actionItem.due,
          textJson: stringify(actionItem.text),
          workItemExternalId: actionItem.workItemId ?? null,
          ctoxSyncKey: syncKey("action-item", actionItem.id),
          updatedAt: new Date()
        }
      });
    }
  });
}

function stringify(value: unknown) {
  return JSON.stringify(value);
}

function syncKey(type: string, id: string) {
  return `operations:${type}:${id}`;
}
