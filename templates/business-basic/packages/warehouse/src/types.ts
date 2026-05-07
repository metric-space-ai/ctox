export type StockStatus = "available" | "receiving" | "reserved" | "picked" | "packed" | "in_transit" | "shipped" | "quarantine" | "damaged";

export type MovementType =
  | "receive"
  | "putaway"
  | "reserve"
  | "release"
  | "pick"
  | "pack"
  | "ship"
  | "ship_cancel"
  | "transfer_ship"
  | "transfer_receive"
  | "return_receive"
  | "adjust";

export type WarehouseCommandType =
  | "PostStockMovement"
  | "ReserveStock"
  | "CancelReservation"
  | "ReleaseReservation"
  | "CreatePickList"
  | "PickReservation"
  | "ShipReservation"
  | "CancelShipment"
  | "ReceiveStock"
  | "CompletePutaway"
  | "AuthorizeReturn"
  | "ReceiveReturn"
  | "StartScannerSession"
  | "IngestScanEvent"
  | "OpenCycleCount"
  | "RecordCycleCountLine"
  | "CloseCycleCount"
  | "CreateShipmentPackage"
  | "CreateFulfillmentLabel"
  | "RecordShipmentTrackingEvent"
  | "IngestIntegrationEvent"
  | "IngestRoboticsEvent"
  | "CreateWavePlan"
  | "CreateSlottingRecommendation"
  | "CreateWarehouseTransfer"
  | "ShipWarehouseTransfer"
  | "ReceiveWarehouseTransfer"
  | "RecordOfflineSyncBatch"
  | "RecordThreePlCharge";

export type InventoryTrackingMode = "none" | "lot" | "serial";

export type InventoryItem = {
  companyId: string;
  externalId: string;
  id: string;
  name: string;
  sku: string;
  trackingMode: InventoryTrackingMode;
  uom: string;
};

export type WarehouseLocation = {
  companyId: string;
  defaultOwnerPartyId?: string;
  externalId: string;
  id: string;
  kind: "warehouse" | "zone" | "bin";
  name: string;
  parentId?: string;
  pickable: boolean;
  receivable: boolean;
};

export type WarehouseNode = {
  companyId: string;
  externalId: string;
  id: string;
  kind: "warehouse" | "store" | "third_party_logistics" | "virtual";
  name: string;
  status: "active" | "inactive";
};

export type WarehousePolicy = {
  allowBackorder: boolean;
  allowNegativeStock: boolean;
  allocationStrategy: "fifo" | "fefo" | "manual";
  companyId: string;
  defaultOwnerPartyId: string;
  id: string;
};

export type BalanceDimension = {
  companyId: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  locationId: string;
  lotId?: string | null;
  serialId?: string | null;
  stockStatus: StockStatus;
};

export type StockBalance = BalanceDimension & {
  balanceKey: string;
  quantity: number;
  updatedAt: string;
};

export type StockMovement = BalanceDimension & {
  externalId: string;
  id: string;
  idempotencyKey: string;
  movementType: MovementType;
  postedAt: string;
  quantity: number;
  sourceId: string;
  sourceLineId?: string;
  sourceType: string;
  stockStatusFrom?: StockStatus;
  stockStatusTo?: StockStatus;
  uom: string;
};

export type StockReservationStatus =
  | "draft"
  | "reserved"
  | "partially_reserved"
  | "released"
  | "partially_consumed"
  | "consumed"
  | "cancelled"
  | "expired";

export type StockReservationLine = BalanceDimension & {
  allowBackorder: boolean;
  id: string;
  pickedQuantity: number;
  quantity: number;
  releasedQuantity: number;
  shippedQuantity: number;
  sourceLineId: string;
};

export type StockReservation = {
  allowPartialReservation: boolean;
  companyId: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  lines: StockReservationLine[];
  sourceId: string;
  sourceType: string;
  status: StockReservationStatus;
  version: number;
};

export type PickListLine = {
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  locationId: string;
  lotId?: string | null;
  pickedQuantity: number;
  quantity: number;
  reservationLineId: string;
  serialId?: string | null;
};

export type PickList = {
  companyId: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  lines: PickListLine[];
  reservationId: string;
  status: "draft" | "ready" | "picked" | "cancelled";
  version: number;
};

export type ReceiptLine = {
  companyId: string;
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  locationId: string;
  lotId?: string | null;
  quantity: number;
  serialId?: string | null;
};

export type Receipt = {
  companyId: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  lines: ReceiptLine[];
  sourceId: string;
  sourceType: string;
  status: "draft" | "received" | "putaway_started" | "putaway_complete" | "cancelled";
  version: number;
};

export type PutawayTask = {
  companyId: string;
  externalId: string;
  fromLocationId: string;
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  lotId?: string | null;
  quantity: number;
  receiptLineId: string;
  serialId?: string | null;
  status: "open" | "done" | "cancelled";
  toLocationId: string;
  version: number;
};

export type ShipmentLine = {
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  locationId: string;
  lotId?: string | null;
  quantity: number;
  reservationLineId: string;
  serialId?: string | null;
};

export type Shipment = {
  carrier?: string;
  companyId: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  lines: ShipmentLine[];
  provider?: string;
  reservationId: string;
  status: "draft" | "packed" | "shipped" | "cancelled";
  trackingNumber?: string;
  version: number;
};

export type ShipmentPackage = {
  carrier?: string;
  companyId: string;
  externalId: string;
  id: string;
  shipmentId: string;
  status: "draft" | "packed" | "labelled" | "shipped" | "cancelled";
  trackingNumber?: string;
  version: number;
};

export type FulfillmentLabel = {
  carrier: string;
  companyId: string;
  externalId: string;
  id: string;
  packageId: string;
  provider: string;
  status: "created" | "voided";
  trackingNumber: string;
  version: number;
};

export type ShipmentTrackingEvent = {
  carrier: string;
  companyId: string;
  eventCode: string;
  eventTime: string;
  externalId: string;
  id: string;
  shipmentId: string;
  trackingNumber: string;
};

export type ReturnLine = {
  acceptedQuantity: number;
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  locationId: string;
  lotId?: string | null;
  quantity: number;
  resellable: boolean;
  serialId?: string | null;
  shipmentLineId: string;
};

export type ReturnAuthorization = {
  companyId: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  lines: ReturnLine[];
  sourceShipmentId: string;
  status: "authorized" | "received" | "closed" | "cancelled";
  version: number;
};

export type ScannerSession = {
  companyId: string;
  deviceId: string;
  endedAt?: string;
  externalId: string;
  id: string;
  locationId?: string;
  scanCount: number;
  startedAt: string;
  status: "active" | "closed";
  userId: string;
  version: number;
};

export type ScanEvent = {
  action: "receive" | "putaway" | "pick" | "pack" | "ship" | "count";
  barcode: string;
  companyId: string;
  externalId: string;
  id: string;
  idempotencyKey: string;
  inventoryItemId?: string;
  locationId?: string;
  lotId?: string | null;
  occurredAt: string;
  quantity: number;
  serialId?: string | null;
  sessionId: string;
};

export type CycleCountLine = BalanceDimension & {
  countedQuantity?: number;
  expectedQuantity: number;
  id: string;
  varianceQuantity?: number;
};

export type CycleCount = {
  closedAt?: string;
  companyId: string;
  externalId: string;
  id: string;
  lines: CycleCountLine[];
  locationId: string;
  openedAt: string;
  status: "open" | "closed" | "cancelled";
  version: number;
};

export type InventoryAdjustment = {
  companyId: string;
  cycleCountId?: string;
  externalId: string;
  id: string;
  lineId: string;
  movementId: string;
  quantity: number;
  reason: "cycle_count" | "manual";
};

export type IntegrationEvent = {
  companyId: string;
  eventType: string;
  externalId: string;
  id: string;
  idempotencyKey: string;
  payload: Record<string, unknown>;
  provider: string;
  receivedAt: string;
  source: "wes" | "mfc" | "payment" | "commerce";
};

export type RoboticsEvent = {
  companyId: string;
  eventType: string;
  externalId: string;
  id: string;
  idempotencyKey: string;
  payload: Record<string, unknown>;
  robotId: string;
  occurredAt: string;
};

export type WavePlanLine = {
  id: string;
  pickListId?: string;
  reservationId: string;
  sequence: number;
};

export type WavePlan = {
  companyId: string;
  externalId: string;
  id: string;
  lines: WavePlanLine[];
  priority: "normal" | "expedite";
  status: "planned" | "released" | "cancelled";
  version: number;
};

export type SlottingRecommendation = {
  companyId: string;
  externalId: string;
  fromLocationId: string;
  id: string;
  inventoryItemId: string;
  reason: string;
  status: "recommended" | "accepted" | "rejected";
  toLocationId: string;
};

export type WarehouseTransferLine = {
  id: string;
  inventoryItemId: string;
  inventoryOwnerPartyId: string;
  lotId?: string | null;
  quantity: number;
  receivedQuantity: number;
  serialId?: string | null;
  shippedQuantity: number;
};

export type WarehouseTransfer = {
  companyId: string;
  externalId: string;
  fromLocationId: string;
  fromNodeId: string;
  id: string;
  lines: WarehouseTransferLine[];
  status: "draft" | "shipped" | "received" | "cancelled";
  toLocationId: string;
  toNodeId: string;
  version: number;
};

export type OfflineSyncEvent = {
  action: string;
  externalId: string;
  id: string;
  idempotencyKey: string;
  payload: Record<string, unknown>;
};

export type OfflineSyncBatch = {
  companyId: string;
  deviceId: string;
  events: OfflineSyncEvent[];
  externalId: string;
  id: string;
  receivedAt: string;
  status: "accepted" | "rejected";
};

export type ThreePlCharge = {
  amountCents: number;
  companyId: string;
  currency: string;
  externalId: string;
  id: string;
  inventoryOwnerPartyId: string;
  metric: "storage_day" | "pick" | "pack" | "ship" | "return";
  quantity: number;
  sourceId: string;
  sourceType: string;
};

export type WarehouseCommand<TPayload extends Record<string, unknown> = Record<string, unknown>> = {
  companyId: string;
  idempotencyKey: string;
  payload: TPayload;
  refId: string;
  refType: string;
  requestedAt: string;
  requestedBy: string;
  type: WarehouseCommandType;
};

export type WarehouseOutboxEvent = {
  companyId: string;
  id: string;
  payload: Record<string, unknown>;
  status: "pending" | "delivered" | "failed";
  topic: string;
};

export type WarehouseState = {
  balances: StockBalance[];
  commandLog: WarehouseCommand[];
  cycleCounts: CycleCount[];
  integrationEvents: IntegrationEvent[];
  items: InventoryItem[];
  locations: WarehouseLocation[];
  movements: StockMovement[];
  outbox: WarehouseOutboxEvent[];
  fulfillmentLabels: FulfillmentLabel[];
  inventoryAdjustments: InventoryAdjustment[];
  nodes: WarehouseNode[];
  offlineSyncBatches: OfflineSyncBatch[];
  pickLists: PickList[];
  policies: WarehousePolicy[];
  putawayTasks: PutawayTask[];
  receipts: Receipt[];
  reservations: StockReservation[];
  returns: ReturnAuthorization[];
  scanEvents: ScanEvent[];
  scannerSessions: ScannerSession[];
  shipments: Shipment[];
  shipmentPackages: ShipmentPackage[];
  shipmentTrackingEvents: ShipmentTrackingEvent[];
  roboticsEvents: RoboticsEvent[];
  slottingRecommendations: SlottingRecommendation[];
  threePlCharges: ThreePlCharge[];
  transfers: WarehouseTransfer[];
  wavePlans: WavePlan[];
};
