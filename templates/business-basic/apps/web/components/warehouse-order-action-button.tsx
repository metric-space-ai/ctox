"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import type { WarehouseMutationAction } from "../lib/warehouse-runtime";

export function WarehouseOrderActionButton({
  action,
  disabled,
  label,
  reservationId
}: {
  action: WarehouseMutationAction;
  disabled?: boolean;
  label: string;
  reservationId: string;
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
          reservationId,
          warehouseAction: action
        })
      });
      const payload = await response.json() as { error?: string; ok?: boolean };
      if (!response.ok || payload.ok === false) throw new Error(payload.error ?? "Order action failed");
      router.refresh();
    } finally {
      setBusy(false);
    }
  }

  return (
    <button disabled={disabled || busy} onClick={() => void run()} type="button">
      {busy ? "Working" : label}
    </button>
  );
}
