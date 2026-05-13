"use client";

import { useState } from "react";
import { businessApiPath } from "@/lib/business-api-path";

type MutationAction = "create" | "update" | "delete" | "sync" | "extract" | "reschedule";

type MutationResponse = {
  ok?: boolean;
  mutation?: {
    recordId?: string;
    deepLink?: {
      href?: string;
      url?: string;
    } | null;
  };
  core?: {
    mode?: string;
    taskId?: string | null;
  };
  error?: string;
};

type Option = {
  label: string;
  value: string;
};

const priorities = ["Low", "Normal", "High", "Urgent"];
const statuses = ["Backlog", "Ready", "In progress", "Review", "Done"];

export function OperationsQueueButton({
  action = "update",
  children,
  className,
  instruction,
  payload,
  recordId,
  resource,
  title
}: {
  action?: MutationAction;
  children: React.ReactNode;
  className?: string;
  instruction?: string;
  payload?: Record<string, unknown>;
  recordId?: string;
  resource: string;
  title?: string;
}) {
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  return (
    <span className="ops-action-inline">
      <button
        className={className}
        disabled={status === "submitting"}
        onClick={async () => {
          setStatus("submitting");
          setMessage("");
          const result = await postMutation(resource, {
            action,
            instruction,
            payload,
            recordId,
            title
          });
          if (result.ok) {
            setStatus("queued");
            setMessage(result.core?.taskId ? `Queued ${result.core.taskId}` : `Queued (${result.core?.mode ?? "planned"})`);
          } else {
            setStatus("error");
            setMessage(result.error ?? "Queue failed");
          }
        }}
        type="button"
      >
        {status === "submitting" ? "Queueing..." : children}
      </button>
      {message ? <small>{message}</small> : null}
    </span>
  );
}

export function OperationsCreateForm({
  defaultOwner = "CTOX Agent",
  dueLabel,
  ownerLabel,
  owners = [],
  projectLabel = "Project",
  projects = [],
  queueLabel,
  resource,
  subjectLabel,
  subjectPlaceholder
}: {
  defaultOwner?: string;
  dueLabel: string;
  ownerLabel: string;
  owners?: Option[];
  projectLabel?: string;
  projects?: Option[];
  queueLabel: string;
  resource: string;
  subjectLabel: string;
  subjectPlaceholder: string;
}) {
  const [subject, setSubject] = useState("");
  const [owner, setOwner] = useState(owners[0]?.value ?? defaultOwner);
  const [projectId, setProjectId] = useState(projects[0]?.value ?? "");
  const [due, setDue] = useState("");
  const [priority, setPriority] = useState("Normal");
  const [statusValue, setStatusValue] = useState("Backlog");
  const [details, setDetails] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  return (
    <form
      className="ops-create-form"
      onSubmit={async (event) => {
        event.preventDefault();
        setStatus("submitting");
        setMessage("");
        const result = await postMutation(resource, {
          action: "create",
          title: subject ? `Create ${resource}: ${subject}` : `Create ${resource}`,
          payload: { subject, owner, projectId, due, priority, status: statusValue, details },
          instruction: `Create a new Operations ${resource} record and keep it synchronized with CTOX.`
        });
        if (result.ok) {
          setStatus("queued");
          setMessage(result.core?.taskId ? `Queued ${result.core.taskId}` : `Queued (${result.core?.mode ?? "planned"})`);
        } else {
          setStatus("error");
          setMessage(result.error ?? "Queue failed");
        }
      }}
    >
      <label className="drawer-field">
        {subjectLabel}
        <input onChange={(event) => setSubject(event.target.value)} placeholder={subjectPlaceholder} type="text" value={subject} />
      </label>
      {projects.length > 0 ? (
        <label className="drawer-field">
          {projectLabel}
          <select onChange={(event) => setProjectId(event.target.value)} value={projectId}>
            {projects.map((project) => <option key={project.value} value={project.value}>{project.label}</option>)}
          </select>
        </label>
      ) : null}
      <label className="drawer-field">
        {ownerLabel}
        {owners.length > 0 ? (
          <select onChange={(event) => setOwner(event.target.value)} value={owner}>
            {owners.map((candidate) => <option key={candidate.value} value={candidate.value}>{candidate.label}</option>)}
          </select>
        ) : (
          <input onChange={(event) => setOwner(event.target.value)} placeholder="CTOX Agent" type="text" value={owner} />
        )}
      </label>
      <label className="drawer-field">
        {dueLabel}
        <input onChange={(event) => setDue(event.target.value)} type="date" value={due} />
      </label>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          Priority
          <select onChange={(event) => setPriority(event.target.value)} value={priority}>
            {priorities.map((item) => <option key={item} value={item}>{item}</option>)}
          </select>
        </label>
        <label className="drawer-field">
          Status
          <select onChange={(event) => setStatusValue(event.target.value)} value={statusValue}>
            {statuses.map((item) => <option key={item} value={item}>{item}</option>)}
          </select>
        </label>
      </div>
      <label className="drawer-field">
        Details
        <textarea onChange={(event) => setDetails(event.target.value)} placeholder="Add operational context, acceptance criteria, or meeting notes." value={details} />
      </label>
      <button className="drawer-primary" disabled={status === "submitting" || !subject.trim()} type="submit">
        {status === "submitting" ? "Queueing..." : queueLabel}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </form>
  );
}

export function OperationsKnowledgeCreateForm({
  cluster,
  element,
  filePath,
  group,
  queueLabel,
  skillId,
  skillTitle,
  sourcePath
}: {
  cluster?: string;
  element: "skill" | "skill_file" | "skillbook" | "runbook";
  filePath?: string;
  group?: string;
  queueLabel: string;
  skillId?: string;
  skillTitle?: string;
  sourcePath?: string;
}) {
  const [title, setTitle] = useState(defaultKnowledgeTitle(element, skillTitle));
  const [path, setPath] = useState(filePath ?? defaultKnowledgePath(element));
  const [summary, setSummary] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  return (
    <form
      className="ops-create-form"
      onSubmit={async (event) => {
        event.preventDefault();
        setStatus("submitting");
        setMessage("");
        const target = knowledgeElementLabel(element);
        const result = await postCtoxPrompt({
          instruction: [
            `Create ${target} ${title.trim() || path.trim()} in the CTOX Knowledge Store.`,
            skillId ? `Attach it to skill ${skillTitle ?? skillId}.` : "Create it in the selected skill group.",
            path.trim() ? `Use path or asset name ${path.trim()}.` : "",
            summary.trim() ? `Purpose: ${summary.trim()}` : "",
            "Preserve the System Skills / Skills hierarchy and return the concrete file, SQLite, or queued patch plan."
          ].filter(Boolean).join(" "),
          context: {
            source: "knowledge-create-form",
            items: [{
              action: "create",
              filePath: path.trim() || undefined,
              group,
              moduleId: "operations",
              recordId: `new-${element}`,
              recordType: `ctox_${element}`,
              label: title.trim() || target,
              skillId,
              sourcePath,
              submoduleId: "knowledge"
            }]
          }
        });
        if (result.ok) {
          setStatus("queued");
          setMessage(result.taskId ? `Queued ${result.taskId}` : "Queued");
        } else {
          setStatus("error");
          setMessage(result.error ?? "Queue failed");
        }
      }}
    >
      <label className="drawer-field">
        Type
        <input readOnly value={knowledgeElementLabel(element)} />
      </label>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          Group
          <input readOnly value={cluster ?? group ?? "knowledge"} />
        </label>
        <label className="drawer-field">
          Skill
          <input readOnly value={skillTitle ?? skillId ?? "new skill"} />
        </label>
      </div>
      <label className="drawer-field">
        Name
        <input onChange={(event) => setTitle(event.target.value)} placeholder="Name the new knowledge element" type="text" value={title} />
      </label>
      <label className="drawer-field">
        File or asset path
        <input onChange={(event) => setPath(event.target.value)} placeholder={defaultKnowledgePath(element)} type="text" value={path} />
      </label>
      <label className="drawer-field">
        Purpose
        <textarea onChange={(event) => setSummary(event.target.value)} placeholder="What should CTOX learn, execute, or keep synchronized here?" value={summary} />
      </label>
      <button className="drawer-primary" disabled={status === "submitting" || (!title.trim() && !path.trim())} type="submit">
        {status === "submitting" ? "Queueing..." : queueLabel}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </form>
  );
}

export function OperationsWorkItemEditor({
  assigneeLabel,
  assignees,
  dueLabel,
  item,
  priorityLabel,
  saveLabel,
  statusLabel,
  syncLabel
}: {
  assigneeLabel: string;
  assignees: Option[];
  dueLabel: string;
  item: {
    assigneeId: string;
    description: string;
    due: string;
    id: string;
    priority: string;
    status: string;
    subject: string;
  };
  priorityLabel: string;
  saveLabel: string;
  statusLabel: string;
  syncLabel: string;
}) {
  const [draft, setDraft] = useState({
    assigneeId: item.assigneeId,
    description: item.description,
    due: item.due,
    priority: item.priority,
    status: item.status
  });
  const payload = { workItemId: item.id, subject: item.subject, draft };

  return (
    <div className="ops-inline-editor">
      <label className="drawer-field">
        Details
        <textarea onChange={(event) => setDraft({ ...draft, description: event.target.value })} value={draft.description} />
      </label>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          {statusLabel}
          <select onChange={(event) => setDraft({ ...draft, status: event.target.value })} value={draft.status}>
            {statuses.map((status) => <option key={status} value={status}>{status}</option>)}
          </select>
        </label>
        <label className="drawer-field">
          {priorityLabel}
          <select onChange={(event) => setDraft({ ...draft, priority: event.target.value })} value={draft.priority}>
            {priorities.map((priority) => <option key={priority} value={priority}>{priority}</option>)}
          </select>
        </label>
      </div>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          {assigneeLabel}
          <select onChange={(event) => setDraft({ ...draft, assigneeId: event.target.value })} value={draft.assigneeId}>
            {assignees.map((assignee) => <option key={assignee.value} value={assignee.value}>{assignee.label}</option>)}
          </select>
        </label>
        <label className="drawer-field">
          {dueLabel}
          <input onChange={(event) => setDraft({ ...draft, due: event.target.value })} type="date" value={draft.due} />
        </label>
      </div>
      <div className="ops-drawer-actions">
        <OperationsQueueButton
          action="update"
          instruction={`Save the current draft changes for Operations work item ${item.subject}.`}
          payload={payload}
          recordId={item.id}
          resource="work-items"
          title={`Save draft: ${item.subject}`}
        >
          {saveLabel}
        </OperationsQueueButton>
        <OperationsQueueButton
          action="sync"
          instruction={`Update and synchronize Operations work item ${item.subject} with CTOX core, related knowledge, boards, and planning views.`}
          payload={payload}
          recordId={item.id}
          resource="work-items"
          title={`Sync work item: ${item.subject}`}
        >
          {syncLabel}
        </OperationsQueueButton>
      </div>
    </div>
  );
}

async function postMutation(resource: string, body: Record<string, unknown>): Promise<MutationResponse> {
  const response = await fetch(`/api/operations/${resource}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  const payload = await response.json().catch(() => null) as MutationResponse | null;
  return payload ?? { ok: false, error: "invalid_response" };
}

async function postCtoxPrompt(body: Record<string, unknown>): Promise<{ ok?: boolean; taskId?: string; error?: string }> {
  const response = await fetch(businessApiPath("/api/ctox/queue-tasks"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  const payload = await response.json().catch(() => null) as { ok?: boolean; task?: { id?: string }; error?: string } | null;
  return { ok: response.ok && payload?.ok !== false, taskId: payload?.task?.id, error: payload?.error };
}

function defaultKnowledgeTitle(element: "skill" | "skill_file" | "skillbook" | "runbook", skillTitle?: string) {
  if (element === "skill") return "";
  if (element === "skill_file") return "New skill asset";
  if (element === "skillbook") return `${skillTitle ?? "Skill"} skillbook`;
  return `${skillTitle ?? "Skill"} runbook`;
}

function defaultKnowledgePath(element: "skill" | "skill_file" | "skillbook" | "runbook") {
  if (element === "skill_file") return "assets/new-asset.md";
  if (element === "skillbook") return "skillbook.md";
  if (element === "runbook") return "runbooks/new-runbook.md";
  return "SKILL.md";
}

function knowledgeElementLabel(element: "skill" | "skill_file" | "skillbook" | "runbook") {
  if (element === "skill_file") return "Skill file";
  if (element === "skillbook") return "Skillbook";
  if (element === "runbook") return "Runbook";
  return "Skill";
}
