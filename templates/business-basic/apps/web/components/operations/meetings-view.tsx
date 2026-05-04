import { text } from "../../lib/operations-seed";
import { OperationsQueueButton } from "./operations-actions";
import { OperationsPaneHead, OperationsSignal, copyLabel } from "./shell";
import { operationsPanelHref, type OperationsSubmoduleViewProps } from "./types";
import { OperationsAgendaTool } from "./ported-tools";

export function OperationsMeetingsView({
  copy,
  data,
  locale,
  query,
  submoduleId
}: OperationsSubmoduleViewProps) {
  const label = (key: string, fallback: string) => copyLabel(copy, key, fallback);
  const sortedMeetings = [...data.meetings].sort((left, right) => left.date.localeCompare(right.date));
  const sortedActions = [...data.actionItems].sort((left, right) => left.due.localeCompare(right.due));

  return (
    <div className="ops-workspace ops-meetings-workspace">
      <section className="ops-pane ops-meeting-list" aria-label={label("meetings", "Meetings")}>
        <OperationsPaneHead
          actionContext={{ action: "create", label: label("newMeeting", "New meeting"), recordId: "new-meeting", recordType: "meeting", submoduleId }}
          actionHref={operationsPanelHref(query, submoduleId, "new", "meeting", "left-bottom")}
          actionLabel={label("newMeeting", "New meeting")}
          description={label("meetingsDescription", "Decision meetings and operational follow-ups.")}
          title={label("meetings", "Meetings")}
        />
        <div className="ops-table ops-meeting-table">
          <div className="ops-table-head">
            <span>{label("meeting", "Meeting")}</span>
            <span>{label("date", "Date")}</span>
            <span>{label("actions", "Actions")}</span>
          </div>
          {sortedMeetings.map((meeting) => {
            const project = data.projects.find((candidate) => candidate.id === meeting.projectId);
            const facilitator = data.people.find((person) => person.id === meeting.facilitatorId);
            const actionCount = meeting.actionItems.length;
            const decisionCount = meeting.decisions.length;

            return (
              <a
                className="ops-table-row"
                data-context-item
                data-context-label={meeting.title}
                data-context-module="operations"
                data-context-record-id={meeting.id}
                data-context-record-type="meeting"
                data-context-submodule={submoduleId}
                href={operationsPanelHref(query, submoduleId, "meeting", meeting.id, "right")}
                key={meeting.id}
              >
                <span>
                  <strong>{meeting.title}</strong>
                  <small>{project?.name} · {facilitator?.name}</small>
                </span>
                <span>
                  <strong>{meeting.date}</strong>
                  <small>{project?.code} · {decisionCount} {label("decisions", "Decisions").toLowerCase()}</small>
                </span>
                <span>
                  <strong>{actionCount}</strong>
                  <small>{meeting.agenda.length} {label("items", "items")}</small>
                </span>
              </a>
            );
          })}
        </div>
        <OperationsAgendaTool
          meetings={sortedMeetings.map((meeting) => {
            const project = data.projects.find((candidate) => candidate.id === meeting.projectId);
            return {
              agenda: meeting.agenda.map((agendaItem) => text(agendaItem, locale)),
              date: meeting.date,
              href: operationsPanelHref(query, submoduleId, "meeting", meeting.id, "right"),
              id: meeting.id,
              project: project?.name ?? meeting.projectId,
              title: meeting.title
            };
          })}
        />
      </section>

      <section className="ops-pane ops-decision-stream" aria-label={label("meetingNotes", "Meeting notes")}>
        <OperationsPaneHead
          description={label("meetingNotesDescription", "Decisions, action items, and follow-up tasks stay attached to the module context.")}
          title={label("meetingNotes", "Meeting notes")}
        />
        <div className="ops-note-feed">
          {sortedMeetings.map((meeting) => {
            const project = data.projects.find((candidate) => candidate.id === meeting.projectId);
            const meetingDecisions = meeting.decisions.map((id) => data.decisions.find((decision) => decision.id === id)).filter(Boolean);
            const meetingActions = meeting.actionItems.map((id) => data.actionItems.find((action) => action.id === id)).filter(Boolean);

            return (
              <a
                data-context-item
                data-context-label={`${meeting.title} agenda`}
                data-context-module="operations"
                data-context-record-id={meeting.id}
                data-context-record-type="meeting_agenda"
                data-context-submodule={submoduleId}
                href={operationsPanelHref(query, submoduleId, "meeting", meeting.id, "right")}
                key={`${meeting.id}-agenda`}
              >
                <strong>{meeting.title}</strong>
                <span>{project?.name} · {meeting.date}</span>
                {meeting.agenda.map((agendaItem) => (
                  <span key={text(agendaItem, locale)}>{text(agendaItem, locale)}</span>
                ))}
                <small>{meetingDecisions.length} {label("decisions", "Decisions").toLowerCase()} · {meetingActions.length} {label("actions", "Actions").toLowerCase()}</small>
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-decision-stream" aria-label={label("decisions", "Decisions")}>
        <OperationsPaneHead
          description={label("decisionsDescription", "Meeting outcomes that affect projects, work items, and knowledge.")}
          title={label("decisions", "Decisions")}
        />
        <div className="ops-note-feed">
          {data.decisions.map((decision) => {
            const project = data.projects.find((candidate) => candidate.id === decision.projectId);
            const meeting = data.meetings.find((candidate) => candidate.id === decision.meetingId);

            return (
              <a
                data-context-item
                data-context-label={text(decision.text, locale)}
                data-context-module="operations"
                data-context-record-id={decision.id}
                data-context-record-type="decision"
                data-context-submodule={submoduleId}
                href={operationsPanelHref(query, submoduleId, "meeting", decision.meetingId, "right")}
                key={decision.id}
              >
                <strong>{text(decision.text, locale)}</strong>
                <span>{project?.name} · {meeting?.title}</span>
                <small>
                  {decision.linkedWorkItemIds.map((workItemId) => data.workItems.find((item) => item.id === workItemId)?.subject).filter(Boolean).join(" · ")}
                </small>
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-linked-context" aria-label={label("actions", "Actions")}>
        <OperationsPaneHead
          description={label("linkedWorkDescription", "Work items that share context with the selected knowledge surface.")}
          title={label("actions", "Actions")}
        />
        <div className="ops-card-stack">
          {sortedActions.map((actionItem) => {
            const owner = data.people.find((person) => person.id === actionItem.ownerId);
            const workItem = actionItem.workItemId ? data.workItems.find((item) => item.id === actionItem.workItemId) : undefined;
            const meeting = data.meetings.find((candidate) => candidate.actionItems.includes(actionItem.id));
            const project = workItem ? data.projects.find((candidate) => candidate.id === workItem.projectId) : meeting ? data.projects.find((candidate) => candidate.id === meeting.projectId) : undefined;
            const href = workItem
              ? operationsPanelHref(query, submoduleId, "work-item", workItem.id, "right")
              : operationsPanelHref(query, submoduleId, "meeting", meeting?.id ?? sortedMeetings[0]?.id ?? actionItem.id, "right");

            return (
              <a
                className={`ops-work-card priority-${workItem?.priority.toLowerCase() ?? "normal"}`}
                data-context-item
                data-context-label={text(actionItem.text, locale)}
                data-context-module="operations"
                data-context-record-id={actionItem.id}
                data-context-record-type="action_item"
                data-context-submodule={submoduleId}
                href={href}
                key={actionItem.id}
              >
                <strong>{text(actionItem.text, locale)}</strong>
                <small>{owner?.name} · {actionItem.due}</small>
                <span>{project?.name ?? meeting?.title ?? label("meeting", "Meeting")}</span>
              </a>
            );
          })}
        </div>
      </section>

      <section className="ops-pane ops-linked-context" aria-label={label("linkedWork", "Linked work")}>
        <OperationsPaneHead
          description={label("workItemsDescription", "Work-package table adapted from the project-management source.")}
          title={label("linkedWork", "Linked work")}
        />
        <div className="ops-table ops-work-table">
          <div className="ops-table-head">
            <span>{label("item", "Item")}</span>
            <span>{label("status", "Status")}</span>
            <span>{label("assignee", "Assignee")}</span>
            <span>{label("due", "Due")}</span>
          </div>
          {data.workItems
            .filter((item) => data.decisions.some((decision) => decision.linkedWorkItemIds.includes(item.id)) || sortedActions.some((action) => action.workItemId === item.id))
            .map((item) => {
              const project = data.projects.find((candidate) => candidate.id === item.projectId);

              return (
                <a
                  className="ops-table-row"
                  data-context-item
                  data-context-label={item.subject}
                  data-context-module="operations"
                  data-context-record-id={item.id}
                  data-context-record-type="work_item"
                  data-context-submodule={submoduleId}
                  href={operationsPanelHref(query, submoduleId, "work-item", item.id, "right")}
                  key={item.id}
                >
                  <span>
                    <strong>{item.subject}</strong>
                    <small>{project?.name} · {item.type}</small>
                  </span>
                  <span>{item.status}</span>
                  <span>{data.people.find((person) => person.id === item.assigneeId)?.name}</span>
                  <span>{item.due}</span>
                </a>
              );
            })}
        </div>
      </section>

      <section className="ops-pane ops-linked-context" aria-label={label("projects", "Projects")}>
        <OperationsPaneHead
          description={label("projectTreeDescription", "Project hierarchy from the project-management base, adapted as one dense workbench.")}
          title={label("projects", "Projects")}
        />
        <div className="ops-card-stack">
          {data.projects
            .filter((project) => sortedMeetings.some((meeting) => meeting.projectId === project.id))
            .map((project) => (
              <a
                className="ops-work-card"
                data-context-item
                data-context-label={project.name}
                data-context-module="operations"
                data-context-record-id={project.id}
                data-context-record-type="project"
                data-context-submodule={submoduleId}
                href={operationsPanelHref(query, submoduleId, "project", project.id, "right")}
                key={project.id}
              >
                <strong>{project.name}</strong>
                <small>{project.code} · {data.people.find((person) => person.id === project.ownerId)?.name}</small>
                <span>{project.health} · {project.progress}% · {project.nextMilestone}</span>
              </a>
            ))}
        </div>
      </section>

      <section className="ops-pane ops-sync-rail" aria-label={label("syncRail", "Sync")}>
        <OperationsPaneHead
          description={label("syncRailDescription", "Cross-module signals that CTOX should keep synchronized.")}
          title={label("syncRail", "Sync")}
        />
        <div className="ops-signal-list">
          <OperationsSignal
            context={{ action: "open-set", label: label("meetingsPlanned", "Meetings"), recordId: "meetings", recordType: "meeting_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "operations-set", "meetings", "right")}
            label={label("meetingsPlanned", "Meetings")}
            value={String(data.meetings.length)}
          />
          <OperationsSignal
            context={{ action: "open-set", label: label("decisions", "Decisions"), recordId: "meetings-decisions", recordType: "meeting_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "operations-set", "meetings-decisions", "right")}
            label={label("decisions", "Decisions")}
            value={String(data.decisions.length)}
          />
          <OperationsSignal
            context={{ action: "open-set", label: label("actions", "Actions"), recordId: "meetings-actions", recordType: "action_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "operations-set", "meetings-actions", "right")}
            label={label("actions", "Actions")}
            value={String(data.actionItems.length)}
          />
          <OperationsSignal
            context={{ action: "open-work-set", label: label("openItems", "Open"), recordId: "open", recordType: "work_set", submoduleId }}
            href={operationsPanelHref(query, submoduleId, "work-set", "open", "right")}
            label={label("openItems", "Open")}
            value={String(data.workItems.filter((item) => item.status !== "Done").length)}
          />
        </div>
        <div className="ops-action-dock">
          <a
            data-context-item
            data-context-label={label("newMeeting", "New meeting")}
            data-context-module="operations"
            data-context-record-id="new-meeting"
            data-context-record-type="meeting"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "new", "meeting", "left-bottom")}
          >
            {label("newMeeting", "New meeting")}
          </a>
          <a
            data-context-item
            data-context-label={label("newWorkItem", "New work item")}
            data-context-module="operations"
            data-context-record-id="new-work-item"
            data-context-record-type="work_item"
            data-context-submodule={submoduleId}
            href={operationsPanelHref(query, submoduleId, "new", "work-item", "left-bottom")}
          >
            {label("newWorkItem", "New work item")}
          </a>
          <OperationsQueueButton
            action="extract"
            instruction="Extract decisions and action items from visible Operations meetings, then synchronize linked work items and meeting records with CTOX core."
            payload={{ meetings: sortedMeetings, decisions: data.decisions, actionItems: sortedActions }}
            recordId={sortedMeetings[0]?.id ?? "meetings"}
            resource="meetings"
            title="Extract Operations meeting decisions"
          >
            {label("askCtoxMeeting", "Ask CTOX to extract decisions")}
          </OperationsQueueButton>
        </div>
      </section>
    </div>
  );
}

export default OperationsMeetingsView;
