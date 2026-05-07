import { eq, sql } from "drizzle-orm";
import {
  businessOutboxEvents,
  createBusinessDb,
  cycleCountLines,
  cycleCounts,
  fulfillmentLabels,
  inventoryAdjustments,
  inventoryCommandLog,
  inventoryItems,
  offlineSyncBatches,
  offlineSyncEvents,
  pickLists,
  putawayTasks,
  receipts,
  returnAuthorizations,
  scanEvents,
  scannerSessions,
  shipmentPackages,
  shipments,
  shipmentTrackingEvents,
  slottingRecommendations,
  stockBalances,
  stockMovements,
  stockReservationLines,
  stockReservations,
  threePlCharges,
  warehouseIntegrationEvents,
  warehouseLocations,
  warehouseNodes,
  warehousePolicies,
  warehouseRoboticsEvents,
  warehouseTransferLines,
  warehouseTransfers,
  warehouseWavePlanLines,
  warehouseWavePlans
} from "@ctox-business/db";
import {
  buildWarehouseDemo,
  cancelReservation,
  createWarehouseCommand,
  ingestIntegrationEvent,
  pickReservation,
  releaseReservation,
  reserveStock,
  shipReservation,
  summarizeWarehouse,
  SYSTEM_OWNER_PARTY_ID,
  WAREHOUSE_COMPANY_ID,
  type InventoryTrackingMode,
  type MovementType,
  type StockReservation,
  type StockReservationStatus,
  type StockStatus,
  type WarehouseCommand,
  type WarehouseCommandType,
  type WarehouseState
} from "@ctox-business/warehouse";

// Drizzle's transaction type is intentionally kept shallow here; inferring the
// full schema-wide transaction type makes the Next app typecheck prohibitively
// slow for this persistence adapter.
type Tx = any;

export type WarehousePersistenceSnapshot = {
  persisted: boolean;
  reason?: string;
  seeded?: boolean;
  snapshot: WarehouseState;
  summary: ReturnType<typeof summarizeWarehouse>;
};

export type WarehouseMutationAction = "reserve" | "release" | "cancel" | "pick" | "ship";

export type WarehouseCheckoutEventType =
  | "checkout.created"
  | "checkout.expired"
  | "payment.failed"
  | "payment.succeeded"
  | "fulfillment.shipped";

export type WarehouseCheckoutLine = {
  inventoryItemId: string;
  inventoryOwnerPartyId?: string;
  locationId?: string;
  lotId?: string | null;
  quantity: number;
  serialId?: string | null;
  sourceLineId?: string;
};

export type WarehouseCheckoutEvent = {
  checkoutSessionId: string;
  eventId: string;
  eventType: WarehouseCheckoutEventType;
  lines?: WarehouseCheckoutLine[];
  orderId?: string;
  paymentIntentId?: string;
  provider?: string;
};

export async function getWarehouseSnapshot(): Promise<WarehousePersistenceSnapshot> {
  if (!process.env.DATABASE_URL) return demoSnapshot("DATABASE_URL not configured");

  try {
    const db = createBusinessDb();
    return await db.transaction(async (tx) => {
      await lockWarehouse(tx);
      let snapshot = await loadWarehouseState(tx);
      let seeded = false;
      if (!snapshot.items.length) {
        snapshot = buildWarehouseDemo();
        await persistWarehouseState(tx, snapshot);
        seeded = true;
      }
      return {
        persisted: true,
        seeded,
        snapshot,
        summary: summarizeWarehouse(snapshot)
      };
    });
  } catch (error) {
    return demoSnapshot(error instanceof Error ? error.message : String(error));
  }
}

export async function executeWarehouseMutation(action: WarehouseMutationAction): Promise<WarehousePersistenceSnapshot & { action: WarehouseMutationAction }> {
  if (!process.env.DATABASE_URL) {
    throw new Error("DATABASE_URL is required for persistent warehouse mutations.");
  }

  const db = createBusinessDb();
  return db.transaction(async (tx) => {
    await lockWarehouse(tx);
    let snapshot = await loadWarehouseState(tx);
    if (!snapshot.items.length) {
      snapshot = buildWarehouseDemo();
      await persistWarehouseState(tx, snapshot);
    }

    const next = applySimulatorAction(snapshot, action);
    await persistWarehouseState(tx, next);
    return {
      action,
      persisted: true,
      snapshot: next,
      summary: summarizeWarehouse(next)
    };
  });
}

export async function executeWarehouseCheckoutEvent(input: WarehouseCheckoutEvent): Promise<WarehousePersistenceSnapshot & { checkoutEvent: WarehouseCheckoutEvent }> {
  if (!process.env.DATABASE_URL) {
    throw new Error("DATABASE_URL is required for persistent warehouse checkout events.");
  }

  const db = createBusinessDb();
  return db.transaction(async (tx) => {
    await lockWarehouse(tx);
    let snapshot = await loadWarehouseState(tx);
    if (!snapshot.items.length) {
      snapshot = buildWarehouseDemo();
      await persistWarehouseState(tx, snapshot);
    }

    const next = applyCheckoutEvent(snapshot, input);
    await persistWarehouseState(tx, next);
    return {
      checkoutEvent: input,
      persisted: true,
      snapshot: next,
      summary: summarizeWarehouse(next)
    };
  });
}

function demoSnapshot(reason: string): WarehousePersistenceSnapshot {
  const snapshot = buildWarehouseDemo();
  return {
    persisted: false,
    reason,
    snapshot,
    summary: summarizeWarehouse(snapshot)
  };
}

function applyCheckoutEvent(state: WarehouseState, input: WarehouseCheckoutEvent) {
  const reservationId = checkoutReservationId(input.checkoutSessionId);
  let next = ingestIntegrationEvent(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: input.eventId,
    eventType: input.eventType,
    idempotencyKey: `checkout:${input.eventId}`,
    payload: {
      checkoutSessionId: input.checkoutSessionId,
      lineCount: input.lines?.length ?? 0,
      orderId: input.orderId,
      paymentIntentId: input.paymentIntentId
    },
    provider: input.provider ?? "stripe",
    source: "payment"
  });

  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (input.eventType === "checkout.created") {
    if (reservation) return next;
    const lines = checkoutLines(input);
    return reserveStock(next, {
      allowPartialReservation: false,
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `checkout:reserve:${input.checkoutSessionId}`,
        payload: {
          checkoutSessionId: input.checkoutSessionId,
          orderId: input.orderId,
          paymentIntentId: input.paymentIntentId,
          provider: input.provider ?? "stripe"
        },
        refId: input.orderId ?? input.checkoutSessionId,
        refType: "checkout",
        requestedBy: input.provider ?? "stripe",
        type: "ReserveStock"
      }),
      lines,
      reservationId,
      sourceId: input.orderId ?? input.checkoutSessionId,
      sourceType: "checkout"
    });
  }

  if (!reservation) return next;
  if (input.eventType === "checkout.expired") {
    if (reservation.status === "reserved" || reservation.status === "partially_reserved") {
      return cancelReservation(next, reservation.id, `checkout:cancel:${input.checkoutSessionId}`);
    }
    return next;
  }

  if (input.eventType === "payment.failed") {
    if (reservation.status === "reserved" || reservation.status === "partially_reserved") {
      return releaseReservation(next, reservation.id, `checkout:release:${input.checkoutSessionId}`);
    }
    return next;
  }

  if (input.eventType === "fulfillment.shipped") {
    if (reservation.status === "consumed") return next;
    const picked = reservation.lines.some((line) => line.pickedQuantity > line.shippedQuantity);
    if (!picked) next = pickReservation(next, reservation.id, `checkout:pick:${input.checkoutSessionId}`);
    return shipReservation(next, reservation.id, `checkout:ship:${input.checkoutSessionId}`);
  }

  return next;
}

function checkoutReservationId(checkoutSessionId: string) {
  return `checkout-${checkoutSessionId}`;
}

function checkoutLines(input: WarehouseCheckoutEvent) {
  const lines = input.lines?.length ? input.lines : [
    {
      inventoryItemId: "item-core-kit",
      quantity: 1
    }
  ];
  return lines.map((line, index) => ({
    allowBackorder: false,
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: line.inventoryItemId,
    inventoryOwnerPartyId: line.inventoryOwnerPartyId ?? SYSTEM_OWNER_PARTY_ID,
    locationId: line.locationId ?? "loc-a-01",
    lotId: line.lotId,
    quantity: line.quantity,
    serialId: line.serialId,
    sourceLineId: line.sourceLineId ?? `${input.checkoutSessionId}-line-${index + 1}`,
    stockStatus: "available" as const
  }));
}

async function lockWarehouse(tx: Tx) {
  await tx.execute(sql`select pg_advisory_xact_lock(hashtext(${`warehouse:${WAREHOUSE_COMPANY_ID}`}))`);
}

function applySimulatorAction(state: WarehouseState, action: WarehouseMutationAction) {
  const reservation = latestSimulatorReservation(state);
  if (action === "reserve") {
    const suffix = Date.now().toString(36);
    return reserveStock(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `simulator:reserve:${suffix}`,
        payload: { source: "persistent-warehouse-simulator" },
        refId: `sim-order-${suffix}`,
        refType: "sales_order",
        requestedBy: "user",
        type: "ReserveStock"
      }),
      lines: [
        {
          allowBackorder: false,
          companyId: WAREHOUSE_COMPANY_ID,
          inventoryItemId: "item-core-kit",
          inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
          locationId: "loc-a-01",
          quantity: 2,
          sourceLineId: `sim-order-${suffix}-line-1`,
          stockStatus: "available"
        }
      ],
      reservationId: `sim-reserve-${suffix}`,
      sourceId: `sim-order-${suffix}`,
      sourceType: "sales_order"
    });
  }

  if (!reservation) throw new Error("simulator_reservation_not_found");
  if (action === "release") return releaseReservation(state, reservation.id, `simulator:release:${reservation.id}`);
  if (action === "cancel") return cancelReservation(state, reservation.id, `simulator:cancel:${reservation.id}`);
  if (action === "pick") return pickReservation(state, reservation.id, `simulator:pick:${reservation.id}`);
  return shipReservation(state, reservation.id, `simulator:ship:${reservation.id}`);
}

function latestSimulatorReservation(state: WarehouseState) {
  return [...state.reservations]
    .reverse()
    .find((reservation) => reservation.id.startsWith("sim-reserve-") && !["cancelled", "consumed", "released"].includes(reservation.status));
}

async function loadWarehouseState(tx: Tx): Promise<WarehouseState> {
  const items = await tx.select().from(inventoryItems).where(eq(inventoryItems.companyId, WAREHOUSE_COMPANY_ID));
  const locations = await tx.select().from(warehouseLocations).where(eq(warehouseLocations.companyId, WAREHOUSE_COMPANY_ID));
  const policies = await tx.select().from(warehousePolicies).where(eq(warehousePolicies.companyId, WAREHOUSE_COMPANY_ID));
  const balances = await tx.select().from(stockBalances).where(eq(stockBalances.companyId, WAREHOUSE_COMPANY_ID));
  const movements = await tx.select().from(stockMovements).where(eq(stockMovements.companyId, WAREHOUSE_COMPANY_ID));
  const commands = await tx.select().from(inventoryCommandLog).where(eq(inventoryCommandLog.companyId, WAREHOUSE_COMPANY_ID));
  const outboxRows = await tx.select().from(businessOutboxEvents).where(sql`${businessOutboxEvents.companyId} = ${WAREHOUSE_COMPANY_ID} and ${businessOutboxEvents.topic} like 'warehouse.%'`);
  const reservations = await tx.select().from(stockReservations).where(eq(stockReservations.companyId, WAREHOUSE_COMPANY_ID));
  const reservationLines = await tx.select().from(stockReservationLines).where(eq(stockReservationLines.companyId, WAREHOUSE_COMPANY_ID));
  const pickListRows = await tx.select().from(pickLists).where(eq(pickLists.companyId, WAREHOUSE_COMPANY_ID));
  const receiptRows = await tx.select().from(receipts).where(eq(receipts.companyId, WAREHOUSE_COMPANY_ID));
  const putawayRows = await tx.select().from(putawayTasks).where(eq(putawayTasks.companyId, WAREHOUSE_COMPANY_ID));
  const shipmentRows = await tx.select().from(shipments).where(eq(shipments.companyId, WAREHOUSE_COMPANY_ID));
  const returnRows = await tx.select().from(returnAuthorizations).where(eq(returnAuthorizations.companyId, WAREHOUSE_COMPANY_ID));
  const scannerRows = await tx.select().from(scannerSessions).where(eq(scannerSessions.companyId, WAREHOUSE_COMPANY_ID));
  const scanRows = await tx.select().from(scanEvents).where(eq(scanEvents.companyId, WAREHOUSE_COMPANY_ID));
  const cycleRows = await tx.select().from(cycleCounts).where(eq(cycleCounts.companyId, WAREHOUSE_COMPANY_ID));
  const cycleLineRows = await tx.select().from(cycleCountLines).where(eq(cycleCountLines.companyId, WAREHOUSE_COMPANY_ID));
  const adjustmentRows = await tx.select().from(inventoryAdjustments).where(eq(inventoryAdjustments.companyId, WAREHOUSE_COMPANY_ID));
  const packageRows = await tx.select().from(shipmentPackages).where(eq(shipmentPackages.companyId, WAREHOUSE_COMPANY_ID));
  const labelRows = await tx.select().from(fulfillmentLabels).where(eq(fulfillmentLabels.companyId, WAREHOUSE_COMPANY_ID));
  const trackingRows = await tx.select().from(shipmentTrackingEvents).where(eq(shipmentTrackingEvents.companyId, WAREHOUSE_COMPANY_ID));
  const nodeRows = await tx.select().from(warehouseNodes).where(eq(warehouseNodes.companyId, WAREHOUSE_COMPANY_ID));
  const integrationRows = await tx.select().from(warehouseIntegrationEvents).where(eq(warehouseIntegrationEvents.companyId, WAREHOUSE_COMPANY_ID));
  const roboticsRows = await tx.select().from(warehouseRoboticsEvents).where(eq(warehouseRoboticsEvents.companyId, WAREHOUSE_COMPANY_ID));
  const waveRows = await tx.select().from(warehouseWavePlans).where(eq(warehouseWavePlans.companyId, WAREHOUSE_COMPANY_ID));
  const waveLineRows = await tx.select().from(warehouseWavePlanLines).where(eq(warehouseWavePlanLines.companyId, WAREHOUSE_COMPANY_ID));
  const slottingRows = await tx.select().from(slottingRecommendations).where(eq(slottingRecommendations.companyId, WAREHOUSE_COMPANY_ID));
  const transferRows = await tx.select().from(warehouseTransfers).where(eq(warehouseTransfers.companyId, WAREHOUSE_COMPANY_ID));
  const transferLineRows = await tx.select().from(warehouseTransferLines).where(eq(warehouseTransferLines.companyId, WAREHOUSE_COMPANY_ID));
  const offlineBatchRows = await tx.select().from(offlineSyncBatches).where(eq(offlineSyncBatches.companyId, WAREHOUSE_COMPANY_ID));
  const offlineEventRows = await tx.select().from(offlineSyncEvents).where(eq(offlineSyncEvents.companyId, WAREHOUSE_COMPANY_ID));
  const threePlRows = await tx.select().from(threePlCharges).where(eq(threePlCharges.companyId, WAREHOUSE_COMPANY_ID));

  return {
    balances: balances.map((row: any) => ({
      balanceKey: row.balanceKey,
      companyId: row.companyId,
      inventoryItemId: row.inventoryItemExternalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      locationId: row.warehouseLocationExternalId,
      lotId: row.lotId,
      quantity: row.quantity,
      serialId: row.serialId,
      stockStatus: row.stockStatus as StockStatus,
      updatedAt: row.updatedAt.toISOString()
    })),
    commandLog: commands.map((row: any) => ({
      companyId: row.companyId,
      idempotencyKey: row.idempotencyKey,
      payload: parseJson(row.payloadJson),
      refId: row.refId,
      refType: row.refType,
      requestedAt: row.createdAt.toISOString(),
      requestedBy: row.requestedBy,
      type: row.type as WarehouseCommandType
    })),
    cycleCounts: cycleRows.map((row: any) => ({
      closedAt: row.closedAt?.toISOString(),
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      lines: cycleLineRows.filter((line: any) => line.cycleCountExternalId === row.externalId).map((line: any) => ({
        companyId: line.companyId,
        countedQuantity: line.countedQuantity ?? undefined,
        expectedQuantity: line.expectedQuantity,
        id: line.externalId,
        inventoryItemId: line.inventoryItemExternalId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        locationId: line.locationExternalId,
        lotId: line.lotId,
        serialId: line.serialId,
        stockStatus: line.stockStatus as StockStatus,
        varianceQuantity: line.varianceQuantity ?? undefined
      })),
      locationId: row.locationExternalId,
      openedAt: row.openedAt.toISOString(),
      status: row.status as "open" | "closed" | "cancelled",
      version: row.version
    })),
    fulfillmentLabels: labelRows.map((row: any) => ({
      carrier: row.carrier,
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      packageId: row.packageExternalId,
      provider: row.provider,
      status: row.status as "created" | "voided",
      trackingNumber: row.trackingNumber,
      version: row.version
    })),
    integrationEvents: integrationRows.map((row: any) => ({
      companyId: row.companyId,
      eventType: row.eventType,
      externalId: row.externalId,
      id: row.externalId,
      idempotencyKey: row.idempotencyKey,
      payload: parseJson(row.payloadJson),
      provider: row.provider,
      receivedAt: row.receivedAt.toISOString(),
      source: row.source as "wes" | "mfc"
    })),
    inventoryAdjustments: adjustmentRows.map((row: any) => ({
      companyId: row.companyId,
      cycleCountId: row.cycleCountExternalId ?? undefined,
      externalId: row.externalId,
      id: row.externalId,
      lineId: row.cycleCountLineExternalId,
      movementId: row.stockMovementExternalId,
      quantity: row.quantity,
      reason: row.reason as "cycle_count" | "manual"
    })),
    items: items.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      name: row.name,
      sku: row.sku,
      trackingMode: row.trackingMode as InventoryTrackingMode,
      uom: row.uom
    })),
    locations: locations.map((row: any) => ({
      companyId: row.companyId,
      defaultOwnerPartyId: row.defaultOwnerPartyId ?? undefined,
      externalId: row.externalId,
      id: row.externalId,
      kind: row.kind as "warehouse" | "zone" | "bin",
      name: row.name,
      parentId: row.parentExternalId ?? undefined,
      pickable: row.pickable === 1,
      receivable: row.receivable === 1
    })),
    movements: movements.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      idempotencyKey: row.idempotencyKey,
      inventoryItemId: row.inventoryItemExternalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      locationId: row.warehouseLocationExternalId,
      lotId: row.lotId,
      movementType: row.movementType as MovementType,
      postedAt: row.postedAt.toISOString(),
      quantity: row.quantity,
      serialId: row.serialId,
      sourceId: row.sourceId,
      sourceLineId: row.sourceLineId ?? undefined,
      sourceType: row.sourceType,
      stockStatus: row.stockStatus as StockStatus,
      stockStatusFrom: row.stockStatusFrom as StockStatus | undefined,
      stockStatusTo: row.stockStatusTo as StockStatus | undefined,
      uom: row.uom
    })),
    nodes: nodeRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      kind: row.kind as "warehouse" | "store" | "third_party_logistics" | "virtual",
      name: row.name,
      status: row.status as "active" | "inactive"
    })),
    offlineSyncBatches: offlineBatchRows.map((row: any) => ({
      companyId: row.companyId,
      deviceId: row.deviceId,
      events: offlineEventRows.filter((event: any) => event.batchExternalId === row.externalId).map((event: any) => ({
        action: event.action,
        externalId: event.externalId,
        id: event.externalId,
        idempotencyKey: event.idempotencyKey,
        payload: parseJson(event.payloadJson)
      })),
      externalId: row.externalId,
      id: row.externalId,
      receivedAt: row.receivedAt.toISOString(),
      status: row.status as "accepted" | "rejected"
    })),
    outbox: outboxRows.map((row: any) => ({
      companyId: row.companyId,
      id: row.externalId.replace(/^warehouse:/, ""),
      payload: parseJson(row.payloadJson),
      status: row.status as "pending" | "delivered" | "failed",
      topic: row.topic
    })),
    pickLists: pickListRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      lines: parseJsonArray(row.payloadJson, "lines"),
      reservationId: row.reservationExternalId,
      status: row.status as "draft" | "ready" | "picked" | "cancelled",
      version: row.version
    })),
    policies: policies.map((row: any) => ({
      allowBackorder: row.allowBackorder === 1,
      allowNegativeStock: row.allowNegativeStock === 1,
      allocationStrategy: row.allocationStrategy as "fifo" | "fefo" | "manual",
      companyId: row.companyId,
      defaultOwnerPartyId: row.defaultOwnerPartyId,
      id: row.externalId
    })),
    putawayTasks: putawayRows.map((row: any) => {
      const payload = parseJson(row.payloadJson);
      return {
        companyId: row.companyId,
        externalId: row.externalId,
        fromLocationId: row.fromLocationExternalId,
        id: row.externalId,
        inventoryItemId: row.inventoryItemExternalId,
        inventoryOwnerPartyId: row.inventoryOwnerPartyId,
        lotId: stringOrNull(payload.lotId),
        quantity: row.quantity,
        receiptLineId: row.receiptLineExternalId,
        serialId: stringOrNull(payload.serialId),
        status: row.status as "open" | "done" | "cancelled",
        toLocationId: row.toLocationExternalId,
        version: row.version
      };
    }),
    receipts: receiptRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      lines: parseJsonArray(row.payloadJson, "lines"),
      sourceId: row.sourceId,
      sourceType: row.sourceType,
      status: row.status as "draft" | "received" | "putaway_started" | "putaway_complete" | "cancelled",
      version: row.version
    })),
    reservations: reservations.map((row: any) => ({
      allowPartialReservation: row.allowPartialReservation === 1,
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      lines: reservationLines.filter((line: any) => line.reservationExternalId === row.externalId).map((line: any) => ({
        allowBackorder: line.allowBackorder === 1,
        companyId: line.companyId,
        id: line.externalId,
        inventoryItemId: line.inventoryItemExternalId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        locationId: line.warehouseLocationExternalId,
        lotId: line.lotId,
        pickedQuantity: line.pickedQuantity,
        quantity: line.quantity,
        releasedQuantity: line.releasedQuantity,
        serialId: line.serialId,
        shippedQuantity: line.shippedQuantity,
        sourceLineId: line.sourceLineId,
        stockStatus: "reserved"
      })),
      sourceId: row.sourceId,
      sourceType: row.sourceType,
      status: row.status as StockReservationStatus,
      version: row.version
    })),
    returns: returnRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      lines: parseJsonArray(row.payloadJson, "lines"),
      sourceShipmentId: row.sourceShipmentExternalId,
      status: row.status as "authorized" | "received" | "closed" | "cancelled",
      version: row.version
    })),
    roboticsEvents: roboticsRows.map((row: any) => ({
      companyId: row.companyId,
      eventType: row.eventType,
      externalId: row.externalId,
      id: row.externalId,
      idempotencyKey: row.idempotencyKey,
      occurredAt: row.occurredAt.toISOString(),
      payload: parseJson(row.payloadJson),
      robotId: row.robotId
    })),
    scanEvents: scanRows.map((row: any) => ({
      action: row.action as "receive" | "putaway" | "pick" | "pack" | "ship" | "count",
      barcode: row.barcode,
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      idempotencyKey: row.idempotencyKey,
      inventoryItemId: row.inventoryItemExternalId ?? undefined,
      locationId: row.locationExternalId ?? undefined,
      lotId: row.lotId,
      occurredAt: row.occurredAt.toISOString(),
      quantity: row.quantity,
      serialId: row.serialId,
      sessionId: row.sessionExternalId
    })),
    scannerSessions: scannerRows.map((row: any) => ({
      companyId: row.companyId,
      deviceId: row.deviceId,
      endedAt: row.endedAt?.toISOString(),
      externalId: row.externalId,
      id: row.externalId,
      locationId: row.locationExternalId ?? undefined,
      scanCount: row.scanCount,
      startedAt: row.startedAt.toISOString(),
      status: row.status as "active" | "closed",
      userId: row.userId,
      version: row.version
    })),
    shipments: shipmentRows.map((row: any) => ({
      carrier: row.carrier ?? undefined,
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      lines: parseJsonArray(row.payloadJson, "lines"),
      provider: row.provider ?? undefined,
      reservationId: row.reservationExternalId,
      status: row.status as "draft" | "packed" | "shipped" | "cancelled",
      trackingNumber: row.trackingNumber ?? undefined,
      version: row.version
    })),
    shipmentPackages: packageRows.map((row: any) => ({
      carrier: row.carrier ?? undefined,
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      shipmentId: row.shipmentExternalId,
      status: row.status as "draft" | "packed" | "labelled" | "shipped" | "cancelled",
      trackingNumber: row.trackingNumber ?? undefined,
      version: row.version
    })),
    shipmentTrackingEvents: trackingRows.map((row: any) => ({
      carrier: row.carrier,
      companyId: row.companyId,
      eventCode: row.eventCode,
      eventTime: row.eventTime.toISOString(),
      externalId: row.externalId,
      id: row.externalId,
      shipmentId: row.shipmentExternalId,
      trackingNumber: row.trackingNumber
    })),
    slottingRecommendations: slottingRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      fromLocationId: row.fromLocationExternalId,
      id: row.externalId,
      inventoryItemId: row.inventoryItemExternalId,
      reason: row.reason,
      status: row.status as "recommended" | "accepted" | "rejected",
      toLocationId: row.toLocationExternalId
    })),
    threePlCharges: threePlRows.map((row: any) => ({
      amountCents: row.amountCents,
      companyId: row.companyId,
      currency: row.currency,
      externalId: row.externalId,
      id: row.externalId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      metric: row.metric as "storage_day" | "pick" | "pack" | "ship" | "return",
      quantity: row.quantity,
      sourceId: row.sourceId,
      sourceType: row.sourceType
    })),
    transfers: transferRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      fromLocationId: row.fromLocationExternalId,
      fromNodeId: row.fromNodeExternalId,
      id: row.externalId,
      lines: transferLineRows.filter((line: any) => line.transferExternalId === row.externalId).map((line: any) => ({
        id: line.externalId,
        inventoryItemId: line.inventoryItemExternalId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        lotId: line.lotId,
        quantity: line.quantity,
        receivedQuantity: line.receivedQuantity,
        serialId: line.serialId,
        shippedQuantity: line.shippedQuantity
      })),
      status: row.status as "draft" | "shipped" | "received" | "cancelled",
      toLocationId: row.toLocationExternalId,
      toNodeId: row.toNodeExternalId,
      version: row.version
    })),
    wavePlans: waveRows.map((row: any) => ({
      companyId: row.companyId,
      externalId: row.externalId,
      id: row.externalId,
      lines: waveLineRows.filter((line: any) => line.wavePlanExternalId === row.externalId).map((line: any) => ({
        id: line.externalId,
        pickListId: line.pickListExternalId ?? undefined,
        reservationId: line.reservationExternalId,
        sequence: line.sequence
      })),
      priority: row.priority as "normal" | "expedite",
      status: row.status as "planned" | "released" | "cancelled",
      version: row.version
    }))
  };
}

async function persistWarehouseState(tx: Tx, state: WarehouseState) {
  const now = new Date();
  for (const item of state.items) {
    const values = {
      companyId: item.companyId,
      externalId: item.id,
      name: item.name,
      sku: item.sku,
      trackingMode: item.trackingMode,
      uom: item.uom,
      updatedAt: now
    };
    await tx.insert(inventoryItems).values(values).onConflictDoUpdate({ target: inventoryItems.externalId, set: values });
  }

  for (const location of state.locations) {
    const values = {
      companyId: location.companyId,
      defaultOwnerPartyId: location.defaultOwnerPartyId ?? null,
      externalId: location.id,
      kind: location.kind,
      name: location.name,
      parentExternalId: location.parentId ?? null,
      pickable: location.pickable ? 1 : 0,
      receivable: location.receivable ? 1 : 0,
      updatedAt: now
    };
    await tx.insert(warehouseLocations).values(values).onConflictDoUpdate({ target: warehouseLocations.externalId, set: values });
  }

  for (const policy of state.policies) {
    const values = {
      allowBackorder: policy.allowBackorder ? 1 : 0,
      allowNegativeStock: policy.allowNegativeStock ? 1 : 0,
      allocationStrategy: policy.allocationStrategy,
      companyId: policy.companyId,
      defaultOwnerPartyId: policy.defaultOwnerPartyId,
      externalId: policy.id,
      updatedAt: now
    };
    await tx.insert(warehousePolicies).values(values).onConflictDoUpdate({ target: warehousePolicies.externalId, set: values });
  }

  for (const balance of state.balances) {
    const values = {
      balanceKey: balance.balanceKey,
      companyId: balance.companyId,
      inventoryItemExternalId: balance.inventoryItemId,
      inventoryOwnerPartyId: balance.inventoryOwnerPartyId,
      lotId: balance.lotId ?? null,
      quantity: balance.quantity,
      serialId: balance.serialId ?? null,
      stockStatus: balance.stockStatus,
      updatedAt: now,
      warehouseLocationExternalId: balance.locationId
    };
    await tx.insert(stockBalances).values(values).onConflictDoUpdate({
      target: [stockBalances.companyId, stockBalances.balanceKey],
      set: values
    });
  }

  for (const movement of state.movements) {
    const values = {
      companyId: movement.companyId,
      externalId: movement.externalId,
      idempotencyKey: movement.idempotencyKey,
      inventoryItemExternalId: movement.inventoryItemId,
      inventoryOwnerPartyId: movement.inventoryOwnerPartyId,
      lotId: movement.lotId ?? null,
      movementType: movement.movementType,
      postedAt: new Date(movement.postedAt),
      quantity: movement.quantity,
      serialId: movement.serialId ?? null,
      sourceId: movement.sourceId,
      sourceLineId: movement.sourceLineId ?? null,
      sourceType: movement.sourceType,
      stockStatus: movement.stockStatus,
      stockStatusFrom: movement.stockStatusFrom ?? null,
      stockStatusTo: movement.stockStatusTo ?? null,
      uom: movement.uom,
      warehouseLocationExternalId: movement.locationId
    };
    await tx.insert(stockMovements).values(values).onConflictDoNothing({ target: stockMovements.externalId });
  }

  for (const command of state.commandLog) {
    const values = {
      companyId: command.companyId,
      idempotencyKey: command.idempotencyKey,
      payloadJson: JSON.stringify(command.payload),
      refId: command.refId,
      refType: command.refType,
      requestedBy: command.requestedBy,
      type: command.type
    };
    await tx.insert(inventoryCommandLog).values(values).onConflictDoNothing({
      target: [inventoryCommandLog.companyId, inventoryCommandLog.idempotencyKey]
    });
  }

  for (const event of state.outbox) {
    const values = {
      companyId: event.companyId,
      externalId: `warehouse:${event.id}`,
      payloadJson: JSON.stringify(event.payload),
      status: event.status,
      topic: event.topic,
      updatedAt: now
    };
    await tx.insert(businessOutboxEvents).values(values).onConflictDoUpdate({
      target: businessOutboxEvents.externalId,
      set: {
        ...values,
        payloadJson: sql`case when ${businessOutboxEvents.status} = 'delivered' then ${businessOutboxEvents.payloadJson} else excluded.payload_json end`,
        status: sql`case when ${businessOutboxEvents.status} = 'delivered' then ${businessOutboxEvents.status} else excluded.status end`
      }
    });
  }

  await persistReservations(tx, state.reservations, now);
  await persistPayloadRows(tx, state, now);
}

async function persistReservations(tx: Tx, reservations: StockReservation[], now: Date) {
  for (const reservation of reservations) {
    const values = {
      allowPartialReservation: reservation.allowPartialReservation ? 1 : 0,
      companyId: reservation.companyId,
      externalId: reservation.id,
      inventoryOwnerPartyId: reservation.inventoryOwnerPartyId,
      sourceId: reservation.sourceId,
      sourceType: reservation.sourceType,
      status: reservation.status,
      updatedAt: now,
      version: reservation.version
    };
    await tx.insert(stockReservations).values(values).onConflictDoUpdate({ target: stockReservations.externalId, set: values });
    for (const line of reservation.lines) {
      const lineValues = {
        allowBackorder: line.allowBackorder ? 1 : 0,
        companyId: line.companyId,
        externalId: line.id,
        inventoryItemExternalId: line.inventoryItemId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        lotId: line.lotId ?? null,
        pickedQuantity: line.pickedQuantity,
        quantity: line.quantity,
        releasedQuantity: line.releasedQuantity,
        reservationExternalId: reservation.id,
        serialId: line.serialId ?? null,
        shippedQuantity: line.shippedQuantity,
        sourceLineId: line.sourceLineId,
        updatedAt: now,
        warehouseLocationExternalId: line.locationId
      };
      await tx.insert(stockReservationLines).values(lineValues).onConflictDoUpdate({
        target: stockReservationLines.externalId,
        set: lineValues
      });
    }
  }
}

async function persistPayloadRows(tx: Tx, state: WarehouseState, now: Date) {
  for (const row of state.pickLists) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      payloadJson: JSON.stringify({ lines: row.lines }),
      reservationExternalId: row.reservationId,
      status: row.status,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(pickLists).values(values).onConflictDoUpdate({ target: pickLists.externalId, set: values });
  }
  for (const row of state.receipts) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      payloadJson: JSON.stringify({ lines: row.lines }),
      sourceId: row.sourceId,
      sourceType: row.sourceType,
      status: row.status,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(receipts).values(values).onConflictDoUpdate({ target: receipts.externalId, set: values });
  }
  for (const row of state.putawayTasks) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      fromLocationExternalId: row.fromLocationId,
      inventoryItemExternalId: row.inventoryItemId,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      payloadJson: JSON.stringify({ lotId: row.lotId, serialId: row.serialId }),
      quantity: row.quantity,
      receiptLineExternalId: row.receiptLineId,
      status: row.status,
      toLocationExternalId: row.toLocationId,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(putawayTasks).values(values).onConflictDoUpdate({ target: putawayTasks.externalId, set: values });
  }
  for (const row of state.shipments) {
    const values = {
      carrier: row.carrier ?? null,
      companyId: row.companyId,
      externalId: row.id,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      payloadJson: JSON.stringify({ lines: row.lines }),
      provider: row.provider ?? null,
      reservationExternalId: row.reservationId,
      status: row.status,
      trackingNumber: row.trackingNumber ?? null,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(shipments).values(values).onConflictDoUpdate({ target: shipments.externalId, set: values });
  }
  for (const row of state.returns) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      payloadJson: JSON.stringify({ lines: row.lines }),
      sourceShipmentExternalId: row.sourceShipmentId,
      status: row.status,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(returnAuthorizations).values(values).onConflictDoUpdate({ target: returnAuthorizations.externalId, set: values });
  }
  for (const row of state.scannerSessions) {
    const values = {
      companyId: row.companyId,
      deviceId: row.deviceId,
      endedAt: row.endedAt ? new Date(row.endedAt) : null,
      externalId: row.id,
      locationExternalId: row.locationId ?? null,
      scanCount: row.scanCount,
      startedAt: new Date(row.startedAt),
      status: row.status,
      updatedAt: now,
      userId: row.userId,
      version: row.version
    };
    await tx.insert(scannerSessions).values(values).onConflictDoUpdate({ target: scannerSessions.externalId, set: values });
  }
  for (const row of state.scanEvents) {
    const values = {
      action: row.action,
      barcode: row.barcode,
      companyId: row.companyId,
      externalId: row.id,
      idempotencyKey: row.idempotencyKey,
      inventoryItemExternalId: row.inventoryItemId ?? null,
      locationExternalId: row.locationId ?? null,
      lotId: row.lotId ?? null,
      occurredAt: new Date(row.occurredAt),
      payloadJson: "{}",
      quantity: row.quantity,
      serialId: row.serialId ?? null,
      sessionExternalId: row.sessionId
    };
    await tx.insert(scanEvents).values(values).onConflictDoNothing({ target: scanEvents.externalId });
  }
  await persistM2M3Rows(tx, state, now);
}

async function persistM2M3Rows(tx: Tx, state: WarehouseState, now: Date) {
  for (const row of state.cycleCounts) {
    const values = {
      closedAt: row.closedAt ? new Date(row.closedAt) : null,
      companyId: row.companyId,
      externalId: row.id,
      locationExternalId: row.locationId,
      openedAt: new Date(row.openedAt),
      status: row.status,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(cycleCounts).values(values).onConflictDoUpdate({ target: cycleCounts.externalId, set: values });
    for (const line of row.lines) {
      const lineValues = {
        companyId: line.companyId,
        countedQuantity: line.countedQuantity ?? null,
        cycleCountExternalId: row.id,
        expectedQuantity: line.expectedQuantity,
        externalId: line.id,
        inventoryItemExternalId: line.inventoryItemId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        locationExternalId: line.locationId,
        lotId: line.lotId ?? null,
        serialId: line.serialId ?? null,
        stockStatus: line.stockStatus,
        updatedAt: now,
        varianceQuantity: line.varianceQuantity ?? null
      };
      await tx.insert(cycleCountLines).values(lineValues).onConflictDoUpdate({ target: cycleCountLines.externalId, set: lineValues });
    }
  }
  for (const row of state.inventoryAdjustments) {
    await tx.insert(inventoryAdjustments).values({
      companyId: row.companyId,
      cycleCountExternalId: row.cycleCountId ?? null,
      cycleCountLineExternalId: row.lineId,
      externalId: row.id,
      quantity: row.quantity,
      reason: row.reason,
      stockMovementExternalId: row.movementId
    }).onConflictDoNothing({ target: inventoryAdjustments.externalId });
  }
  for (const row of state.shipmentPackages) {
    const values = {
      carrier: row.carrier ?? null,
      companyId: row.companyId,
      externalId: row.id,
      shipmentExternalId: row.shipmentId,
      status: row.status,
      trackingNumber: row.trackingNumber ?? null,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(shipmentPackages).values(values).onConflictDoUpdate({ target: shipmentPackages.externalId, set: values });
  }
  for (const row of state.fulfillmentLabels) {
    const values = {
      carrier: row.carrier,
      companyId: row.companyId,
      externalId: row.id,
      packageExternalId: row.packageId,
      provider: row.provider,
      status: row.status,
      trackingNumber: row.trackingNumber,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(fulfillmentLabels).values(values).onConflictDoUpdate({ target: fulfillmentLabels.externalId, set: values });
  }
  for (const row of state.shipmentTrackingEvents) {
    await tx.insert(shipmentTrackingEvents).values({
      carrier: row.carrier,
      companyId: row.companyId,
      eventCode: row.eventCode,
      eventTime: new Date(row.eventTime),
      externalId: row.id,
      shipmentExternalId: row.shipmentId,
      trackingNumber: row.trackingNumber
    }).onConflictDoNothing({ target: shipmentTrackingEvents.externalId });
  }
  await persistM3Rows(tx, state, now);
}

async function persistM3Rows(tx: Tx, state: WarehouseState, now: Date) {
  for (const row of state.nodes) {
    const values = { companyId: row.companyId, externalId: row.id, kind: row.kind, name: row.name, status: row.status, updatedAt: now };
    await tx.insert(warehouseNodes).values(values).onConflictDoUpdate({ target: warehouseNodes.externalId, set: values });
  }
  for (const row of state.integrationEvents) {
    await tx.insert(warehouseIntegrationEvents).values({
      companyId: row.companyId,
      eventType: row.eventType,
      externalId: row.id,
      idempotencyKey: row.idempotencyKey,
      payloadJson: JSON.stringify(row.payload),
      provider: row.provider,
      receivedAt: new Date(row.receivedAt),
      source: row.source
    }).onConflictDoNothing({ target: warehouseIntegrationEvents.externalId });
  }
  for (const row of state.roboticsEvents) {
    await tx.insert(warehouseRoboticsEvents).values({
      companyId: row.companyId,
      eventType: row.eventType,
      externalId: row.id,
      idempotencyKey: row.idempotencyKey,
      occurredAt: new Date(row.occurredAt),
      payloadJson: JSON.stringify(row.payload),
      robotId: row.robotId
    }).onConflictDoNothing({ target: warehouseRoboticsEvents.externalId });
  }
  for (const row of state.wavePlans) {
    const values = { companyId: row.companyId, externalId: row.id, priority: row.priority, status: row.status, updatedAt: now, version: row.version };
    await tx.insert(warehouseWavePlans).values(values).onConflictDoUpdate({ target: warehouseWavePlans.externalId, set: values });
    for (const line of row.lines) {
      await tx.insert(warehouseWavePlanLines).values({
        companyId: row.companyId,
        externalId: line.id,
        pickListExternalId: line.pickListId ?? null,
        reservationExternalId: line.reservationId,
        sequence: line.sequence,
        wavePlanExternalId: row.id
      }).onConflictDoNothing({ target: warehouseWavePlanLines.externalId });
    }
  }
  for (const row of state.slottingRecommendations) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      fromLocationExternalId: row.fromLocationId,
      inventoryItemExternalId: row.inventoryItemId,
      reason: row.reason,
      status: row.status,
      toLocationExternalId: row.toLocationId,
      updatedAt: now
    };
    await tx.insert(slottingRecommendations).values(values).onConflictDoUpdate({ target: slottingRecommendations.externalId, set: values });
  }
  for (const row of state.transfers) {
    const values = {
      companyId: row.companyId,
      externalId: row.id,
      fromLocationExternalId: row.fromLocationId,
      fromNodeExternalId: row.fromNodeId,
      status: row.status,
      toLocationExternalId: row.toLocationId,
      toNodeExternalId: row.toNodeId,
      updatedAt: now,
      version: row.version
    };
    await tx.insert(warehouseTransfers).values(values).onConflictDoUpdate({ target: warehouseTransfers.externalId, set: values });
    for (const line of row.lines) {
      const lineValues = {
        companyId: row.companyId,
        externalId: line.id,
        inventoryItemExternalId: line.inventoryItemId,
        inventoryOwnerPartyId: line.inventoryOwnerPartyId,
        lotId: line.lotId ?? null,
        quantity: line.quantity,
        receivedQuantity: line.receivedQuantity,
        serialId: line.serialId ?? null,
        shippedQuantity: line.shippedQuantity,
        transferExternalId: row.id,
        updatedAt: now
      };
      await tx.insert(warehouseTransferLines).values(lineValues).onConflictDoUpdate({ target: warehouseTransferLines.externalId, set: lineValues });
    }
  }
  for (const row of state.offlineSyncBatches) {
    await tx.insert(offlineSyncBatches).values({
      companyId: row.companyId,
      deviceId: row.deviceId,
      externalId: row.id,
      receivedAt: new Date(row.receivedAt),
      status: row.status
    }).onConflictDoNothing({ target: offlineSyncBatches.externalId });
    for (const event of row.events) {
      await tx.insert(offlineSyncEvents).values({
        action: event.action,
        batchExternalId: row.id,
        companyId: row.companyId,
        externalId: event.id,
        idempotencyKey: event.idempotencyKey,
        payloadJson: JSON.stringify(event.payload)
      }).onConflictDoNothing({ target: offlineSyncEvents.externalId });
    }
  }
  for (const row of state.threePlCharges) {
    await tx.insert(threePlCharges).values({
      amountCents: row.amountCents,
      companyId: row.companyId,
      currency: row.currency,
      externalId: row.id,
      inventoryOwnerPartyId: row.inventoryOwnerPartyId,
      metric: row.metric,
      quantity: row.quantity,
      sourceId: row.sourceId,
      sourceType: row.sourceType
    }).onConflictDoNothing({ target: threePlCharges.externalId });
  }
}

function parseJson(value: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(value) as unknown;
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, unknown> : {};
  } catch {
    return {};
  }
}

function parseJsonArray<T = never>(value: string, key: string): T[] {
  const parsed = parseJson(value)[key];
  return Array.isArray(parsed) ? parsed as T[] : [];
}

function stringOrNull(value: unknown) {
  return typeof value === "string" ? value : null;
}
