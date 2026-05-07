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

export async function GET() {
  const warehouse = await getWarehouseSnapshot();
  return NextResponse.json({
    ok: true,
    resource: "warehouse",
    ...warehouse
  });
}

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as Partial<BusinessMutationRequest> & {
    balanceKey?: string;
    layoutAction?: WarehouseLayoutAction;
    lineId?: string;
    locationName?: string;
    parentId?: string;
    quantity?: number;
    reservationId?: string;
    slotCount?: number;
    targetLocationId?: string;
    warehouseAction?: WarehouseMutationAction;
    workStep?: WarehouseWorkStep;
  };
  const url = new URL(request.url);
  const layoutAction = parseLayoutAction(body.layoutAction);
  if (layoutAction) {
    try {
      const result = await executeWarehouseLayoutMutation({
        action: layoutAction,
        balanceKey: body.balanceKey,
        locationName: body.locationName,
        parentId: body.parentId,
        quantity: body.quantity,
        slotCount: body.slotCount,
        targetLocationId: body.targetLocationId
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
    value === "duplicateLocation" ||
    value === "moveStock" ||
    value === "renameLocation" ||
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
