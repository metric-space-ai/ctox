"use client";

import { type CSSProperties, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ContextMenu,
  Editor,
  Gantt,
  HeaderMenu,
  Toolbar,
  Tooltip,
  Willow,
  type IApi,
  type IColumnConfig,
  type ITask
} from "@svar-ui/react-gantt";
import "@svar-ui/react-gantt/all.css";

type TreeProject = {
  code: string;
  health: string;
  href: string;
  id: string;
  memberCount: number;
  name: string;
  parentProjectId?: string;
  progress: number;
};

type BoardCard = {
  assignee: string;
  due: string;
  estimate: number;
  href: string;
  id: string;
  priority: string;
  project: string;
  status: string;
  subject: string;
  type: string;
};

type BoardColumn = {
  id: string;
  title: string;
  wipLimit: number;
  cards: BoardCard[];
};

type AgendaMeeting = {
  date: string;
  href: string;
  id: string;
  project: string;
  title: string;
  agenda: string[];
};

type AgendaState = Array<Omit<AgendaMeeting, "agenda"> & { agenda: Array<{ id: string; title: string }> }>;

type ProjectTreeNode = TreeProject & {
  children: ProjectTreeNode[];
  level: number;
};

type GanttItemKind = "project" | "work_item" | "milestone";

const operationsGanttColumns: IColumnConfig[] = [
  { id: "text", header: "Aufgabe", width: 245, sort: true },
  { id: "start", header: "Start", width: 92, align: "center", sort: true },
  { id: "duration", header: "Dauer", width: 74, align: "center", sort: true },
  { id: "add-task", header: "+", width: 42, align: "center", sort: false, resize: false }
];

export type OperationsGanttItem = {
  assignee?: string;
  code?: string;
  due: string;
  end: string;
  health?: string;
  href?: string;
  id: string;
  kind: GanttItemKind;
  priority?: string;
  progress: number;
  projectId: string;
  start: string;
  status: string;
  subtitle?: string;
  title: string;
};

export function OperationsProjectTreeTool({ projects }: { projects: TreeProject[] }) {
  const tree = useMemo(() => buildProjectTree(projects), [projects]);
  if (tree.length === 0) return <p className="ops-empty-state">No projects yet.</p>;

  return (
    <ul className="ops-ported-tree" role="tree">
      {tree.map((node) => <ProjectTreeRow key={node.id} node={node} />)}
    </ul>
  );
}

export function OperationsKanbanTool({ columns }: { columns: BoardColumn[] }) {
  const [localColumns, setLocalColumns] = useState(columns);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [moveNotice, setMoveNotice] = useState("");

  return (
    <div className="ops-ported-kanban" data-testid="operations-ported-kanban">
      {moveNotice ? <div className="ops-local-notice">{moveNotice}</div> : null}
      {localColumns.map((column) => {
        const estimate = column.cards.reduce((sum, card) => sum + card.estimate, 0);
        const overLimit = column.cards.length > column.wipLimit && column.id !== "backlog" && column.id !== "done";

        return (
          <section
            aria-label={column.title}
            className={`ops-ported-column ${dropTarget === column.id ? "is-drop-target" : ""}`}
            data-column-id={column.id}
            key={column.id}
            onDragLeave={(event) => {
              const next = event.relatedTarget as Node | null;
              if (!next || !event.currentTarget.contains(next)) setDropTarget(null);
            }}
            onDragOver={(event) => {
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              setDropTarget(column.id);
            }}
            onDrop={(event) => {
              event.preventDefault();
              const cardId = event.dataTransfer.getData("text/plain");
              setDropTarget(null);
              if (!cardId) return;
              setLocalColumns((current) => moveCard(current, cardId, column.id));
              setMoveNotice(`Moved ${cardId} to ${column.title}. Queue CTOX from the card context menu to persist the change.`);
            }}
          >
            <header>
              <strong>{column.title}</strong>
              <small>{column.cards.length}/{column.wipLimit} WIP - {estimate} pts{overLimit ? " - over limit" : ""}</small>
            </header>
            <div className="ops-ported-card-stack">
              {column.cards.map((card) => (
                <a
                  className={`ops-ported-card priority-${card.priority.toLowerCase()}`}
                  data-context-item
                  data-context-label={card.subject}
                  data-context-module="operations"
                  data-context-record-id={card.id}
                  data-context-record-type="work_item"
                  data-context-status={card.status}
                  data-context-submodule="boards"
                  draggable
                  href={card.href}
                  key={card.id}
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData("text/plain", card.id);
                  }}
                >
                  <strong>{card.subject}</strong>
                  <span>{card.priority} - {card.type} - {card.estimate} pts</span>
                  <small>{card.project} - {card.assignee} - {card.due}</small>
                </a>
              ))}
              {column.cards.length === 0 ? <span className="ops-empty-state">Drop work here.</span> : null}
            </div>
          </section>
        );
      })}
    </div>
  );
}

export function OperationsAgendaTool({ meetings }: { meetings: AgendaMeeting[] }) {
  const [meetingState, setMeetingState] = useState(() => meetings.map((meeting) => ({
    ...meeting,
    agenda: meeting.agenda.map((title, index) => ({ id: `${meeting.id}-${index}`, title }))
  })));
  const [newTopic, setNewTopic] = useState<Record<string, string>>({});

  return (
    <div className="ops-agenda-tool" data-testid="operations-agenda-tool">
      {meetingState.map((meeting) => (
        <section className="ops-agenda-meeting" key={meeting.id}>
          <header>
            <a
              data-context-item
              data-context-label={meeting.title}
              data-context-module="operations"
              data-context-record-id={meeting.id}
              data-context-record-type="meeting"
              data-context-submodule="meetings"
              href={meeting.href}
            >
              {meeting.title}
            </a>
            <small>{meeting.project} - {meeting.date}</small>
          </header>
          <ol>
            {meeting.agenda.map((item, index) => (
              <li draggable key={item.id} onDragStart={(event) => event.dataTransfer.setData("text/plain", item.id)} onDragOver={(event) => event.preventDefault()} onDrop={(event) => {
                event.preventDefault();
                const sourceId = event.dataTransfer.getData("text/plain");
                setMeetingState((current) => reorderAgenda(current, meeting.id, sourceId, item.id));
              }}>
                <span>{index + 1}. {item.title}</span>
                <button type="button" aria-label="Move up" disabled={index === 0} onClick={() => setMeetingState((current) => moveAgendaItem(current, meeting.id, index, index - 1))}>Up</button>
                <button type="button" aria-label="Move down" disabled={index === meeting.agenda.length - 1} onClick={() => setMeetingState((current) => moveAgendaItem(current, meeting.id, index, index + 1))}>Down</button>
              </li>
            ))}
          </ol>
          <form onSubmit={(event) => {
            event.preventDefault();
            const value = newTopic[meeting.id]?.trim();
            if (!value) return;
            setMeetingState((current) => current.map((candidate) => candidate.id === meeting.id ? {
              ...candidate,
              agenda: [...candidate.agenda, { id: `${meeting.id}-new-${candidate.agenda.length + 1}`, title: value }]
            } : candidate));
            setNewTopic((current) => ({ ...current, [meeting.id]: "" }));
          }}>
            <input aria-label={`New agenda item for ${meeting.title}`} onChange={(event) => setNewTopic((current) => ({ ...current, [meeting.id]: event.target.value }))} placeholder="New agenda item" value={newTopic[meeting.id] ?? ""} />
            <button type="submit">Add</button>
          </form>
        </section>
      ))}
    </div>
  );
}

export function OperationsGanttTool({
  items,
  selectedProjectId
}: {
  items: OperationsGanttItem[];
  selectedProjectId: string;
}) {
  const [localItems, setLocalItems] = useState(items);
  const [api, setApi] = useState<IApi | null>(null);
  const [mounted, setMounted] = useState(false);
  const [notice, setNotice] = useState("");
  const lastCommitted = useRef(new Map<string, string>());
  const itemById = useMemo(() => new Map(localItems.map((item) => [item.id, item])), [localItems]);
  const tasks = useMemo(
    () => localItems.map((item) => ganttItemToSvarTask(item, selectedProjectId)),
    [localItems, selectedProjectId]
  );
  const { rangeStart, rangeEnd } = useMemo(() => buildSvarRange(tasks), [tasks]);

  useEffect(() => {
    setLocalItems(items);
  }, [items]);

  useEffect(() => {
    setMounted(true);
  }, []);

  useEffect(() => {
    const next = new Map<string, string>();
    for (const task of tasks) {
      const start = coerceSvarDate(task.start);
      const end = coerceSvarDate(task.end);
      if (start && end) next.set(String(task.id), `${formatSvarDate(start)}_${formatSvarDate(end)}`);
    }
    lastCommitted.current = next;
  }, [tasks]);

  const init = useCallback((ganttApi: IApi) => {
    setApi(ganttApi);

    ganttApi.on("update-task", (event: { id?: number | string }) => {
      if (event.id == null) return;
      const task = ganttApi.getTask(event.id as never) as ITask | undefined;
      if (!task || task.id == null) return;
      const item = itemById.get(String(task.id));
      const start = coerceSvarDate(task.start);
      const end = coerceSvarDate(task.end);
      if (!item || !start || !end) return;

      const startISO = formatSvarDate(start);
      const dueISO = formatSvarDate(item.kind === "milestone" ? start : end);
      const key = `${startISO}_${dueISO}`;
      if (lastCommitted.current.get(item.id) === key) return;
      lastCommitted.current.set(item.id, key);

      const updated: OperationsGanttItem = {
        ...item,
        due: dueISO,
        end: dueISO,
        start: startISO
      };
      setLocalItems((current) => current.map((candidate) => candidate.id === updated.id ? updated : candidate));
      setNotice(`Terminverschiebung fuer ${updated.title} wird an CTOX uebergeben.`);

      void (async () => {
        const result = await postOperationsMutation(ganttMutationResource(updated.kind), {
          action: "reschedule",
          recordId: updated.id,
          title: `Reschedule ${updated.title}`,
          payload: {
            due: updated.due,
            end: updated.end,
            kind: updated.kind,
            progress: updated.progress,
            projectId: updated.projectId,
            start: updated.start,
            status: updated.status,
            title: updated.title
          },
          instruction: `Apply the SVAR Gantt drag change for ${updated.kind} ${updated.id}. Keep Operations, CTOX queue context, linked work, and project schedule synchronized.`
        });
        setNotice(result.ok
          ? result.core?.taskId ? `Gantt-Aenderung queued: ${result.core.taskId}` : `Gantt-Aenderung fuer ${updated.title} queued.`
          : `Gantt-Aenderung konnte nicht queued werden: ${result.error ?? "unknown_error"}`);
      })();
    });

    ganttApi.on("add-task", (event: { id?: number | string }) => {
      if (event.id == null) return;
      ganttApi.exec("show-editor", { id: event.id });
    });

    setTimeout(() => {
      try {
        ganttApi.exec("scroll-chart", { date: new Date() });
      } catch {
        // SVAR can reject this before the chart body is fully mounted.
      }
    }, 50);
  }, [itemById]);
  const svarApi = (api ?? undefined) as never;

  if (!mounted) {
    return (
      <div className="ops-svar-gantt-tool" data-testid="operations-svar-gantt-tool">
        <div className="ops-svar-gantt-loading">Gantt wird geladen.</div>
      </div>
    );
  }

  return (
    <div className="ops-svar-gantt-tool" data-testid="operations-svar-gantt-tool">
      {notice ? <div className="ops-local-notice">{notice}</div> : null}
      <Willow>
        <div className="ops-svar-gantt-shell" data-testid="svar-gantt-root">
          <Toolbar api={svarApi} />
          <ContextMenu api={svarApi}>
            <HeaderMenu api={svarApi}>
              <Tooltip api={svarApi}>
                <Gantt
                  cellHeight={38}
                  cellWidth={18}
                  columns={operationsGanttColumns}
                  end={rangeEnd}
                  init={init}
                  links={[]}
                  scaleHeight={36}
                  scales={[
                    { unit: "month", step: 1, format: "%F %Y" },
                    { unit: "day", step: 1, format: "%j" }
                  ]}
                  start={rangeStart}
                  tasks={tasks}
                  zoom={true}
                />
              </Tooltip>
            </HeaderMenu>
          </ContextMenu>
          {api ? <Editor api={svarApi} /> : null}
        </div>
      </Willow>
    </div>
  );
}

function ganttItemToSvarTask(item: OperationsGanttItem, selectedProjectId: string): ITask {
  const start = parseDateInput(item.start) ?? parseDateInput(item.due) ?? new Date();
  const end = parseDateInput(item.end) ?? parseDateInput(item.due) ?? addDays(start, item.kind === "milestone" ? 0 : 1);
  const safeEnd = end < start ? start : end;

  return {
    id: item.id,
    text: item.code ? `${item.code} ${item.title}` : item.title,
    start,
    end: item.kind === "milestone" ? start : safeEnd,
    duration: Math.max(1, Math.ceil((safeEnd.getTime() - start.getTime()) / 86_400_000)),
    open: item.kind === "project",
    parent: item.kind === "project" ? 0 : selectedProjectId,
    type: item.kind === "milestone" ? "milestone" : item.kind === "project" ? "summary" : "task",
    progress: Math.max(0, Math.min(100, item.progress ?? 0))
  } as ITask;
}

function buildSvarRange(tasks: ITask[]) {
  const today = new Date();
  today.setHours(0, 0, 0, 0);

  if (tasks.length === 0) {
    return {
      rangeStart: new Date(today.getFullYear(), today.getMonth(), 1),
      rangeEnd: new Date(today.getFullYear(), today.getMonth() + 2, 0)
    };
  }

  let earliest = coerceSvarDate(tasks[0].start) ?? today;
  let latest = coerceSvarDate(tasks[0].end) ?? earliest;
  for (const task of tasks) {
    const start = coerceSvarDate(task.start);
    const end = coerceSvarDate(task.end);
    if (start && start < earliest) earliest = start;
    if (end && end > latest) latest = end;
  }

  const candidateStart = addDays(earliest, -3);
  return {
    rangeStart: candidateStart < today ? candidateStart : today,
    rangeEnd: addDays(latest, 30)
  };
}

function coerceSvarDate(value: Date | string | null | undefined) {
  if (value instanceof Date) return value;
  return typeof value === "string" ? parseDateInput(value) : undefined;
}

function formatSvarDate(date: Date) {
  return formatDateInput(date);
}

function GanttBottomEditor({
  item,
  onClose,
  onQueue
}: {
  item: OperationsGanttItem;
  onClose: () => void;
  onQueue: (item: OperationsGanttItem) => void;
}) {
  const [draft, setDraft] = useState(item);
  const [queueState, setQueueState] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  return (
    <section
      aria-label={`Edit ${item.title}`}
      className="ops-gantt-bottom-editor"
      data-context-item
      data-context-label={item.title}
      data-context-module="operations"
      data-context-record-id={item.id}
      data-context-record-type={item.kind}
      data-context-submodule="projects"
    >
      <header>
        <span>
          <strong>{item.title}</strong>
          <small>{item.kind.replace("_", " ")} - {item.subtitle ?? item.status}</small>
        </span>
        <button onClick={onClose} type="button">Close</button>
      </header>
      <div className="ops-gantt-editor-grid">
        <label>
          Title
          <input
            onChange={(event) => setDraft((current) => ({ ...current, title: event.target.value }))}
            value={draft.title}
          />
        </label>
        <label>
          Start
          <input
            onChange={(event) => setDraft((current) => ({ ...current, start: event.target.value }))}
            type="date"
            value={draft.start}
          />
        </label>
        <label>
          End
          <input
            onChange={(event) => setDraft((current) => ({ ...current, due: event.target.value, end: event.target.value }))}
            type="date"
            value={draft.end}
          />
        </label>
        <label>
          Status
          <select
            onChange={(event) => setDraft((current) => ({ ...current, status: event.target.value }))}
            value={draft.status}
          >
            {["Backlog", "Ready", "In progress", "Review", "Done", "Upcoming", "At risk", "Complete"].map((status) => (
              <option key={status} value={status}>{status}</option>
            ))}
          </select>
        </label>
        <label>
          Progress
          <input
            max="100"
            min="0"
            onChange={(event) => setDraft((current) => ({ ...current, progress: Number(event.target.value) }))}
            type="range"
            value={draft.progress}
          />
          <span>{draft.progress}%</span>
        </label>
      </div>
      <footer>
        <button
          data-context-action="prompt"
          disabled={queueState === "submitting"}
          onClick={async () => {
            setQueueState("submitting");
            setMessage("");
            const result = await postOperationsMutation(ganttMutationResource(draft.kind), {
              action: "reschedule",
              recordId: draft.id,
              title: `Reschedule ${draft.title}`,
              payload: {
                due: draft.due,
                end: draft.end,
                kind: draft.kind,
                progress: draft.progress,
                projectId: draft.projectId,
                start: draft.start,
                status: draft.status,
                title: draft.title
              },
              instruction: `Apply the Gantt editor change for ${draft.kind} ${draft.id}. Keep Operations, CTOX queue context, linked work, and project schedule synchronized.`
            });
            if (result.ok) {
              setQueueState("queued");
              setMessage(result.core?.taskId ? `Queued ${result.core.taskId}` : "Queued for CTOX");
              onQueue(draft);
            } else {
              setQueueState("error");
              setMessage(result.error ?? "Queue failed");
            }
          }}
          type="button"
        >
          {queueState === "submitting" ? "Queueing..." : "Queue CTOX schedule update"}
        </button>
        {message ? <small className="ops-action-status">{message}</small> : null}
      </footer>
    </section>
  );
}

async function postOperationsMutation(resource: string, body: Record<string, unknown>): Promise<{
  ok?: boolean;
  core?: { taskId?: string | null };
  error?: string;
}> {
  const response = await fetch(`/api/operations/${resource}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  const payload = await response.json().catch(() => null) as { ok?: boolean; core?: { taskId?: string | null }; error?: string } | null;
  return payload ?? { ok: false, error: "invalid_response" };
}

function ganttMutationResource(kind: GanttItemKind) {
  if (kind === "work_item") return "work-items";
  if (kind === "milestone") return "milestones";
  return "projects";
}

function ProjectTreeRow({ node }: { node: ProjectTreeNode }) {
  const [expanded, setExpanded] = useState(node.level === 0);
  const hasChildren = node.children.length > 0;

  return (
    <li aria-expanded={hasChildren ? expanded : undefined} role="treeitem">
      <div className="ops-ported-tree-row" style={{ "--tree-level": node.level } as CSSProperties}>
        {hasChildren ? (
          <button aria-label={expanded ? "Collapse project" : "Expand project"} onClick={() => setExpanded((value) => !value)} type="button">
            {expanded ? "v" : ">"}
          </button>
        ) : <span aria-hidden="true" />}
        <a
          data-context-item
          data-context-label={node.name}
          data-context-module="operations"
          data-context-record-id={node.id}
          data-context-record-type="project"
          data-context-submodule="projects"
          href={node.href}
        >
          <strong>{node.code} - {node.name}</strong>
          <small>{node.health} - {node.progress}% - {node.memberCount} members</small>
        </a>
      </div>
      {hasChildren && expanded ? (
        <ul role="group">
          {node.children.map((child) => <ProjectTreeRow key={child.id} node={child} />)}
        </ul>
      ) : null}
    </li>
  );
}

function buildProjectTree(projects: TreeProject[]) {
  const nodes = new Map<string, ProjectTreeNode>();
  projects.forEach((project) => nodes.set(project.id, { ...project, children: [], level: 0 }));
  const roots: ProjectTreeNode[] = [];

  nodes.forEach((node) => {
    const parent = node.parentProjectId ? nodes.get(node.parentProjectId) : undefined;
    if (parent) {
      node.level = parent.level + 1;
      parent.children.push(node);
    } else {
      roots.push(node);
    }
  });

  const assignLevel = (node: ProjectTreeNode, level: number) => {
    node.level = level;
    node.children.forEach((child) => assignLevel(child, level + 1));
  };
  roots.forEach((node) => assignLevel(node, 0));
  return roots;
}

function buildGanttRange(items: OperationsGanttItem[]) {
  const today = formatDateInput(new Date());
  const starts = items.map((item) => parseDateInput(item.start)).filter(Boolean) as Date[];
  const ends = items.map((item) => parseDateInput(item.end)).filter(Boolean) as Date[];
  const fallbackStart = parseDateInput(today) ?? new Date();
  const minStart = starts.length > 0 ? new Date(Math.min(...starts.map((date) => date.getTime()))) : fallbackStart;
  const maxEnd = ends.length > 0 ? new Date(Math.max(...ends.map((date) => date.getTime()))) : addDays(fallbackStart, 28);
  const start = formatDateInput(addDays(minStart, -4));
  const end = formatDateInput(addDays(maxEnd, 6));
  const totalDays = Math.max(14, daysBetween(start, end));
  const pixelPerDay = totalDays > 150 ? 8 : totalDays > 90 ? 12 : totalDays > 45 ? 18 : 26;

  return {
    end,
    months: buildMonthBands(start, end, pixelPerDay),
    pixelPerDay,
    start,
    ticks: buildDayTicks(start, end, pixelPerDay),
    totalDays,
    weekendBands: buildWeekendBands(start, end, pixelPerDay)
  };
}

function buildMonthBands(start: string, end: string, pixelPerDay: number) {
  const startDate = parseDateInput(start) ?? new Date();
  const endDate = parseDateInput(end) ?? addDays(startDate, 30);
  const bands: Array<{ key: string; label: string; left: number; width: number }> = [];
  let cursor = new Date(startDate.getFullYear(), startDate.getMonth(), 1);

  while (cursor <= endDate) {
    const monthStart = cursor < startDate ? startDate : cursor;
    const nextMonth = new Date(cursor.getFullYear(), cursor.getMonth() + 1, 1);
    const monthEnd = nextMonth > endDate ? endDate : nextMonth;
    bands.push({
      key: `${cursor.getFullYear()}-${cursor.getMonth()}`,
      label: cursor.toLocaleString("en", { month: "short", year: "numeric" }),
      left: dateOffsetPx(formatDateInput(monthStart), start, pixelPerDay),
      width: Math.max(76, daysBetween(formatDateInput(monthStart), formatDateInput(monthEnd)) * pixelPerDay)
    });
    cursor = nextMonth;
  }

  return bands;
}

function buildDayTicks(start: string, end: string, pixelPerDay: number) {
  const totalDays = daysBetween(start, end);
  const step = pixelPerDay < 10 ? 14 : pixelPerDay < 18 ? 7 : 2;
  const ticks: Array<{ key: string; label: string; left: number }> = [];
  const startDate = parseDateInput(start) ?? new Date();

  for (let index = 0; index <= totalDays; index += step) {
    const date = addDays(startDate, index);
    ticks.push({
      key: formatDateInput(date),
      label: String(date.getDate()).padStart(2, "0"),
      left: index * pixelPerDay
    });
  }

  return ticks;
}

function buildWeekendBands(start: string, end: string, pixelPerDay: number) {
  const totalDays = daysBetween(start, end);
  const bands: Array<{ key: string; left: number; width: number }> = [];
  const startDate = parseDateInput(start) ?? new Date();

  for (let index = 0; index <= totalDays; index += 1) {
    const date = addDays(startDate, index);
    if (date.getDay() === 0 || date.getDay() === 6) {
      bands.push({ key: formatDateInput(date), left: index * pixelPerDay, width: pixelPerDay });
    }
  }

  return bands;
}

function dateOffsetPx(date: string, start: string, pixelPerDay: number) {
  const startDate = parseDateInput(start);
  const dateValue = parseDateInput(date);
  if (!startDate || !dateValue) return 0;
  return Math.max(0, Math.round((dateValue.getTime() - startDate.getTime()) / 86_400_000) * pixelPerDay);
}

function daysBetween(start: string, end: string) {
  const startDate = parseDateInput(start);
  const endDate = parseDateInput(end);
  if (!startDate || !endDate) return 1;
  return Math.max(1, Math.round((endDate.getTime() - startDate.getTime()) / 86_400_000) + 1);
}

function parseDateInput(value: string) {
  const [year, month, day] = value.split("-").map(Number);
  if (!year || !month || !day) return null;
  return new Date(year, month - 1, day);
}

function addDays(date: Date, days: number) {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function formatDateInput(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function moveCard(columns: BoardColumn[], cardId: string, targetColumnId: string) {
  let moved: BoardCard | undefined;
  const without = columns.map((column) => ({
    ...column,
    cards: column.cards.filter((card) => {
      if (card.id !== cardId) return true;
      moved = { ...card, status: columnTitle(targetColumnId, columns) };
      return false;
    })
  }));
  if (!moved) return columns;
  const movedCard = moved;
  return without.map((column) => column.id === targetColumnId ? { ...column, cards: [...column.cards, movedCard] } : column);
}

function columnTitle(columnId: string, columns: BoardColumn[]) {
  return columns.find((column) => column.id === columnId)?.title ?? columnId;
}

function moveAgendaItem(meetings: AgendaState, meetingId: string, from: number, to: number) {
  return meetings.map((meeting) => {
    if (meeting.id !== meetingId || to < 0 || to >= meeting.agenda.length) return meeting;
    const agenda = [...meeting.agenda];
    const [item] = agenda.splice(from, 1);
    if (!item) return meeting;
    agenda.splice(to, 0, item);
    return { ...meeting, agenda };
  });
}

function reorderAgenda(meetings: AgendaState, meetingId: string, sourceId: string, targetId: string) {
  return meetings.map((meeting) => {
    if (meeting.id !== meetingId || !sourceId || sourceId === targetId) return meeting;
    const from = meeting.agenda.findIndex((item) => item.id === sourceId);
    const to = meeting.agenda.findIndex((item) => item.id === targetId);
    if (from < 0 || to < 0) return meeting;
    return moveAgendaItem([meeting], meetingId, from, to)[0] ?? meeting;
  });
}
