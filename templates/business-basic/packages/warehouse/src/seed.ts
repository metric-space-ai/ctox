import {
  authorizeReturn,
  closeCycleCount,
  completePutaway,
  createFulfillmentLabel,
  createEmptyWarehouseState,
  createShipmentPackage,
  createSlottingRecommendation,
  createPutawayTasks,
  createWarehouseTransfer,
  createWarehouseCommand,
  createWavePlan,
  ingestIntegrationEvent,
  ingestRoboticsEvent,
  ingestScanEvent,
  openCycleCount,
  pickReservation,
  receiveStock,
  receiveReturn,
  recordCycleCountLine,
  recordOfflineSyncBatch,
  recordShipmentTrackingEvent,
  recordThreePlCharge,
  receiveWarehouseTransfer,
  reserveStock,
  shipReservation,
  shipWarehouseTransfer,
  startScannerSession
} from "./kernel";
import type { WarehouseState } from "./types";

export const WAREHOUSE_COMPANY_ID = "business-basic-company";
export const SYSTEM_OWNER_PARTY_ID = "owner-system";
export const CUSTOMER_OWNER_PARTY_ID = "cust-nova";

export function buildWarehouseDemo(): WarehouseState {
  let state = createEmptyWarehouseState();
  state = {
    ...state,
    items: [
      {
        companyId: WAREHOUSE_COMPANY_ID,
        externalId: "sku-core-kit",
        id: "item-core-kit",
        name: "CTOX Core Kit",
        sku: "CTOX-KIT",
        trackingMode: "none",
        uom: "ea"
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        externalId: "sku-sensor-pack",
        id: "item-sensor-pack",
        name: "Sensor Pack",
        sku: "SNS-PACK",
        trackingMode: "lot",
        uom: "ea"
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        externalId: "sku-gateway",
        id: "item-gateway",
        name: "Edge Gateway",
        sku: "EDGE-GW",
        trackingMode: "serial",
        uom: "ea"
      }
    ],
    locations: [
      {
        companyId: WAREHOUSE_COMPANY_ID,
        defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        externalId: "wh-berlin",
        id: "loc-berlin",
        kind: "warehouse",
        name: "Berlin DC",
        pickable: false,
        receivable: true
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        externalId: "wh-hamburg",
        id: "loc-hamburg",
        kind: "warehouse",
        name: "Hamburg Store",
        pickable: false,
        receivable: true
      },
      ...warehouseSection("A", "Electronics", 12),
      ...warehouseSection("B", "Appliances", 12),
      ...warehouseSection("C", "Home Decor", 12),
      ...warehouseSection("D", "Sports", 12),
      {
        companyId: WAREHOUSE_COMPANY_ID,
        defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        externalId: "dock-inbound",
        id: "loc-receiving",
        kind: "zone",
        name: "Inbound Dock",
        parentId: "loc-berlin",
        pickable: false,
        receivable: true
      }
    ],
    nodes: [
      {
        companyId: WAREHOUSE_COMPANY_ID,
        externalId: "node-berlin",
        id: "node-berlin",
        kind: "warehouse",
        name: "Berlin DC",
        status: "active"
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        externalId: "node-3pl-nova",
        id: "node-3pl-nova",
        kind: "third_party_logistics",
        name: "Nova 3PL",
        status: "active"
      }
    ],
    policies: [
      {
        allowBackorder: false,
        allowNegativeStock: false,
        allocationStrategy: "fefo",
        companyId: WAREHOUSE_COMPANY_ID,
        defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        id: "policy-default"
      }
    ]
  };

  state = receiveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { source: "purchase_receipt" },
      refId: "receipt-1001",
      refType: "receipt",
      requestedBy: "warehouse-agent",
      type: "ReceiveStock"
    }),
    lines: [
      {
        companyId: WAREHOUSE_COMPANY_ID,
        id: "receipt-1001-line-1",
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-receiving",
        quantity: 80
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        id: "receipt-1001-line-2",
        inventoryItemId: "item-sensor-pack",
        inventoryOwnerPartyId: CUSTOMER_OWNER_PARTY_ID,
        locationId: "loc-receiving",
        lotId: "LOT-2026-05",
        quantity: 48
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        id: "receipt-1001-line-3",
        inventoryItemId: "item-gateway",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-receiving",
        quantity: 1,
        serialId: "GW-0001"
      }
    ],
    receiptId: "receipt-1001",
    sourceId: "po-9001",
    sourceType: "purchase_order"
  });
  state = createPutawayTasks(state, "receipt-1001", "loc-a-01");
  for (const task of state.putawayTasks) {
    state = completePutaway(state, task.id, `putaway:${task.id}`);
  }

  state = reserveStock(state, {
    allowPartialReservation: false,
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-7001" },
      refId: "so-7001",
      refType: "sales_order",
      requestedBy: "sales-agent",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: 12,
        sourceLineId: "so-7001-line-1",
        stockStatus: "available"
      },
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-sensor-pack",
        inventoryOwnerPartyId: CUSTOMER_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        lotId: "LOT-2026-05",
        quantity: 8,
        sourceLineId: "so-7001-line-2",
        stockStatus: "available"
      }
    ],
    reservationId: "res-7001",
    sourceId: "so-7001",
    sourceType: "sales_order"
  });
  state = pickReservation(state, "res-7001", "pick:res-7001");
  state = shipReservation(state, "res-7001", "ship:res-7001");
  state = authorizeReturn(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "demo-return" },
      refId: "ret-7001",
      refType: "return_authorization",
      requestedBy: "support-agent",
      type: "AuthorizeReturn"
    }),
    lines: [
      {
        quantity: 1,
        resellable: true,
        shipmentLineId: "ship-res-7001-line-1"
      }
    ],
    returnId: "ret-7001",
    shipmentId: "ship-res-7001"
  });
  state = receiveReturn(state, "ret-7001", "return:ret-7001");
  state = createShipmentPackage(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { source: "demo-package" },
      refId: "pkg-7001",
      refType: "shipment_package",
      requestedBy: "fulfillment-agent",
      type: "CreateShipmentPackage"
    }),
    packageId: "pkg-7001",
    shipmentId: "ship-res-7001"
  });
  state = createFulfillmentLabel(state, {
    carrier: "DHL",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { source: "demo-label" },
      refId: "label-7001",
      refType: "fulfillment_label",
      requestedBy: "fulfillment-agent",
      type: "CreateFulfillmentLabel"
    }),
    labelId: "label-7001",
    packageId: "pkg-7001",
    provider: "demo-carrier",
    trackingNumber: "TRACK-RES-7001"
  });
  state = recordShipmentTrackingEvent(state, {
    carrier: "DHL",
    companyId: WAREHOUSE_COMPANY_ID,
    eventCode: "picked_up",
    eventId: "track-7001-picked-up",
    shipmentId: "ship-res-7001",
    trackingNumber: "TRACK-RES-7001"
  });

  state = reserveStock(state, {
    allowPartialReservation: false,
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-7002" },
      refId: "so-7002",
      refType: "sales_order",
      requestedBy: "sales-agent",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-gateway",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: 1,
        serialId: "GW-0001",
        sourceLineId: "so-7002-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-7002",
    sourceId: "so-7002",
    sourceType: "sales_order"
  });

  state = startScannerSession(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { deviceId: "scan-gun-01" },
      refId: "scan-session-01",
      refType: "scanner_session",
      requestedBy: "warehouse-agent",
      type: "StartScannerSession"
    }),
    deviceId: "scan-gun-01",
    locationId: "loc-a-01",
    sessionId: "scan-session-01",
    userId: "warehouse-agent"
  });
  state = ingestScanEvent(state, {
    action: "count",
    barcode: "CTOX-KIT",
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "scan-event-01",
    inventoryItemId: "item-core-kit",
    quantity: 1,
    sessionId: "scan-session-01"
  });
  state = openCycleCount(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "demo-cycle-count" },
      refId: "cycle-7001",
      refType: "cycle_count",
      requestedBy: "warehouse-agent",
      type: "OpenCycleCount"
    }),
    countId: "cycle-7001",
    inventoryItemIds: ["item-core-kit"],
    locationId: "loc-a-01"
  });
  const coreLine = state.cycleCounts.find((count) => count.id === "cycle-7001")?.lines[0];
  if (coreLine) {
    state = recordCycleCountLine(state, {
      countedQuantity: coreLine.expectedQuantity - 1,
      countId: "cycle-7001",
      idempotencyKey: "cycle-line:cycle-7001:1",
      lineId: coreLine.id
    });
    state = closeCycleCount(state, "cycle-7001", "cycle-close:cycle-7001");
  }
  state = ingestIntegrationEvent(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "wes-evt-001",
    eventType: "pick_task_acknowledged",
    payload: { pickListId: "pick-res-7001" },
    provider: "demo-wes",
    source: "wes"
  });
  state = ingestRoboticsEvent(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "robot-evt-001",
    eventType: "tote_arrived",
    payload: { toteId: "tote-001" },
    robotId: "amr-01"
  });
  state = createWavePlan(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { strategy: "demo-wave" },
      refId: "wave-7001",
      refType: "wave_plan",
      requestedBy: "warehouse-agent",
      type: "CreateWavePlan"
    }),
    priority: "normal",
    reservationIds: ["res-7002"],
    waveId: "wave-7001"
  });
  state = createSlottingRecommendation(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "fast-mover" },
      refId: "slot-7001",
      refType: "slotting_recommendation",
      requestedBy: "warehouse-agent",
      type: "CreateSlottingRecommendation"
    }),
    fromLocationId: "loc-receiving",
    inventoryItemId: "item-core-kit",
    reason: "Move fast-moving kits closer to pick face",
    recommendationId: "slot-7001",
    toLocationId: "loc-a-01"
  });
  state = createWarehouseTransfer(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "3pl-replenishment" },
      refId: "transfer-7001",
      refType: "warehouse_transfer",
      requestedBy: "warehouse-agent",
      type: "CreateWarehouseTransfer"
    }),
    fromLocationId: "loc-a-01",
    fromNodeId: "node-berlin",
    lines: [
      {
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        quantity: 2
      }
    ],
    toLocationId: "loc-receiving",
    toNodeId: "node-3pl-nova",
    transferId: "transfer-7001"
  });
  state = shipWarehouseTransfer(state, "transfer-7001", "transfer-ship:transfer-7001");
  state = receiveWarehouseTransfer(state, "transfer-7001", "transfer-receive:transfer-7001");
  state = recordOfflineSyncBatch(state, {
    batchId: "offline-batch-7001",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { deviceId: "scan-gun-01" },
      refId: "offline-batch-7001",
      refType: "offline_sync_batch",
      requestedBy: "sync-agent",
      type: "RecordOfflineSyncBatch"
    }),
    deviceId: "scan-gun-01",
    events: [
      {
        action: "scan",
        externalId: "offline-event-001",
        id: "offline-event-001",
        idempotencyKey: "offline:scan-gun-01:001",
        payload: { barcode: "CTOX-KIT" }
      }
    ]
  });
  state = recordThreePlCharge(state, {
    amountCents: 250,
    chargeId: "3pl-charge-7001",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { tariff: "demo-pick" },
      refId: "3pl-charge-7001",
      refType: "three_pl_charge",
      requestedBy: "billing-agent",
      type: "RecordThreePlCharge"
    }),
    currency: "EUR",
    inventoryOwnerPartyId: CUSTOMER_OWNER_PARTY_ID,
    metric: "pick",
    quantity: 1,
    sourceId: "pick-res-7001",
    sourceType: "pick_list"
  });

  return state;
}

function warehouseSection(code: string, name: string, slotCount: number): WarehouseState["locations"] {
  const lower = code.toLowerCase();
  return [
    {
      companyId: WAREHOUSE_COMPANY_ID,
      defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
      externalId: `section-${lower}`,
      id: `loc-zone-${lower}`,
      kind: "zone",
      name: `${code}-${name}`,
      parentId: "loc-berlin",
      pickable: false,
      receivable: false
    },
    ...Array.from({ length: slotCount }, (_, index) => {
      const slotNumber = index + 1;
      const padded = String(slotNumber).padStart(2, "0");
      return {
        companyId: WAREHOUSE_COMPANY_ID,
        defaultOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        externalId: `bin-${lower}-${padded}`,
        id: `loc-${lower}-${padded}`,
        kind: "bin" as const,
        name: `${code}${slotNumber}`,
        parentId: `loc-zone-${lower}`,
        pickable: true,
        receivable: false
      };
    })
  ];
}

export function summarizeWarehouse(state = buildWarehouseDemo()) {
  const available = state.balances
    .filter((balance) => balance.stockStatus === "available")
    .reduce((sum, balance) => sum + balance.quantity, 0);
  const reserved = state.balances
    .filter((balance) => balance.stockStatus === "reserved")
    .reduce((sum, balance) => sum + balance.quantity, 0);
  const shipped = state.balances
    .filter((balance) => balance.stockStatus === "shipped")
    .reduce((sum, balance) => sum + balance.quantity, 0);
  return {
    available,
    balanceRows: state.balances.length,
    movementRows: state.movements.length,
    outboxPending: state.outbox.filter((event) => event.status === "pending").length,
    scanEvents: state.scanEvents.length,
    cycleCounts: state.cycleCounts.length,
    inventoryAdjustments: state.inventoryAdjustments.length,
    integrationEvents: state.integrationEvents.length,
    offlineSyncBatches: state.offlineSyncBatches.length,
    roboticsEvents: state.roboticsEvents.length,
    shipmentTrackingEvents: state.shipmentTrackingEvents.length,
    threePlCharges: state.threePlCharges.length,
    transfers: state.transfers.length,
    wavePlans: state.wavePlans.length,
    reserved,
    shipments: state.shipments.length,
    shipped
  };
}
