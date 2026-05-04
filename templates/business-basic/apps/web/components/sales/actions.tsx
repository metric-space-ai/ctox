"use client";

import { useState } from "react";

type SalesAction = "create" | "update" | "delete" | "sync" | "convert";

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

export function SalesQueueButton({
  action = "update",
  children,
  className,
  errorLabel = "Failed",
  instruction,
  payload,
  pendingLabel = "...",
  recordId,
  resource,
  successLabel = "OK",
  title
}: {
  action?: SalesAction;
  children: React.ReactNode;
  className?: string;
  errorLabel?: string;
  instruction?: string;
  payload?: Record<string, unknown>;
  pendingLabel?: string;
  recordId?: string;
  resource: string;
  successLabel?: string;
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
            setMessage(successLabel);
          } else {
            setStatus("error");
            setMessage(result.error ?? errorLabel);
          }
        }}
        type="button"
      >
        {status === "submitting" ? pendingLabel : children}
      </button>
      {message ? <small>{message}</small> : null}
    </span>
  );
}

export function SalesCreateForm({
  accounts,
  contacts,
  owners,
  queueLabel,
  resource
}: {
  accounts: Option[];
  contacts: Option[];
  owners: Option[];
  queueLabel: string;
  resource: string;
}) {
  const [name, setName] = useState("");
  const [ownerId, setOwnerId] = useState(owners[0]?.value ?? "");
  const [accountId, setAccountId] = useState(accounts[0]?.value ?? "");
  const [contactId, setContactId] = useState(contacts[0]?.value ?? "");
  const [value, setValue] = useState("12000");
  const [date, setDate] = useState("");
  const [nextStep, setNextStep] = useState("");
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
          title: name ? `Create ${resource}: ${name}` : `Create ${resource}`,
          payload: { name, ownerId, accountId, contactId, value, date, nextStep },
          instruction: `Create a new Sales ${resource} record, link it to the CRM context, and keep CTOX synchronized.`
        });
        if (result.ok) {
          setStatus("queued");
          setMessage("OK");
        } else {
          setStatus("error");
          setMessage(result.error ?? "Failed");
        }
      }}
    >
      <label className="drawer-field">
        Name
        <input onChange={(event) => setName(event.target.value)} placeholder="Record name..." type="text" value={name} />
      </label>
      {resource !== "accounts" && resource !== "customers" ? (
        <label className="drawer-field">
          Account
          <select onChange={(event) => setAccountId(event.target.value)} value={accountId}>
            {accounts.map((account) => <option key={account.value} value={account.value}>{account.label}</option>)}
          </select>
        </label>
      ) : null}
      {resource === "opportunities" || resource === "offers" || resource === "tasks" ? (
        <label className="drawer-field">
          Contact
          <select onChange={(event) => setContactId(event.target.value)} value={contactId}>
            {contacts.map((contact) => <option key={contact.value} value={contact.value}>{contact.label}</option>)}
          </select>
        </label>
      ) : null}
      <label className="drawer-field">
        Owner
        <select onChange={(event) => setOwnerId(event.target.value)} value={ownerId}>
          {owners.map((owner) => <option key={owner.value} value={owner.value}>{owner.label}</option>)}
        </select>
      </label>
      <div className="drawer-field-grid">
        <label className="drawer-field">
          Value
          <input onChange={(event) => setValue(event.target.value)} type="number" value={value} />
        </label>
        <label className="drawer-field">
          Date
          <input onChange={(event) => setDate(event.target.value)} type="date" value={date} />
        </label>
      </div>
      <label className="drawer-field">
        Next step
        <textarea onChange={(event) => setNextStep(event.target.value)} placeholder="What should happen next?" value={nextStep} />
      </label>
      <button className="drawer-primary" disabled={status === "submitting" || !name.trim()} type="submit">
        {status === "submitting" ? "..." : queueLabel}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </form>
  );
}

async function postMutation(resource: string, payload: Record<string, unknown>): Promise<MutationResponse> {
  const response = await fetch(`/api/sales/${resource}`, {
    body: JSON.stringify(payload),
    headers: { "content-type": "application/json" },
    method: "POST"
  });
  return response.json() as Promise<MutationResponse>;
}
