import {
  text,
  type OperationsHealth,
  type OperationsProject
} from "../../lib/operations-seed";
import { OperationsPaneHead, OperationsSignal } from "./shell";
import { operationsPanelHref, operationsSelectionHref, type OperationsSubmoduleViewProps } from "./types";
import { OperationsGanttTool, OperationsProjectTreeTool, type OperationsGanttItem } from "./ported-tools";

const healthRank: Record<OperationsHealth, number> = {
  Red: 0,
  Amber: 1,
  Green: 2
};

const moduleLabels: Record<OperationsProject["linkedModules"][number], string> = {
  sales: "Sales",
  marketing: "Marketing",
  business: "Business",
  ctox: "CTOX"
};

export function OperationsProjectsView({
  copy,
  data,
  locale,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const projectModels = data.projects
    .map((project) => {
      const projectWork = data.workItems
        .filter((item) => item.projectId === project.id)
        .sort((left, right) => left.due.localeCompare(right.due));
      const activeWork = projectWork.filter((item) => item.status !== "Done");
      const projectMilestones = data.milestones
        .filter((milestone) => milestone.projectId === project.id)
        .sort((left, right) => left.date.localeCompare(right.date));
      const customer = project.customerId ? data.customers.find((item) => item.id === project.customerId) : undefined;
      const owner = data.people.find((person) => person.id === project.ownerId);
      const customerOwner = data.people.find((person) => person.id === customer?.ownerId);

      return {
        project,
        owner,
        customer,
        customerOwner,
        projectWork,
        activeWork,
        projectMilestones,
        riskMilestones: projectMilestones.filter((milestone) => milestone.status === "At risk")
      };
    })
    .sort((left, right) => healthRank[left.project.health] - healthRank[right.project.health] || left.project.end.localeCompare(right.project.end));

  const activeWork = projectModels.flatMap((model) => model.activeWork.map((item) => ({ item, project: model.project })));
  const urgentWork = activeWork.filter(({ item }) => item.priority === "Urgent");
  const atRiskProjects = projectModels.filter((model) => model.project.health !== "Green" || model.riskMilestones.length > 0);
  const customerProjects = projectModels.filter((model) => model.customer);
  const selectedProject =
    projectModels.find((model) => model.project.id === query.selectedId) ??
    projectModels.find((model) => model.project.id === query.recordId) ??
    projectModels[0];
  const selectedProjectItems = selectedProject ? buildProjectGanttItems(selectedProject, locale, query, submoduleId) : [];

  return (
    <div className="ops-workspace ops-project-gantt-workspace">
      <section className="ops-pane ops-project-selector" aria-label={copy.projects}>
        <OperationsPaneHead
          description={copy.projectTreeDescription}
          title={copy.projects}
        >
          <a
            aria-label={copy.newProject}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newProject}
            data-context-module="operations"
            data-context-record-id="project"
            data-context-record-type="project"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "new", "project", "left-bottom")}
          >
            New project
          </a>
        </OperationsPaneHead>
        <OperationsProjectTreeTool
          projects={projectModels.map(({ project }) => ({
            code: project.code,
            health: project.health,
            href: operationsSelectionHref(query, submoduleId, project.id),
            id: project.id,
            memberCount: project.memberIds?.length ?? 1,
            name: project.name,
            parentProjectId: project.parentProjectId,
            progress: project.progress
          }))}
        />
        <div className="ops-project-list">
          {projectModels.map(({ activeWork: modelActiveWork, customer, customerOwner, owner, project }) => (
            <a
              className="ops-project-row"
              data-context-item
              data-context-label={project.name}
              data-context-module="operations"
              data-context-record-id={project.id}
              data-context-record-type="project"
              data-context-submodule={submoduleId}
              href={operationsSelectionHref(query, submoduleId, project.id)}
              key={project.id}
            >
              <span className={`ops-health ops-health-${project.health.toLowerCase()}`} />
              <strong>{project.code} - {project.name}</strong>
              <small>{owner?.name ?? copy.owner} - {customer?.name ?? customerOwner?.name ?? copy.project}</small>
              <small>{project.progress}% - {modelActiveWork.length} {copy.openItems.toLowerCase()} - {project.linkedModules.map((module) => moduleLabels[module]).join(" / ")}</small>
              <meter max="100" min="0" value={project.progress} />
            </a>
          ))}
        </div>
      </section>

      <section className="ops-pane ops-project-gantt-pane" aria-label={selectedProject?.project.name ?? copy.projects}>
        <OperationsPaneHead
          description={selectedProject ? text(selectedProject.project.summary, locale) : copy.workItemsDescription}
          title={selectedProject ? `${selectedProject.project.code} - ${selectedProject.project.name}` : copy.activeWork}
        >
          <a
            aria-label={copy.newWorkItem}
            data-context-action="create"
            data-context-item
            data-context-label={copy.newWorkItem}
            data-context-module="operations"
            data-context-record-id="work-item"
            data-context-record-type="work_item"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "new", "work-item", "left-bottom")}
          >
            New work item
          </a>
        </OperationsPaneHead>
        <div className="ops-project-gantt-summary">
          <OperationsSignal
            context={{ action: "open-set", label: copy.projects, recordId: "projects", recordType: "project_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "operations-set", "projects", "right")}
            label={copy.projects}
            value={String(projectModels.length)}
          />
          <OperationsSignal
            context={{ action: "open-work-set", label: copy.openItems, recordId: "open", recordType: "work_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "work-set", "open", "right")}
            label={copy.openItems}
            value={String(activeWork.length)}
          />
          <OperationsSignal
            context={{ action: "open-work-set", label: copy.urgentItems, recordId: "urgent", recordType: "work_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "work-set", "urgent", "right")}
            label={copy.urgentItems}
            value={String(urgentWork.length)}
          />
          <OperationsSignal
            context={{ action: "open-set", label: copy.customer, recordId: "customer-projects", recordType: "project_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "operations-set", "customer-projects", "right")}
            label={copy.customer}
            value={String(customerProjects.length)}
          />
        </div>
        {selectedProject ? (
          <OperationsGanttTool
            items={selectedProjectItems}
            key={selectedProject.project.id}
            selectedProjectId={selectedProject.project.id}
          />
        ) : <p className="ops-empty-state">No project selected.</p>}
        <div className="ops-action-dock">
          <a
            data-context-item
            data-context-label={copy.risk}
            data-context-module="operations"
            data-context-record-id="projects-at-risk"
            data-context-record-type="project_set"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "operations-set", "projects-at-risk", "right")}
          >
            {copy.risk}: {atRiskProjects.length}
          </a>
          {selectedProject?.activeWork.slice(0, 4).map((item) => (
            <a
              data-context-item
              data-context-label={item.subject}
              data-context-module="operations"
              data-context-record-id={item.id}
              data-context-record-type="work_item"
              data-context-submodule={submoduleId}
              href={operationsPanelHref(query, submoduleId, "work-item", item.id, "right")}
              key={item.id}
            >
              {item.subject}
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

function buildProjectGanttItems(
  model: {
    project: OperationsProject;
    owner?: { name: string };
    projectWork: OperationsSubmoduleViewProps["data"]["workItems"];
    projectMilestones: OperationsSubmoduleViewProps["data"]["milestones"];
  },
  locale: OperationsSubmoduleViewProps["locale"],
  query: OperationsSubmoduleViewProps["query"],
  submoduleId: string
): OperationsGanttItem[] {
  const projectItem: OperationsGanttItem = {
    assignee: model.owner?.name,
    code: model.project.code,
    due: model.project.end,
    end: model.project.end,
    health: model.project.health,
    href: operationsPanelHref(query, submoduleId, "project", model.project.id, "right"),
    id: model.project.id,
    kind: "project",
    progress: model.project.progress,
    projectId: model.project.id,
    start: model.project.start,
    status: model.project.health,
    subtitle: text(model.project.summary, locale),
    title: model.project.name
  };

  const workItems = model.projectWork.map((item) => ({
    assignee: item.assigneeId,
    due: item.due,
    end: item.due,
    health: item.priority === "Urgent" ? "Red" : item.priority === "High" ? "Amber" : "Green",
    href: operationsPanelHref(query, submoduleId, "work-item", item.id, "right"),
    id: item.id,
    kind: "work_item" as const,
    priority: item.priority,
    progress: item.doneRatio ?? statusProgress(item.status),
    projectId: item.projectId,
    start: item.start ?? model.project.start,
    status: item.status,
    subtitle: `${item.type} - ${item.priority}`,
    title: item.subject
  }));

  const milestones = model.projectMilestones.map((milestone) => ({
    due: milestone.date,
    end: milestone.date,
    health: milestone.status === "At risk" ? "Amber" : milestone.status === "Complete" ? "Green" : "Green",
    href: operationsPanelHref(query, submoduleId, "milestone", milestone.id, "right"),
    id: milestone.id,
    kind: "milestone" as const,
    progress: milestone.status === "Complete" ? 100 : 0,
    projectId: milestone.projectId,
    start: milestone.date,
    status: milestone.status,
    subtitle: "Milestone",
    title: milestone.title
  }));

  return [projectItem, ...workItems, ...milestones].sort((left, right) => left.start.localeCompare(right.start));
}

function statusProgress(status: string) {
  if (status === "Done") return 100;
  if (status === "Review") return 80;
  if (status === "In progress") return 45;
  if (status === "Ready") return 15;
  return 0;
}

function formatMilestoneSummary(milestones: Array<{ status: string }>) {
  const complete = milestones.filter((milestone) => milestone.status === "Complete").length;
  const atRisk = milestones.filter((milestone) => milestone.status === "At risk").length;
  return `${complete}/${milestones.length} complete${atRisk > 0 ? ` - ${atRisk} at risk` : ""}`;
}
