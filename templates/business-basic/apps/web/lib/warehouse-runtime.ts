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
  authorizeReturn,
  buildWarehouseDemo,
  cancelReservation,
  closeCycleCount,
  completePutaway,
  createBalanceKey,
  createFulfillmentLabel,
  createPickList,
  createShipmentPackage,
  createSlottingRecommendation,
  createWarehouseTransfer,
  createWarehouseCommand,
  createWavePlan,
  ingestIntegrationEvent,
  ingestScanEvent,
  openCycleCount,
  pickReservation,
  releaseReservation,
  receiveStock,
  receiveReturn,
  receiveWarehouseTransfer,
  reserveStock,
  recordCycleCountLine,
  recordOfflineSyncBatch,
  recordShipmentTrackingEvent,
  recordThreePlCharge,
  shipReservation,
  shipWarehouseTransfer,
  startScannerSession,
  summarizeWarehouse,
  SYSTEM_OWNER_PARTY_ID,
  WAREHOUSE_COMPANY_ID,
  type InventoryItem,
  type InventoryTrackingMode,
  type MovementType,
  type OfflineSyncEvent,
  type PutawayTask,
  type ReceiptLine,
  type StockBalance,
  type StockMovement,
  type StockReservation,
  type StockReservationStatus,
  type StockStatus,
  type WarehouseCommand,
  type WarehouseCommandType,
  type WarehouseLocation,
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

export type WarehouseLayoutAction =
  | "createWarehouse"
  | "createSection"
  | "createSlot"
  | "createItem"
  | "createPickList"
  | "createInterWarehouseTransfer"
  | "createShipmentLabel"
  | "deactivateItem"
  | "duplicateItem"
  | "duplicateLocation"
  | "closeCycleCount"
  | "adjustBalance"
  | "authorizeReturn"
  | "changeStockStatus"
  | "completePutaway"
  | "moveStock"
  | "openCycleCount"
  | "packShipment"
  | "planSlotting"
  | "planWave"
  | "receiveInbound"
  | "receiveInterWarehouseTransfer"
  | "receiveReturn"
  | "recordCarrierHandover"
  | "recordImportDryRun"
  | "recordOpsHandover"
  | "recordRoleReview"
  | "recordSyncConflict"
  | "recordThreePlCharge"
  | "reserveBalance"
  | "resolveQualityHold"
  | "recordCycleCount"
  | "renameItem"
  | "scrapQualityHold"
  | "scanPutaway"
  | "scanPick"
  | "shipInterWarehouseTransfer"
  | "renameLocation"
  | "toggleLocationActive"
  | "toggleLocationPickable";

export type WarehouseWorkStep = "build" | "qa" | "pack";

export type WarehouseLayoutMutation = {
  action: WarehouseLayoutAction;
  balanceKey?: string;
  countedQuantities?: Record<string, number>;
  countId?: string;
  packageId?: string;
  reservationId?: string;
  shipmentId?: string;
  inventoryItemId?: string;
  inventoryOwnerPartyId?: string;
  damagedQuantity?: number;
  expectedQuantity?: number;
  itemName?: string;
  itemSku?: string;
  itemTrackingMode?: InventoryTrackingMode;
  itemUom?: string;
  lotId?: string;
  locationAisle?: string;
  locationBay?: string;
  locationCapacityUnits?: number;
  locationLevel?: string;
  locationName?: string;
  locationPositionNote?: string;
  locationSlotType?: WarehouseLocation["slotType"];
  parentId?: string;
  putawayTaskId?: string;
  reasonCode?: string;
  receiptDisposition?: "quarantine" | "damaged";
  scanBarcode?: string;
  scannerDeviceId?: string;
  serialId?: string;
  sourceId?: string;
  slotCount?: number;
  stockStatusTo?: StockStatus;
  targetLocationId?: string;
  transferId?: string;
  quantity?: number;
  adjustedQuantity?: number;
};

export type WarehouseWorkStepMutation = {
  lineId?: string;
  reservationId: string;
  step: WarehouseWorkStep;
};

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

export async function executeWarehouseMutation(action: WarehouseMutationAction, reservationId?: string): Promise<WarehousePersistenceSnapshot & { action: WarehouseMutationAction }> {
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

    const next = applySimulatorAction(snapshot, action, reservationId);
    await persistWarehouseState(tx, next);
    return {
      action,
      persisted: true,
      snapshot: next,
      summary: summarizeWarehouse(next)
    };
  });
}

export async function executeWarehouseLayoutMutation(input: WarehouseLayoutMutation): Promise<WarehousePersistenceSnapshot & { layoutAction: WarehouseLayoutAction }> {
  if (!process.env.DATABASE_URL) {
    throw new Error("DATABASE_URL is required for persistent warehouse layout mutations.");
  }

  const db = createBusinessDb();
  return db.transaction(async (tx) => {
    await lockWarehouse(tx);
    let snapshot = await loadWarehouseState(tx);
    if (!snapshot.items.length) {
      snapshot = buildWarehouseDemo();
      await persistWarehouseState(tx, snapshot);
    }

    const next = applyLayoutMutation(snapshot, input);
    await persistWarehouseState(tx, next);
    return {
      layoutAction: input.action,
      persisted: true,
      snapshot: next,
      summary: summarizeWarehouse(next)
    };
  });
}

export async function executeWarehouseWorkStepMutation(input: WarehouseWorkStepMutation): Promise<WarehousePersistenceSnapshot & { workStep: WarehouseWorkStep }> {
  if (!process.env.DATABASE_URL) {
    throw new Error("DATABASE_URL is required for persistent warehouse work-step mutations.");
  }

  const db = createBusinessDb();
  return db.transaction(async (tx) => {
    await lockWarehouse(tx);
    let snapshot = await loadWarehouseState(tx);
    if (!snapshot.items.length) {
      snapshot = buildWarehouseDemo();
      await persistWarehouseState(tx, snapshot);
    }

    const next = applyWorkStepMutation(snapshot, input);
    await persistWarehouseState(tx, next);
    return {
      persisted: true,
      snapshot: next,
      summary: summarizeWarehouse(next),
      workStep: input.step
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

function applyLayoutMutation(state: WarehouseState, input: WarehouseLayoutMutation): WarehouseState {
  if (input.action === "createWarehouse") {
    const warehouses = state.locations.filter((location) => location.kind === "warehouse");
    const nextNumber = warehouses.length + 1;
    const name = input.locationName?.trim() || `Warehouse ${nextNumber}`;
    const id = nextLocationId(state, `loc-${slugPart(name)}`);
    return {
      ...state,
      locations: [
        ...state.locations,
        {
          companyId: WAREHOUSE_COMPANY_ID,
          defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
          externalId: id,
          id,
          kind: "warehouse",
          name,
          pickable: false,
          receivable: true
        }
      ],
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `layout:create-warehouse:${id}`,
          payload: { locationId: id, name },
          refId: id,
          refType: "warehouse_location",
          requestedBy: "user",
          type: "PostStockMovement"
        })
      ]
    };
  }

  if (input.action === "createItem") {
    const item = createInventoryItem(state, input);
    return {
      ...state,
      items: [...state.items, item],
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `item:create:${item.id}`,
          payload: item,
          refId: item.id,
          refType: "inventory_item",
          requestedBy: "user",
          type: "CreateInventoryItem"
        })
      ]
    };
  }

  if (input.action === "duplicateItem") {
    const source = state.items.find((item) => item.id === input.inventoryItemId);
    if (!source) throw new Error("Inventory item not found.");
    const id = nextItemId(state, `${source.id}-copy`);
    const sku = uniqueSku(state, input.itemSku?.trim() || `${source.sku}-COPY`);
    const item: InventoryItem = {
      ...source,
      externalId: id,
      id,
      name: input.itemName?.trim() || `${source.name} Copy`,
      sku
    };
    return {
      ...state,
      items: [...state.items, item],
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `item:duplicate:${source.id}:${id}`,
          payload: { item, sourceItemId: source.id },
          refId: id,
          refType: "inventory_item",
          requestedBy: "user",
          type: "DuplicateInventoryItem"
        })
      ]
    };
  }

  if (input.action === "renameItem") {
    const item = state.items.find((entry) => entry.id === input.inventoryItemId);
    const name = input.itemName?.trim();
    if (!item) throw new Error("Inventory item not found.");
    if (!name) throw new Error("Inventory item name is required.");
    const sku = input.itemSku?.trim() || item.sku;
    return {
      ...state,
      items: state.items.map((entry) => entry.id === item.id ? { ...entry, name, sku, uom: input.itemUom?.trim() || entry.uom, trackingMode: input.itemTrackingMode ?? entry.trackingMode } : entry),
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `item:rename:${item.id}:${name}:${sku}`,
          payload: { itemId: item.id, name, sku },
          refId: item.id,
          refType: "inventory_item",
          requestedBy: "user",
          type: "RenameInventoryItem"
        })
      ]
    };
  }

  if (input.action === "deactivateItem") {
    const item = state.items.find((entry) => entry.id === input.inventoryItemId);
    if (!item) throw new Error("Inventory item not found.");
    return {
      ...state,
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `item:deactivate:${item.id}`,
          payload: { itemId: item.id, status: "inactive" },
          refId: item.id,
          refType: "inventory_item",
          requestedBy: "user",
          type: "DeactivateInventoryItem"
        })
      ]
    };
  }

  if (input.action === "openCycleCount") {
    const location = state.locations.find((entry) => entry.id === input.parentId);
    if (!location || location.kind !== "bin") throw new Error("Slot not found.");
    const countId = nextCycleCountId(state, location.id);
    return openCycleCount(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `cycle:open:${countId}`,
        payload: { locationId: location.id },
        refId: location.id,
        refType: "warehouse_location",
        requestedBy: "user",
        type: "OpenCycleCount"
      }),
      countId,
      locationId: location.id
    });
  }

  if (input.action === "recordCycleCount") {
    const countId = input.countId;
    if (!countId) throw new Error("Cycle count is required.");
    const count = state.cycleCounts.find((entry) => entry.id === countId);
    if (!count) throw new Error("Cycle count not found.");
    const countedQuantities = input.countedQuantities ?? {};
    return count.lines.reduce((next, line) => {
      const raw = countedQuantities[line.id];
      if (typeof raw !== "number" || !Number.isFinite(raw)) return next;
      return recordCycleCountLine(next, {
        countedQuantity: Math.max(0, Math.floor(raw)),
        countId,
        idempotencyKey: `cycle:record:${countId}:${line.id}:${Math.max(0, Math.floor(raw))}`,
        lineId: line.id
      });
    }, state);
  }

  if (input.action === "closeCycleCount") {
    const countId = input.countId;
    if (!countId) throw new Error("Cycle count is required.");
    return closeCycleCount(state, countId, `cycle:close:${countId}:${Date.now()}`);
  }

  if (input.action === "createPickList") {
    const reservationId = input.reservationId;
    if (!reservationId) throw new Error("Reservation is required.");
    return createPickList(state, reservationId, `picklist:create:${reservationId}:${Date.now()}`);
  }

  if (input.action === "scanPick") {
    const reservationId = input.reservationId;
    if (!reservationId) throw new Error("Reservation is required.");
    const reservation = state.reservations.find((entry) => entry.id === reservationId);
    const pickList = state.pickLists.find((entry) => entry.reservationId === reservationId);
    const barcode = input.scanBarcode?.trim();
    if (!reservation) throw new Error("Reservation not found.");
    if (!pickList || pickList.status !== "ready") throw new Error("Ready pick list is required.");
    if (!barcode) throw new Error("Scan code is required.");
    const acceptedCodes = new Set([
      pickList.id,
      reservation.id,
      reservation.sourceId,
      ...pickList.lines.flatMap((line) => {
        const item = state.items.find((entry) => entry.id === line.inventoryItemId);
        const location = state.locations.find((entry) => entry.id === line.locationId);
        return [line.id, item?.sku, item?.id, item?.name, location?.name, location?.id].filter((value): value is string => Boolean(value));
      })
    ].map((value) => value.toLowerCase()));
    if (!acceptedCodes.has(barcode.toLowerCase())) throw new Error("Scan does not match pick list, item, or source slot.");
    const deviceId = input.scannerDeviceId?.trim() || "web-scanner";
    const dateKey = new Date().toISOString().slice(0, 10).replaceAll("-", "");
    const sessionId = `scan-${deviceId}-${dateKey}`;
    const firstLine = pickList.lines[0];
    const withSession = state.scannerSessions.some((session) => session.id === sessionId)
      ? state
      : startScannerSession(state, {
          command: createWarehouseCommand({
            companyId: WAREHOUSE_COMPANY_ID,
            idempotencyKey: `scanner:start:${sessionId}`,
            payload: { deviceId, locationId: firstLine?.locationId },
            refId: sessionId,
            refType: "scanner_session",
            requestedBy: "user",
            type: "StartScannerSession"
          }),
          deviceId,
          locationId: firstLine?.locationId,
          sessionId,
          userId: "warehouse-user"
        });
    const eventId = `scan-pick-${slugPart(pickList.id)}-${Date.now().toString(36)}`;
    const scanned = ingestScanEvent(withSession, {
      action: "pick",
      barcode,
      companyId: WAREHOUSE_COMPANY_ID,
      eventId,
      inventoryItemId: firstLine?.inventoryItemId,
      locationId: firstLine?.locationId,
      lotId: firstLine?.lotId,
      quantity: pickList.lines.reduce((sum, line) => sum + line.quantity, 0),
      serialId: firstLine?.serialId,
      sessionId
    });
    return pickReservation(scanned, reservation.id, `pick:scan:${reservation.id}:${eventId}`);
  }

  if (input.action === "planWave") {
    const reservationIds = state.reservations
      .filter((reservation) => reservation.status !== "consumed" && reservation.status !== "cancelled" && reservation.status !== "released")
      .slice(0, 8)
      .map((reservation) => reservation.id);
    if (!reservationIds.length) throw new Error("No open reservations for wave planning.");
    const waveId = `wave-${dateStamp()}-${state.wavePlans.length + 1}`;
    return createWavePlan(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `wave:create:${waveId}`,
        payload: { reservationIds },
        refId: waveId,
        refType: "warehouse_wave_plan",
        requestedBy: "user",
        type: "CreateWavePlan"
      }),
      priority: reservationIds.length > 2 ? "expedite" : "normal",
      reservationIds,
      waveId
    });
  }

  if (input.action === "planSlotting") {
    const source = input.balanceKey ? state.balances.find((balance) => balance.balanceKey === input.balanceKey) : state.balances.find((balance) => balance.quantity > 0 && balance.stockStatus === "available");
    if (!source) throw new Error("Available stock is required.");
    const target = state.locations.find((location) => location.kind === "bin" && location.pickable && location.id !== source.locationId && getLocationLoad(state, location.id) === 0)
      ?? state.locations.find((location) => location.kind === "bin" && location.pickable && location.id !== source.locationId);
    if (!target) throw new Error("No target slot available for slotting recommendation.");
    const recommendationId = `slotting-${dateStamp()}-${state.slottingRecommendations.length + 1}`;
    return createSlottingRecommendation(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `slotting:create:${recommendationId}`,
        payload: { fromLocationId: source.locationId, inventoryItemId: source.inventoryItemId, reason: "pick_path_or_capacity_review", toLocationId: target.id },
        refId: recommendationId,
        refType: "slotting_recommendation",
        requestedBy: "user",
        type: "CreateSlottingRecommendation"
      }),
      fromLocationId: source.locationId,
      inventoryItemId: source.inventoryItemId,
      reason: "pick_path_or_capacity_review",
      recommendationId,
      toLocationId: target.id
    });
  }

  if (input.action === "createInterWarehouseTransfer") {
    const source = input.balanceKey ? state.balances.find((balance) => balance.balanceKey === input.balanceKey) : state.balances.find((balance) => balance.quantity > 0 && balance.stockStatus === "available");
    if (!source) throw new Error("Available stock is required.");
    const sourceWarehouseId = findWarehouseRootId(state, source.locationId);
    const target = state.locations.find((location) => location.kind === "bin" && location.pickable && findWarehouseRootId(state, location.id) !== sourceWarehouseId);
    if (!target) throw new Error("A target slot in another warehouse is required.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? 1)));
    const transferId = `transfer-${dateStamp()}-${state.transfers.length + 1}`;
    return createWarehouseTransfer(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `transfer:create:${transferId}`,
        payload: { fromLocationId: source.locationId, quantity, toLocationId: target.id },
        refId: transferId,
        refType: "warehouse_transfer",
        requestedBy: "user",
        type: "CreateWarehouseTransfer"
      }),
      fromLocationId: source.locationId,
      fromNodeId: nodeForWarehouse(state, sourceWarehouseId),
      lines: [{
        inventoryItemId: source.inventoryItemId,
        inventoryOwnerPartyId: source.inventoryOwnerPartyId,
        lotId: source.lotId,
        quantity,
        serialId: source.serialId
      }],
      toLocationId: target.id,
      toNodeId: nodeForWarehouse(state, findWarehouseRootId(state, target.id)),
      transferId
    });
  }

  if (input.action === "shipInterWarehouseTransfer") {
    const transferId = input.transferId ?? state.transfers.find((transfer) => transfer.status === "draft")?.id;
    if (!transferId) throw new Error("Draft transfer is required.");
    return shipWarehouseTransfer(state, transferId, `transfer:ship:${transferId}:${Date.now()}`);
  }

  if (input.action === "receiveInterWarehouseTransfer") {
    const transferId = input.transferId ?? state.transfers.find((transfer) => transfer.status === "shipped")?.id;
    if (!transferId) throw new Error("Shipped transfer is required.");
    return receiveWarehouseTransfer(state, transferId, `transfer:receive:${transferId}:${Date.now()}`);
  }

  if (input.action === "packShipment") {
    const shipmentId = input.shipmentId ?? state.shipments.find((shipment) => !state.shipmentPackages.some((pkg) => pkg.shipmentId === shipment.id))?.id;
    if (!shipmentId) throw new Error("Shipment without package is required.");
    const packageId = `pkg-${slugPart(shipmentId)}-${state.shipmentPackages.length + 1}`;
    return createShipmentPackage(state, {
      carrier: "DHL",
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `package:create:${packageId}`,
        payload: { shipmentId },
        refId: packageId,
        refType: "shipment_package",
        requestedBy: "user",
        type: "CreateShipmentPackage"
      }),
      packageId,
      shipmentId
    });
  }

  if (input.action === "createShipmentLabel") {
    const packageId = input.packageId ?? state.shipmentPackages.find((pkg) => pkg.status === "packed")?.id;
    if (!packageId) throw new Error("Packed package is required.");
    const labelId = `label-${slugPart(packageId)}-${state.fulfillmentLabels.length + 1}`;
    return createFulfillmentLabel(state, {
      carrier: "DHL",
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `label:create:${labelId}`,
        payload: { packageId, provider: "stripe-shipping" },
        refId: labelId,
        refType: "fulfillment_label",
        requestedBy: "user",
        type: "CreateFulfillmentLabel"
      }),
      labelId,
      packageId,
      provider: "stripe-shipping",
      trackingNumber: `TRACK-${labelId.toUpperCase()}`
    });
  }

  if (input.action === "recordCarrierHandover") {
    const labelledPackage = state.shipmentPackages.find((pkg) => pkg.status === "labelled" && pkg.trackingNumber);
    if (!labelledPackage) throw new Error("Labelled package is required.");
    const shipment = state.shipments.find((entry) => entry.id === labelledPackage.shipmentId);
    if (!shipment) throw new Error("Shipment not found.");
    const eventId = `handover-${slugPart(labelledPackage.id)}-${Date.now().toString(36)}`;
    return recordShipmentTrackingEvent(state, {
      carrier: labelledPackage.carrier ?? "DHL",
      companyId: WAREHOUSE_COMPANY_ID,
      eventCode: "carrier_handover",
      eventId,
      shipmentId: shipment.id,
      trackingNumber: labelledPackage.trackingNumber ?? shipment.trackingNumber ?? eventId
    });
  }

  if (input.action === "authorizeReturn") {
    const shipment = input.shipmentId ? state.shipments.find((entry) => entry.id === input.shipmentId) : state.shipments.find((entry) => entry.status === "shipped" && entry.lines.length > 0 && !state.returns.some((ret) => ret.sourceShipmentId === entry.id));
    if (!shipment) throw new Error("Shipped shipment without return is required.");
    const returnId = `return-${slugPart(shipment.id)}-${state.returns.length + 1}`;
    return authorizeReturn(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `return:authorize:${returnId}`,
        payload: { shipmentId: shipment.id },
        refId: returnId,
        refType: "return_authorization",
        requestedBy: "user",
        type: "AuthorizeReturn"
      }),
      lines: shipment.lines.slice(0, 1).map((line) => ({ acceptedQuantity: line.quantity, quantity: line.quantity, resellable: true, shipmentLineId: line.id })),
      returnId,
      shipmentId: shipment.id
    });
  }

  if (input.action === "receiveReturn") {
    const returnId = input.sourceId ?? state.returns.find((entry) => entry.status === "authorized")?.id;
    if (!returnId) throw new Error("Authorized return is required.");
    return receiveReturn(state, returnId, `return:receive:${returnId}:${Date.now()}`);
  }

  if (input.action === "recordImportDryRun") {
    const batchId = `import-dry-run-${dateStamp()}-${state.offlineSyncBatches.length + 1}`;
    const events: OfflineSyncEvent[] = state.balances.filter((balance) => balance.quantity > 0).slice(0, 3).map((balance, index) => ({
      action: "validate_start_balance",
      externalId: `${batchId}-event-${index + 1}`,
      id: `${batchId}-event-${index + 1}`,
      idempotencyKey: `${batchId}:${balance.balanceKey}`,
      payload: { balanceKey: balance.balanceKey, quantity: balance.quantity }
    }));
    if (!events.length) throw new Error("At least one balance is required for import dry run.");
    return recordOfflineSyncBatch(state, {
      batchId,
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `import:dry-run:${batchId}`,
        payload: { eventCount: events.length },
        refId: batchId,
        refType: "warehouse_import",
        requestedBy: "user",
        type: "RecordOfflineSyncBatch"
      }),
      deviceId: "import-wizard",
      events
    });
  }

  if (input.action === "recordSyncConflict") {
    const item = state.items[0];
    if (!item) throw new Error("Inventory item is required.");
    const eventId = `sync-conflict-${slugPart(item.sku)}-${Date.now().toString(36)}`;
    return ingestIntegrationEvent(state, {
      companyId: WAREHOUSE_COMPANY_ID,
      eventId,
      eventType: "stock_conflict_detected",
      payload: { ctoxAvailable: getAvailableQuantityForItem(state, item.id), itemSku: item.sku, webshopAvailable: Math.max(0, getAvailableQuantityForItem(state, item.id) - 1) },
      provider: "webshop",
      source: "commerce"
    });
  }

  if (input.action === "recordOpsHandover") {
    const eventId = `shift-handover-${dateStamp()}-${state.integrationEvents.length + 1}`;
    return ingestIntegrationEvent(state, {
      companyId: WAREHOUSE_COMPANY_ID,
      eventId,
      eventType: "shift_handover_signed",
      payload: {
        openPickLists: state.pickLists.filter((pickList) => pickList.status === "ready").length,
        openPutaway: state.putawayTasks.filter((task) => task.status === "open").length,
        qualityHolds: state.balances.filter((balance) => balance.quantity > 0 && (balance.stockStatus === "quarantine" || balance.stockStatus === "damaged")).length
      },
      provider: "ctox-ops",
      source: "wes"
    });
  }

  if (input.action === "recordRoleReview") {
    const eventId = `role-gate-${dateStamp()}-${state.integrationEvents.length + 1}`;
    return ingestIntegrationEvent(state, {
      companyId: WAREHOUSE_COMPANY_ID,
      eventId,
      eventType: "role_gate_reviewed",
      payload: { criticalActions: ["adjustBalance", "resolveQualityHold", "recordThreePlCharge"], role: "warehouse-lead" },
      provider: "ctox-rbac",
      source: "wes"
    });
  }

  if (input.action === "recordThreePlCharge") {
    const ownerId = state.balances.find((balance) => balance.inventoryOwnerPartyId !== SYSTEM_OWNER_PARTY_ID)?.inventoryOwnerPartyId
      ?? state.balances[0]?.inventoryOwnerPartyId
      ?? SYSTEM_OWNER_PARTY_ID;
    const chargeId = `3pl-${slugPart(ownerId)}-${dateStamp()}-${state.threePlCharges.length + 1}`;
    return recordThreePlCharge(state, {
      amountCents: 250,
      chargeId,
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `3pl:charge:${chargeId}`,
        payload: { metric: "pick", ownerId },
        refId: chargeId,
        refType: "three_pl_charge",
        requestedBy: "user",
        type: "RecordThreePlCharge"
      }),
      currency: "EUR",
      inventoryOwnerPartyId: ownerId,
      metric: "pick",
      quantity: 1,
      sourceId: state.pickLists.find((pickList) => pickList.status === "picked")?.id ?? "manual",
      sourceType: "pick_list"
    });
  }

  if (input.action === "receiveInbound") {
    const item = state.items.find((entry) => entry.id === input.inventoryItemId);
    const target = state.locations.find((location) => location.id === input.targetLocationId);
    const receiving = (input.sourceId ? state.locations.find((location) => location.id === input.sourceId) : undefined)
      ?? state.locations.find((location) => location.id === "loc-receiving")
      ?? state.locations.find((location) => location.kind === "bin" && location.receivable);
    if (!item) throw new Error("Inventory item not found.");
    if (isInventoryItemInactive(state, item.id)) throw new Error("Inventory item is inactive.");
    if (!target || target.kind !== "bin") throw new Error("Target slot not found.");
    if (!receiving) throw new Error("Receiving location not found.");
    if (item.trackingMode === "lot" && !input.lotId?.trim()) throw new Error("Lot number is required for lot-tracked items.");
    if (item.trackingMode === "serial" && !input.serialId?.trim()) throw new Error("Serial number is required for serial-tracked items.");
    const acceptedQuantity = item.trackingMode === "serial" ? Math.max(0, Math.min(1, Math.floor(input.quantity ?? 1))) : Math.max(0, Math.floor(input.quantity ?? 1));
    const damagedQuantity = item.trackingMode === "serial" ? Math.max(0, Math.min(1, Math.floor(input.damagedQuantity ?? 0))) : Math.max(0, Math.floor(input.damagedQuantity ?? 0));
    const totalReceivedQuantity = acceptedQuantity + damagedQuantity;
    if (totalReceivedQuantity <= 0) throw new Error("At least one received or damaged unit is required.");
    if (item.trackingMode === "serial" && totalReceivedQuantity > 1) throw new Error("Serial-tracked receipts can only process one unit at a time.");
    assertLocationCapacity(state, target.id, acceptedQuantity);
    const ownerId = input.inventoryOwnerPartyId?.trim() || target.defaultOwnerPartyId || SYSTEM_OWNER_PARTY_ID;
    const receiptId = nextReceiptId(state);
    const goodLineId = `${receiptId}-line-1`;
    const damageLineId = `${receiptId}-damage-1`;
    const receiptLines: ReceiptLine[] = [];
    if (acceptedQuantity > 0) {
      receiptLines.push({
        companyId: WAREHOUSE_COMPANY_ID,
        id: goodLineId,
        inventoryItemId: item.id,
        inventoryOwnerPartyId: ownerId,
        locationId: receiving.id,
        lotId: input.lotId?.trim() || undefined,
        quantity: acceptedQuantity,
        serialId: input.serialId?.trim() || undefined
      });
    }
    if (damagedQuantity > 0) {
      receiptLines.push({
        companyId: WAREHOUSE_COMPANY_ID,
        id: damageLineId,
        inventoryItemId: item.id,
        inventoryOwnerPartyId: ownerId,
        locationId: receiving.id,
        lotId: input.lotId?.trim() || undefined,
        quantity: damagedQuantity,
        serialId: input.serialId?.trim() || undefined
      });
    }
    const expectedQuantity = Math.max(0, Math.floor(input.expectedQuantity ?? totalReceivedQuantity));
    const receiptDisposition = input.receiptDisposition === "damaged" ? "damaged" : "quarantine";
    const received = receiveStock(state, {
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `receipt:receive:${receiptId}`,
        payload: {
          acceptedQuantity,
          damagedQuantity,
          expectedQuantity,
          inventoryItemId: item.id,
          inventoryOwnerPartyId: ownerId,
          lotId: input.lotId?.trim() || undefined,
          quantity: totalReceivedQuantity,
          receiptDisposition: damagedQuantity > 0 ? receiptDisposition : undefined,
          serialId: input.serialId?.trim() || undefined,
          sourceId: input.sourceId?.trim() || "manual-receipt",
          targetLocationId: target.id,
          varianceQuantity: totalReceivedQuantity - expectedQuantity
        },
        refId: receiptId,
        refType: "warehouse_receipt",
        requestedBy: "user",
        type: "ReceiveStock"
      }),
      lines: receiptLines,
      receiptId,
      sourceId: input.sourceId?.trim() || `manual-${receiptId}`,
      sourceType: "manual_receipt"
    });
    const withPutaway = acceptedQuantity > 0
      ? createPutawayTasksForReceiptLines(received, receiptId, [goodLineId], target.id)
      : received;
    if (damagedQuantity <= 0) return withPutaway;
    const damageBalance = withPutaway.balances.find((balance) =>
      balance.inventoryItemId === item.id &&
      balance.inventoryOwnerPartyId === ownerId &&
      balance.locationId === receiving.id &&
      balance.stockStatus === "receiving" &&
      (balance.lotId ?? undefined) === (input.lotId?.trim() || undefined) &&
      (balance.serialId ?? undefined) === (input.serialId?.trim() || undefined)
    );
    if (!damageBalance) return withPutaway;
    return transferBalanceStatus(withPutaway, damageBalance, receiptDisposition, damagedQuantity, "receipt_exception");
  }

  if (input.action === "completePutaway") {
    const taskId = input.putawayTaskId;
    if (!taskId) throw new Error("Putaway task is required.");
    const task = state.putawayTasks.find((entry) => entry.id === taskId);
    if (!task) throw new Error("Putaway task not found.");
    if (isInventoryItemInactive(state, task.inventoryItemId)) throw new Error("Inventory item is inactive.");
    assertLocationCapacity(state, task.toLocationId, task.quantity, { excludePutawayTaskId: task.id });
    const moved = completePutaway(state, taskId, `putaway:complete:${taskId}:${Date.now()}`);
    const relatedReceipt = moved.receipts.find((receipt) => receipt.lines.some((line) => line.id === task.receiptLineId));
    if (!relatedReceipt) return moved;
    const receiptLineIds = new Set(relatedReceipt.lines.map((line) => line.id));
    const openTask = moved.putawayTasks.some((entry) => receiptLineIds.has(entry.receiptLineId) && entry.status === "open");
    return {
      ...moved,
      receipts: moved.receipts.map((receipt) => receipt.id === relatedReceipt.id
        ? { ...receipt, status: openTask ? "putaway_started" : "putaway_complete", version: receipt.version + 1 }
        : receipt)
    };
  }

  if (input.action === "scanPutaway") {
    const taskId = input.putawayTaskId;
    if (!taskId) throw new Error("Putaway task is required.");
    const task = state.putawayTasks.find((entry) => entry.id === taskId);
    if (!task) throw new Error("Putaway task not found.");
    if (task.status !== "open") throw new Error("Putaway task is not open.");
    const item = state.items.find((entry) => entry.id === task.inventoryItemId);
    const target = state.locations.find((entry) => entry.id === task.toLocationId);
    if (!item) throw new Error("Inventory item not found.");
    if (!target) throw new Error("Target slot not found.");
    const barcode = input.scanBarcode?.trim();
    if (!barcode) throw new Error("Scan code is required.");
    const acceptedCodes = new Set([item.sku, item.id, item.name, target.name, target.id, task.id, task.receiptLineId].map((value) => value.toLowerCase()));
    if (!acceptedCodes.has(barcode.toLowerCase())) throw new Error("Scan does not match item, target slot, or putaway task.");
    const deviceId = input.scannerDeviceId?.trim() || "web-scanner";
    const dateKey = new Date().toISOString().slice(0, 10).replaceAll("-", "");
    const sessionId = `scan-${deviceId}-${dateKey}`;
    const withSession = state.scannerSessions.some((session) => session.id === sessionId)
      ? state
      : startScannerSession(state, {
          command: createWarehouseCommand({
            companyId: WAREHOUSE_COMPANY_ID,
            idempotencyKey: `scanner:start:${sessionId}`,
            payload: { deviceId, locationId: target.id },
            refId: sessionId,
            refType: "scanner_session",
            requestedBy: "user",
            type: "StartScannerSession"
          }),
          deviceId,
          locationId: target.id,
          sessionId,
          userId: "warehouse-user"
        });
    const eventId = `scan-putaway-${slugPart(task.id)}-${Date.now().toString(36)}`;
    const scanned = ingestScanEvent(withSession, {
      action: "putaway",
      barcode,
      companyId: WAREHOUSE_COMPANY_ID,
      eventId,
      inventoryItemId: task.inventoryItemId,
      locationId: target.id,
      lotId: task.lotId,
      quantity: task.quantity,
      serialId: task.serialId,
      sessionId
    });
    const moved = completePutaway(scanned, taskId, `putaway:scan-complete:${taskId}:${eventId}`);
    const relatedReceipt = moved.receipts.find((receipt) => receipt.lines.some((line) => line.id === task.receiptLineId));
    if (!relatedReceipt) return moved;
    const receiptLineIds = new Set(relatedReceipt.lines.map((line) => line.id));
    const openTask = moved.putawayTasks.some((entry) => receiptLineIds.has(entry.receiptLineId) && entry.status === "open");
    return {
      ...moved,
      receipts: moved.receipts.map((receipt) => receipt.id === relatedReceipt.id
        ? { ...receipt, status: openTask ? "putaway_started" : "putaway_complete", version: receipt.version + 1 }
        : receipt)
    };
  }

  if (input.action === "reserveBalance") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    if (!source) throw new Error("Source balance not found.");
    if (source.stockStatus !== "available") throw new Error("Only available stock can be reserved.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? source.quantity)));
    const reservationId = nextManualReservationId(state);
    return reserveStock(state, {
      allowPartialReservation: false,
      command: createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: `manual:reserve:${reservationId}`,
        payload: {
          balanceKey: source.balanceKey,
          quantity,
          sourceId: input.sourceId?.trim() || reservationId
        },
        refId: input.sourceId?.trim() || reservationId,
        refType: "manual_order",
        requestedBy: "user",
        type: "ReserveStock"
      }),
      lines: [
        {
          allowBackorder: false,
          companyId: source.companyId,
          inventoryItemId: source.inventoryItemId,
          inventoryOwnerPartyId: source.inventoryOwnerPartyId,
          locationId: source.locationId,
          lotId: source.lotId,
          quantity,
          serialId: source.serialId,
          sourceLineId: `${reservationId}-line-1`,
          stockStatus: "available"
        }
      ],
      reservationId,
      sourceId: input.sourceId?.trim() || reservationId,
      sourceType: "manual_order"
    });
  }

  if (input.action === "changeStockStatus") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    const stockStatusTo = input.stockStatusTo;
    if (!source) throw new Error("Source balance not found.");
    if (!stockStatusTo) throw new Error("Target stock status is required.");
    if (source.stockStatus === stockStatusTo) throw new Error("Source and target stock status are identical.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? source.quantity)));
    return transferBalanceStatus(state, source, stockStatusTo, quantity, input.reasonCode?.trim() || "manual_status_change");
  }

  if (input.action === "adjustBalance") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    if (!source) throw new Error("Source balance not found.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    const adjustedQuantity = Math.max(0, Math.floor(input.adjustedQuantity ?? source.quantity));
    if (adjustedQuantity > source.quantity) {
      assertLocationCapacity(state, source.locationId, adjustedQuantity, { excludeBalanceKey: source.balanceKey });
    }
    return adjustBalanceQuantity(state, source, adjustedQuantity, input.reasonCode?.trim() || "manual_adjustment");
  }

  if (input.action === "resolveQualityHold") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    const target = state.locations.find((location) => location.id === input.targetLocationId);
    if (!source) throw new Error("Source balance not found.");
    if (!target || target.kind !== "bin") throw new Error("Target slot not found.");
    if (source.stockStatus !== "quarantine" && source.stockStatus !== "damaged") throw new Error("Only QA or damaged stock can be released.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? source.quantity)));
    assertLocationCapacity(state, target.id, quantity);
    return transferBalanceToLocationStatus(state, source, target.id, "available", quantity, input.reasonCode?.trim() || "qa_release");
  }

  if (input.action === "scrapQualityHold") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    if (!source) throw new Error("Source balance not found.");
    if (source.stockStatus !== "quarantine" && source.stockStatus !== "damaged") throw new Error("Only QA or damaged stock can be scrapped.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? source.quantity)));
    return scrapBalanceQuantity(state, source, quantity, input.reasonCode?.trim() || "qa_scrap");
  }

  if (input.action === "moveStock") {
    const source = state.balances.find((balance) => balance.balanceKey === input.balanceKey);
    const target = state.locations.find((location) => location.id === input.targetLocationId);
    if (!source) throw new Error("Source balance not found.");
    if (isInventoryItemInactive(state, source.inventoryItemId)) throw new Error("Inventory item is inactive.");
    if (!target || target.kind !== "bin") throw new Error("Target slot not found.");
    const quantity = Math.max(1, Math.min(source.quantity, Math.floor(input.quantity ?? source.quantity)));
    assertLocationCapacity(state, target.id, quantity, { excludeBalanceKey: source.locationId === target.id ? source.balanceKey : undefined });
    const nextSource = { ...source, quantity: source.quantity - quantity, updatedAt: new Date().toISOString() };
    const targetDimension = { ...source, locationId: target.id };
    const targetBalanceKey = createBalanceKey(targetDimension);
    const existingTarget = state.balances.find((balance) => balance.balanceKey === targetBalanceKey);
    const balances = state.balances
      .map((balance) => balance.balanceKey === source.balanceKey ? nextSource : balance)
      .filter((balance) => balance.quantity > 0);
    const nextBalances = existingTarget
      ? balances.map((balance) => balance.balanceKey === targetBalanceKey ? { ...balance, quantity: balance.quantity + quantity, updatedAt: new Date().toISOString() } : balance)
      : [
          ...balances,
          {
            ...targetDimension,
            balanceKey: targetBalanceKey,
            locationId: target.id,
            quantity,
            updatedAt: new Date().toISOString()
          }
        ];
    return {
      ...state,
      balances: nextBalances,
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `layout:move:${source.balanceKey}:${target.id}:${Date.now()}`,
          payload: {
            fromLocationId: source.locationId,
            inventoryItemId: source.inventoryItemId,
            quantity,
            stockStatus: source.stockStatus,
            toLocationId: target.id
          },
          refId: source.balanceKey,
          refType: "stock_balance",
          requestedBy: "user",
          type: "PostStockMovement"
        })
      ]
    };
  }

  const parent = state.locations.find((location) => location.id === input.parentId);
  if (!parent) throw new Error("Parent warehouse or section not found.");

  if (input.action === "renameLocation") {
    const name = input.locationName?.trim();
    if (!name) throw new Error("Location name is required.");
    const capacityUnits = typeof input.locationCapacityUnits === "number" && Number.isFinite(input.locationCapacityUnits)
      ? Math.max(0, Math.floor(input.locationCapacityUnits))
      : undefined;
    return {
      ...state,
      locations: state.locations.map((location) => location.id === parent.id ? {
        ...location,
        aisle: input.locationAisle !== undefined ? input.locationAisle.trim() || undefined : location.aisle,
        bay: input.locationBay !== undefined ? input.locationBay.trim() || undefined : location.bay,
        capacityUnits: capacityUnits ?? location.capacityUnits,
        level: input.locationLevel !== undefined ? input.locationLevel.trim() || undefined : location.level,
        name,
        positionNote: input.locationPositionNote !== undefined ? input.locationPositionNote.trim() || undefined : location.positionNote,
        slotType: input.locationSlotType ?? location.slotType
      } : location),
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `layout:rename:${parent.id}:${name}`,
          payload: { locationId: parent.id, name },
          refId: parent.id,
          refType: "warehouse_location",
          requestedBy: "user",
          type: "PostStockMovement"
        })
      ]
    };
  }

  if (input.action === "toggleLocationPickable") {
    return {
      ...state,
      locations: state.locations.map((location) => location.id === parent.id ? { ...location, pickable: !location.pickable } : location),
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `layout:toggle-pickable:${parent.id}:${!parent.pickable}`,
          payload: { locationId: parent.id, pickable: !parent.pickable },
          refId: parent.id,
          refType: "warehouse_location",
          requestedBy: "user",
          type: "PostStockMovement"
        })
      ]
    };
  }

  if (input.action === "toggleLocationActive") {
    const inactive = latestLocationStatus(state, parent.id) !== "inactive";
    const affectedIds = new Set([parent.id, ...descendantLocationIds(state, parent.id)]);
    return {
      ...state,
      locations: state.locations.map((location) => {
        if (!affectedIds.has(location.id)) return location;
        if (inactive) return { ...location, pickable: false, receivable: false };
        if (location.kind === "warehouse") return { ...location, receivable: true };
        if (location.kind === "bin") return { ...location, pickable: true };
        return location;
      }),
      commandLog: [
        ...state.commandLog,
        createWarehouseCommand({
          companyId: WAREHOUSE_COMPANY_ID,
          idempotencyKey: `layout:toggle-active:${parent.id}:${inactive ? "inactive" : "active"}:${Date.now()}`,
          payload: { affectedLocationIds: [...affectedIds], locationId: parent.id, status: inactive ? "inactive" : "active" },
          refId: parent.id,
          refType: "warehouse_location",
          requestedBy: "user",
          type: "PostStockMovement"
        })
      ]
    };
  }

  if (input.action === "duplicateLocation") {
    if (parent.kind === "warehouse") {
      const id = nextLocationId(state, `${parent.id}-copy`);
      return {
        ...state,
        locations: [
          ...state.locations,
          {
            ...parent,
            externalId: id,
            id,
            name: `${parent.name} Copy`
          }
        ]
      };
    }
    if (parent.kind === "zone") {
      const id = nextLocationId(state, `${parent.id}-copy`);
      const childSlots = state.locations.filter((location) => location.parentId === parent.id && location.kind === "bin");
      const slotCopies = childSlots.map((slot, index): WarehouseLocation => {
        const slotId = nextLocationId(state, `${slot.id}-copy-${index + 1}`);
        return {
          ...slot,
          externalId: slotId,
          id: slotId,
          name: `${slot.name} Copy`,
          parentId: id
        };
      });
      return {
        ...state,
        locations: [
          ...state.locations,
          { ...parent, externalId: id, id, name: `${parent.name} Copy` },
          ...slotCopies
        ]
      };
    }
    const id = nextLocationId(state, `${parent.id}-copy`);
    return {
      ...state,
      locations: [
        ...state.locations,
        {
          ...parent,
          externalId: id,
          id,
          name: `${parent.name} Copy`
        }
      ]
    };
  }

  if (input.action === "createSection") {
    if (parent.kind !== "warehouse") throw new Error("Sections can only be added to a warehouse.");
    const existing = state.locations.filter((location) => location.parentId === parent.id && location.kind === "zone");
    const code = sectionCode(existing.length);
    const id = nextLocationId(state, `loc-zone-${slugPart(parent.name)}-${code.toLowerCase()}`);
    return {
      ...state,
      locations: [
        ...state.locations,
        {
          companyId: WAREHOUSE_COMPANY_ID,
          defaultOwnerPartyId: parent.defaultOwnerPartyId ?? SYSTEM_OWNER_PARTY_ID,
          externalId: id,
          id,
          kind: "zone",
          name: `${code}-Section`,
          parentId: parent.id,
          pickable: false,
          receivable: false
        }
      ]
    };
  }

  if (parent.kind !== "zone") throw new Error("Slots can only be added to a section.");
  const existingSlots = state.locations.filter((location) => location.parentId === parent.id && location.kind === "bin");
  const count = Math.max(1, Math.min(24, input.slotCount ?? 4));
  const sectionPrefix = parent.name.match(/[A-Z]/)?.[0] ?? "S";
  const additions = Array.from({ length: count }, (_, index): WarehouseLocation => {
    const slotNumber = existingSlots.length + index + 1;
    const id = nextLocationId(state, `loc-${slugPart(parent.name)}-${String(slotNumber).padStart(2, "0")}`);
    return {
      aisle: sectionPrefix,
      bay: String(slotNumber),
      capacityUnits: 100,
      companyId: WAREHOUSE_COMPANY_ID,
      defaultOwnerPartyId: parent.defaultOwnerPartyId ?? SYSTEM_OWNER_PARTY_ID,
      externalId: id,
      id,
      kind: "bin",
      level: "1",
      name: `${sectionPrefix}${slotNumber}`,
      parentId: parent.id,
      pickable: true,
      receivable: false,
      slotType: "pick_face"
    };
  });
  return {
    ...state,
    locations: [...state.locations, ...additions]
  };
}

function applyWorkStepMutation(state: WarehouseState, input: WarehouseWorkStepMutation): WarehouseState {
  const reservation = state.reservations.find((item) => item.id === input.reservationId);
  if (!reservation) throw new Error("Reservation not found.");
  const line = input.lineId ? reservation.lines.find((item) => item.id === input.lineId) : undefined;
  if (input.lineId && !line) throw new Error("Reservation line not found.");
  const idempotencyKey = `work-step:${input.reservationId}:${input.lineId ?? "order"}:${input.step}`;
  if (state.commandLog.some((command) => command.idempotencyKey === idempotencyKey)) return state;
  return {
    ...state,
    commandLog: [
      ...state.commandLog,
      createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey,
        payload: {
          lineId: input.lineId,
          reservationId: input.reservationId,
          sourceLineId: line?.sourceLineId,
          sourceId: reservation.sourceId,
          step: input.step
        },
        refId: input.lineId ?? input.reservationId,
        refType: input.lineId ? "warehouse_order_line" : "warehouse_order",
        requestedBy: "user",
        type: "CompleteValueStep"
      })
    ]
  };
}

function nextLocationId(state: WarehouseState, base: string) {
  const existing = new Set(state.locations.map((location) => location.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function createInventoryItem(state: WarehouseState, input: WarehouseLayoutMutation): InventoryItem {
  const nextNumber = state.items.length + 1;
  const rawName = input.itemName?.trim() || `New inventory item ${nextNumber}`;
  const id = nextItemId(state, `item-${slugPart(rawName)}`);
  const sku = uniqueSku(state, input.itemSku?.trim() || `SKU-${String(nextNumber).padStart(4, "0")}`);
  return {
    companyId: WAREHOUSE_COMPANY_ID,
    externalId: id,
    id,
    name: rawName,
    sku,
    trackingMode: input.itemTrackingMode ?? "none",
    uom: input.itemUom?.trim() || "pcs"
  };
}

function nextItemId(state: WarehouseState, base: string) {
  const existing = new Set(state.items.map((item) => item.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function descendantLocationIds(state: WarehouseState, parentId: string): string[] {
  return state.locations
    .filter((location) => location.parentId === parentId)
    .flatMap((location) => [location.id, ...descendantLocationIds(state, location.id)]);
}

function latestLocationStatus(state: WarehouseState, locationId: string) {
  const statusCommand = [...state.commandLog]
    .reverse()
    .find((command) =>
      command.refType === "warehouse_location" &&
      command.refId === locationId &&
      (command.payload.status === "inactive" || command.payload.status === "active")
    );
  return statusCommand?.payload.status;
}

function dateStamp() {
  return new Date().toISOString().slice(0, 10).replaceAll("-", "");
}

function getLocationLoad(state: WarehouseState, locationId: string) {
  return state.balances
    .filter((balance) => balance.locationId === locationId && balance.quantity > 0 && balance.stockStatus !== "shipped")
    .reduce((sum, balance) => sum + balance.quantity, 0);
}

function findWarehouseRootId(state: WarehouseState, locationId: string) {
  let location = state.locations.find((entry) => entry.id === locationId);
  while (location?.parentId) {
    const parent = state.locations.find((entry) => entry.id === location?.parentId);
    if (!parent) break;
    location = parent;
  }
  return location?.kind === "warehouse" ? location.id : undefined;
}

function nodeForWarehouse(state: WarehouseState, warehouseId?: string) {
  const warehouse = warehouseId ? state.locations.find((location) => location.id === warehouseId) : undefined;
  return state.nodes.find((node) => node.kind === "warehouse" && (!warehouse || node.name === warehouse.name))?.id
    ?? state.nodes.find((node) => node.kind === "warehouse")?.id
    ?? "node-warehouse";
}

function getAvailableQuantityForItem(state: WarehouseState, inventoryItemId: string) {
  return state.balances
    .filter((balance) => balance.inventoryItemId === inventoryItemId && balance.stockStatus === "available")
    .reduce((sum, balance) => sum + balance.quantity, 0);
}

function nextCycleCountId(state: WarehouseState, locationId: string) {
  const base = `cycle-${slugPart(locationId)}-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}`;
  const existing = new Set(state.cycleCounts.map((count) => count.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function nextReceiptId(state: WarehouseState) {
  const base = `receipt-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}`;
  const existing = new Set(state.receipts.map((receipt) => receipt.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function nextManualReservationId(state: WarehouseState) {
  const base = `manual-reserve-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}`;
  const existing = new Set(state.reservations.map((reservation) => reservation.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function createPutawayTasksForReceiptLines(state: WarehouseState, receiptId: string, receiptLineIds: string[], toLocationId: string) {
  const receipt = state.receipts.find((item) => item.id === receiptId);
  if (!receipt) throw new Error("receipt_not_found");
  const lineIds = new Set(receiptLineIds);
  const newTasks = receipt.lines
    .filter((line) => lineIds.has(line.id))
    .map((line): PutawayTask => ({
      companyId: receipt.companyId,
      externalId: `putaway-${receipt.id}-${line.id}`,
      fromLocationId: line.locationId,
      id: `putaway-${receipt.id}-${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      lotId: line.lotId,
      quantity: line.quantity,
      receiptLineId: line.id,
      serialId: line.serialId,
      status: "open",
      toLocationId,
      version: 1
    }));
  return {
    ...state,
    putawayTasks: [...state.putawayTasks, ...newTasks],
    receipts: state.receipts.map((entry) => entry.id === receiptId
      ? { ...entry, status: newTasks.length > 0 ? "putaway_started" : entry.status, version: entry.version + (newTasks.length > 0 ? 1 : 0) }
      : entry)
  };
}

function nextManualMovementId(state: WarehouseState, prefix: string) {
  const base = `mov-${prefix}-${Date.now().toString(36)}`;
  const existing = new Set(state.movements.map((movement) => movement.id));
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function transferBalanceStatus(state: WarehouseState, source: StockBalance, stockStatusTo: StockStatus, quantity: number, reasonCode: string): WarehouseState {
  const now = new Date().toISOString();
  const nextSource = { ...source, quantity: source.quantity - quantity, updatedAt: now };
  const targetDimension = { ...source, stockStatus: stockStatusTo };
  const targetBalanceKey = createBalanceKey(targetDimension);
  const existingTarget = state.balances.find((balance) => balance.balanceKey === targetBalanceKey);
  const balances = state.balances
    .map((balance) => balance.balanceKey === source.balanceKey ? nextSource : balance)
    .filter((balance) => balance.quantity > 0);
  const nextBalances = existingTarget
    ? balances.map((balance) => balance.balanceKey === targetBalanceKey ? { ...balance, quantity: balance.quantity + quantity, updatedAt: now } : balance)
    : [
        ...balances,
        {
          ...targetDimension,
          balanceKey: targetBalanceKey,
          quantity,
          updatedAt: now
        }
      ];
  const movementId = nextManualMovementId(state, "status");
  const movement: StockMovement = {
    companyId: source.companyId,
    externalId: movementId,
    id: movementId,
    idempotencyKey: `manual:status:${source.balanceKey}:${stockStatusTo}:${quantity}:${Date.now()}`,
    inventoryItemId: source.inventoryItemId,
    inventoryOwnerPartyId: source.inventoryOwnerPartyId,
    locationId: source.locationId,
    lotId: source.lotId,
    movementType: "adjust",
    postedAt: now,
    quantity,
    serialId: source.serialId,
    sourceId: reasonCode,
    sourceType: "manual_status_change",
    stockStatus: stockStatusTo,
    stockStatusFrom: source.stockStatus,
    stockStatusTo,
    uom: state.items.find((item) => item.id === source.inventoryItemId)?.uom ?? "ea"
  };
  return {
    ...state,
    balances: nextBalances,
    commandLog: [
      ...state.commandLog,
      createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: movement.idempotencyKey,
        payload: {
          balanceKey: source.balanceKey,
          from: source.stockStatus,
          quantity,
          reasonCode,
          to: stockStatusTo
        },
        refId: source.balanceKey,
        refType: "stock_balance",
        requestedBy: "user",
        type: "PostStockMovement"
      })
    ],
    movements: [...state.movements, movement]
  };
}

function transferBalanceToLocationStatus(
  state: WarehouseState,
  source: StockBalance,
  targetLocationId: string,
  stockStatusTo: StockStatus,
  quantity: number,
  reasonCode: string
): WarehouseState {
  const now = new Date().toISOString();
  const nextSource = { ...source, quantity: source.quantity - quantity, updatedAt: now };
  const targetDimension = { ...source, locationId: targetLocationId, stockStatus: stockStatusTo };
  const targetBalanceKey = createBalanceKey(targetDimension);
  const existingTarget = state.balances.find((balance) => balance.balanceKey === targetBalanceKey);
  const balances = state.balances
    .map((balance) => balance.balanceKey === source.balanceKey ? nextSource : balance)
    .filter((balance) => balance.quantity > 0);
  const nextBalances = existingTarget
    ? balances.map((balance) => balance.balanceKey === targetBalanceKey ? { ...balance, quantity: balance.quantity + quantity, updatedAt: now } : balance)
    : [
        ...balances,
        {
          ...targetDimension,
          balanceKey: targetBalanceKey,
          quantity,
          updatedAt: now
        }
      ];
  const movementId = nextManualMovementId(state, "qa-release");
  const movement: StockMovement = {
    companyId: source.companyId,
    externalId: movementId,
    id: movementId,
    idempotencyKey: `manual:qa-release:${source.balanceKey}:${targetLocationId}:${quantity}:${Date.now()}`,
    inventoryItemId: source.inventoryItemId,
    inventoryOwnerPartyId: source.inventoryOwnerPartyId,
    locationId: targetLocationId,
    lotId: source.lotId,
    movementType: "adjust",
    postedAt: now,
    quantity,
    serialId: source.serialId,
    sourceId: reasonCode,
    sourceType: "quality_review",
    stockStatus: stockStatusTo,
    stockStatusFrom: source.stockStatus,
    stockStatusTo,
    uom: state.items.find((item) => item.id === source.inventoryItemId)?.uom ?? "ea"
  };
  return {
    ...state,
    balances: nextBalances,
    commandLog: [
      ...state.commandLog,
      createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: movement.idempotencyKey,
        payload: {
          balanceKey: source.balanceKey,
          fromLocationId: source.locationId,
          fromStatus: source.stockStatus,
          quantity,
          reasonCode,
          toLocationId: targetLocationId,
          toStatus: stockStatusTo
        },
        refId: source.balanceKey,
        refType: "stock_balance",
        requestedBy: "user",
        type: "PostStockMovement"
      })
    ],
    movements: [...state.movements, movement]
  };
}

function scrapBalanceQuantity(state: WarehouseState, source: StockBalance, quantity: number, reasonCode: string): WarehouseState {
  const now = new Date().toISOString();
  const movementId = nextManualMovementId(state, "qa-scrap");
  const movement: StockMovement = {
    companyId: source.companyId,
    externalId: movementId,
    id: movementId,
    idempotencyKey: `manual:qa-scrap:${source.balanceKey}:${quantity}:${Date.now()}`,
    inventoryItemId: source.inventoryItemId,
    inventoryOwnerPartyId: source.inventoryOwnerPartyId,
    locationId: source.locationId,
    lotId: source.lotId,
    movementType: "adjust",
    postedAt: now,
    quantity,
    serialId: source.serialId,
    sourceId: reasonCode,
    sourceType: "quality_review_scrap",
    stockStatus: source.stockStatus,
    stockStatusFrom: source.stockStatus,
    stockStatusTo: source.stockStatus,
    uom: state.items.find((item) => item.id === source.inventoryItemId)?.uom ?? "ea"
  };
  return {
    ...state,
    balances: state.balances
      .map((balance) => balance.balanceKey === source.balanceKey ? { ...balance, quantity: balance.quantity - quantity, updatedAt: now } : balance)
      .filter((balance) => balance.quantity > 0),
    commandLog: [
      ...state.commandLog,
      createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: movement.idempotencyKey,
        payload: {
          balanceKey: source.balanceKey,
          previousQuantity: source.quantity,
          quantity,
          reasonCode,
          status: source.stockStatus
        },
        refId: source.balanceKey,
        refType: "stock_balance",
        requestedBy: "user",
        type: "PostStockMovement"
      })
    ],
    movements: [...state.movements, movement]
  };
}

function adjustBalanceQuantity(state: WarehouseState, source: StockBalance, adjustedQuantity: number, reasonCode: string): WarehouseState {
  const now = new Date().toISOString();
  const delta = adjustedQuantity - source.quantity;
  if (delta === 0) return state;
  const movementId = nextManualMovementId(state, "adjust");
  const movement: StockMovement = {
    companyId: source.companyId,
    externalId: movementId,
    id: movementId,
    idempotencyKey: `manual:adjust:${source.balanceKey}:${adjustedQuantity}:${Date.now()}`,
    inventoryItemId: source.inventoryItemId,
    inventoryOwnerPartyId: source.inventoryOwnerPartyId,
    locationId: source.locationId,
    lotId: source.lotId,
    movementType: "adjust",
    postedAt: now,
    quantity: Math.abs(delta),
    serialId: source.serialId,
    sourceId: reasonCode,
    sourceType: "manual_adjustment",
    stockStatus: source.stockStatus,
    stockStatusFrom: source.stockStatus,
    stockStatusTo: source.stockStatus,
    uom: state.items.find((item) => item.id === source.inventoryItemId)?.uom ?? "ea"
  };
  return {
    ...state,
    balances: state.balances
      .map((balance) => balance.balanceKey === source.balanceKey ? { ...balance, quantity: adjustedQuantity, updatedAt: now } : balance)
      .filter((balance) => balance.quantity > 0),
    commandLog: [
      ...state.commandLog,
      createWarehouseCommand({
        companyId: WAREHOUSE_COMPANY_ID,
        idempotencyKey: movement.idempotencyKey,
        payload: {
          adjustedQuantity,
          balanceKey: source.balanceKey,
          previousQuantity: source.quantity,
          reasonCode
        },
        refId: source.balanceKey,
        refType: "stock_balance",
        requestedBy: "user",
        type: "PostStockMovement"
      })
    ],
    movements: [...state.movements, movement]
  };
}

function uniqueSku(state: WarehouseState, base: string) {
  const existing = new Set(state.items.map((item) => item.sku.toLowerCase()));
  if (!existing.has(base.toLowerCase())) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`.toLowerCase())) suffix += 1;
  return `${base}-${suffix}`;
}

function isInventoryItemInactive(state: WarehouseState, inventoryItemId: string) {
  const statusCommand = [...state.commandLog]
    .reverse()
    .find((command) =>
      command.refType === "inventory_item" &&
      command.refId === inventoryItemId &&
      command.payload.status === "inactive"
    );
  return Boolean(statusCommand);
}

function assertLocationCapacity(
  state: WarehouseState,
  locationId: string,
  incomingQuantity: number,
  options: { excludeBalanceKey?: string; excludePutawayTaskId?: string } = {}
) {
  const location = state.locations.find((entry) => entry.id === locationId);
  const capacity = location?.capacityUnits;
  if (!capacity || capacity <= 0) return;
  const currentQuantity = state.balances
    .filter((balance) =>
      balance.locationId === locationId &&
      balance.quantity > 0 &&
      balance.balanceKey !== options.excludeBalanceKey &&
      balance.stockStatus !== "shipped"
    )
    .reduce((sum, balance) => sum + balance.quantity, 0);
  const plannedQuantity = state.putawayTasks
    .filter((task) => task.toLocationId === locationId && task.status === "open" && task.id !== options.excludePutawayTaskId)
    .reduce((sum, task) => sum + task.quantity, 0);
  if (currentQuantity + plannedQuantity + incomingQuantity > capacity) {
    throw new Error(`Target slot capacity exceeded (${currentQuantity + plannedQuantity + incomingQuantity}/${capacity}).`);
  }
}

function sectionCode(index: number) {
  return String.fromCharCode(65 + (index % 26));
}

function slugPart(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "warehouse";
}

function isWarehouseSlotType(value: unknown): value is NonNullable<WarehouseLocation["slotType"]> {
  return value === "standard" || value === "pick_face" || value === "bulk" || value === "staging" || value === "quarantine" || value === "returns";
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

function applySimulatorAction(state: WarehouseState, action: WarehouseMutationAction, reservationId?: string) {
  const reservation = reservationId
    ? state.reservations.find((item) => item.id === reservationId)
    : latestSimulatorReservation(state);
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
    locations: locations.map((row: any) => {
      const payload = parseJson(row.payloadJson);
      return {
        aisle: typeof payload.aisle === "string" ? payload.aisle : undefined,
        bay: typeof payload.bay === "string" ? payload.bay : undefined,
        capacityUnits: typeof payload.capacityUnits === "number" ? payload.capacityUnits : undefined,
        companyId: row.companyId,
        defaultOwnerPartyId: row.defaultOwnerPartyId ?? undefined,
        externalId: row.externalId,
        id: row.externalId,
        kind: row.kind as "warehouse" | "zone" | "bin",
        level: typeof payload.level === "string" ? payload.level : undefined,
        name: row.name,
        parentId: row.parentExternalId ?? undefined,
        pickable: row.pickable === 1,
        positionNote: typeof payload.positionNote === "string" ? payload.positionNote : undefined,
        receivable: row.receivable === 1,
        slotType: isWarehouseSlotType(payload.slotType) ? payload.slotType : undefined
      };
    }),
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
      payloadJson: JSON.stringify({
        aisle: location.aisle,
        bay: location.bay,
        capacityUnits: location.capacityUnits,
        level: location.level,
        positionNote: location.positionNote,
        slotType: location.slotType
      }),
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

  await tx.delete(stockBalances).where(eq(stockBalances.companyId, WAREHOUSE_COMPANY_ID));
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
