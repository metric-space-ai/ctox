import { resolveLocale, type WorkSurfacePanelState } from "@ctox-business/ui";
import {
  operationsStatusColumns,
  text,
  type OperationsActionItem,
  type OperationsKnowledgeItem,
  type OperationsMeeting,
  type OperationsMilestone,
  type OperationsProject,
  type OperationsWorkItem,
  type SupportedLocale
} from "../lib/operations-seed";
import { getOperationsBundle, type OperationsBundle } from "../lib/operations-store";
import { OperationsBoardView as OperationsBoardSliceView } from "./operations/boards-view";
import { OperationsKnowledgeView as OperationsKnowledgeSliceView } from "./operations/knowledge-view";
import { OperationsMeetingsView as OperationsMeetingsSliceView } from "./operations/meetings-view";
import { OperationsCreateForm, OperationsKnowledgeCreateForm, OperationsQueueButton, OperationsWorkItemEditor } from "./operations/operations-actions";
import { OperationsPlanningView as OperationsPlanningSliceView } from "./operations/planning-view";
import { OperationsProjectsView } from "./operations/projects-view";
import { OperationsWorkItemsView } from "./operations/work-items-view";
import { WorkforceWorkbench } from "./workforce-workbench";
import { getWorkforceSnapshot } from "../lib/workforce-runtime";

type QueryState = {
  drawer?: string;
  filePath?: string;
  group?: string;
  locale?: string;
  panel?: string;
  recordId?: string;
  runbookId?: string;
  skillbookId?: string;
  skillId?: string;
  theme?: string;
};

export async function OperationsWorkspace({
  submoduleId,
  query
}: {
  submoduleId: string;
  query: QueryState;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = operationsCopy[locale];
  const data = await getOperationsBundle();
  const normalizedSubmodule = submoduleId === "wiki" ? "knowledge" : submoduleId;

  if (normalizedSubmodule === "boards") {
    return <OperationsBoardSliceView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  }

  if (normalizedSubmodule === "planning") {
    return <OperationsPlanningSliceView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  }

  if (normalizedSubmodule === "knowledge") {
    return <OperationsKnowledgeSliceView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  }

  if (normalizedSubmodule === "meetings") {
    return <OperationsMeetingsSliceView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  }

  if (normalizedSubmodule === "work-items") {
    return <OperationsWorkItemsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
  }

  if (normalizedSubmodule === "workforce") {
    const workforce = await getWorkforceSnapshot();
    return <WorkforceWorkbench query={query} snapshot={workforce} />;
  }

  return <OperationsProjectsView copy={copy} data={data} locale={locale} query={query} submoduleId={submoduleId} />;
}

export async function OperationsPanel({
  panelState,
  query,
  submoduleId
}: {
  panelState?: WorkSurfacePanelState;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = operationsCopy[locale];
  const data = await getOperationsBundle();
  const panel = panelState?.panel;
  const recordId = panelState?.recordId;
  const workItem = data.workItems.find((item) => item.id === recordId);
  const project = data.projects.find((item) => item.id === recordId) ?? (workItem ? data.projects.find((item) => item.id === workItem.projectId) : undefined);
  const meeting = data.meetings.find((item) => item.id === recordId);
  const knowledge = data.knowledgeItems.find((item) => item.id === recordId);
  const selectedKnowledgeSkill = data.ctoxKnowledge.skills.find((item) => item.id === query.skillId);

  if (panel === "new" && recordId?.startsWith("new-ctox-")) {
    const element = resolveKnowledgeElement(recordId);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={copy.newKnowledgeElement} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{copy.newKnowledgeElementDescription}</p>
        <OperationsKnowledgeCreateForm
          cluster={selectedKnowledgeSkill?.cluster}
          element={element}
          filePath={query.filePath}
          group={query.group}
          queueLabel={copy.queueCreate}
          skillId={selectedKnowledgeSkill?.id ?? query.skillId}
          skillTitle={selectedKnowledgeSkill?.title}
          sourcePath={selectedKnowledgeSkill?.sourcePath}
        />
      </div>
    );
  }

  if (panel === "operations-set") {
    const operationsSet = resolveOperationsSet(recordId, data, copy, locale);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={operationsSet.title} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{operationsSet.description}</p>
        <dl className="drawer-facts">
          <div><dt>{copy.items}</dt><dd>{operationsSet.items.length}</dd></div>
          <div><dt>{copy.project}</dt><dd>{operationsSet.resource}</dd></div>
          <div><dt>{copy.updated}</dt><dd>{operationsSet.freshness}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.workSetItems}</h3>
          <div className="ops-mini-list">
            {operationsSet.items.length > 0 ? operationsSet.items.map((item) => (
              <a
                data-context-item
                data-context-label={item.label}
                data-context-module="operations"
                data-context-record-id={item.id}
                data-context-record-type={item.type}
                data-context-submodule={submoduleId}
                href={operationsRecordHref(query, submoduleId, item.panel, item.recordId)}
                key={`${item.type}-${item.id}`}
              >
                {item.label} · {item.meta}
              </a>
            )) : <span>{copy.workSetEmpty}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.syncRail}</h3>
          <OperationsQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Review and synchronize this Operations context set: ${operationsSet.title}.`}
            payload={{ filter: recordId, items: operationsSet.items }}
            recordId={recordId ?? "operations-set"}
            resource={operationsSet.resource}
            title={`Sync Operations set: ${operationsSet.title}`}
          >
            {copy.askCtoxWorkSet}
          </OperationsQueueButton>
        </section>
      </div>
    );
  }

  if (panel === "work-set") {
    const workSet = resolveWorkSet(recordId, data.workItems, copy);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={workSet.title} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{workSet.description}</p>
        <dl className="drawer-facts">
          <div><dt>{copy.items}</dt><dd>{workSet.items.length}</dd></div>
          <div><dt>{copy.urgentItems}</dt><dd>{workSet.items.filter((item) => item.priority === "Urgent").length}</dd></div>
          <div><dt>{copy.review}</dt><dd>{workSet.items.filter((item) => item.status === "Review").length}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.workSetItems}</h3>
          <div className="ops-mini-list">
            {workSet.items.length > 0 ? workSet.items.map((item) => (
              <a
                data-context-item
                data-context-label={item.subject}
                data-context-module="operations"
                data-context-record-id={item.id}
                data-context-record-type="work_item"
                data-context-submodule={submoduleId}
                href={operationsRecordHref(query, submoduleId, "work-item", item.id)}
                key={item.id}
              >
                {item.subject} · {item.status} · {item.priority}
              </a>
            )) : <span>{copy.workSetEmpty}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.syncRail}</h3>
          <OperationsQueueButton
            action="sync"
            className="drawer-primary"
            instruction={`Review and synchronize this Operations work set: ${workSet.title}.`}
            payload={{ filter: recordId, workItems: workSet.items }}
            recordId={recordId ?? "work-set"}
            resource="work-items"
            title={`Sync work set: ${workSet.title}`}
          >
            {copy.askCtoxWorkSet}
          </OperationsQueueButton>
        </section>
      </div>
    );
  }

  if (panel === "new") {
    const resource = resolveNewResource(recordId, submoduleId);

    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={copy.newItem} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{copy.newItemDescription}</p>
        <OperationsCreateForm
          dueLabel={copy.due}
          ownerLabel={copy.owner}
          owners={data.people.map((person) => ({ label: person.name, value: person.id }))}
          projectLabel={copy.project}
          projects={data.projects.map((project) => ({ label: `${project.code} - ${project.name}`, value: project.id }))}
          queueLabel={copy.queueCreate}
          resource={resource}
          subjectLabel={copy.subject}
          subjectPlaceholder={copy.subjectPlaceholder}
        />
      </div>
    );
  }

  if (panel === "meeting" && meeting) {
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={meeting.title} query={query} submoduleId={submoduleId} />
        <dl className="drawer-facts">
          <div><dt>{copy.project}</dt><dd>{data.projects.find((item) => item.id === meeting.projectId)?.name}</dd></div>
          <div><dt>{copy.date}</dt><dd>{meeting.date}</dd></div>
          <div><dt>{copy.decisions}</dt><dd>{meeting.decisions.length}</dd></div>
          <div><dt>{copy.actions}</dt><dd>{meeting.actionItems.length}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.meetingNotes}</h3>
          <div className="ops-mini-list">
            {meeting.agenda.map((agendaItem) => (
              <span key={text(agendaItem, locale)}>{text(agendaItem, locale)}</span>
            ))}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.decisions}</h3>
          <div className="ops-mini-list">
            {meeting.decisions.map((decisionId) => {
              const decision = data.decisions.find((item) => item.id === decisionId);
              return decision ? <span key={decision.id}>{text(decision.text, locale)}</span> : null;
            })}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.actions}</h3>
          <div className="ops-mini-list">
            {meeting.actionItems.map((actionId) => {
              const action = data.actionItems.find((item) => item.id === actionId);
              return action ? <span key={action.id}>{text(action.text, locale)}</span> : null;
            })}
          </div>
          <OperationsQueueButton
            action="extract"
            className="drawer-primary"
            instruction={`Extract decisions and action items from Operations meeting ${meeting.title}.`}
            payload={{ meeting }}
            recordId={meeting.id}
            resource="meetings"
            title={`Extract meeting actions: ${meeting.title}`}
          >
            {copy.askCtoxMeeting}
          </OperationsQueueButton>
        </section>
      </div>
    );
  }

  if (panel === "knowledge" && knowledge) {
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={knowledge.title} query={query} submoduleId={submoduleId} />
        <dl className="drawer-facts">
          <div><dt>{copy.kind}</dt><dd>{knowledge.kind}</dd></div>
          <div><dt>{copy.project}</dt><dd>{data.projects.find((item) => item.id === knowledge.projectId)?.name}</dd></div>
          <div><dt>{copy.updated}</dt><dd>{knowledge.updated}</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.pageSections}</h3>
          <div className="ops-mini-list">
            {knowledge.sections.map((section) => (
              <span key={text(section.title, locale)}>{text(section.title, locale)}: {text(section.body, locale)}</span>
            ))}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.linkedWork}</h3>
          <div className="ops-mini-list">
            {knowledge.linkedItems.map((id) => {
              const item = data.workItems.find((candidate) => candidate.id === id);
              return item ? <span key={id}>{item.subject}</span> : null;
            })}
          </div>
        </section>
      </div>
    );
  }

  if (workItem) {
    return (
      <div className="drawer-content ops-drawer">
        <DrawerHeader title={workItem.subject} query={query} submoduleId={submoduleId} />
        <p className="drawer-description">{text(workItem.description, locale)}</p>
        <dl className="drawer-facts">
          <div><dt>{copy.id}</dt><dd>{workItem.id}</dd></div>
          {workItem.semanticId ? <div><dt>{copy.semanticId}</dt><dd>{workItem.semanticId}</dd></div> : null}
          <div><dt>{copy.project}</dt><dd>{project?.name}</dd></div>
          <div><dt>{copy.status}</dt><dd>{workItem.status}</dd></div>
          <div><dt>{copy.priority}</dt><dd>{workItem.priority}</dd></div>
          <div><dt>{copy.assignee}</dt><dd>{data.people.find((item) => item.id === workItem.assigneeId)?.name}</dd></div>
          <div><dt>{copy.due}</dt><dd>{workItem.due}</dd></div>
          <div><dt>{copy.doneRatio}</dt><dd>{workItem.doneRatio ?? 0}%</dd></div>
        </dl>
        <section className="ops-drawer-section">
          <h3>{copy.relations}</h3>
          <div className="ops-mini-list">
            {workItem.relations && workItem.relations.length > 0 ? workItem.relations.map((relation) => {
              const target = data.workItems.find((candidate) => candidate.id === relation.targetId);
              return (
                <a
                  data-context-item
                  data-context-label={target?.subject ?? relation.targetId}
                  data-context-module="operations"
                  data-context-record-id={relation.targetId}
                  data-context-record-type="work_item"
                  data-context-submodule={submoduleId}
                  href={operationsRecordHref(query, submoduleId, "work-item", relation.targetId)}
                  key={`${relation.type}-${relation.targetId}`}
                >
                  {relation.type} - {target?.semanticId ?? relation.targetId} - {target?.subject ?? copy.item}
                </a>
              );
            }) : <span>{copy.noRelations}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.customFields}</h3>
          <div className="ops-mini-list">
            {workItem.customFields && Object.keys(workItem.customFields).length > 0 ? Object.entries(workItem.customFields).map(([key, value]) => (
              <span key={key}>{key}: {value}</span>
            )) : <span>{copy.noCustomFields}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.timeAndReminders}</h3>
          <div className="ops-mini-list">
            {(workItem.timeEntries ?? []).map((entry) => (
              <span key={`${entry.personId}-${entry.date}`}>{entry.date} - {entry.hours}h - {data.people.find((person) => person.id === entry.personId)?.name ?? entry.personId}: {entry.note}</span>
            ))}
            {(workItem.reminders ?? []).map((reminder) => (
              <span key={reminder.id}>{reminder.channel} - {reminder.due}: {reminder.note}</span>
            ))}
            {(workItem.timeEntries?.length ?? 0) + (workItem.reminders?.length ?? 0) === 0 ? <span>{copy.noTimeEntries}</span> : null}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.activity}</h3>
          <div className="ops-mini-list">
            {workItem.comments && workItem.comments.length > 0 ? workItem.comments.map((comment) => (
              <span key={`${comment.personId}-${comment.date}`}>{comment.date} - {data.people.find((person) => person.id === comment.personId)?.name ?? comment.personId}: {comment.body}</span>
            )) : <span>{copy.noActivity}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.linkedKnowledge}</h3>
          <div className="ops-mini-list">
            {workItem.linkedKnowledgeIds.length > 0 ? workItem.linkedKnowledgeIds.map((id) => {
              const item = data.knowledgeItems.find((candidate) => candidate.id === id);
              return item ? <span key={id}>{item.title}</span> : null;
            }) : <span>{copy.noLinkedKnowledge}</span>}
          </div>
        </section>
        <section className="ops-drawer-section">
          <h3>{copy.inlineWork}</h3>
          <OperationsWorkItemEditor
            assigneeLabel={copy.assignee}
            assignees={data.people.map((person) => ({ label: person.name, value: person.id }))}
            dueLabel={copy.due}
            item={{
              assigneeId: workItem.assigneeId,
              description: text(workItem.description, locale),
              due: workItem.due,
              id: workItem.id,
              priority: workItem.priority,
              status: workItem.status,
              subject: workItem.subject
            }}
            priorityLabel={copy.priority}
            saveLabel={copy.saveDraft}
            statusLabel={copy.status}
            syncLabel={copy.askCtoxWorkItem}
          />
        </section>
      </div>
    );
  }

  const selectedProject = project ?? data.projects[0];
  if (!selectedProject) return null;
  return (
    <div className="drawer-content ops-drawer">
      <DrawerHeader title={selectedProject.name} query={query} submoduleId={submoduleId} />
      <p className="drawer-description">{text(selectedProject.summary, locale)}</p>
      <dl className="drawer-facts">
        <div><dt>{copy.code}</dt><dd>{selectedProject.code}</dd></div>
        <div><dt>{copy.owner}</dt><dd>{data.people.find((item) => item.id === selectedProject.ownerId)?.name}</dd></div>
        {selectedProject.parentProjectId ? <div><dt>{copy.parentProject}</dt><dd>{data.projects.find((item) => item.id === selectedProject.parentProjectId)?.name}</dd></div> : null}
        {selectedProject.customerId ? <div><dt>{copy.customer}</dt><dd>{data.customers.find((item) => item.id === selectedProject.customerId)?.name}</dd></div> : null}
        <div><dt>{copy.health}</dt><dd>{selectedProject.health}</dd></div>
        <div><dt>{copy.progress}</dt><dd>{selectedProject.progress}%</dd></div>
        <div><dt>{copy.nextMilestone}</dt><dd>{selectedProject.nextMilestone}</dd></div>
        <div><dt>{copy.budget}</dt><dd>{selectedProject.spentHours ?? 0}/{selectedProject.budgetHours ?? 0}h</dd></div>
        <div><dt>{copy.storage}</dt><dd>{selectedProject.storageUsedGb ?? 0}/{selectedProject.storageQuotaGb ?? 0} GB</dd></div>
      </dl>
      <section className="ops-drawer-section">
        <h3>{copy.members}</h3>
        <div className="ops-mini-list">
          {(selectedProject.memberIds ?? [selectedProject.ownerId]).map((memberId) => {
            const person = data.people.find((item) => item.id === memberId);
            return <span key={memberId}>{person?.name ?? memberId} - {person?.role ?? copy.member}</span>;
          })}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{copy.childProjects}</h3>
        <div className="ops-mini-list">
          {data.projects.filter((item) => item.parentProjectId === selectedProject.id).map((child) => (
            <a
              data-context-item
              data-context-label={child.name}
              data-context-module="operations"
              data-context-record-id={child.id}
              data-context-record-type="project"
              data-context-submodule={submoduleId}
              href={operationsRecordHref(query, submoduleId, "project", child.id)}
              key={child.id}
            >
              {child.code} - {child.name} - {child.health}
            </a>
          ))}
          {data.projects.some((item) => item.parentProjectId === selectedProject.id) ? null : <span>{copy.noChildProjects}</span>}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{copy.milestones}</h3>
        <div className="ops-mini-list">
          {data.milestones.filter((milestone) => milestone.projectId === selectedProject.id).map((milestone) => (
            <span key={milestone.id}>{milestone.title} · {milestone.date} · {milestone.status}</span>
          ))}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{copy.activeWork}</h3>
        <div className="ops-mini-list">
          {data.workItems.filter((item) => item.projectId === selectedProject.id).map((item) => (
            <span key={item.id}>{item.subject}</span>
          ))}
        </div>
      </section>
      <section className="ops-drawer-section">
        <h3>{copy.syncRail}</h3>
        <OperationsQueueButton
          action="sync"
          className="drawer-primary"
          instruction={`Synchronize Operations project ${selectedProject.name} across work items, planning, knowledge, meetings, CTOX queue, and cross-module links.`}
          payload={{ project: selectedProject }}
          recordId={selectedProject.id}
          resource="projects"
          title={`Sync project: ${selectedProject.name}`}
        >
          {copy.askCtoxSync}
        </OperationsQueueButton>
      </section>
    </div>
  );
}

function DrawerHeader({
  title,
  query,
  submoduleId
}: {
  title: string;
  query: QueryState;
  submoduleId: string;
}) {
  const locale = resolveLocale(query.locale) as SupportedLocale;
  const copy = operationsCopy[locale];

  return (
    <div className="drawer-head">
      <strong>{title}</strong>
      <a href={baseHref(query, submoduleId)}>{copy.close}</a>
    </div>
  );
}

function baseHref(query: QueryState, submoduleId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  const queryString = params.toString();
  return queryString ? `/app/operations/${submoduleId}?${queryString}` : `/app/operations/${submoduleId}`;
}

function operationsRecordHref(query: QueryState, submoduleId: string, panel: string, recordId: string) {
  const params = new URLSearchParams();
  if (query.locale) params.set("locale", query.locale);
  if (query.theme) params.set("theme", query.theme);
  params.set("panel", panel);
  params.set("recordId", recordId);
  params.set("drawer", "right");
  return `/app/operations/${submoduleId}?${params.toString()}`;
}

function resolveWorkSet(recordId: string | undefined, items: OperationsWorkItem[], copy: Copy) {
  const key = recordId ?? "open";

  if (key === "open") {
    return {
      title: `${copy.openItems} ${copy.workItems}`,
      description: copy.workSetOpenDescription,
      items: items.filter((item) => item.status !== "Done")
    };
  }

  if (key === "urgent") {
    return {
      title: `${copy.urgentItems} ${copy.workItems}`,
      description: copy.workSetPriorityDescription,
      items: items.filter((item) => item.priority === "Urgent")
    };
  }

  if (key === "review") {
    return {
      title: `${copy.review} ${copy.workItems}`,
      description: copy.workSetStatusDescription,
      items: items.filter((item) => item.status === "Review")
    };
  }

  if (key === "linked-knowledge") {
    return {
      title: copy.linkedKnowledge,
      description: copy.workSetKnowledgeDescription,
      items: items.filter((item) => item.linkedKnowledgeIds.length > 0)
    };
  }

  if (key.startsWith("status-")) {
    const slug = key.replace("status-", "");
    const status = operationsStatusColumns.find((candidate) => candidate.toLowerCase().replace(/\s+/g, "-") === slug);
    return {
      title: status ?? copy.status,
      description: copy.workSetStatusDescription,
      items: status ? items.filter((item) => item.status === status) : []
    };
  }

  if (key.startsWith("priority-")) {
    const priority = titleCase(key.replace("priority-", ""));
    return {
      title: `${priority} ${copy.priority}`,
      description: copy.workSetPriorityDescription,
      items: items.filter((item) => item.priority.toLowerCase() === key.replace("priority-", ""))
    };
  }

  if (key.startsWith("assignee-")) {
    const assigneeId = key.replace("assignee-", "");
    return {
      title: `${copy.assignee}: ${assigneeId}`,
      description: copy.workSetAssigneeDescription,
      items: items.filter((item) => item.assigneeId === assigneeId && item.status !== "Done")
    };
  }

  return {
    title: copy.workItems,
    description: copy.workSetOpenDescription,
    items: items.filter((item) => item.status !== "Done")
  };
}

function titleCase(value: string) {
  return value.split("-").map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : part).join(" ");
}

type OperationsSetItem = {
  id: string;
  label: string;
  meta: string;
  panel: string;
  recordId: string;
  type: string;
};

function resolveOperationsSet(recordId: string | undefined, data: OperationsBundle, copy: Copy, locale: SupportedLocale) {
  const key = recordId ?? "projects";
  const projectItems = (items: OperationsProject[]): OperationsSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.name,
    meta: `${item.health} · ${item.progress}% · ${item.nextMilestone}`,
    panel: "project",
    recordId: item.id,
    type: "project"
  }));
  const milestoneItems = (items: OperationsMilestone[]): OperationsSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.title,
    meta: `${item.status} · ${item.date} · ${data.projects.find((project) => project.id === item.projectId)?.name ?? item.projectId}`,
    panel: "project",
    recordId: item.projectId,
    type: "milestone"
  }));
  const knowledgeItems = (items: OperationsKnowledgeItem[]): OperationsSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.title,
    meta: `${item.kind} · ${item.updated} · ${item.sections.length} ${copy.items.toLowerCase()}`,
    panel: "knowledge",
    recordId: item.id,
    type: "knowledge"
  }));
  const meetingItems = (items: OperationsMeeting[]): OperationsSetItem[] => items.map((item) => ({
    id: item.id,
    label: item.title,
    meta: `${item.date} · ${item.decisions.length} ${copy.decisions.toLowerCase()} · ${item.actionItems.length} ${copy.actions.toLowerCase()}`,
    panel: "meeting",
    recordId: item.id,
    type: "meeting"
  }));
  const actionItems = (items: OperationsActionItem[]): OperationsSetItem[] => items.map((item) => ({
    id: item.id,
    label: text(item.text, locale),
    meta: `${item.due} · ${data.people.find((person) => person.id === item.ownerId)?.name ?? item.ownerId}`,
    panel: item.workItemId ? "work-item" : "new",
    recordId: item.workItemId ?? item.id,
    type: "action_item"
  }));

  if (key === "projects-at-risk") return operationsSet(copy.risk, copy.operationsSetProjectsRiskDescription, projectItems(data.projects.filter((item) => item.health !== "Green" || data.milestones.some((milestone) => milestone.projectId === item.id && milestone.status === "At risk"))), "projects", todayLabel(data.projects.map((item) => item.end)));
  if (key === "customer-projects") return operationsSet(copy.customer, copy.operationsSetCustomerDescription, projectItems(data.projects.filter((item) => item.customerId)), "projects", todayLabel(data.projects.map((item) => item.end)));
  if (key === "milestones-at-risk") return operationsSet(copy.milestones, copy.operationsSetMilestonesDescription, milestoneItems(data.milestones.filter((item) => item.status === "At risk")), "milestones", todayLabel(data.milestones.map((item) => item.date)));
  if (key === "milestones-upcoming") return operationsSet(copy.nextMilestone, copy.operationsSetMilestonesDescription, milestoneItems(data.milestones.filter((item) => item.status === "Upcoming")), "milestones", todayLabel(data.milestones.map((item) => item.date)));
  if (key === "knowledge-ctox") return operationsSet(copy.knowledge, copy.operationsSetKnowledgeDescription, knowledgeItems(data.knowledgeItems.filter((item) => item.ownerId === "ctox-agent")), "knowledge", todayLabel(data.knowledgeItems.map((item) => item.updated)));
  if (key === "knowledge-linked") return operationsSet(copy.linkedKnowledge, copy.operationsSetKnowledgeDescription, knowledgeItems(data.knowledgeItems.filter((item) => item.linkedItems.length > 0)), "knowledge", todayLabel(data.knowledgeItems.map((item) => item.updated)));
  if (key === "meetings-decisions") return operationsSet(copy.decisions, copy.operationsSetMeetingsDescription, meetingItems(data.meetings.filter((item) => item.decisions.length > 0)), "meetings", todayLabel(data.meetings.map((item) => item.date.slice(0, 10))));
  if (key === "meetings-actions") return operationsSet(copy.actions, copy.operationsSetMeetingsDescription, meetingItems(data.meetings.filter((item) => item.actionItems.length > 0)), "meetings", todayLabel(data.meetings.map((item) => item.date.slice(0, 10))));
  if (key === "open-actions") return operationsSet(copy.actions, copy.operationsSetActionsDescription, actionItems(data.actionItems), "action-items", todayLabel(data.actionItems.map((item) => item.due)));
  if (key === "meetings") return operationsSet(copy.meetings, copy.operationsSetMeetingsDescription, meetingItems(data.meetings), "meetings", todayLabel(data.meetings.map((item) => item.date.slice(0, 10))));
  if (key === "knowledge") return operationsSet(copy.knowledge, copy.operationsSetKnowledgeDescription, knowledgeItems(data.knowledgeItems), "knowledge", todayLabel(data.knowledgeItems.map((item) => item.updated)));
  if (key === "milestones") return operationsSet(copy.milestones, copy.operationsSetMilestonesDescription, milestoneItems(data.milestones), "milestones", todayLabel(data.milestones.map((item) => item.date)));
  return operationsSet(copy.projects, copy.operationsSetProjectsDescription, projectItems(data.projects), "projects", todayLabel(data.projects.map((item) => item.end)));
}

function operationsSet(title: string, description: string, items: OperationsSetItem[], resource: string, freshness: string) {
  return { title, description, freshness, items, resource };
}

function todayLabel(values: string[]) {
  return values.length > 0 ? values.sort().at(-1) ?? "-" : "-";
}

function resolveNewResource(recordId: string | undefined, submoduleId: string) {
  if (recordId?.includes("meeting") || submoduleId === "meetings") return "meetings";
  if (recordId?.includes("knowledge") || submoduleId === "knowledge" || submoduleId === "wiki") return "knowledge";
  if (recordId?.includes("project") || submoduleId === "projects") return "projects";
  if (recordId?.includes("timeline") || submoduleId === "planning") return "milestones";
  return "work-items";
}

function resolveKnowledgeElement(recordId: string): "skill" | "skill_file" | "skillbook" | "runbook" {
  if (recordId === "new-ctox-skill-file") return "skill_file";
  if (recordId === "new-ctox-skillbook") return "skillbook";
  if (recordId === "new-ctox-runbook") return "runbook";
  return "skill";
}

type Copy = typeof operationsCopy.en;

const operationsCopy = {
  en: {
    actions: "Actions",
    activeWork: "Active work",
    activity: "Activity",
    assignee: "Assignee",
    askCtoxMeeting: "Ask CTOX to extract decisions",
    askCtoxSync: "Ask CTOX to sync Operations",
    askCtoxWorkItem: "Ask CTOX to update this",
    askCtoxWorkSet: "Ask CTOX to process this set",
    budgetDecision: "Budget variance moves into Business reporting once reviewed.",
    budget: "Budget",
    calendar: "Calendar",
    calendarDescription: "Due dates from all visible work items.",
    close: "Close",
    code: "Code",
    childProjects: "Child projects",
    customer: "Customer",
    date: "Date",
    decisions: "Decisions",
    decisionsDescription: "Meeting outcomes that affect projects, work items, and knowledge.",
    docsLinked: "Docs",
    due: "Due",
    doneRatio: "Done",
    filters: "Filters",
    health: "Health",
    id: "ID",
    inlineWork: "Inline work",
    item: "Item",
    items: "items",
    kind: "Kind",
    ctoxLearning: "CTOX learning",
    knowledge: "Knowledge Store",
    knowledgeBlocks: "Blocks",
    knowledgeDescription: "Skillbooks and runbooks CTOX can learn, execute, and keep linked to daily work.",
    knowledgeStoreLinked: "Work coverage",
    knowledgeStoreOwners: "CTOX owned",
    knowledgeStorePages: "Skillbooks / runbooks",
    knowledgeStoreSections: "Training blocks",
    learnReady: "Learning readiness",
    learnWithCtox: "Learn",
    linkedWork: "Linked work",
    linkedWorkDescription: "Execution context CTOX should use when learning or applying this knowledge.",
    linkedWorkShort: "work links",
    linkedKnowledge: "Linked knowledge",
    meeting: "Meeting",
    meetingNotes: "Meeting notes",
    meetingNotesDescription: "Decisions, action items, and follow-up tasks stay attached to the module context.",
    meetings: "Meetings",
    meetingsDescription: "Decision meetings and operational follow-ups.",
    meetingsPlanned: "Meetings",
    milestones: "Milestones",
    modalOnlyDecision: "Every normal detail opens as a drawer; no deep page navigation for daily work.",
    member: "Member",
    members: "Members",
    newItem: "New Operations item",
    newItemDescription: "Create the record in this submodule and queue CTOX for follow-up wiring.",
    newFile: "New file",
    newKnowledgeElement: "New knowledge element",
    newKnowledgeElementDescription: "Create a CTOX skill, skill file, skillbook, or runbook through the CTOX queue with the current hierarchy attached.",
    newKnowledge: "New skillbook",
    newMeeting: "New meeting",
    newProject: "New project",
    newRunbook: "New runbook",
    newSkill: "New skill",
    newWorkItem: "New work item",
    noRunbooks: "No runbooks linked",
    noActivity: "No activity yet.",
    noChildProjects: "No child projects.",
    noCustomFields: "No custom fields yet.",
    noLinkedKnowledge: "No linked knowledge yet.",
    noRelations: "No relations yet.",
    noTimeEntries: "No time entries or reminders yet.",
    nextMilestone: "Next milestone",
    openDrawer: "Open drawer",
    openItems: "Open",
    owner: "Owner",
    parentProject: "Parent project",
    pageSections: "Training blocks",
    planning: "Planning",
    planningDescription: "Gantt-like project bars and calendar pressure in one surface.",
    priority: "Priority",
    progress: "Progress",
    project: "Project",
    projectTreeDescription: "Project hierarchy from the project-management base, adapted as one dense workbench.",
    projects: "Projects",
    queueCreate: "Queue create",
    review: "Review",
    relations: "Relations",
    risk: "Risk",
    saveDraft: "Save draft",
    runbooks: "Runbooks",
    skillFile: "Skill file",
    sectionPreviewDescription: "Training blocks with purpose, trigger, steps, and boundaries for CTOX learning.",
    skillbooks: "Skillbooks",
    status: "Status",
    storage: "Storage",
    subject: "Subject",
    subjectPlaceholder: "Describe the work item...",
    summary: "Summary",
    syncDecision: "Right-click prompt context is mandatory for selected Operations records.",
    syncRail: "Sync",
    syncRailDescription: "Cross-module signals that CTOX should keep synchronized.",
    customFields: "Custom fields",
    semanticId: "Key",
    timeAndReminders: "Time and reminders",
    updated: "Updated",
    urgentItems: "Urgent",
    wiki: "Wiki",
    workItems: "Work items",
    workItemsDescription: "Work-package table adapted from the project-management source.",
    workSetAssigneeDescription: "Selected work for one owner, ready for reassignment, follow-up, or CTOX queue processing.",
    workSetEmpty: "No work items match this selection yet.",
    workSetItems: "Selected items",
    workSetKnowledgeDescription: "Work items with linked knowledge pages, documents, or runbooks.",
    workSetOpenDescription: "All active work that is not closed yet.",
    workSetPriorityDescription: "Work grouped by priority for focused triage and CTOX instructions.",
    workSetStatusDescription: "Work grouped by status for local review without leaving this submodule.",
    operationsSetActionsDescription: "Open action items from meetings and planning that CTOX can route into work.",
    operationsSetCustomerDescription: "Customer-facing projects with active delivery, Sales handoff, or Business implications.",
    operationsSetKnowledgeDescription: "Skillbooks and runbooks that should stay linked to CTOX core and daily execution.",
    operationsSetMeetingsDescription: "Meeting records with decisions, action items, and project follow-up context.",
    operationsSetMilestonesDescription: "Milestones from the operating plan, including delivery risk and date pressure.",
    operationsSetProjectsDescription: "All Operations projects with health, progress, owner, milestones, and linked modules.",
    operationsSetProjectsRiskDescription: "Projects with red or amber health, at-risk milestones, or operational follow-up needs."
  },
  de: {
    actions: "Aktionen",
    activeWork: "Aktive Arbeit",
    activity: "Aktivität",
    assignee: "Zugewiesen",
    askCtoxMeeting: "CTOX Entscheidungen extrahieren lassen",
    askCtoxSync: "CTOX Operations synchronisieren lassen",
    askCtoxWorkItem: "CTOX damit beauftragen",
    askCtoxWorkSet: "CTOX mit dieser Auswahl beauftragen",
    budgetDecision: "Budgetabweichungen gehen nach Review in Business Reporting.",
    budget: "Budget",
    calendar: "Kalender",
    calendarDescription: "Fälligkeiten aus allen sichtbaren Work Items.",
    close: "Schließen",
    code: "Code",
    childProjects: "Unterprojekte",
    customer: "Kunde",
    date: "Datum",
    decisions: "Entscheidungen",
    decisionsDescription: "Meeting-Ergebnisse, die Projekte, Work Items und Wissen betreffen.",
    docsLinked: "Docs",
    due: "Fällig",
    doneRatio: "Erledigt",
    filters: "Filter",
    health: "Status",
    id: "ID",
    inlineWork: "Inline-Arbeit",
    item: "Eintrag",
    items: "Einträge",
    kind: "Art",
    ctoxLearning: "CTOX Learning",
    knowledge: "Knowledge Store",
    knowledgeBlocks: "Blöcke",
    knowledgeDescription: "Skillbooks und Runbooks, die CTOX lernen, ausführen und mit Tagesarbeit verknüpfen kann.",
    knowledgeStoreLinked: "Work Coverage",
    knowledgeStoreOwners: "CTOX owned",
    knowledgeStorePages: "Skillbooks / Runbooks",
    knowledgeStoreSections: "Trainingsblöcke",
    learnReady: "Learning Readiness",
    learnWithCtox: "Lernen",
    linkedWork: "Verknüpfte Arbeit",
    linkedWorkDescription: "Ausführungskontext, den CTOX beim Lernen oder Anwenden nutzen soll.",
    linkedWorkShort: "Work Links",
    linkedKnowledge: "Verknüpftes Wissen",
    meeting: "Meeting",
    meetingNotes: "Meeting Notes",
    meetingNotesDescription: "Entscheidungen, Aktionen und Follow-ups bleiben am Modulkontext.",
    meetings: "Meetings",
    meetingsDescription: "Entscheidungsmeetings und operative Follow-ups.",
    meetingsPlanned: "Meetings",
    milestones: "Meilensteine",
    modalOnlyDecision: "Alle normalen Details öffnen als Drawer; keine tiefen Seiten für Tagesgeschäft.",
    member: "Mitglied",
    members: "Mitglieder",
    newItem: "Neues Operations Item",
    newItemDescription: "Datensatz in diesem Submodul anlegen und CTOX für die weitere Verdrahtung queuen.",
    newFile: "Neue Datei",
    newKnowledgeElement: "Neues Knowledge Element",
    newKnowledgeElementDescription: "CTOX Skill, Skill-Datei, Skillbook oder Runbook über die CTOX Queue mit aktueller Hierarchie anlegen.",
    newKnowledge: "Neues Skillbook",
    newMeeting: "Neues Meeting",
    newProject: "Neues Projekt",
    newRunbook: "Neues Runbook",
    newSkill: "Neuer Skill",
    newWorkItem: "Neues Work Item",
    noRunbooks: "Keine Runbooks verknüpft",
    noActivity: "Noch keine Aktivität.",
    noChildProjects: "Keine Unterprojekte.",
    noCustomFields: "Noch keine Custom Fields.",
    noLinkedKnowledge: "Noch kein Wissen verknüpft.",
    noRelations: "Noch keine Relationen.",
    noTimeEntries: "Noch keine Zeiten oder Erinnerungen.",
    nextMilestone: "Nächster Meilenstein",
    openDrawer: "Drawer öffnen",
    openItems: "Offen",
    owner: "Owner",
    parentProject: "Oberprojekt",
    pageSections: "Trainingsblöcke",
    planning: "Planung",
    planningDescription: "Gantt-artige Projektbalken und Kalenderdruck in einer Fläche.",
    priority: "Priorität",
    progress: "Fortschritt",
    project: "Projekt",
    projectTreeDescription: "Projekt-Hierarchie aus der Project-Management-Basis als dichte Workbench.",
    projects: "Projekte",
    queueCreate: "Create queuen",
    review: "Review",
    relations: "Relationen",
    risk: "Risiko",
    saveDraft: "Draft speichern",
    runbooks: "Runbooks",
    skillFile: "Skill-Datei",
    sectionPreviewDescription: "Trainingsblöcke mit Zweck, Triggern, Schritten und Grenzen für CTOX Learning.",
    skillbooks: "Skillbooks",
    status: "Status",
    storage: "Speicher",
    subject: "Betreff",
    subjectPlaceholder: "Work Item beschreiben...",
    summary: "Zusammenfassung",
    syncDecision: "Rechtsklick-Prompt-Kontext ist Pflicht für markierte Operations-Datensätze.",
    syncRail: "Sync",
    syncRailDescription: "Modulübergreifende Signale, die CTOX synchron halten soll.",
    customFields: "Custom Fields",
    semanticId: "Key",
    timeAndReminders: "Zeiten und Erinnerungen",
    updated: "Aktualisiert",
    urgentItems: "Dringend",
    wiki: "Wiki",
    workItems: "Work Items",
    workItemsDescription: "Work-Package-Tabelle aus der Project-Management-Basis angepasst.",
    workSetAssigneeDescription: "Ausgewählte Arbeit für einen Owner, bereit für Reassignment, Follow-up oder CTOX Queue.",
    workSetEmpty: "Keine Work Items passen aktuell zu dieser Auswahl.",
    workSetItems: "Ausgewählte Einträge",
    workSetKnowledgeDescription: "Work Items mit verknüpften Knowledge-Seiten, Dokumenten oder Runbooks.",
    workSetOpenDescription: "Alle aktiven Work Items, die noch nicht geschlossen sind.",
    workSetPriorityDescription: "Nach Priorität gruppierte Arbeit für fokussierte Triage und CTOX-Anweisungen.",
    workSetStatusDescription: "Nach Status gruppierte Arbeit für lokale Prüfung ohne Verlassen des Submoduls.",
    operationsSetActionsDescription: "Offene Aktionen aus Meetings und Planung, die CTOX in Arbeit routen kann.",
    operationsSetCustomerDescription: "Kundennahe Projekte mit aktiver Delivery, Sales-Handoff oder Business-Bezug.",
    operationsSetKnowledgeDescription: "Skillbooks und Runbooks, die mit CTOX Core und der täglichen Ausführung verknüpft bleiben sollen.",
    operationsSetMeetingsDescription: "Meeting-Datensätze mit Entscheidungen, Aktionen und Projekt-Follow-up-Kontext.",
    operationsSetMilestonesDescription: "Meilensteine aus dem Betriebsplan inklusive Delivery-Risiko und Termindruck.",
    operationsSetProjectsDescription: "Alle Operations-Projekte mit Status, Fortschritt, Owner, Meilensteinen und verknüpften Modulen.",
    operationsSetProjectsRiskDescription: "Projekte mit rotem oder gelbem Status, riskanten Meilensteinen oder operativem Follow-up."
  }
} satisfies Record<SupportedLocale, Record<string, string>>;
