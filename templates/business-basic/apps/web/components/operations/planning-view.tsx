import type { CSSProperties } from "react";
import { text } from "../../lib/operations-seed";
import { OperationsPaneHead, copyLabel } from "./shell";
import { operationsPanelHref, type OperationsSubmoduleViewProps } from "./types";

const today = "2026-05-02";

type CalendarEntry = {
  date: string;
  href: string;
  id: string;
  label: string;
  meta: string;
  recordId: string;
  recordType: string;
  tone: "green" | "amber" | "red";
};

export function OperationsPlanningView({
  copy,
  data,
  locale,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const label = (key: string, fallback: string) => copyLabel(copy, key, fallback);
  const sortedMilestones = [...data.milestones].sort((left, right) => left.date.localeCompare(right.date));
  const sortedProjects = [...data.projects].sort((left, right) => left.start.localeCompare(right.start));
  const calendarEntries = buildCalendarEntries(data, query, submoduleId, locale);
  const risks = buildRiskSignals(data, query, submoduleId);
  const windowStart = sortedProjects.reduce((earliest, project) => minDate(earliest, project.start), sortedProjects[0]?.start ?? today);
  const windowEnd = sortedProjects.reduce((latest, project) => maxDate(latest, project.end), sortedProjects[0]?.end ?? today);

  return (
    <div className="ops-workspace ops-planning-workspace">
      <section className="ops-pane ops-timeline-pane" aria-label={label("planning", "Planning")}>
        <OperationsPaneHead
          actionContext={{ action: "create", label: label("newWorkItem", "New work item"), recordId: "timeline-item", recordType: "milestone", submoduleId }}
          actionHref={operationsPanelHref(query, submoduleId, "new", "timeline-item", "left-bottom")}
          actionLabel={label("newWorkItem", "New work item")}
          description={label("planningDescription", "Milestones, project load, due dates, and delivery risk in one dense operating plan.")}
          title={label("planning", "Planning")}
        />
        <div className="ops-timeline">
          <div className="ops-timeline-row" aria-hidden="true">
            <span>{formatDate(windowStart)} - {formatDate(windowEnd)}</span>
            <div style={axisStyle}>
              {sortedMilestones.map((milestone) => (
                <span
                  key={milestone.id}
                  style={{ ...tickStyle, left: `${dateOffsetPercent(milestone.date, windowStart, windowEnd)}%` }}
                  title={`${milestone.title} ${formatDate(milestone.date)}`}
                />
              ))}
            </div>
          </div>
          {sortedProjects.map((project) => {
            const projectMilestones = sortedMilestones.filter((milestone) => milestone.projectId === project.id);
            const projectOpenWork = data.workItems.filter((item) => item.projectId === project.id && item.status !== "Done");
            const criticalCount = projectOpenWork.filter((item) => item.priority === "Urgent" || item.priority === "High").length;
            const left = dateOffsetPercent(project.start, windowStart, windowEnd);
            const width = Math.max(10, dateSpanPercent(project.start, project.end, windowStart, windowEnd));

            return (
              <div
                className="ops-timeline-row"
                data-context-item
                data-context-module="operations"
                data-context-submodule={submoduleId}
                data-context-record-type="project"
                data-context-record-id={project.id}
                data-context-label={project.name}
                key={project.id}
              >
                <span>
                  {project.code} | {project.name}
                </span>
                <div style={laneStyle}>
                  <a
                    className="ops-timeline-bar"
                    data-context-item
                    data-context-module="operations"
                    data-context-submodule={submoduleId}
                    data-context-record-type="project"
                    data-context-record-id={project.id}
                    data-context-label={project.name}
                    href={operationsPanelHref(query, submoduleId, "project", project.id, "right")}
                    style={projectBarStyle(left, width, project.health)}
                  >
                    {project.nextMilestone} | {project.progress}%
                  </a>
                  {projectMilestones.map((milestone) => (
                    <a
                      aria-label={`${milestone.title}: ${milestone.status}`}
                      data-context-item
                      data-context-module="operations"
                      data-context-submodule={submoduleId}
                      data-context-record-type="milestone"
                      data-context-record-id={milestone.id}
                      data-context-label={milestone.title}
                      href={operationsPanelHref(query, submoduleId, "project", project.id, "right")}
                      key={milestone.id}
                      style={milestoneStyle(milestone.date, windowStart, windowEnd, milestone.status)}
                      title={`${milestone.title} | ${formatDate(milestone.date)} | ${milestone.status}`}
                    />
                  ))}
                </div>
                <small style={rowMetaStyle}>
                  {data.people.find((person) => person.id === project.ownerId)?.name ?? project.ownerId} | {projectOpenWork.length} open | {criticalCount} critical
                </small>
              </div>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-calendar-pane" aria-label={label("calendar", "Calendar")}>
        <OperationsPaneHead title={label("calendar", "Calendar")} description={label("calendarDescription", "Due dates and delivery signals ordered by date.")} />
        <div className="ops-calendar-list">
          {calendarEntries.map((entry) => (
            <a
              data-context-item
              data-context-module="operations"
              data-context-submodule={submoduleId}
              data-context-record-type={entry.recordType}
              data-context-record-id={entry.recordId}
              data-context-label={entry.label}
              href={entry.href}
              key={entry.id}
              style={calendarToneStyle(entry.tone)}
            >
              <time dateTime={entry.date}>{formatDate(entry.date)}</time>
              <span>
                <strong style={calendarTitleStyle}>{entry.label}</strong>
                <small style={calendarMetaStyle}>{entry.meta}</small>
              </span>
            </a>
          ))}
        </div>
        <div className="ops-signal-list" aria-label="Planning risk signals">
          <a
            data-context-item
            data-context-module="operations"
            data-context-submodule={submoduleId}
            data-context-record-type="milestone_set"
            data-context-record-id="milestones-at-risk"
            data-context-label={label("milestones", "Milestones")}
            href={operationsPanelHref(query, submoduleId, "operations-set", "milestones-at-risk", "right")}
            style={riskSignalStyle("red")}
          >
            <span>{label("milestones", "Milestones")}</span>
            <strong>{data.milestones.filter((milestone) => milestone.status === "At risk").length}</strong>
            <small>At risk</small>
          </a>
          <a
            data-context-item
            data-context-module="operations"
            data-context-submodule={submoduleId}
            data-context-record-type="action_set"
            data-context-record-id="open-actions"
            data-context-label={label("actions", "Actions")}
            href={operationsPanelHref(query, submoduleId, "operations-set", "open-actions", "right")}
            style={riskSignalStyle("amber")}
          >
            <span>{label("actions", "Actions")}</span>
            <strong>{data.actionItems.length}</strong>
            <small>Meeting follow-up</small>
          </a>
          {risks.map((risk) => (
            <a
              data-context-item
              data-context-module="operations"
              data-context-submodule={submoduleId}
              data-context-record-type={risk.recordType}
              data-context-record-id={risk.recordId}
              data-context-label={risk.label}
              href={risk.href}
              key={risk.id}
              style={riskSignalStyle(risk.tone)}
            >
              <span>{risk.type}</span>
              <strong>{risk.label}</strong>
              <small>{risk.meta}</small>
            </a>
          ))}
        </div>
      </section>
    </div>
  );
}

export default OperationsPlanningView;

function buildCalendarEntries(
  data: OperationsSubmoduleViewProps["data"],
  query: OperationsSubmoduleViewProps["query"],
  submoduleId: string,
  locale: OperationsSubmoduleViewProps["locale"]
): CalendarEntry[] {
  const dueWork: CalendarEntry[] = data.workItems.map((item) => {
    const project = data.projects.find((candidate) => candidate.id === item.projectId);
    return {
      date: item.due,
      href: operationsPanelHref(query, submoduleId, "work-item", item.id, "right"),
      id: `work-${item.id}`,
      label: item.subject,
      meta: `${project?.name ?? item.projectId} | ${item.priority} | ${data.people.find((person) => person.id === item.assigneeId)?.name ?? item.assigneeId}`,
      recordId: item.id,
      recordType: "work_item",
      tone: item.status !== "Done" && item.due <= today ? "red" : item.priority === "Urgent" || item.priority === "High" ? "amber" : "green"
    };
  });

  const milestoneDates: CalendarEntry[] = data.milestones.map((milestone) => {
    const project = data.projects.find((candidate) => candidate.id === milestone.projectId);
    return {
      date: milestone.date,
      href: operationsPanelHref(query, submoduleId, "project", milestone.projectId, "right"),
      id: `milestone-${milestone.id}`,
      label: milestone.title,
      meta: `${project?.name ?? milestone.projectId} | ${milestone.status}`,
      recordId: milestone.id,
      recordType: "milestone",
      tone: milestone.status === "At risk" ? "red" : milestone.status === "Complete" ? "green" : "amber"
    };
  });

  const meetingDates: CalendarEntry[] = data.meetings.map((meeting) => ({
    date: meeting.date.slice(0, 10),
    href: operationsPanelHref(query, submoduleId, "meeting", meeting.id, "right"),
    id: `meeting-${meeting.id}`,
    label: meeting.title,
    meta: `${data.projects.find((project) => project.id === meeting.projectId)?.name ?? meeting.projectId} | ${meeting.date.slice(11)}`,
    recordId: meeting.id,
    recordType: "meeting",
    tone: "green"
  }));

  const actionDates: CalendarEntry[] = data.actionItems.map((action) => {
    const linkedWork = action.workItemId ? data.workItems.find((item) => item.id === action.workItemId) : undefined;
    const project = linkedWork ? data.projects.find((candidate) => candidate.id === linkedWork.projectId) : undefined;

    return {
      date: action.due,
      href: operationsPanelHref(query, submoduleId, linkedWork ? "work-item" : "new", linkedWork?.id ?? action.id, linkedWork ? "right" : "left-bottom"),
      id: `action-${action.id}`,
      label: text(action.text, locale),
      meta: `${project?.name ?? "Operations"} | ${data.people.find((person) => person.id === action.ownerId)?.name ?? action.ownerId}`,
      recordId: action.id,
      recordType: "action_item",
      tone: action.due <= today ? "amber" : "green"
    };
  });

  return [...dueWork, ...milestoneDates, ...meetingDates, ...actionDates].sort((left, right) => {
    const dateSort = left.date.localeCompare(right.date);
    return dateSort === 0 ? left.label.localeCompare(right.label) : dateSort;
  });
}

function buildRiskSignals(data: OperationsSubmoduleViewProps["data"], query: OperationsSubmoduleViewProps["query"], submoduleId: string) {
  const atRiskMilestones = data.milestones.filter((milestone) => milestone.status === "At risk");
  const redProjects = data.projects.filter((project) => project.health === "Red");
  const urgentOpenWork = data.workItems.filter((item) => item.status !== "Done" && item.priority === "Urgent");
  const sameDayPressure = Object.entries(
    data.workItems.reduce<Record<string, typeof data.workItems>>((groups, item) => {
      if (item.status === "Done") return groups;
      groups[item.due] = [...(groups[item.due] ?? []), item];
      return groups;
    }, {})
  )
    .filter(([, items]) => items.length > 1)
    .sort(([left], [right]) => left.localeCompare(right));
  const ownerLoad = Object.entries(
    data.workItems.reduce<Record<string, typeof data.workItems>>((groups, item) => {
      if (item.status === "Done") return groups;
      groups[item.assigneeId] = [...(groups[item.assigneeId] ?? []), item];
      return groups;
    }, {})
  )
    .filter(([, items]) => items.length >= 2)
    .sort(([, leftItems], [, rightItems]) => rightItems.length - leftItems.length);

  return [
    ...atRiskMilestones.map((milestone) => ({
      href: operationsPanelHref(query, submoduleId, "project", milestone.projectId, "right"),
      id: `risk-${milestone.id}`,
      label: milestone.title,
      meta: `${data.projects.find((project) => project.id === milestone.projectId)?.name ?? milestone.projectId} | ${formatDate(milestone.date)}`,
      recordId: milestone.id,
      recordType: "milestone",
      tone: "red" as const,
      type: "At-risk milestone"
    })),
    ...redProjects.map((project) => ({
      href: operationsPanelHref(query, submoduleId, "project", project.id, "right"),
      id: `risk-${project.id}`,
      label: project.name,
      meta: `${project.progress}% progress | ${project.activeItems} active items`,
      recordId: project.id,
      recordType: "project",
      tone: "red" as const,
      type: "Red project"
    })),
    ...urgentOpenWork.map((item) => ({
      href: operationsPanelHref(query, submoduleId, "work-item", item.id, "right"),
      id: `risk-${item.id}`,
      label: item.subject,
      meta: `${data.projects.find((project) => project.id === item.projectId)?.name ?? item.projectId} | due ${formatDate(item.due)}`,
      recordId: item.id,
      recordType: "work_item",
      tone: "red" as const,
      type: "Urgent work"
    })),
    ...sameDayPressure.slice(0, 3).map(([date, items]) => ({
      href: operationsPanelHref(query, submoduleId, "work-item", items[0].id, "right"),
      id: `risk-date-${date}`,
      label: `${items.length} due ${formatDate(date)}`,
      meta: items.map((item) => item.subject).join(" | "),
      recordId: items[0].id,
      recordType: "work_item",
      tone: "amber" as const,
      type: "Due-date conflict"
    })),
    ...ownerLoad.slice(0, 3).map(([ownerId, items]) => ({
      href: operationsPanelHref(query, submoduleId, "work-item", items[0].id, "right"),
      id: `risk-owner-${ownerId}`,
      label: data.people.find((person) => person.id === ownerId)?.name ?? ownerId,
      meta: `${items.length} open items | ${items.reduce((sum, item) => sum + item.estimate, 0)}h estimated`,
      recordId: items[0].id,
      recordType: "person",
      tone: "amber" as const,
      type: "Owner load"
    }))
  ];
}

function formatDate(date: string) {
  return date.slice(5, 10);
}

function minDate(left: string, right: string) {
  return left <= right ? left : right;
}

function maxDate(left: string, right: string) {
  return left >= right ? left : right;
}

function daysBetween(left: string, right: string) {
  const leftDate = Date.UTC(Number(left.slice(0, 4)), Number(left.slice(5, 7)) - 1, Number(left.slice(8, 10)));
  const rightDate = Date.UTC(Number(right.slice(0, 4)), Number(right.slice(5, 7)) - 1, Number(right.slice(8, 10)));
  return Math.round((rightDate - leftDate) / 86_400_000);
}

function dateOffsetPercent(date: string, windowStart: string, windowEnd: string) {
  const windowDays = Math.max(1, daysBetween(windowStart, windowEnd));
  return Math.min(100, Math.max(0, (daysBetween(windowStart, date) / windowDays) * 100));
}

function dateSpanPercent(start: string, end: string, windowStart: string, windowEnd: string) {
  const windowDays = Math.max(1, daysBetween(windowStart, windowEnd));
  return Math.min(100, Math.max(0, (daysBetween(start, end) / windowDays) * 100));
}

function projectBarStyle(left: number, width: number, health: string): CSSProperties {
  return {
    background:
      health === "Red"
        ? "color-mix(in srgb, #b74333 18%, var(--surface))"
        : health === "Amber"
          ? "color-mix(in srgb, #a46a1f 18%, var(--surface))"
          : undefined,
    borderColor: health === "Red" ? "#b74333" : health === "Amber" ? "#a46a1f" : undefined,
    left: `${left}%`,
    width: `${width}%`
  };
}

function milestoneStyle(date: string, windowStart: string, windowEnd: string, status: string): CSSProperties {
  const color = status === "At risk" ? "#b74333" : status === "Complete" ? "#5e7a31" : "#a46a1f";
  return {
    background: color,
    border: "2px solid var(--surface)",
    borderRadius: "999px",
    height: 12,
    left: `calc(${dateOffsetPercent(date, windowStart, windowEnd)}% - 6px)`,
    position: "absolute",
    top: "50%",
    transform: "translateY(-50%)",
    width: 12,
    zIndex: 3
  };
}

function calendarToneStyle(tone: CalendarEntry["tone"]): CSSProperties {
  return {
    boxShadow: tone === "red" ? "inset 3px 0 0 #b74333" : tone === "amber" ? "inset 3px 0 0 #a46a1f" : "inset 3px 0 0 #5e7a31"
  };
}

function riskSignalStyle(tone: "amber" | "red"): CSSProperties {
  return {
    borderBottom: "1px solid var(--border)",
    boxShadow: tone === "red" ? "inset 3px 0 0 #b74333" : "inset 3px 0 0 #a46a1f",
    display: "grid",
    gap: 3,
    padding: "9px 10px"
  };
}

const axisStyle: CSSProperties = {
  gridColumn: 2,
  height: 24,
  position: "relative"
};

const tickStyle: CSSProperties = {
  background: "var(--border)",
  bottom: 0,
  position: "absolute",
  top: 0,
  width: 1
};

const laneStyle: CSSProperties = {
  gridColumn: 2,
  height: 42,
  position: "relative"
};

const rowMetaStyle: CSSProperties = {
  color: "var(--muted)",
  fontSize: 11,
  gridColumn: "2",
  minWidth: 0,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap"
};

const calendarTitleStyle: CSSProperties = {
  display: "block",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap"
};

const calendarMetaStyle: CSSProperties = {
  color: "var(--muted)",
  display: "block",
  fontSize: 11,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap"
};
