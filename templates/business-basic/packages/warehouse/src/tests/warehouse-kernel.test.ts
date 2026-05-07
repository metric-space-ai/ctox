import assert from "node:assert/strict";
import { test } from "node:test";
import {
  authorizeReturn,
  buildWarehouseDemo,
  cancelReservation,
  cancelShipment,
  closeCycleCount,
  createPickList,
  createBalanceKey,
  createEmptyWarehouseState,
  createFulfillmentLabel,
  createShipmentPackage,
  createSlottingRecommendation,
  createWarehouseTransfer,
  createWavePlan,
  createWarehouseCommand,
  getAvailableQuantity,
  getPickCandidates,
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
  releaseReservation,
  replayMovements,
  reserveStock,
  shipReservation,
  shipWarehouseTransfer,
  startScannerSession,
  SYSTEM_OWNER_PARTY_ID,
  WAREHOUSE_COMPANY_ID
} from "../index";

test("balance key uses sentinels for optional lot and serial dimensions", () => {
  const base = {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01",
    stockStatus: "available" as const
  };
  assert.equal(createBalanceKey(base), createBalanceKey({ ...base, lotId: null, serialId: null }));
  assert.notEqual(createBalanceKey(base), createBalanceKey({ ...base, lotId: "LOT-1", serialId: null }));
});

test("receipt command is idempotent and owner-aware", () => {
  let state = createEmptyWarehouseState();
  const command = createWarehouseCommand({
    companyId: WAREHOUSE_COMPANY_ID,
    payload: { test: true },
    refId: "receipt-test",
    refType: "receipt",
    requestedBy: "test",
    type: "ReceiveStock"
  });
  const receipt = {
    command,
    lines: [
      {
        companyId: WAREHOUSE_COMPANY_ID,
        id: "line-1",
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-receiving",
        quantity: 10
      },
      {
        companyId: WAREHOUSE_COMPANY_ID,
        id: "line-2",
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: "cust-nova",
        locationId: "loc-receiving",
        quantity: 5
      }
    ],
    receiptId: "receipt-test",
    sourceId: "po-test",
    sourceType: "purchase_order"
  };
  state = receiveStock(state, receipt);
  state = receiveStock(state, receipt);
  assert.equal(state.movements.length, 2);
  assert.equal(state.balances.length, 2);
  assert.equal(state.balances.find((balance) => balance.inventoryOwnerPartyId === SYSTEM_OWNER_PARTY_ID)?.quantity, 10);
  assert.equal(state.balances.find((balance) => balance.inventoryOwnerPartyId === "cust-nova")?.quantity, 5);
});

test("reservation lifecycle reserves and releases through commands", () => {
  let state = buildWarehouseDemo();
  const before = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  state = reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-release" },
      refId: "so-release",
      refType: "sales_order",
      requestedBy: "test",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: 3,
        sourceLineId: "so-release-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-release",
    sourceId: "so-release",
    sourceType: "sales_order"
  });
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), before - 3);
  state = releaseReservation(state, "res-release", "release:res-release");
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), before);
});

test("partial reservation reserves only currently available quantity", () => {
  let state = buildWarehouseDemo();
  const before = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  state = reserveStock(state, {
    allowPartialReservation: true,
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-partial" },
      refId: "so-partial",
      refType: "sales_order",
      requestedBy: "test",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: before + 5,
        sourceLineId: "so-partial-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-partial",
    sourceId: "so-partial",
    sourceType: "sales_order"
  });
  const reservation = state.reservations.find((item) => item.id === "res-partial");
  assert.equal(reservation?.status, "partially_reserved");
  assert.equal(reservation?.lines[0]?.quantity, before);
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), 0);
});

test("reservation cancellation releases unpicked stock and blocks pick", () => {
  let state = buildWarehouseDemo();
  state = reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-cancel" },
      refId: "so-cancel",
      refType: "sales_order",
      requestedBy: "test",
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
        sourceLineId: "so-cancel-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-cancel",
    sourceId: "so-cancel",
    sourceType: "sales_order"
  });
  state = cancelReservation(state, "res-cancel", "cancel:res-cancel");
  assert.equal(state.reservations.find((item) => item.id === "res-cancel")?.status, "cancelled");
  assert.throws(() => pickReservation(state, "res-cancel", "pick:res-cancel"), /reservation_not_pickable/);
});

test("pick list candidates validate pickable locations and reserved quantity", () => {
  let state = buildWarehouseDemo();
  state = reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-pick-ready" },
      refId: "so-pick-ready",
      refType: "sales_order",
      requestedBy: "test",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: 4,
        sourceLineId: "so-pick-ready-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-pick-ready",
    sourceId: "so-pick-ready",
    sourceType: "sales_order"
  });
  assert.equal(getPickCandidates(state, "res-pick-ready")[0]?.quantity, 4);
  state = createPickList(state, "res-pick-ready", "picklist:res-pick-ready");
  assert.equal(state.pickLists.find((item) => item.id === "pick-res-pick-ready")?.status, "ready");

  const blocked = structuredClone(state);
  const location = blocked.locations.find((item) => item.id === "loc-a-01");
  if (location) location.pickable = false;
  assert.throws(() => getPickCandidates(blocked, "res-pick-ready"), /location_not_pickable/);
});

test("serial reservations cannot double book the same serial", () => {
  const state = buildWarehouseDemo();
  assert.throws(() => reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-serial-conflict" },
      refId: "so-serial-conflict",
      refType: "sales_order",
      requestedBy: "test",
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
        sourceLineId: "so-serial-conflict-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-serial-conflict",
    sourceId: "so-serial-conflict",
    sourceType: "sales_order"
  }), /serial_already_reserved/);
});

test("over-pick and over-reserve are blocked by available quantity", () => {
  const state = buildWarehouseDemo();
  assert.throws(() => reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-too-large" },
      refId: "so-too-large",
      refType: "sales_order",
      requestedBy: "test",
      type: "ReserveStock"
    }),
    lines: [
      {
        allowBackorder: false,
        companyId: WAREHOUSE_COMPANY_ID,
        inventoryItemId: "item-core-kit",
        inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
        locationId: "loc-a-01",
        quantity: 1000,
        sourceLineId: "so-too-large-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-too-large",
    sourceId: "so-too-large",
    sourceType: "sales_order"
  }), /insufficient_available_quantity/);
});

test("return authorization and intake post resellable returned stock", () => {
  let state = buildWarehouseDemo();
  const before = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  state = authorizeReturn(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "wrong-size" },
      refId: "ret-test",
      refType: "return_authorization",
      requestedBy: "test",
      type: "AuthorizeReturn"
    }),
    lines: [
      {
        quantity: 2,
        resellable: true,
        shipmentLineId: "ship-res-7001-line-1"
      }
    ],
    returnId: "ret-test",
    shipmentId: "ship-res-7001"
  });
  state = receiveReturn(state, "ret-test", "return:ret-test");
  assert.equal(state.returns.find((item) => item.id === "ret-test")?.status, "received");
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), before + 2);
});

test("scan event ingestion is idempotent per scanner session", () => {
  let state = buildWarehouseDemo();
  state = startScannerSession(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { deviceId: "scan-test" },
      refId: "scan-test-session",
      refType: "scanner_session",
      requestedBy: "test",
      type: "StartScannerSession"
    }),
    deviceId: "scan-test",
    locationId: "loc-a-01",
    sessionId: "scan-test-session",
    userId: "test"
  });
  const scan = {
    action: "pick" as const,
    barcode: "CTOX-KIT",
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "scan-test-event-1",
    inventoryItemId: "item-core-kit",
    quantity: 1,
    sessionId: "scan-test-session"
  };
  state = ingestScanEvent(state, scan);
  state = ingestScanEvent(state, scan);
  assert.equal(state.scanEvents.filter((event) => event.id === "scan-test-event-1").length, 1);
  assert.equal(state.scannerSessions.find((session) => session.id === "scan-test-session")?.scanCount, 1);
});

test("cycle count closure posts adjustment movements instead of mutating balances directly", () => {
  let state = buildWarehouseDemo();
  const before = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  state = openCycleCount(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "test-count" },
      refId: "cycle-test",
      refType: "cycle_count",
      requestedBy: "test",
      type: "OpenCycleCount"
    }),
    countId: "cycle-test",
    inventoryItemIds: ["item-core-kit"],
    locationId: "loc-a-01"
  });
  const line = state.cycleCounts.find((count) => count.id === "cycle-test")?.lines[0];
  assert.ok(line);
  const movementsBeforeClose = state.movements.length;
  state = recordCycleCountLine(state, {
    countedQuantity: before - 2,
    countId: "cycle-test",
    idempotencyKey: "cycle-test-line-1",
    lineId: line.id
  });
  state = closeCycleCount(state, "cycle-test", "cycle-test-close");
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), before - 2);
  assert.equal(state.movements.length, movementsBeforeClose + 1);
  assert.equal(state.movements.at(-1)?.movementType, "adjust");
  assert.equal(state.inventoryAdjustments.at(-1)?.quantity, -2);
});

test("shipment cancellation restocks through compensating movements", () => {
  let state = buildWarehouseDemo();
  state = reserveStock(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { orderId: "so-cancel-ship" },
      refId: "so-cancel-ship",
      refType: "sales_order",
      requestedBy: "test",
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
        sourceLineId: "so-cancel-ship-line-1",
        stockStatus: "available"
      }
    ],
    reservationId: "res-cancel-ship",
    sourceId: "so-cancel-ship",
    sourceType: "sales_order"
  });
  state = pickReservation(state, "res-cancel-ship", "pick:res-cancel-ship");
  state = shipReservation(state, "res-cancel-ship", "ship:res-cancel-ship");
  const beforeCancel = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  const movementsBeforeCancel = state.movements.length;
  state = cancelShipment(state, "ship-res-cancel-ship", "cancel-ship:res-cancel-ship");
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), beforeCancel + 2);
  assert.equal(state.movements.length, movementsBeforeCancel + 2);
  assert.equal(state.shipments.find((shipment) => shipment.id === "ship-res-cancel-ship")?.status, "cancelled");
});

test("package labels and tracking events are idempotent", () => {
  let state = buildWarehouseDemo();
  state = createShipmentPackage(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { carrier: "DHL" },
      refId: "pkg-test",
      refType: "shipment_package",
      requestedBy: "test",
      type: "CreateShipmentPackage"
    }),
    packageId: "pkg-test",
    shipmentId: "ship-res-7001"
  });
  state = createFulfillmentLabel(state, {
    carrier: "DHL",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { provider: "demo-carrier" },
      refId: "label-test",
      refType: "fulfillment_label",
      requestedBy: "test",
      type: "CreateFulfillmentLabel"
    }),
    labelId: "label-test",
    packageId: "pkg-test",
    provider: "demo-carrier",
    trackingNumber: "TRACK-LABEL-TEST"
  });
  const tracking = {
    carrier: "DHL",
    companyId: WAREHOUSE_COMPANY_ID,
    eventCode: "in_transit",
    eventId: "track-test-1",
    shipmentId: "ship-res-7001",
    trackingNumber: "TRACK-LABEL-TEST"
  };
  state = recordShipmentTrackingEvent(state, tracking);
  state = recordShipmentTrackingEvent(state, tracking);
  assert.equal(state.fulfillmentLabels.find((label) => label.id === "label-test")?.trackingNumber, "TRACK-LABEL-TEST");
  assert.equal(state.shipmentTrackingEvents.filter((event) => event.id === "track-test-1").length, 1);
});

test("WES and robotics events are idempotent integration envelopes", () => {
  let state = buildWarehouseDemo();
  const wes = {
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "wes-test-1",
    eventType: "pick_task_acknowledged",
    payload: { pickListId: "pick-res-7001" },
    provider: "demo-wes",
    source: "wes" as const
  };
  state = ingestIntegrationEvent(state, wes);
  state = ingestIntegrationEvent(state, wes);
  const robot = {
    companyId: WAREHOUSE_COMPANY_ID,
    eventId: "robot-test-1",
    eventType: "tote_arrived",
    payload: { toteId: "tote-test" },
    robotId: "amr-test"
  };
  state = ingestRoboticsEvent(state, robot);
  state = ingestRoboticsEvent(state, robot);
  assert.equal(state.integrationEvents.filter((event) => event.id === "wes-test-1").length, 1);
  assert.equal(state.roboticsEvents.filter((event) => event.id === "robot-test-1").length, 1);
});

test("wave planning and slotting recommendations capture advanced pick planning", () => {
  let state = buildWarehouseDemo();
  state = createWavePlan(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { strategy: "test-wave" },
      refId: "wave-test",
      refType: "wave_plan",
      requestedBy: "test",
      type: "CreateWavePlan"
    }),
    priority: "expedite",
    reservationIds: ["res-7002"],
    waveId: "wave-test"
  });
  state = createSlottingRecommendation(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { score: 0.91 },
      refId: "slot-test",
      refType: "slotting_recommendation",
      requestedBy: "test",
      type: "CreateSlottingRecommendation"
    }),
    fromLocationId: "loc-receiving",
    inventoryItemId: "item-core-kit",
    reason: "Fast mover",
    recommendationId: "slot-test",
    toLocationId: "loc-a-01"
  });
  assert.equal(state.wavePlans.find((wave) => wave.id === "wave-test")?.lines[0]?.reservationId, "res-7002");
  assert.equal(state.slottingRecommendations.find((item) => item.id === "slot-test")?.status, "recommended");
});

test("multi-node transfers use in-transit and receiving movement rows", () => {
  let state = buildWarehouseDemo();
  const beforeFrom = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  });
  const beforeTo = getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-receiving"
  });
  state = createWarehouseTransfer(state, {
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { reason: "test-transfer" },
      refId: "transfer-test",
      refType: "warehouse_transfer",
      requestedBy: "test",
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
    transferId: "transfer-test"
  });
  state = shipWarehouseTransfer(state, "transfer-test", "transfer-test-ship");
  state = receiveWarehouseTransfer(state, "transfer-test", "transfer-test-receive");
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-a-01"
  }), beforeFrom - 2);
  assert.equal(getAvailableQuantity(state, {
    companyId: WAREHOUSE_COMPANY_ID,
    inventoryItemId: "item-core-kit",
    inventoryOwnerPartyId: SYSTEM_OWNER_PARTY_ID,
    locationId: "loc-receiving"
  }), beforeTo + 2);
  assert.equal(state.transfers.find((transfer) => transfer.id === "transfer-test")?.status, "received");
  assert.equal(state.movements.at(-1)?.movementType, "transfer_receive");
});

test("offline sync and 3PL billing are idempotent extension records", () => {
  let state = buildWarehouseDemo();
  const batch = {
    batchId: "offline-test",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { deviceId: "scan-test" },
      refId: "offline-test",
      refType: "offline_sync_batch",
      requestedBy: "test",
      type: "RecordOfflineSyncBatch"
    }),
    deviceId: "scan-test",
    events: [
      {
        action: "scan",
        externalId: "offline-test-event",
        id: "offline-test-event",
        idempotencyKey: "offline-test-event-key",
        payload: { barcode: "CTOX-KIT" }
      },
      {
        action: "scan",
        externalId: "offline-test-event-duplicate",
        id: "offline-test-event-duplicate",
        idempotencyKey: "offline-test-event-key",
        payload: { barcode: "CTOX-KIT" }
      }
    ]
  };
  state = recordOfflineSyncBatch(state, batch);
  state = recordOfflineSyncBatch(state, batch);
  state = recordThreePlCharge(state, {
    amountCents: 175,
    chargeId: "charge-test",
    command: createWarehouseCommand({
      companyId: WAREHOUSE_COMPANY_ID,
      payload: { tariff: "pick" },
      refId: "charge-test",
      refType: "three_pl_charge",
      requestedBy: "test",
      type: "RecordThreePlCharge"
    }),
    currency: "EUR",
    inventoryOwnerPartyId: "cust-nova",
    metric: "pick",
    quantity: 1,
    sourceId: "pick-res-7001",
    sourceType: "pick_list"
  });
  assert.equal(state.offlineSyncBatches.find((item) => item.id === "offline-test")?.events.length, 1);
  assert.equal(state.threePlCharges.find((item) => item.id === "charge-test")?.amountCents, 175);
});

test("movement replay recreates ledger balances", () => {
  const state = buildWarehouseDemo();
  const replayed = replayMovements(state);
  const actual = new Map(state.balances.map((balance) => [balance.balanceKey, balance.quantity]));
  for (const balance of replayed) {
    assert.equal(balance.quantity, actual.get(balance.balanceKey));
  }
});
