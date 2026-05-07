"use client";

import { useMemo, useState } from "react";
import {
  getAvailableQuantity,
  SYSTEM_OWNER_PARTY_ID,
  WAREHOUSE_COMPANY_ID,
  type WarehouseState
} from "@ctox-business/warehouse";
import type { WarehouseCheckoutEventType } from "../lib/warehouse-runtime";

const checkoutSessionId = "cs_demo_warehouse";
const orderId = "web-order-9001";

export function WarehouseCheckoutSimulator({ initialSnapshot }: { initialSnapshot: WarehouseState }) {
  const [state, setState] = useState(initialSnapshot);
  const [message, setMessage] = useState("Stripe-ready checkout hook");
  const [busyEvent, setBusyEvent] = useState<WarehouseCheckoutEventType | null>(null);
  const reservation = state.reservations.find((item) => item.id === `checkout-${checkoutSessionId}`);
  const shipment = state.shipments.find((item) => item.reservationId === reservation?.id);
  const availableCore = useMemo(() => getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), [state]);

  async function send(eventType: WarehouseCheckoutEventType, successMessage: string) {
    setBusyEvent(eventType);
    try {
      const response = await fetch("/api/business/warehouse/checkout", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          checkoutSessionId,
          eventId: `${eventType}:${checkoutSessionId}`,
          eventType,
          lines: [
            {
              inventoryItemId: "item-core-kit",
              quantity: 1
            }
          ],
          orderId,
          paymentIntentId: "pi_demo_warehouse",
          provider: "stripe"
        })
      });
      const payload = await response.json() as {
        error?: string;
        ok?: boolean;
        snapshot?: WarehouseState;
      };
      if (!response.ok || payload.ok === false || !payload.snapshot) throw new Error(payload.error ?? "Checkout event failed");
      setState(payload.snapshot);
      setMessage(successMessage);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Checkout event failed");
    } finally {
      setBusyEvent(null);
    }
  }

  return (
    <section className="warehouse-simulator warehouse-checkout-simulator" aria-label="Warehouse checkout simulator">
      <div>
        <strong>Checkout inventory hook</strong>
        <span>{message}</span>
      </div>
      <div className="warehouse-command-row">
        <button
          type="button"
          onClick={() => void send("checkout.created", "Checkout reserved 1 unit in Postgres")}
          disabled={busyEvent !== null || Boolean(reservation)}
        >
          Checkout
        </button>
        <button
          type="button"
          onClick={() => void send("payment.failed", "Payment failure released reserved stock")}
          disabled={busyEvent !== null || !reservation || !["reserved", "partially_reserved"].includes(reservation.status)}
        >
          Fail
        </button>
        <button
          type="button"
          onClick={() => void send("payment.succeeded", "Payment success recorded for fulfillment")}
          disabled={busyEvent !== null || !reservation || reservation.status !== "reserved"}
        >
          Paid
        </button>
        <button
          type="button"
          onClick={() => void send("fulfillment.shipped", "Paid order picked and shipped")}
          disabled={busyEvent !== null || !reservation || reservation.status !== "reserved"}
        >
          Ship paid
        </button>
      </div>
      <dl className="warehouse-sim-metrics">
        <div><dt>Checkout status</dt><dd>{reservation?.status ?? "none"}</dd></div>
        <div><dt>Shipment</dt><dd>{shipment?.status ?? "none"}</dd></div>
        <div><dt>Available Core Kit</dt><dd>{availableCore}</dd></div>
        <div><dt>Movements</dt><dd>{state.movements.length}</dd></div>
      </dl>
    </section>
  );
}
