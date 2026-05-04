import { NextResponse } from "next/server";
import { queueSalesMutation, type SalesMutationRequest } from "@/lib/sales-runtime";
import { getSalesResource } from "@/lib/sales-seed";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getSalesResource(resource);

  if (!items) {
    return NextResponse.json({ error: "unknown_sales_resource" }, { status: 404 });
  }

  return NextResponse.json({ resource, items });
}

export async function POST(
  request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getSalesResource(resource);

  if (!items) {
    return NextResponse.json({ ok: false, error: "unknown_sales_resource" }, { status: 404 });
  }

  const body = await request.json().catch(() => ({})) as Partial<SalesMutationRequest>;
  const action = parseAction(body.action);
  const url = new URL(request.url);
  const result = await queueSalesMutation({
    action,
    resource,
    recordId: body.recordId,
    title: body.title,
    instruction: body.instruction,
    payload: body.payload,
    source: body.source ?? `sales-${resource}-api`,
    locale: body.locale,
    theme: body.theme
  }, url.origin);

  return NextResponse.json(result);
}

function parseAction(value: unknown): SalesMutationRequest["action"] {
  if (value === "create" || value === "update" || value === "delete" || value === "sync" || value === "convert") return value;
  return "update";
}
