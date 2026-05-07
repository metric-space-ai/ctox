import type {
  BalanceDimension,
  CycleCount,
  FulfillmentLabel,
  InventoryAdjustment,
  IntegrationEvent,
  OfflineSyncBatch,
  OfflineSyncEvent,
  PickList,
  PickListLine,
  PutawayTask,
  Receipt,
  ReceiptLine,
  ReturnAuthorization,
  RoboticsEvent,
  ScanEvent,
  ScannerSession,
  Shipment,
  ShipmentPackage,
  ShipmentTrackingEvent,
  SlottingRecommendation,
  StockBalance,
  StockMovement,
  StockReservation,
  StockReservationLine,
  StockStatus,
  ThreePlCharge,
  WarehouseCommand,
  WarehouseCommandType,
  WarehouseOutboxEvent,
  WarehouseState,
  WarehouseTransfer,
  WarehouseTransferLine,
  WavePlan
} from "./types";

const NULL_LOT = "_no_lot";
const NULL_SERIAL = "_no_serial";
const SYSTEM_OWNER = "owner-system";

export function createWarehouseCommand<TPayload extends Record<string, unknown>>(
  input: Omit<WarehouseCommand<TPayload>, "idempotencyKey" | "requestedAt"> & {
    idempotencyKey?: string;
    requestedAt?: string;
  }
): WarehouseCommand<TPayload> {
  return {
    ...input,
    idempotencyKey: input.idempotencyKey ?? `${input.companyId}:${input.type}:${input.refType}:${input.refId}`,
    requestedAt: input.requestedAt ?? new Date().toISOString()
  };
}

export function createBalanceKey(input: BalanceDimension) {
  return [
    input.companyId,
    input.inventoryOwnerPartyId || SYSTEM_OWNER,
    input.inventoryItemId,
    input.locationId,
    input.stockStatus,
    input.lotId || NULL_LOT,
    input.serialId || NULL_SERIAL
  ].join("|");
}

export function cloneWarehouseState(state: WarehouseState): WarehouseState {
  return structuredClone(state) as WarehouseState;
}

export function createEmptyWarehouseState(): WarehouseState {
  return {
    balances: [],
    commandLog: [],
    cycleCounts: [],
    fulfillmentLabels: [],
    inventoryAdjustments: [],
    integrationEvents: [],
    items: [],
    locations: [],
    movements: [],
    nodes: [],
    offlineSyncBatches: [],
    outbox: [],
    pickLists: [],
    policies: [],
    putawayTasks: [],
    receipts: [],
    reservations: [],
    returns: [],
    scanEvents: [],
    scannerSessions: [],
    shipments: [],
    shipmentPackages: [],
    shipmentTrackingEvents: [],
    roboticsEvents: [],
    slottingRecommendations: [],
    threePlCharges: [],
    transfers: [],
    wavePlans: []
  };
}

export function postStockMovement(
  state: WarehouseState,
  input: Omit<StockMovement, "balanceKey" | "id" | "externalId">
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.idempotencyKey)) return next;

  const movement: StockMovement = {
    ...input,
    externalId: `mov-${input.idempotencyKey}`,
    id: `mov-${next.movements.length + 1}`
  };

  applyBalanceDelta(next, movement, movement.stockStatus, movement.quantity);
  next.movements.push(movement);
  appendCommand(next, input.companyId, "PostStockMovement", input.idempotencyKey, input.sourceType, input.sourceId, movement);
  appendOutbox(next, input.companyId, "warehouse.stock_moved", {
    movementId: movement.id,
    movementType: movement.movementType,
    quantity: movement.quantity,
    sourceId: movement.sourceId,
    sourceType: movement.sourceType
  });
  return next;
}

export function receiveStock(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    lines: ReceiptLine[];
    receiptId: string;
    sourceId: string;
    sourceType: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;

  const receipt: Receipt = {
    companyId: input.command.companyId,
    externalId: input.receiptId,
    id: input.receiptId,
    inventoryOwnerPartyId: input.lines[0]?.inventoryOwnerPartyId ?? SYSTEM_OWNER,
    lines: input.lines,
    sourceId: input.sourceId,
    sourceType: input.sourceType,
    status: "received",
    version: 1
  };
  next.receipts.push(receipt);

  for (const line of input.lines) {
    const movement = {
      companyId: input.command.companyId,
      externalId: `mov-${input.command.idempotencyKey}-${line.id}`,
      id: `mov-${next.movements.length + 1}`,
      idempotencyKey: `${input.command.idempotencyKey}:${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      movementType: "receive",
      postedAt: input.command.requestedAt,
      quantity: line.quantity,
      serialId: line.serialId,
      sourceId: receipt.id,
      sourceLineId: line.id,
      sourceType: "receipt",
      stockStatus: "receiving",
      stockStatusTo: "receiving",
      uom: "ea"
    } satisfies StockMovement;
    applyBalanceDelta(next, movement, "receiving", line.quantity);
    next.movements.push(movement);
  }

  appendCommand(next, input.command.companyId, input.command.type, input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.receipt_received", { receiptId: receipt.id, lineCount: receipt.lines.length });
  return next;
}

export function createPutawayTasks(state: WarehouseState, receiptId: string, toLocationId: string) {
  const next = cloneWarehouseState(state);
  const receipt = next.receipts.find((item) => item.id === receiptId);
  if (!receipt) throw new Error("receipt_not_found");
  receipt.status = "putaway_started";
  receipt.version += 1;

  for (const line of receipt.lines) {
    const task: PutawayTask = {
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
    };
    next.putawayTasks.push(task);
  }
  return next;
}

export function completePutaway(state: WarehouseState, taskId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const task = next.putawayTasks.find((item) => item.id === taskId);
  if (!task) throw new Error("putaway_task_not_found");
  if (task.status !== "open") return next;

  moveStatus(next, {
    companyId: task.companyId,
    idempotencyKey,
    inventoryItemId: task.inventoryItemId,
    inventoryOwnerPartyId: task.inventoryOwnerPartyId,
    locationId: task.fromLocationId,
    lotId: task.lotId,
    movementType: "putaway",
    postedAt: new Date().toISOString(),
    quantity: task.quantity,
    serialId: task.serialId,
    sourceId: task.id,
    sourceLineId: task.receiptLineId,
    sourceType: "putaway_task",
    uom: "ea"
  }, "receiving", "available", task.toLocationId);

  task.status = "done";
  task.version += 1;
  appendCommand(next, task.companyId, "CompletePutaway", idempotencyKey, "putaway_task", task.id, { taskId });
  appendOutbox(next, task.companyId, "warehouse.putaway_completed", { taskId: task.id });
  return next;
}

export function reserveStock(
  state: WarehouseState,
  input: {
    allowBackorder?: boolean;
    allowPartialReservation?: boolean;
    command: WarehouseCommand;
    lines: Array<Omit<StockReservationLine, "id" | "pickedQuantity" | "releasedQuantity" | "shippedQuantity">>;
    reservationId: string;
    sourceId: string;
    sourceType: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;

  const reservationLines: StockReservationLine[] = [];
  let totalRequested = 0;
  let totalReserved = 0;

  input.lines.forEach((line, index) => {
    totalRequested += line.quantity;
    if (line.serialId && hasReservedSerial(next, line.companyId, line.inventoryOwnerPartyId, line.inventoryItemId, line.serialId)) {
      throw new Error("serial_already_reserved");
    }
    const available = getAvailableQuantity(next, line);
    const allowedQuantity = Math.min(line.quantity, available);
    if (allowedQuantity < line.quantity && !input.allowPartialReservation && !line.allowBackorder) {
      throw new Error("insufficient_available_quantity");
    }
    const reservedQuantity = line.allowBackorder ? line.quantity : allowedQuantity;
    if (reservedQuantity <= 0) return;
    totalReserved += reservedQuantity;
    moveStatus(next, {
      companyId: line.companyId,
      idempotencyKey: `${input.command.idempotencyKey}:line-${index}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      movementType: "reserve",
      postedAt: input.command.requestedAt,
      quantity: reservedQuantity,
      serialId: line.serialId,
      sourceId: input.reservationId,
      sourceLineId: line.sourceLineId,
      sourceType: "stock_reservation",
      uom: "ea"
    }, "available", "reserved");
    reservationLines.push({
      ...line,
      id: `${input.reservationId}-line-${index + 1}`,
      pickedQuantity: 0,
      quantity: reservedQuantity,
      releasedQuantity: 0,
      shippedQuantity: 0
    });
  });

  const reservation: StockReservation = {
    allowPartialReservation: input.allowPartialReservation ?? false,
    companyId: input.command.companyId,
    externalId: input.reservationId,
    id: input.reservationId,
    inventoryOwnerPartyId: reservationLines[0]?.inventoryOwnerPartyId ?? SYSTEM_OWNER,
    lines: reservationLines,
    sourceId: input.sourceId,
    sourceType: input.sourceType,
    status: totalReserved === totalRequested ? "reserved" : "partially_reserved",
    version: 1
  };
  next.reservations.push(reservation);
  appendCommand(next, input.command.companyId, input.command.type, input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.stock_reserved", { reservationId: reservation.id, status: reservation.status });
  return next;
}

export function releaseReservation(state: WarehouseState, reservationId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");

  for (const line of reservation.lines) {
    const releasable = line.quantity - line.pickedQuantity - line.shippedQuantity - line.releasedQuantity;
    if (releasable <= 0) continue;
    moveStatus(next, movementFromReservationLine(reservation, line, idempotencyKey, "release", releasable), "reserved", "available");
    line.releasedQuantity += releasable;
  }
  reservation.status = "released";
  reservation.version += 1;
  appendCommand(next, reservation.companyId, "ReleaseReservation", idempotencyKey, "stock_reservation", reservation.id, { reservationId });
  appendOutbox(next, reservation.companyId, "warehouse.reservation_released", { reservationId });
  return next;
}

export function cancelReservation(state: WarehouseState, reservationId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");
  if (reservation.status === "consumed") throw new Error("reservation_already_consumed");
  if (reservation.lines.some((line) => line.pickedQuantity > line.shippedQuantity)) {
    throw new Error("reservation_already_picked");
  }

  for (const line of reservation.lines) {
    const releasable = line.quantity - line.pickedQuantity - line.shippedQuantity - line.releasedQuantity;
    if (releasable <= 0) continue;
    moveStatus(next, movementFromReservationLine(reservation, line, idempotencyKey, "release", releasable), "reserved", "available");
    line.releasedQuantity += releasable;
  }
  reservation.status = "cancelled";
  reservation.version += 1;
  appendCommand(next, reservation.companyId, "CancelReservation", idempotencyKey, "stock_reservation", reservation.id, { reservationId });
  appendOutbox(next, reservation.companyId, "warehouse.reservation_cancelled", { reservationId });
  return next;
}

export function getPickCandidates(state: WarehouseState, reservationId: string) {
  const reservation = state.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");
  return collectPickCandidates(state, reservation);
}

export function createPickList(state: WarehouseState, reservationId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");
  const candidates = collectPickCandidates(next, reservation);
  if (candidates.length === 0) throw new Error("no_pick_candidates");
  const pickList: PickList = {
    companyId: reservation.companyId,
    externalId: `pick-${reservation.id}`,
    id: `pick-${reservation.id}`,
    inventoryOwnerPartyId: reservation.inventoryOwnerPartyId,
    lines: candidates,
    reservationId: reservation.id,
    status: "ready",
    version: 1
  };
  const existingPickList = next.pickLists.find((item) => item.id === pickList.id);
  if (existingPickList) {
    existingPickList.lines = candidates;
    existingPickList.status = "ready";
    existingPickList.version += 1;
  } else {
    next.pickLists.push(pickList);
  }
  appendCommand(next, reservation.companyId, "CreatePickList", idempotencyKey, "stock_reservation", reservation.id, { reservationId });
  appendOutbox(next, reservation.companyId, "warehouse.picklist_ready", { pickListId: pickList.id, reservationId });
  return next;
}

export function pickReservation(state: WarehouseState, reservationId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");
  const candidates = collectPickCandidates(next, reservation);
  if (candidates.length === 0) throw new Error("no_pick_candidates");

  const pickList = next.pickLists.find((item) => item.id === `pick-${reservation.id}`) ?? {
    companyId: reservation.companyId,
    externalId: `pick-${reservation.id}`,
    id: `pick-${reservation.id}`,
    inventoryOwnerPartyId: reservation.inventoryOwnerPartyId,
    lines: [],
    reservationId: reservation.id,
    status: "picked",
    version: 1
  };

  pickList.lines = [];
  for (const candidate of candidates) {
    const line = reservation.lines.find((item) => item.id === candidate.reservationLineId);
    if (!line) throw new Error("reservation_line_not_found");
    const pickable = candidate.quantity;
    moveStatus(next, movementFromReservationLine(reservation, line, idempotencyKey, "pick", pickable), "reserved", "picked");
    line.pickedQuantity += pickable;
    pickList.lines.push({
      id: `${pickList.id}-line-${pickList.lines.length + 1}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      pickedQuantity: pickable,
      quantity: pickable,
      reservationLineId: line.id,
      serialId: line.serialId
    });
  }
  pickList.status = "picked";
  if (!next.pickLists.some((item) => item.id === pickList.id)) next.pickLists.push(pickList);
  reservation.status = "partially_consumed";
  reservation.version += 1;
  appendCommand(next, reservation.companyId, "PickReservation", idempotencyKey, "stock_reservation", reservation.id, { reservationId });
  appendOutbox(next, reservation.companyId, "warehouse.picklist_picked", { pickListId: pickList.id, reservationId });
  return next;
}

export function shipReservation(state: WarehouseState, reservationId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const reservation = next.reservations.find((item) => item.id === reservationId);
  if (!reservation) throw new Error("reservation_not_found");

  const shipment: Shipment = {
    carrier: "DHL",
    companyId: reservation.companyId,
    externalId: `ship-${reservation.id}`,
    id: `ship-${reservation.id}`,
    inventoryOwnerPartyId: reservation.inventoryOwnerPartyId,
    lines: [],
    provider: "demo-carrier",
    reservationId: reservation.id,
    status: "shipped",
    trackingNumber: `TRACK-${reservation.id.toUpperCase()}`,
    version: 1
  };

  for (const line of reservation.lines) {
    const shippable = line.pickedQuantity - line.shippedQuantity;
    if (shippable <= 0) continue;
    moveStatus(next, movementFromReservationLine(reservation, line, idempotencyKey, "ship", shippable), "picked", "shipped");
    line.shippedQuantity += shippable;
    shipment.lines.push({
      id: `${shipment.id}-line-${shipment.lines.length + 1}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      quantity: shippable,
      reservationLineId: line.id,
      serialId: line.serialId
    });
  }
  next.shipments.push(shipment);
  reservation.status = "consumed";
  reservation.version += 1;
  appendCommand(next, reservation.companyId, "ShipReservation", idempotencyKey, "stock_reservation", reservation.id, { reservationId });
  appendOutbox(next, reservation.companyId, "warehouse.shipment_shipped", { reservationId, shipmentId: shipment.id, trackingNumber: shipment.trackingNumber });
  return next;
}

export function cancelShipment(state: WarehouseState, shipmentId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const shipment = next.shipments.find((item) => item.id === shipmentId);
  if (!shipment) throw new Error("shipment_not_found");
  if (shipment.status === "cancelled") return next;

  for (const line of shipment.lines) {
    moveStatus(next, {
      companyId: shipment.companyId,
      idempotencyKey: `${idempotencyKey}:${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      movementType: "ship_cancel",
      postedAt: new Date().toISOString(),
      quantity: line.quantity,
      serialId: line.serialId,
      sourceId: shipment.id,
      sourceLineId: line.id,
      sourceType: "shipment",
      uom: "ea"
    }, "shipped", "available");
  }
  shipment.status = "cancelled";
  shipment.version += 1;
  const reservation = next.reservations.find((item) => item.id === shipment.reservationId);
  if (reservation) {
    for (const line of reservation.lines) {
      const cancelledQuantity = shipment.lines
        .filter((shipmentLine) => shipmentLine.reservationLineId === line.id)
        .reduce((sum, shipmentLine) => sum + shipmentLine.quantity, 0);
      line.shippedQuantity = Math.max(0, line.shippedQuantity - cancelledQuantity);
      line.pickedQuantity = Math.max(0, line.pickedQuantity - cancelledQuantity);
    }
    reservation.status = "cancelled";
    reservation.version += 1;
  }
  appendCommand(next, shipment.companyId, "CancelShipment", idempotencyKey, "shipment", shipment.id, { shipmentId });
  appendOutbox(next, shipment.companyId, "warehouse.shipment_cancelled", { shipmentId });
  return next;
}

export function authorizeReturn(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    lines: Array<{
      acceptedQuantity?: number;
      quantity: number;
      resellable: boolean;
      shipmentLineId: string;
    }>;
    returnId: string;
    shipmentId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const shipment = next.shipments.find((item) => item.id === input.shipmentId);
  if (!shipment) throw new Error("shipment_not_found");
  if (shipment.status !== "shipped") throw new Error("shipment_not_shipped");

  const returnLines = input.lines.map((line, index) => {
    const shipmentLine = shipment.lines.find((item) => item.id === line.shipmentLineId);
    if (!shipmentLine) throw new Error("shipment_line_not_found");
    if (line.quantity <= 0 || line.quantity > shipmentLine.quantity) throw new Error("return_quantity_exceeds_shipped");
    const acceptedQuantity = line.acceptedQuantity ?? line.quantity;
    if (acceptedQuantity < 0 || acceptedQuantity > line.quantity) throw new Error("return_accepted_quantity_invalid");
    return {
      acceptedQuantity,
      id: `${input.returnId}-line-${index + 1}`,
      inventoryItemId: shipmentLine.inventoryItemId,
      inventoryOwnerPartyId: shipmentLine.inventoryOwnerPartyId,
      locationId: shipmentLine.locationId,
      lotId: shipmentLine.lotId,
      quantity: line.quantity,
      resellable: line.resellable,
      serialId: shipmentLine.serialId,
      shipmentLineId: shipmentLine.id
    };
  });

  const authorization: ReturnAuthorization = {
    companyId: shipment.companyId,
    externalId: input.returnId,
    id: input.returnId,
    inventoryOwnerPartyId: shipment.inventoryOwnerPartyId,
    lines: returnLines,
    sourceShipmentId: shipment.id,
    status: "authorized",
    version: 1
  };
  next.returns.push(authorization);
  appendCommand(next, input.command.companyId, "AuthorizeReturn", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.return_authorized", { returnId: authorization.id, shipmentId: shipment.id });
  return next;
}

export function receiveReturn(state: WarehouseState, returnId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const authorization = next.returns.find((item) => item.id === returnId);
  if (!authorization) throw new Error("return_not_found");
  if (authorization.status !== "authorized") return next;

  for (const line of authorization.lines) {
    if (line.acceptedQuantity <= 0) continue;
    const movement = {
      companyId: authorization.companyId,
      externalId: `mov-${idempotencyKey}-${line.id}`,
      id: `mov-${next.movements.length + 1}`,
      idempotencyKey: `${idempotencyKey}:${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      movementType: "return_receive",
      postedAt: new Date().toISOString(),
      quantity: line.acceptedQuantity,
      serialId: line.serialId,
      sourceId: authorization.id,
      sourceLineId: line.id,
      sourceType: "return_authorization",
      stockStatus: line.resellable ? "available" : "damaged",
      stockStatusTo: line.resellable ? "available" : "damaged",
      uom: "ea"
    } satisfies StockMovement;
    applyBalanceDelta(next, movement, movement.stockStatus, movement.quantity);
    next.movements.push(movement);
  }
  authorization.status = "received";
  authorization.version += 1;
  appendCommand(next, authorization.companyId, "ReceiveReturn", idempotencyKey, "return_authorization", authorization.id, { returnId });
  appendOutbox(next, authorization.companyId, "warehouse.return_received", { returnId: authorization.id });
  return next;
}

export function startScannerSession(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    deviceId: string;
    locationId?: string;
    sessionId: string;
    userId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const session: ScannerSession = {
    companyId: input.command.companyId,
    deviceId: input.deviceId,
    externalId: input.sessionId,
    id: input.sessionId,
    locationId: input.locationId,
    scanCount: 0,
    startedAt: input.command.requestedAt,
    status: "active",
    userId: input.userId,
    version: 1
  };
  next.scannerSessions.push(session);
  appendCommand(next, input.command.companyId, "StartScannerSession", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.scanner_session_started", { sessionId: session.id, deviceId: session.deviceId });
  return next;
}

export function ingestScanEvent(
  state: WarehouseState,
  input: {
    action: ScanEvent["action"];
    barcode: string;
    companyId: string;
    eventId: string;
    idempotencyKey?: string;
    inventoryItemId?: string;
    locationId?: string;
    lotId?: string | null;
    occurredAt?: string;
    quantity?: number;
    serialId?: string | null;
    sessionId: string;
  }
) {
  const next = cloneWarehouseState(state);
  const idempotencyKey = input.idempotencyKey ?? `${input.companyId}:scan:${input.sessionId}:${input.eventId}`;
  if (next.scanEvents.some((event) => event.idempotencyKey === idempotencyKey || event.externalId === input.eventId)) return next;
  const session = next.scannerSessions.find((item) => item.id === input.sessionId);
  if (!session) throw new Error("scanner_session_not_found");
  if (session.status !== "active") throw new Error("scanner_session_closed");
  const event: ScanEvent = {
    action: input.action,
    barcode: input.barcode,
    companyId: input.companyId,
    externalId: input.eventId,
    id: input.eventId,
    idempotencyKey,
    inventoryItemId: input.inventoryItemId,
    locationId: input.locationId ?? session.locationId,
    lotId: input.lotId,
    occurredAt: input.occurredAt ?? new Date().toISOString(),
    quantity: input.quantity ?? 1,
    serialId: input.serialId,
    sessionId: input.sessionId
  };
  next.scanEvents.push(event);
  session.scanCount += 1;
  session.version += 1;
  appendCommand(next, input.companyId, "IngestScanEvent", idempotencyKey, "scanner_session", input.sessionId, event);
  appendOutbox(next, input.companyId, "warehouse.scan_ingested", { action: event.action, eventId: event.id, sessionId: event.sessionId });
  return next;
}

export function openCycleCount(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    countId: string;
    inventoryItemIds?: string[];
    locationId: string;
    stockStatus?: StockStatus;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const stockStatus = input.stockStatus ?? "available";
  const lines = next.balances
    .filter((balance) =>
      balance.companyId === input.command.companyId &&
      balance.locationId === input.locationId &&
      balance.stockStatus === stockStatus &&
      balance.quantity !== 0 &&
      (!input.inventoryItemIds || input.inventoryItemIds.includes(balance.inventoryItemId))
    )
    .map((balance, index) => ({
      companyId: balance.companyId,
      expectedQuantity: balance.quantity,
      id: `${input.countId}-line-${index + 1}`,
      inventoryItemId: balance.inventoryItemId,
      inventoryOwnerPartyId: balance.inventoryOwnerPartyId,
      locationId: balance.locationId,
      lotId: balance.lotId,
      serialId: balance.serialId,
      stockStatus: balance.stockStatus
    }));
  const count: CycleCount = {
    companyId: input.command.companyId,
    externalId: input.countId,
    id: input.countId,
    lines,
    locationId: input.locationId,
    openedAt: input.command.requestedAt,
    status: "open",
    version: 1
  };
  next.cycleCounts.push(count);
  appendCommand(next, input.command.companyId, "OpenCycleCount", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.cycle_count_opened", { countId: count.id, lineCount: count.lines.length });
  return next;
}

export function recordCycleCountLine(
  state: WarehouseState,
  input: {
    countedQuantity: number;
    countId: string;
    idempotencyKey: string;
    lineId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.idempotencyKey)) return next;
  const count = next.cycleCounts.find((item) => item.id === input.countId);
  if (!count) throw new Error("cycle_count_not_found");
  if (count.status !== "open") throw new Error("cycle_count_not_open");
  const line = count.lines.find((item) => item.id === input.lineId);
  if (!line) throw new Error("cycle_count_line_not_found");
  line.countedQuantity = input.countedQuantity;
  line.varianceQuantity = input.countedQuantity - line.expectedQuantity;
  count.version += 1;
  appendCommand(next, count.companyId, "RecordCycleCountLine", input.idempotencyKey, "cycle_count", count.id, {
    countedQuantity: input.countedQuantity,
    lineId: input.lineId
  });
  appendOutbox(next, count.companyId, "warehouse.cycle_count_line_recorded", { countId: count.id, lineId: line.id });
  return next;
}

export function closeCycleCount(state: WarehouseState, countId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const count = next.cycleCounts.find((item) => item.id === countId);
  if (!count) throw new Error("cycle_count_not_found");
  if (count.status !== "open") return next;

  for (const line of count.lines) {
    const countedQuantity = line.countedQuantity ?? line.expectedQuantity;
    const variance = countedQuantity - line.expectedQuantity;
    line.countedQuantity = countedQuantity;
    line.varianceQuantity = variance;
    if (variance === 0) continue;
    const movement: StockMovement = {
      companyId: count.companyId,
      externalId: `mov-${idempotencyKey}-${line.id}`,
      id: `mov-${next.movements.length + 1}`,
      idempotencyKey: `${idempotencyKey}:${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      movementType: "adjust",
      postedAt: new Date().toISOString(),
      quantity: variance,
      serialId: line.serialId,
      sourceId: count.id,
      sourceLineId: line.id,
      sourceType: "cycle_count",
      stockStatus: line.stockStatus,
      stockStatusTo: line.stockStatus,
      uom: "ea"
    };
    applyBalanceDelta(next, movement, movement.stockStatus, movement.quantity);
    next.movements.push(movement);
    const adjustment: InventoryAdjustment = {
      companyId: count.companyId,
      cycleCountId: count.id,
      externalId: `adj-${count.id}-${line.id}`,
      id: `adj-${count.id}-${line.id}`,
      lineId: line.id,
      movementId: movement.id,
      quantity: variance,
      reason: "cycle_count"
    };
    next.inventoryAdjustments.push(adjustment);
  }
  count.status = "closed";
  count.closedAt = new Date().toISOString();
  count.version += 1;
  appendCommand(next, count.companyId, "CloseCycleCount", idempotencyKey, "cycle_count", count.id, { countId });
  appendOutbox(next, count.companyId, "warehouse.cycle_count_closed", {
    adjustmentCount: next.inventoryAdjustments.filter((item) => item.cycleCountId === count.id).length,
    countId: count.id
  });
  return next;
}

export function createShipmentPackage(
  state: WarehouseState,
  input: {
    carrier?: string;
    command: WarehouseCommand;
    packageId: string;
    shipmentId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const shipment = next.shipments.find((item) => item.id === input.shipmentId);
  if (!shipment) throw new Error("shipment_not_found");
  const shipmentPackage: ShipmentPackage = {
    carrier: input.carrier ?? shipment.carrier,
    companyId: shipment.companyId,
    externalId: input.packageId,
    id: input.packageId,
    shipmentId: shipment.id,
    status: "packed",
    trackingNumber: shipment.trackingNumber,
    version: 1
  };
  next.shipmentPackages.push(shipmentPackage);
  appendCommand(next, input.command.companyId, "CreateShipmentPackage", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.shipment_package_created", { packageId: shipmentPackage.id, shipmentId: shipment.id });
  return next;
}

export function createFulfillmentLabel(
  state: WarehouseState,
  input: {
    carrier: string;
    command: WarehouseCommand;
    labelId: string;
    packageId: string;
    provider: string;
    trackingNumber: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const shipmentPackage = next.shipmentPackages.find((item) => item.id === input.packageId);
  if (!shipmentPackage) throw new Error("shipment_package_not_found");
  const label: FulfillmentLabel = {
    carrier: input.carrier,
    companyId: input.command.companyId,
    externalId: input.labelId,
    id: input.labelId,
    packageId: input.packageId,
    provider: input.provider,
    status: "created",
    trackingNumber: input.trackingNumber,
    version: 1
  };
  shipmentPackage.status = "labelled";
  shipmentPackage.trackingNumber = input.trackingNumber;
  shipmentPackage.version += 1;
  next.fulfillmentLabels.push(label);
  appendCommand(next, input.command.companyId, "CreateFulfillmentLabel", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.fulfillment_label_created", {
    labelId: label.id,
    packageId: label.packageId,
    trackingNumber: label.trackingNumber
  });
  return next;
}

export function recordShipmentTrackingEvent(
  state: WarehouseState,
  input: {
    carrier: string;
    companyId: string;
    eventCode: string;
    eventId: string;
    eventTime?: string;
    idempotencyKey?: string;
    shipmentId: string;
    trackingNumber: string;
  }
) {
  const next = cloneWarehouseState(state);
  const idempotencyKey = input.idempotencyKey ?? `${input.companyId}:tracking:${input.eventId}`;
  if (hasCommand(next, idempotencyKey) || next.shipmentTrackingEvents.some((event) => event.externalId === input.eventId)) return next;
  const shipment = next.shipments.find((item) => item.id === input.shipmentId);
  if (!shipment) throw new Error("shipment_not_found");
  const event: ShipmentTrackingEvent = {
    carrier: input.carrier,
    companyId: input.companyId,
    eventCode: input.eventCode,
    eventTime: input.eventTime ?? new Date().toISOString(),
    externalId: input.eventId,
    id: input.eventId,
    shipmentId: input.shipmentId,
    trackingNumber: input.trackingNumber
  };
  next.shipmentTrackingEvents.push(event);
  appendCommand(next, input.companyId, "RecordShipmentTrackingEvent", idempotencyKey, "shipment", input.shipmentId, event);
  appendOutbox(next, input.companyId, "warehouse.shipment_tracking_recorded", {
    eventCode: event.eventCode,
    eventId: event.id,
    shipmentId: event.shipmentId
  });
  return next;
}

export function ingestIntegrationEvent(
  state: WarehouseState,
  input: {
    companyId: string;
    eventId: string;
    eventType: string;
    idempotencyKey?: string;
    payload: Record<string, unknown>;
    provider: string;
    receivedAt?: string;
    source: IntegrationEvent["source"];
  }
) {
  const next = cloneWarehouseState(state);
  const idempotencyKey = input.idempotencyKey ?? `${input.companyId}:${input.source}:${input.provider}:${input.eventId}`;
  if (hasCommand(next, idempotencyKey) || next.integrationEvents.some((event) => event.externalId === input.eventId)) return next;
  const event: IntegrationEvent = {
    companyId: input.companyId,
    eventType: input.eventType,
    externalId: input.eventId,
    id: input.eventId,
    idempotencyKey,
    payload: input.payload,
    provider: input.provider,
    receivedAt: input.receivedAt ?? new Date().toISOString(),
    source: input.source
  };
  next.integrationEvents.push(event);
  appendCommand(next, input.companyId, "IngestIntegrationEvent", idempotencyKey, input.source, input.eventId, event);
  appendOutbox(next, input.companyId, "warehouse.integration_event_ingested", {
    eventId: event.id,
    eventType: event.eventType,
    provider: event.provider,
    source: event.source
  });
  return next;
}

export function ingestRoboticsEvent(
  state: WarehouseState,
  input: {
    companyId: string;
    eventId: string;
    eventType: string;
    idempotencyKey?: string;
    occurredAt?: string;
    payload: Record<string, unknown>;
    robotId: string;
  }
) {
  const next = cloneWarehouseState(state);
  const idempotencyKey = input.idempotencyKey ?? `${input.companyId}:robot:${input.robotId}:${input.eventId}`;
  if (hasCommand(next, idempotencyKey) || next.roboticsEvents.some((event) => event.externalId === input.eventId)) return next;
  const event: RoboticsEvent = {
    companyId: input.companyId,
    eventType: input.eventType,
    externalId: input.eventId,
    id: input.eventId,
    idempotencyKey,
    occurredAt: input.occurredAt ?? new Date().toISOString(),
    payload: input.payload,
    robotId: input.robotId
  };
  next.roboticsEvents.push(event);
  appendCommand(next, input.companyId, "IngestRoboticsEvent", idempotencyKey, "robotics", input.eventId, event);
  appendOutbox(next, input.companyId, "warehouse.robotics_event_ingested", {
    eventId: event.id,
    eventType: event.eventType,
    robotId: event.robotId
  });
  return next;
}

export function createWavePlan(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    priority?: WavePlan["priority"];
    reservationIds: string[];
    waveId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const lines = input.reservationIds.map((reservationId, index) => {
    const reservation = next.reservations.find((item) => item.id === reservationId);
    if (!reservation) throw new Error("reservation_not_found");
    const pickList = next.pickLists.find((item) => item.reservationId === reservationId);
    return {
      id: `${input.waveId}-line-${index + 1}`,
      pickListId: pickList?.id,
      reservationId,
      sequence: index + 1
    };
  });
  const wavePlan: WavePlan = {
    companyId: input.command.companyId,
    externalId: input.waveId,
    id: input.waveId,
    lines,
    priority: input.priority ?? "normal",
    status: "planned",
    version: 1
  };
  next.wavePlans.push(wavePlan);
  appendCommand(next, input.command.companyId, "CreateWavePlan", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.wave_plan_created", { lineCount: wavePlan.lines.length, waveId: wavePlan.id });
  return next;
}

export function createSlottingRecommendation(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    fromLocationId: string;
    inventoryItemId: string;
    reason: string;
    recommendationId: string;
    toLocationId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const recommendation: SlottingRecommendation = {
    companyId: input.command.companyId,
    externalId: input.recommendationId,
    fromLocationId: input.fromLocationId,
    id: input.recommendationId,
    inventoryItemId: input.inventoryItemId,
    reason: input.reason,
    status: "recommended",
    toLocationId: input.toLocationId
  };
  next.slottingRecommendations.push(recommendation);
  appendCommand(next, input.command.companyId, "CreateSlottingRecommendation", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.slotting_recommendation_created", { recommendationId: recommendation.id });
  return next;
}

export function createWarehouseTransfer(
  state: WarehouseState,
  input: {
    command: WarehouseCommand;
    fromLocationId: string;
    fromNodeId: string;
    lines: Array<Omit<WarehouseTransferLine, "id" | "receivedQuantity" | "shippedQuantity">>;
    toLocationId: string;
    toNodeId: string;
    transferId: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey)) return next;
  const transfer: WarehouseTransfer = {
    companyId: input.command.companyId,
    externalId: input.transferId,
    fromLocationId: input.fromLocationId,
    fromNodeId: input.fromNodeId,
    id: input.transferId,
    lines: input.lines.map((line, index) => ({
      ...line,
      id: `${input.transferId}-line-${index + 1}`,
      receivedQuantity: 0,
      shippedQuantity: 0
    })),
    status: "draft",
    toLocationId: input.toLocationId,
    toNodeId: input.toNodeId,
    version: 1
  };
  next.transfers.push(transfer);
  appendCommand(next, input.command.companyId, "CreateWarehouseTransfer", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.transfer_created", { lineCount: transfer.lines.length, transferId: transfer.id });
  return next;
}

export function shipWarehouseTransfer(state: WarehouseState, transferId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const transfer = next.transfers.find((item) => item.id === transferId);
  if (!transfer) throw new Error("transfer_not_found");
  if (transfer.status !== "draft") return next;
  for (const line of transfer.lines) {
    const available = findBalanceQuantity(next, {
      companyId: transfer.companyId,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: transfer.fromLocationId,
      lotId: line.lotId,
      serialId: line.serialId,
      stockStatus: "available"
    });
    if (available < line.quantity) throw new Error("insufficient_transfer_quantity");
    moveStatus(next, transferMovement(transfer, line, idempotencyKey, "transfer_ship", line.quantity), "available", "in_transit", transfer.toLocationId);
    line.shippedQuantity = line.quantity;
  }
  transfer.status = "shipped";
  transfer.version += 1;
  appendCommand(next, transfer.companyId, "ShipWarehouseTransfer", idempotencyKey, "warehouse_transfer", transfer.id, { transferId });
  appendOutbox(next, transfer.companyId, "warehouse.transfer_shipped", { transferId: transfer.id });
  return next;
}

export function receiveWarehouseTransfer(state: WarehouseState, transferId: string, idempotencyKey: string) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, idempotencyKey)) return next;
  const transfer = next.transfers.find((item) => item.id === transferId);
  if (!transfer) throw new Error("transfer_not_found");
  if (transfer.status !== "shipped") return next;
  for (const line of transfer.lines) {
    const receivable = line.shippedQuantity - line.receivedQuantity;
    if (receivable <= 0) continue;
    moveStatus(next, transferMovement(transfer, line, idempotencyKey, "transfer_receive", receivable), "in_transit", "available", transfer.toLocationId);
    line.receivedQuantity += receivable;
  }
  transfer.status = "received";
  transfer.version += 1;
  appendCommand(next, transfer.companyId, "ReceiveWarehouseTransfer", idempotencyKey, "warehouse_transfer", transfer.id, { transferId });
  appendOutbox(next, transfer.companyId, "warehouse.transfer_received", { transferId: transfer.id });
  return next;
}

export function recordOfflineSyncBatch(
  state: WarehouseState,
  input: {
    batchId: string;
    command: WarehouseCommand;
    deviceId: string;
    events: OfflineSyncEvent[];
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey) || next.offlineSyncBatches.some((batch) => batch.id === input.batchId)) return next;
  const seen = new Set<string>();
  const events = input.events.filter((event) => {
    if (seen.has(event.idempotencyKey)) return false;
    seen.add(event.idempotencyKey);
    return !next.offlineSyncBatches.some((batch) => batch.events.some((existing) => existing.idempotencyKey === event.idempotencyKey));
  });
  const batch: OfflineSyncBatch = {
    companyId: input.command.companyId,
    deviceId: input.deviceId,
    events,
    externalId: input.batchId,
    id: input.batchId,
    receivedAt: input.command.requestedAt,
    status: "accepted"
  };
  next.offlineSyncBatches.push(batch);
  appendCommand(next, input.command.companyId, "RecordOfflineSyncBatch", input.command.idempotencyKey, input.command.refType, input.command.refId, {
    ...input.command.payload,
    acceptedEvents: events.length
  });
  appendOutbox(next, input.command.companyId, "warehouse.offline_sync_batch_recorded", { batchId: batch.id, eventCount: events.length });
  return next;
}

export function recordThreePlCharge(
  state: WarehouseState,
  input: {
    amountCents: number;
    chargeId: string;
    command: WarehouseCommand;
    currency: string;
    inventoryOwnerPartyId: string;
    metric: ThreePlCharge["metric"];
    quantity: number;
    sourceId: string;
    sourceType: string;
  }
) {
  const next = cloneWarehouseState(state);
  if (hasCommand(next, input.command.idempotencyKey) || next.threePlCharges.some((charge) => charge.id === input.chargeId)) return next;
  const charge: ThreePlCharge = {
    amountCents: input.amountCents,
    companyId: input.command.companyId,
    currency: input.currency,
    externalId: input.chargeId,
    id: input.chargeId,
    inventoryOwnerPartyId: input.inventoryOwnerPartyId,
    metric: input.metric,
    quantity: input.quantity,
    sourceId: input.sourceId,
    sourceType: input.sourceType
  };
  next.threePlCharges.push(charge);
  appendCommand(next, input.command.companyId, "RecordThreePlCharge", input.command.idempotencyKey, input.command.refType, input.command.refId, input.command.payload);
  appendOutbox(next, input.command.companyId, "warehouse.three_pl_charge_recorded", {
    amountCents: charge.amountCents,
    chargeId: charge.id,
    metric: charge.metric
  });
  return next;
}

export function getAvailableQuantity(state: WarehouseState, input: Omit<BalanceDimension, "stockStatus">) {
  return getBalance(state, { ...input, stockStatus: "available" }).quantity;
}

export function replayMovements(state: Pick<WarehouseState, "movements">) {
  const replay = createEmptyWarehouseState();
  const movements = [...state.movements].sort((a, b) =>
    a.postedAt === b.postedAt ? movementSequence(a.id) - movementSequence(b.id) : a.postedAt.localeCompare(b.postedAt)
  );
  for (const movement of movements) {
    applyBalanceDelta(replay, movement, movement.stockStatus, movement.quantity);
  }
  return replay.balances;
}

function movementSequence(id: string) {
  const sequence = Number(id.replace(/^\D+/, ""));
  return Number.isFinite(sequence) ? sequence : 0;
}

function collectPickCandidates(state: WarehouseState, reservation: StockReservation): PickListLine[] {
  if (reservation.status === "released" || reservation.status === "cancelled" || reservation.status === "consumed") {
    throw new Error("reservation_not_pickable");
  }
  return reservation.lines.flatMap((line) => {
    const pickable = line.quantity - line.pickedQuantity - line.shippedQuantity - line.releasedQuantity;
    if (pickable <= 0) return [];
    const location = state.locations.find((item) => item.id === line.locationId && item.companyId === line.companyId);
    if (!location?.pickable) throw new Error("location_not_pickable");
    const reservedQuantity = findBalanceQuantity(state, { ...line, stockStatus: "reserved" });
    if (reservedQuantity < pickable) throw new Error("pick_quantity_exceeds_reserved");
    return [{
      id: `pick-${reservation.id}-line-${line.id}`,
      inventoryItemId: line.inventoryItemId,
      inventoryOwnerPartyId: line.inventoryOwnerPartyId,
      locationId: line.locationId,
      lotId: line.lotId,
      pickedQuantity: 0,
      quantity: pickable,
      reservationLineId: line.id,
      serialId: line.serialId
    }];
  });
}

function movementFromReservationLine(
  reservation: StockReservation,
  line: StockReservationLine,
  idempotencyKey: string,
  movementType: "release" | "pick" | "ship",
  quantity: number
) {
  return {
    companyId: reservation.companyId,
    idempotencyKey: `${idempotencyKey}:${line.id}`,
    inventoryItemId: line.inventoryItemId,
    inventoryOwnerPartyId: line.inventoryOwnerPartyId,
    locationId: line.locationId,
    lotId: line.lotId,
    movementType,
    postedAt: new Date().toISOString(),
    quantity,
    serialId: line.serialId,
    sourceId: reservation.id,
    sourceLineId: line.id,
    sourceType: "stock_reservation",
    uom: "ea"
  };
}

function transferMovement(
  transfer: WarehouseTransfer,
  line: WarehouseTransferLine,
  idempotencyKey: string,
  movementType: "transfer_ship" | "transfer_receive",
  quantity: number
) {
  return {
    companyId: transfer.companyId,
    idempotencyKey: `${idempotencyKey}:${line.id}`,
    inventoryItemId: line.inventoryItemId,
    inventoryOwnerPartyId: line.inventoryOwnerPartyId,
    locationId: movementType === "transfer_ship" ? transfer.fromLocationId : transfer.toLocationId,
    lotId: line.lotId,
    movementType,
    postedAt: new Date().toISOString(),
    quantity,
    serialId: line.serialId,
    sourceId: transfer.id,
    sourceLineId: line.id,
    sourceType: "warehouse_transfer",
    uom: "ea"
  };
}

function moveStatus(
  state: WarehouseState,
  movement: Omit<StockMovement, "externalId" | "id" | "stockStatus">,
  fromStatus: StockStatus,
  toStatus: StockStatus,
  toLocationId = movement.locationId
) {
  const fromMovement = {
    ...movement,
    externalId: `mov-${movement.idempotencyKey}-from`,
    id: `mov-${state.movements.length + 1}`,
    quantity: -movement.quantity,
    stockStatus: fromStatus,
    stockStatusFrom: fromStatus,
    stockStatusTo: toStatus
  };
  const toMovement = {
    ...movement,
    externalId: `mov-${movement.idempotencyKey}-to`,
    id: `mov-${state.movements.length + 2}`,
    locationId: toLocationId,
    quantity: movement.quantity,
    stockStatus: toStatus,
    stockStatusFrom: fromStatus,
    stockStatusTo: toStatus
  };
  applyBalanceDelta(state, fromMovement, fromStatus, fromMovement.quantity);
  applyBalanceDelta(state, toMovement, toStatus, toMovement.quantity);
  state.movements.push(fromMovement, toMovement);
}

function applyBalanceDelta(state: WarehouseState, movement: BalanceDimension, stockStatus: StockStatus, quantity: number) {
  const dimension = { ...movement, stockStatus };
  const balance = getBalance(state, dimension);
  const nextQuantity = balance.quantity + quantity;
  const policy = state.policies.find((item) => item.companyId === movement.companyId);
  if (nextQuantity < 0 && !policy?.allowNegativeStock) {
    throw new Error("negative_stock_not_allowed");
  }
  balance.quantity = nextQuantity;
  balance.updatedAt = new Date().toISOString();
}

function getBalance(state: WarehouseState, input: BalanceDimension): StockBalance {
  const balanceKey = createBalanceKey(input);
  let balance = state.balances.find((item) => item.companyId === input.companyId && item.balanceKey === balanceKey);
  if (!balance) {
    balance = {
      ...input,
      balanceKey,
      inventoryOwnerPartyId: input.inventoryOwnerPartyId || SYSTEM_OWNER,
      lotId: input.lotId ?? null,
      quantity: 0,
      serialId: input.serialId ?? null,
      updatedAt: new Date().toISOString()
    };
    state.balances.push(balance);
  }
  return balance;
}

function findBalanceQuantity(state: WarehouseState, input: BalanceDimension) {
  const balanceKey = createBalanceKey(input);
  return state.balances.find((item) => item.companyId === input.companyId && item.balanceKey === balanceKey)?.quantity ?? 0;
}

function hasReservedSerial(
  state: WarehouseState,
  companyId: string,
  inventoryOwnerPartyId: string,
  inventoryItemId: string,
  serialId: string
) {
  return state.reservations.some((reservation) =>
    reservation.status !== "released" &&
    reservation.status !== "cancelled" &&
    reservation.lines.some((line) =>
      line.companyId === companyId &&
      line.inventoryOwnerPartyId === inventoryOwnerPartyId &&
      line.inventoryItemId === inventoryItemId &&
      line.serialId === serialId &&
      line.quantity > line.releasedQuantity + line.shippedQuantity
    )
  );
}

function appendCommand(
  state: WarehouseState,
  companyId: string,
  type: WarehouseCommandType,
  idempotencyKey: string,
  refType: string,
  refId: string,
  payload: Record<string, unknown>
) {
  state.commandLog.push(createWarehouseCommand({
    companyId,
    idempotencyKey,
    payload,
    refId,
    refType,
    requestedBy: "system",
    type
  }));
}

function appendOutbox(state: WarehouseState, companyId: string, topic: string, payload: Record<string, unknown>) {
  const event: WarehouseOutboxEvent = {
    companyId,
    id: `outbox-${topic}-${state.outbox.length + 1}`,
    payload,
    status: "pending",
    topic
  };
  state.outbox.push(event);
}

function hasCommand(state: WarehouseState, idempotencyKey: string) {
  return state.commandLog.some((command) => command.idempotencyKey === idempotencyKey);
}
