import { NextRequest, NextResponse } from "next/server";
import { getMarketingResource } from "../../../../lib/marketing-seed";
import { queueMarketingMutation, type MarketingMutationRequest } from "../../../../lib/marketing-runtime";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const data = await getMarketingResource(resource);

  if (!data) {
    return NextResponse.json({ ok: false, error: "Unknown marketing resource" }, { status: 404 });
  }

  return NextResponse.json({ ok: true, data });
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  if (!await getMarketingResource(resource)) {
    return NextResponse.json({ ok: false, error: "Unknown marketing resource" }, { status: 404 });
  }

  const body = await request.json().catch(() => ({})) as Partial<MarketingMutationRequest>;
  const result = await queueMarketingMutation({
    action: body.action ?? "sync",
    resource,
    recordId: body.recordId,
    title: body.title,
    instruction: body.instruction,
    payload: body.payload,
    source: body.source,
    locale: body.locale,
    theme: body.theme
  }, request.nextUrl.origin);

  return NextResponse.json(result);
}
