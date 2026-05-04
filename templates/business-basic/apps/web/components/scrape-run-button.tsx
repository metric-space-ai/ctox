"use client";

import { useState } from "react";

type ScrapeRunButtonProps = {
  label: string;
};

export function ScrapeRunButton({ label }: ScrapeRunButtonProps) {
  const [status, setStatus] = useState<"idle" | "running" | "queued" | "failed">("idle");

  async function queueRun() {
    setStatus("running");
    const response = await fetch("/api/marketing/competitive-analysis/scrape", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ mode: "rescrape_now", triggerKind: "manual" })
    });

    setStatus(response.ok ? "queued" : "failed");
  }

  return (
    <button aria-label={status === "queued" ? `${label}: queued` : label} onClick={queueRun} type="button">
      {status === "running" ? "..." : status === "queued" ? "Queued" : status === "failed" ? "Failed" : label}
    </button>
  );
}
