"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import type { WarehouseLayoutAction } from "../lib/warehouse-runtime";

export function WarehouseLayoutActions({
  sectionId,
  warehouseId
}: {
  sectionId?: string;
  warehouseId: string;
}) {
  const router = useRouter();
  const [busyAction, setBusyAction] = useState<WarehouseLayoutAction | null>(null);

  async function run(layoutAction: WarehouseLayoutAction, parentId?: string) {
    setBusyAction(layoutAction);
    try {
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          layoutAction,
          parentId,
          slotCount: 4
        })
      });
      const payload = await response.json() as { error?: string; ok?: boolean };
      if (!response.ok || payload.ok === false) throw new Error(payload.error ?? "Layout update failed");
      router.refresh();
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <div className="warehouse-layout-actions" aria-label="Warehouse layout actions">
      <button type="button" disabled={busyAction !== null} onClick={() => void run("createWarehouse")}>
        New warehouse
      </button>
      <button type="button" disabled={busyAction !== null} onClick={() => void run("createSection", warehouseId)}>
        Add section
      </button>
      <button type="button" disabled={busyAction !== null || !sectionId} onClick={() => void run("createSlot", sectionId)}>
        Add slots
      </button>
    </div>
  );
}
