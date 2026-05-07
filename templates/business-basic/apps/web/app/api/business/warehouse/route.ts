import { NextResponse } from "next/server";
import { queueBusinessMutation, type BusinessMutationRequest } from "@/lib/business-runtime";
import { executeWarehouseMutation, getWarehouseSnapshot, type WarehouseMutationAction } from "@/lib/warehouse-runtime";

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
    warehouseAction?: WarehouseMutationAction;
  };
  const url = new URL(request.url);
  const warehouseAction = parseWarehouseAction(body.warehouseAction ?? body.action);
  if (warehouseAction) {
    try {
      const result = await executeWarehouseMutation(warehouseAction);
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

function parseAction(value: unknown): BusinessMutationRequest["action"] {
  if (value === "create" || value === "update" || value === "delete" || value === "sync" || value === "export" || value === "payment" || value === "send" || value === "post" || value === "match") return value;
  return "sync";
}
