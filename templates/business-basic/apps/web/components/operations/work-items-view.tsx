import {
  operationsStatusColumns,
  type OperationsWorkItem,
  type WorkPriority
} from "../../lib/operations-seed";
import { OperationsPaneHead, copyLabel } from "./shell";
import { operationsPanelHref, type OperationsSubmoduleViewProps } from "./types";

const priorityOrder: Record<WorkPriority, number> = {
  Urgent: 0,
  High: 1,
  Normal: 2,
  Low: 3
};

export function OperationsWorkItemsView({
  copy,
  data,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const workItems = sortWorkItems(data.workItems);
  const openItems = workItems.filter((item) => item.status !== "Done");
  const urgentItems = workItems.filter((item) => item.priority === "Urgent");
  const linkedItems = workItems.filter((item) => item.linkedKnowledgeIds.length > 0);
  const reviewItems = workItems.filter((item) => item.status === "Review");
  const summaryActions = [
    { id: "open", label: copyLabel(copy, "openItems", "Open"), items: openItems },
    { id: "urgent", label: copyLabel(copy, "urgentItems", "Urgent"), items: urgentItems },
    { id: "review", label: copyLabel(copy, "review", "Review"), items: reviewItems },
    { id: "linked-knowledge", label: copyLabel(copy, "linkedKnowledge", "Linked knowledge"), items: linkedItems }
  ];

  return (
    <div className="ops-workspace ops-knowledge-workspace">
      <section className="ops-pane ops-work-items" aria-label={copyLabel(copy, "workItems", "Work items")}>
        <OperationsPaneHead
          title={copyLabel(copy, "workItems", "Work items")}
          description={copyLabel(copy, "workItemsDescription", "Dense work-package table with operational ownership and knowledge links.")}
        >
          <a
            aria-label={copyLabel(copy, "newWorkItem", "New work item")}
            data-context-action="create"
            data-context-item
            data-context-label={copyLabel(copy, "newWorkItem", "New work item")}
            data-context-module="operations"
            data-context-record-id="work-item"
            data-context-record-type="work_item"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "new", "work-item", "left-bottom")}
          >
            +
          </a>
        </OperationsPaneHead>

        <div className="ops-action-dock" aria-label={copyLabel(copy, "summary", "Summary")}>
          {summaryActions.map((action) => (
            <a
              data-context-action="open-work-summary"
              data-context-item
              data-context-label={action.label}
              data-context-module="operations"
              data-context-record-id={action.id}
              data-context-record-type="work_summary"
              data-context-submodule={submoduleId}
              href={workSetHref(query, submoduleId, action.items, action.id)}
              key={action.id}
            >
              {action.label} {action.items.length}
            </a>
          ))}
        </div>

        <div className="ops-action-dock" aria-label={copyLabel(copy, "filters", "Filters")}>
          {operationsStatusColumns.map((status) => (
            <a
              data-context-action="open-status"
              data-context-item
              data-context-label={status}
              data-context-module="operations"
              data-context-record-id={statusSlug(status)}
              data-context-record-type="work_status"
              data-context-status={status}
              data-context-submodule={submoduleId}
              href={workSetHref(query, submoduleId, workItems.filter((item) => item.status === status), `status-${statusSlug(status)}`)}
              key={status}
            >
              {status} {workItems.filter((item) => item.status === status).length}
            </a>
          ))}
        </div>

        <div className="ops-table ops-work-table" role="table">
          <div className="ops-table-head" role="row">
            <span role="columnheader">{copyLabel(copy, "item", "Item")}</span>
            <span role="columnheader">{copyLabel(copy, "status", "Status")} / {copyLabel(copy, "priority", "Priority")}</span>
            <span role="columnheader">{copyLabel(copy, "assignee", "Assignee")} / {copyLabel(copy, "due", "Due")}</span>
            <span role="columnheader">{copyLabel(copy, "linkedKnowledge", "Linked knowledge")}</span>
          </div>
          {workItems.map((item) => (
            <WorkItemRow
              copy={copy}
              data={data}
              item={item}
              key={item.id}
              query={query}
              submoduleId={submoduleId}
            />
          ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={copyLabel(copy, "filters", "Filters")}>
        <OperationsPaneHead
          title={copyLabel(copy, "filters", "Filters")}
          description={copyLabel(copy, "syncRailDescription", "Cross-module signals that CTOX should keep synchronized.")}
        />
        <div className="ops-signal-list">
          {data.projects.map((project) => {
            const projectItems = workItems.filter((item) => item.projectId === project.id);
            const activeProjectItems = projectItems.filter((item) => item.status !== "Done");

            return (
              <a
                className="ops-signal"
                data-context-item
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="project"
                data-context-record-id={project.id}
                data-context-label={project.name}
                href={operationsPanelHref(query, submoduleId, "project", project.id, "right")}
                key={project.id}
              >
                <span>{project.code} · {project.name}</span>
                <strong>{activeProjectItems.length}/{projectItems.length}</strong>
              </a>
            );
          })}
        </div>
        <div className="ops-action-dock" aria-label={copyLabel(copy, "priority", "Priority")}>
          {(["Urgent", "High", "Normal", "Low"] as WorkPriority[]).map((priority) => (
            <a
              data-context-action="open-priority"
              data-context-item
              data-context-label={priority}
              data-context-module="operations"
              data-context-priority={priority}
              data-context-record-id={priority.toLowerCase()}
              data-context-record-type="work_priority"
              data-context-submodule={submoduleId}
              href={workSetHref(query, submoduleId, workItems.filter((item) => item.priority === priority), `priority-${priority.toLowerCase()}`)}
              key={priority}
            >
              {priority} {workItems.filter((item) => item.priority === priority).length}
            </a>
          ))}
        </div>
        <div className="ops-action-dock" aria-label={copyLabel(copy, "assignee", "Assignee")}>
          {data.people.map((person) => {
            const assigned = workItems.filter((item) => item.assigneeId === person.id && item.status !== "Done");
            if (assigned.length === 0) return null;

            return (
              <a
                data-context-action="open-assignee"
                data-context-assignee-id={person.id}
                data-context-item
                data-context-label={person.name}
                data-context-module="operations"
                data-context-record-id={person.id}
                data-context-record-type="work_assignee"
                data-context-submodule={submoduleId}
                href={workSetHref(query, submoduleId, assigned, `assignee-${person.id}`)}
                key={person.id}
              >
                {person.name} {assigned.length}
              </a>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function WorkItemRow({
  copy,
  data,
  item,
  query,
  submoduleId
}: {
  copy: OperationsSubmoduleViewProps["copy"];
  data: OperationsSubmoduleViewProps["data"];
  item: OperationsWorkItem;
  query: OperationsSubmoduleViewProps["query"];
  submoduleId: string;
}) {
  const project = data.projects.find((candidate) => candidate.id === item.projectId);
  const assignee = data.people.find((candidate) => candidate.id === item.assigneeId);
  const knowledgeItems = item.linkedKnowledgeIds.map((id) => data.knowledgeItems.find((candidate) => candidate.id === id)).filter((knowledge) => knowledge !== undefined);

  return (
    <div
      className="ops-table-row"
      data-context-item
      data-context-module="operations"
      data-context-submodule={submoduleId}
      data-context-record-type="work_item"
      data-context-record-id={item.id}
      data-context-label={item.subject}
      role="row"
    >
      <span role="cell">
        <a
          data-context-action="open-work-item"
          data-context-label={item.subject}
          data-context-module="operations"
          data-context-record-id={item.id}
          data-context-record-type="work_item"
          data-context-submodule={submoduleId}
          href={operationsPanelHref(query, submoduleId, "work-item", item.id, "right")}
        >
          <strong>{item.subject}</strong>
          <small>{item.id} · {project?.name ?? item.projectId} · {item.type}</small>
        </a>
      </span>
      <span role="cell">
        <strong>{item.status}</strong>
        <small>{item.priority} · {item.estimate}h</small>
      </span>
      <span role="cell">
        <strong>{assignee?.name ?? item.assigneeId}</strong>
        <small>{copyLabel(copy, "due", "Due")} {item.due}</small>
      </span>
      <span role="cell">
        <strong>{knowledgeItems.length} {copyLabel(copy, "docsLinked", "Docs")}</strong>
        {knowledgeItems.length > 0 ? (
          <small>
            {knowledgeItems.map((knowledge, index) => (
              <a
                data-context-item
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="knowledge"
                data-context-record-id={knowledge.id}
                data-context-label={knowledge.title}
                href={operationsPanelHref(query, submoduleId, "knowledge", knowledge.id, "right")}
                key={knowledge.id}
              >
                {index > 0 ? ", " : ""}
                {knowledge.title}
              </a>
            ))}
          </small>
        ) : (
          <small>{copyLabel(copy, "noLinkedKnowledge", "No linked knowledge yet.")}</small>
        )}
      </span>
    </div>
  );
}

function sortWorkItems(items: OperationsWorkItem[]) {
  return [...items].sort((left, right) => {
    const statusDelta = operationsStatusColumns.indexOf(left.status) - operationsStatusColumns.indexOf(right.status);
    if (statusDelta !== 0) return statusDelta;

    const priorityDelta = priorityOrder[left.priority] - priorityOrder[right.priority];
    if (priorityDelta !== 0) return priorityDelta;

    return left.due.localeCompare(right.due);
  });
}

function workSetHref(
  query: OperationsSubmoduleViewProps["query"],
  submoduleId: string,
  _items: OperationsWorkItem[],
  fallbackId: string
) {
  return operationsPanelHref(query, submoduleId, "work-set", fallbackId, "right");
}

function statusSlug(status: string) {
  return status.toLowerCase().replace(/\s+/g, "-");
}
