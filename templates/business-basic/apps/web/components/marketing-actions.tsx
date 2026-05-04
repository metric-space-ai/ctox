"use client";

import { useState } from "react";

type MarketingAction = "create" | "update" | "delete" | "sync" | "publish" | "schedule";

type MarketingMutationResponse = {
  ok?: boolean;
  core?: {
    mode?: string;
    taskId?: string | null;
  };
  error?: string;
};

export function MarketingQueueButton({
  action = "sync",
  children,
  className,
  instruction,
  payload,
  recordId,
  resource,
  title
}: {
  action?: MarketingAction;
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
          const result = await postMutation(resource, { action, instruction, payload, recordId, title });
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
      {message ? <small className="ops-action-status">{message}</small> : null}
    </span>
  );
}

export function MarketingCreateForm({
  ownerOptions,
  resource,
  resourceLabel
}: {
  ownerOptions: Array<{ label: string; value: string }>;
  resource: string;
  resourceLabel: string;
}) {
  const [title, setTitle] = useState("");
  const [ownerId, setOwnerId] = useState(ownerOptions[0]?.value ?? "");
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
          title: `Create marketing ${resource}: ${title}`,
          payload: { title, ownerId },
          instruction: `Create a new Marketing ${resourceLabel} record and wire it into the Business OS shell, CTOX prompts, bug reporting, and cross-module links.`
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
        Name
        <input onChange={(event) => setTitle(event.target.value)} placeholder={`New ${resourceLabel}`} type="text" value={title} />
      </label>
      <label className="drawer-field">
        Owner
        <select onChange={(event) => setOwnerId(event.target.value)} value={ownerId}>
          {ownerOptions.map((owner) => <option key={owner.value} value={owner.value}>{owner.label}</option>)}
        </select>
      </label>
      <button className="drawer-primary" disabled={status === "submitting" || !title.trim()} type="submit">
        {status === "submitting" ? "Queueing..." : `Queue ${resourceLabel}`}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </form>
  );
}

async function postMutation(resource: string, body: Record<string, unknown>): Promise<MarketingMutationResponse> {
  const response = await fetch(`/api/marketing/${resource}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });

  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<MarketingMutationResponse>;
}
