"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  getAvailableQuantity,
  SYSTEM_OWNER_PARTY_ID,
  WAREHOUSE_COMPANY_ID,
  type WarehouseState
} from "@ctox-business/warehouse";

export function WarehouseSimulator({
  cancelLabel,
  initialSnapshot,
  reserveLabel,
  releaseLabel,
  pickLabel,
  shipLabel
}: {
  cancelLabel: string;
  initialSnapshot: WarehouseState;
  reserveLabel: string;
  releaseLabel: string;
  pickLabel: string;
  shipLabel: string;
}) {
  const router = useRouter();
  const [state, setState] = useState(initialSnapshot);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [message, setMessage] = useState("Ready");
  useEffect(() => {
    setState(initialSnapshot);
  }, [initialSnapshot]);
  const simulatorReservation = [...state.reservations]
    .reverse()
    .find((reservation) => reservation.id.startsWith("sim-reserve-"));
  const openReservation = simulatorReservation &&
    simulatorReservation.status !== "cancelled" &&
    simulatorReservation.status !== "consumed" &&
    simulatorReservation.status !== "released"
    ? simulatorReservation
    : undefined;
  const pickedReservation = openReservation?.lines.some((line) => line.pickedQuantity > line.shippedQuantity) ? openReservation : undefined;
  const availableCore = useMemo(() => getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), [state]);

  async function run(action: "reserve" | "release" | "cancel" | "pick" | "ship", successMessage: string) {
    setBusyAction(action);
    try {
      const response = await fetch("/api/business/warehouse", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ warehouseAction: action })
      });
      const payload = await response.json() as {
        error?: string;
        ok?: boolean;
        snapshot?: WarehouseState;
      };
      if (!response.ok || payload.ok === false || !payload.snapshot) {
        throw new Error(payload.error ?? "Action failed");
      }
      setState(payload.snapshot);
      router.refresh();
      setMessage(successMessage);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Action failed");
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <section className="warehouse-simulator" aria-label="Warehouse command simulator">
      <div>
        <strong>Manual reservation</strong>
        <span>{message}</span>
      </div>
      <div className="warehouse-command-row">
        <button
          type="button"
          onClick={() => void run("reserve", "Reserved 2 CTOX Core Kit units in Postgres")}
          disabled={Boolean(openReservation) || busyAction !== null}
        >
          {reserveLabel}
        </button>
        <button
          type="button"
          onClick={() => void run("release", "Released simulator reservation in Postgres")}
          disabled={!openReservation || pickedReservation !== undefined || busyAction !== null}
        >
          {releaseLabel}
        </button>
        <button
          type="button"
          onClick={() => void run("cancel", "Cancelled simulator reservation in Postgres")}
          disabled={!openReservation || pickedReservation !== undefined || busyAction !== null}
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          onClick={() => void run("pick", "Picked simulator reservation in Postgres")}
          disabled={!openReservation || pickedReservation !== undefined || busyAction !== null}
        >
          {pickLabel}
        </button>
        <button
          type="button"
          onClick={() => void run("ship", "Shipped simulator reservation in Postgres")}
          disabled={!pickedReservation || busyAction !== null}
        >
          {shipLabel}
        </button>
      </div>
      <dl className="warehouse-sim-metrics">
        <div><dt>Available Core Kit</dt><dd>{availableCore}</dd></div>
        <div><dt>Reservations</dt><dd>{state.reservations.length}</dd></div>
        <div><dt>Movements</dt><dd>{state.movements.length}</dd></div>
        <div><dt>Outbox</dt><dd>{state.outbox.length}</dd></div>
      </dl>
    </section>
  );
}
