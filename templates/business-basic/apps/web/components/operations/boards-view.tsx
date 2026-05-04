import {
  operationsStatusColumns,
  type WorkPriority,
  type WorkStatus
} from "../../lib/operations-seed";
import { operationsPanelHref, type OperationsSubmoduleViewProps } from "./types";
import { OperationsKanbanTool } from "./ported-tools";

const WIP_LIMITS: Record<WorkStatus, number> = {
  Backlog: 8,
  Ready: 5,
  "In progress": 3,
  Review: 3,
  Done: 10
};

const PRIORITY_RANK: Record<WorkPriority, number> = {
  Urgent: 0,
  High: 1,
  Normal: 2,
  Low: 3
};

export function OperationsBoardView({
  data,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const itemsByStatus = operationsStatusColumns.map((status) => {
    const items = data.workItems
      .filter((item) => item.status === status)
      .sort((left, right) => {
        const priorityDelta = PRIORITY_RANK[left.priority] - PRIORITY_RANK[right.priority];
        return priorityDelta === 0 ? left.due.localeCompare(right.due) : priorityDelta;
      });

    return { status, items };
  });

  return (
    <div className="ops-workspace ops-board-workspace" data-context-module="operations" data-context-submodule={submoduleId}>
      <OperationsKanbanTool
        columns={itemsByStatus.map(({ status, items }) => ({
          id: statusSlug(status),
          title: status,
          wipLimit: WIP_LIMITS[status],
          cards: items.map((item) => {
            const project = data.projects.find((candidate) => candidate.id === item.projectId);
            const assignee = data.people.find((candidate) => candidate.id === item.assigneeId);
            return {
              assignee: assignee?.name ?? "Unassigned",
              due: item.due,
              estimate: item.estimate,
              href: operationsPanelHref(query, submoduleId, "work-item", item.id, "right"),
              id: item.id,
              priority: item.priority,
              project: `${project?.code ?? item.projectId} - ${project?.name ?? "Unassigned project"}`,
              status: item.status,
              subject: item.subject,
              type: item.type
            };
          })
        }))}
      />
    </div>
  );
}

function statusSlug(status: WorkStatus) {
  return status.toLowerCase().replace(/\s+/g, "-");
}

export default OperationsBoardView;
