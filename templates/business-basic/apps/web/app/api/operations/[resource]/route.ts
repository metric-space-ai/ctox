import { NextResponse } from "next/server";
import { queueOperationsMutation, type OperationsMutationRequest } from "@/lib/operations-runtime";
import { getOperationsResource } from "@/lib/operations-store";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getOperationsResource(resource);

  if (!items) {
    return NextResponse.json({ error: "unknown_operations_resource" }, { status: 404 });
  }

  return NextResponse.json({ resource, items });
}

export async function POST(
  request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getOperationsResource(resource);

  if (!items) {
    return NextResponse.json({ ok: false, error: "unknown_operations_resource" }, { status: 404 });
  }

  const body = await request.json().catch(() => ({})) as Partial<OperationsMutationRequest>;
  const action = parseAction(body.action);
  const url = new URL(request.url);
  const result = await queueOperationsMutation({
    action,
    resource,
    recordId: body.recordId,
    title: body.title,
    instruction: body.instruction,
    payload: body.payload,
    source: body.source ?? `operations-${resource}-api`,
    locale: body.locale,
    theme: body.theme
  }, url.origin);

  return NextResponse.json(result);
}

function parseAction(value: unknown): OperationsMutationRequest["action"] {
  if (value === "create" || value === "update" || value === "delete" || value === "sync" || value === "extract" || value === "reschedule") return value;
  return "update";
}
