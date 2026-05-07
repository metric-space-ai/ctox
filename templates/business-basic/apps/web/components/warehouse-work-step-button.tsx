"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import type { WarehouseWorkStep } from "../lib/warehouse-runtime";

export function WarehouseWorkStepButton({
  disabled,
  done,
  label,
  lineId,
  reservationId,
  sourceId,
  step
}: {
  disabled?: boolean;
  done?: boolean;
  label: string;
  lineId?: string;
  reservationId: string;
  sourceId: string;
  step: WarehouseWorkStep;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);

  async function run() {
    setBusy(true);
    try {
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          lineId,
          reservationId,
          workStep: step
        })
      });
      const payload = await response.json() as { error?: string; ok?: boolean };
      if (!response.ok || payload.ok === false) throw new Error(payload.error ?? "Work step failed");
      router.refresh();
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      className={done ? "is-done" : disabled ? undefined : "is-active"}
      data-context-item
      data-context-label={`${sourceId} ${label}`}
      data-context-module="business"
      data-context-record-id={`${reservationId}:${lineId ?? "order"}:${step}`}
      data-context-record-type="warehouse_work_step"
      data-context-submodule="warehouse"
      disabled={disabled || done || busy}
      onClick={() => void run()}
      type="button"
    >
      {busy ? "Working" : label}
    </button>
  );
}
