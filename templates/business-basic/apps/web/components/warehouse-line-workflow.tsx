"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

type Stage = "material" | "build" | "qa" | "pack" | "ready";

const stages: Array<{ key: Stage; label: string }> = [
  { key: "material", label: "Material" },
  { key: "build", label: "Build" },
  { key: "qa", label: "QA" },
  { key: "pack", label: "Pack" },
  { key: "ready", label: "Ready" }
];

export function WarehouseLineWorkflow({
  basisScore,
  buildScore,
  currentStage,
  evidenceHref,
  label,
  lineId,
  packScore,
  qaScore,
  reservationId,
  sourceId
}: {
  basisScore: number;
  buildScore: number;
  currentStage: Stage;
  evidenceHref: string;
  label: string;
  lineId: string;
  packScore: number;
  qaScore: number;
  reservationId: string;
  sourceId: string;
}) {
  const router = useRouter();
  const [busyStage, setBusyStage] = useState<Stage | null>(null);

  function stageState(stage: Stage) {
    if (stage === currentStage) return "is-active";
    if (stage === "material" && basisScore >= 100) return "is-done";
    if (stage === "build" && buildScore >= 100) return "is-done";
    if (stage === "qa" && qaScore >= 100) return "is-done";
    if (stage === "pack" && packScore >= 100) return "is-done";
    if (stage === "ready" && currentStage === "ready") return "is-done";
    return "";
  }

  function canMove(stage: Stage) {
    if (stage === currentStage || busyStage) return false;
    if (stage === "material") return true;
    if (stage === "build") return basisScore >= 100 && buildScore < 100;
    if (stage === "qa") return buildScore >= 100 && qaScore < 100;
    if (stage === "pack") return qaScore >= 100 && packScore < 100;
    return packScore >= 100;
  }

  async function moveTo(stage: Stage) {
    if (!canMove(stage)) return;
    setBusyStage(stage);
    try {
      const body = stage === "material"
        ? { reservationId, warehouseAction: "pick" }
        : stage === "ready"
          ? { reservationId, warehouseAction: "ship" }
          : { lineId, reservationId, workStep: stage };
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body)
      });
      const payload = await response.json() as { error?: string; ok?: boolean };
      if (!response.ok || payload.ok === false) throw new Error(payload.error ?? "Workflow update failed");
      router.refresh();
    } finally {
      setBusyStage(null);
    }
  }

  return (
    <div
      className="warehouse-line-dnd"
      data-context-item
      data-context-label={`${sourceId} ${label}`}
      data-context-module="business"
      data-context-record-id={`${reservationId}:${lineId}`}
      data-context-record-type="fulfillment_line_workflow"
      data-context-submodule="fulfillment"
      draggable
      onDragStart={(event) => {
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("application/x-ctox-warehouse-line", JSON.stringify({ lineId, reservationId }));
        event.dataTransfer.setData("text/plain", `${sourceId} ${label}`);
      }}
    >
      <div className="warehouse-line-status" aria-label="Item workflow">
        {stages.map((stage) => (
          <button
            aria-current={stage.key === currentStage ? "step" : undefined}
            className={stageState(stage.key)}
            disabled={!canMove(stage.key)}
            key={stage.key}
            onClick={() => void moveTo(stage.key)}
            onDragOver={(event) => {
              if (canMove(stage.key)) event.preventDefault();
            }}
            onDrop={(event) => {
              event.preventDefault();
              const data = event.dataTransfer.getData("application/x-ctox-warehouse-line");
              if (!data) return;
              void moveTo(stage.key);
            }}
            title={canMove(stage.key) ? `Move ${label} to ${stage.label}` : stage.label}
            type="button"
          >
            {busyStage === stage.key ? "..." : stage.label}
          </button>
        ))}
      </div>
      <div className="warehouse-line-actions">
        <a href={evidenceHref}>Score</a>
      </div>
    </div>
  );
}
