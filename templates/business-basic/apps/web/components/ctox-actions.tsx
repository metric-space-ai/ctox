"use client";

import { useState } from "react";
import { businessApiPath } from "@/lib/business-api-path";

type CtoxQueueResponse = {
  ok?: boolean;
  core?: {
    mode?: string;
    taskId?: string | null;
  };
  error?: string;
};

export function CtoxQueueButton({
  instruction,
  label,
  recordId,
  recordType,
  submoduleId
}: {
  instruction: string;
  label: string;
  recordId: string;
  recordType: string;
  submoduleId: string;
}) {
  const [status, setStatus] = useState<"idle" | "submitting" | "queued" | "error">("idle");
  const [message, setMessage] = useState("");

  return (
    <span className="ops-action-inline">
      <button
        className="drawer-primary"
        disabled={status === "submitting"}
        onClick={async () => {
          setStatus("submitting");
          setMessage("");
          const result = await postQueue({
            instruction,
            context: {
              source: "ctox-module",
              items: [{
                moduleId: "ctox",
                submoduleId,
                recordType,
                recordId,
                label
              }]
            }
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
        {status === "submitting" ? "Queueing..." : "Queue instruction"}
      </button>
      {message ? <small className="ops-action-status">{message}</small> : null}
    </span>
  );
}

async function postQueue(body: Record<string, unknown>): Promise<CtoxQueueResponse> {
  const response = await fetch(businessApiPath("/api/ctox/queue-tasks"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });

  return response.json().catch(() => ({ ok: false, error: "Invalid response" })) as Promise<CtoxQueueResponse>;
}
