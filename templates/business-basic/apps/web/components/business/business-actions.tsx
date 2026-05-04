"use client";

import { useState } from "react";

type MutationAction = "create" | "update" | "delete" | "sync" | "export" | "payment";

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

const statuses = ["Draft", "Active", "Review", "Sent", "Paid", "Export ready", "Needs review"];

export function BusinessQueueButton({
  action = "sync",
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
          const result = await postBusinessMutation(resource, {
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

export function BusinessCreateForm({
  amountLabel,
  customerLabel,
  customers = [],
  dueLabel,
  queueLabel,
  resource,
  statusLabel,
  subjectLabel,
  subjectPlaceholder,
  taxLabel
}: {
  amountLabel: string;
  customerLabel: string;
  customers?: Option[];
  dueLabel: string;
  queueLabel: string;
  resource: string;
  statusLabel: string;
  subjectLabel: string;
  subjectPlaceholder: string;
  taxLabel: string;
}) {
  const [subject, setSubject] = useState("");
  const [customerId, setCustomerId] = useState(customers[0]?.value ?? "");
  const [amount, setAmount] = useState("");
  const [tax, setTax] = useState("19");
  const [due, setDue] = useState("");
  const [statusValue, setStatusValue] = useState(statuses[0]);
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
        const result = await postBusinessMutation(resource, {
          action: "create",
          title: subject ? `Create ${resource}: ${subject}` : `Create ${resource}`,
          payload: { subject, customerId, amount, tax, due, status: statusValue, details },
          instruction: `Create a new Business ${resource} record and keep tax, export, and CTOX queue context connected.`
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
      {customers.length > 0 ? (
        <label className="drawer-field">
          {customerLabel}
          <select onChange={(event) => setCustomerId(event.target.value)} value={customerId}>
            {customers.map((customer) => <option key={customer.value} value={customer.value}>{customer.label}</option>)}
          </select>
        </label>
      ) : null}
      <div className="drawer-field-grid">
        <label className="drawer-field">
          {amountLabel}
          <input min="0" onChange={(event) => setAmount(event.target.value)} placeholder="0" step="0.01" type="number" value={amount} />
        </label>
        <label className="drawer-field">
          {taxLabel}
          <input min="0" onChange={(event) => setTax(event.target.value)} step="0.01" type="number" value={tax} />
        </label>
      </div>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          {dueLabel}
          <input onChange={(event) => setDue(event.target.value)} type="date" value={due} />
        </label>
        <label className="drawer-field">
          {statusLabel}
          <select onChange={(event) => setStatusValue(event.target.value)} value={statusValue}>
            {statuses.map((item) => <option key={item} value={item}>{item}</option>)}
          </select>
        </label>
      </div>
      <label className="drawer-field">
        Details
        <textarea onChange={(event) => setDetails(event.target.value)} placeholder="Add billing, tax, export, or report context." value={details} />
      </label>
      <button className="drawer-primary" disabled={status === "submitting" || !subject.trim()} type="submit">
        {status === "submitting" ? "Queueing..." : queueLabel}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </form>
  );
}

async function postBusinessMutation(resource: string, body: Record<string, unknown>): Promise<MutationResponse> {
  const response = await fetch(`/api/business/${resource}`, {
    body: JSON.stringify(body),
    headers: { "content-type": "application/json" },
    method: "POST"
  });

  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<MutationResponse>;
}
