"use client";

import { useState } from "react";

export function AccountingApiButton({ label, path }: { label: string; path: string }) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  async function run() {
    setBusy(true);
    setStatus("");
    try {
      const response = await fetch(path, { method: "POST" });
      const payload = await response.json().catch(() => ({})) as { command?: { type?: string }; error?: string; persisted?: boolean; snapshot?: { accounts?: unknown[] }; period?: { status?: string } };
      if (!response.ok || payload.error) {
        setStatus(payload.error ?? "Command failed.");
        return;
      }
      if (payload.command?.type) setStatus(`${payload.command.type} prepared.`);
      else if (payload.snapshot?.accounts) setStatus(`${payload.snapshot.accounts.length} accounts prepared.`);
      else if (payload.period?.status) setStatus(`Period ${payload.period.status}.`);
      else setStatus(payload.persisted ? "Persisted." : "Prepared.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="accounting-command-control">
      <button className="drawer-primary" disabled={busy} onClick={run} type="button">
        {busy ? "..." : label}
      </button>
      {status ? <small className="invoice-delivery-status is-sent">{status}</small> : null}
    </div>
  );
}
