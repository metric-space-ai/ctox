import { NextResponse } from "next/server";
import { getBusinessBundle, getBusinessResource } from "@/lib/business-seed";
import { getDatabaseBackedBusinessResource } from "@/lib/business-db-bundle";
import { queueBusinessMutation, type BusinessMutationRequest } from "@/lib/business-runtime";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getDatabaseBackedBusinessResource(resource, await getBusinessBundle());

  if (!items) {
    return NextResponse.json({ error: "unknown_business_resource" }, { status: 404 });
  }

  return NextResponse.json({ resource, items });
}

export async function POST(
  request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const items = await getBusinessResource(resource);

  if (!items) {
    return NextResponse.json({ ok: false, error: "unknown_business_resource" }, { status: 404 });
  }

  const body = await request.json().catch(() => ({})) as Partial<BusinessMutationRequest>;
  const action = parseAction(body.action);
  const url = new URL(request.url);
  const result = await queueBusinessMutation({
    action,
    resource,
    recordId: body.recordId,
    title: body.title,
    instruction: body.instruction,
    payload: body.payload,
    source: body.source ?? `business-${resource}-api`,
    locale: body.locale,
    theme: body.theme
  }, url.origin);

  return NextResponse.json(result, { status: result.ok ? 200 : 400 });
}

function parseAction(value: unknown): BusinessMutationRequest["action"] {
  if (value === "create" || value === "update" || value === "delete" || value === "sync" || value === "export" || value === "payment" || value === "send" || value === "post" || value === "match") return value;
  return "update";
}
