import { NextResponse } from "next/server";
import { queueBusinessMutation, type BusinessMutationRequest } from "@/lib/business-runtime";
import {
  executeWarehouseLayoutMutation,
  executeWarehouseMutation,
  executeWarehouseWorkStepMutation,
  getWarehouseSnapshot,
  type WarehouseLayoutAction,
  type WarehouseMutationAction,
  type WarehouseWorkStep
} from "@/lib/warehouse-runtime";

export async function GET(request: Request) {
  const warehouse = await getWarehouseSnapshot();
  const url = new URL(request.url);
  if (url.searchParams.get("format") === "csv") {
    const rows = [
      ["warehouse_location", "item", "owner", "status", "quantity"],
      ...warehouse.snapshot.balances.map((balance) => [
        balance.locationId,
        balance.inventoryItemId,
        balance.inventoryOwnerPartyId,
        balance.stockStatus,
        String(balance.quantity)
      ])
    ];
    return new NextResponse(rows.map((row) => row.map(csvCell).join(",")).join("\n"), {
      headers: {
        "content-disposition": `attachment; filename=\"warehouse-report-${new Date().toISOString().slice(0, 10)}.csv\"`,
        "content-type": "text/csv; charset=utf-8"
      }
    });
  }
  return NextResponse.json({
    ok: true,
    resource: "warehouse",
    ...warehouse
  });
}

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as Partial<BusinessMutationRequest> & {
    balanceKey?: string;
    countedQuantities?: Record<string, number>;
    countId?: string;
    adjustedQuantity?: number;
    damagedQuantity?: number;
    expectedQuantity?: number;
    inventoryItemId?: string;
    inventoryOwnerPartyId?: string;
    itemName?: string;
    itemSku?: string;
    itemTrackingMode?: "none" | "lot" | "serial";
    itemUom?: string;
    layoutAction?: WarehouseLayoutAction;
    lineId?: string;
    locationAisle?: string;
    locationBay?: string;
    locationCapacityUnits?: number;
    locationLevel?: string;
    locationName?: string;
    locationPositionNote?: string;
    locationSlotType?: "standard" | "pick_face" | "bulk" | "staging" | "quarantine" | "returns";
    lotId?: string;
    packageId?: string;
    parentId?: string;
    putawayTaskId?: string;
    quantity?: number;
    reasonCode?: string;
    receiptDisposition?: "quarantine" | "damaged";
    reservationId?: string;
    scanBarcode?: string;
    scannerDeviceId?: string;
    serialId?: string;
    shipmentId?: string;
    slotCount?: number;
    sourceId?: string;
    stockStatusTo?: "available" | "receiving" | "reserved" | "picked" | "packed" | "in_transit" | "shipped" | "quarantine" | "damaged";
    targetLocationId?: string;
    transferId?: string;
    warehouseAction?: WarehouseMutationAction;
    workStep?: WarehouseWorkStep;
  };
  const url = new URL(request.url);
  const layoutAction = parseLayoutAction(body.layoutAction);
  if (layoutAction) {
    try {
      const result = await executeWarehouseLayoutMutation({
        action: layoutAction,
        adjustedQuantity: body.adjustedQuantity,
        balanceKey: body.balanceKey,
        countedQuantities: body.countedQuantities,
        countId: body.countId,
        damagedQuantity: body.damagedQuantity,
        expectedQuantity: body.expectedQuantity,
        inventoryItemId: body.inventoryItemId,
        inventoryOwnerPartyId: body.inventoryOwnerPartyId,
        itemName: body.itemName,
        itemSku: body.itemSku,
        itemTrackingMode: body.itemTrackingMode,
        itemUom: body.itemUom,
        packageId: body.packageId,
        locationAisle: body.locationAisle,
        locationBay: body.locationBay,
        locationCapacityUnits: body.locationCapacityUnits,
        locationLevel: body.locationLevel,
        lotId: body.lotId,
        locationName: body.locationName,
        locationPositionNote: body.locationPositionNote,
        locationSlotType: body.locationSlotType,
        parentId: body.parentId,
        putawayTaskId: body.putawayTaskId,
        quantity: body.quantity,
        reasonCode: body.reasonCode,
        reservationId: body.reservationId,
        receiptDisposition: body.receiptDisposition,
        scanBarcode: body.scanBarcode,
        scannerDeviceId: body.scannerDeviceId,
        serialId: body.serialId,
        shipmentId: body.shipmentId,
        slotCount: body.slotCount,
        sourceId: body.sourceId,
        stockStatusTo: body.stockStatusTo,
        targetLocationId: body.targetLocationId,
        transferId: body.transferId
      });
      return NextResponse.json({
        ok: true,
        resource: "warehouse",
        ...result
      });
    } catch (error) {
      return NextResponse.json({
        ok: false,
        error: error instanceof Error ? error.message : String(error),
        resource: "warehouse"
      }, { status: 400 });
    }
  }

  const warehouseAction = parseWarehouseAction(body.warehouseAction ?? body.action);
  if (warehouseAction) {
    try {
      const result = await executeWarehouseMutation(warehouseAction, typeof body.reservationId === "string" ? body.reservationId : undefined);
      return NextResponse.json({
        ok: true,
        resource: "warehouse",
        ...result
      });
    } catch (error) {
      return NextResponse.json({
        ok: false,
        error: error instanceof Error ? error.message : String(error),
        resource: "warehouse"
      }, { status: 400 });
    }
  }

  const workStep = parseWorkStep(body.workStep);
  if (workStep && typeof body.reservationId === "string") {
    try {
      const result = await executeWarehouseWorkStepMutation({
        lineId: typeof body.lineId === "string" ? body.lineId : undefined,
        reservationId: body.reservationId,
        step: workStep
      });
      return NextResponse.json({
        ok: true,
        resource: "warehouse",
        ...result
      });
    } catch (error) {
      return NextResponse.json({
        ok: false,
        error: error instanceof Error ? error.message : String(error),
        resource: "warehouse"
      }, { status: 400 });
    }
  }

  const result = await queueBusinessMutation({
    action: parseAction(body.action),
    instruction: body.instruction ?? "Review the Business warehouse command context.",
    payload: body.payload ?? { warehouse: (await getWarehouseSnapshot()).summary },
    recordId: body.recordId ?? "warehouse-replay",
    resource: "warehouse",
    source: body.source ?? "business-warehouse-api",
    title: body.title ?? "Warehouse review"
  }, url.origin);

  return NextResponse.json(result, { status: result.ok ? 200 : 400 });
}

function parseWarehouseAction(value: unknown): WarehouseMutationAction | null {
  if (value === "reserve" || value === "release" || value === "cancel" || value === "pick" || value === "ship") return value;
  return null;
}

function parseLayoutAction(value: unknown): WarehouseLayoutAction | null {
  if (
    value === "createWarehouse" ||
    value === "createSection" ||
    value === "createSlot" ||
    value === "createItem" ||
    value === "createPickList" ||
    value === "createInterWarehouseTransfer" ||
    value === "createShipmentLabel" ||
    value === "adjustBalance" ||
    value === "authorizeReturn" ||
    value === "changeStockStatus" ||
    value === "deactivateItem" ||
    value === "duplicateItem" ||
    value === "duplicateLocation" ||
    value === "closeCycleCount" ||
    value === "completePutaway" ||
    value === "moveStock" ||
    value === "openCycleCount" ||
    value === "packShipment" ||
    value === "planSlotting" ||
    value === "planWave" ||
    value === "receiveInbound" ||
    value === "receiveInterWarehouseTransfer" ||
    value === "receiveReturn" ||
    value === "recordCarrierHandover" ||
    value === "recordImportDryRun" ||
    value === "recordOpsHandover" ||
    value === "recordRoleReview" ||
    value === "recordSyncConflict" ||
    value === "recordThreePlCharge" ||
    value === "reserveBalance" ||
    value === "resolveQualityHold" ||
    value === "recordCycleCount" ||
    value === "renameItem" ||
    value === "scrapQualityHold" ||
    value === "scanPick" ||
    value === "scanPutaway" ||
    value === "shipInterWarehouseTransfer" ||
    value === "renameLocation" ||
    value === "toggleLocationActive" ||
    value === "toggleLocationPickable"
  ) return value;
  return null;
}

function parseWorkStep(value: unknown): WarehouseWorkStep | null {
  if (value === "build" || value === "qa" || value === "pack") return value;
  return null;
}

function parseAction(value: unknown): BusinessMutationRequest["action"] {
  if (value === "create" || value === "update" || value === "delete" || value === "sync" || value === "export" || value === "payment" || value === "send" || value === "post" || value === "match") return value;
  return "sync";
}

function csvCell(value: string) {
  return /[",\n]/.test(value) ? `"${value.replaceAll("\"", "\"\"")}"` : value;
}
