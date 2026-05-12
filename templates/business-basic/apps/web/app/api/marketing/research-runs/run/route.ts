import { NextRequest, NextResponse } from "next/server";
import { runMarketingResearch } from "../../../../../lib/marketing-research-runner";

export const maxDuration = 300;
export const dynamic = "force-dynamic";

export async function POST(request: NextRequest) {
  const body = await request.json().catch(() => ({})) as { runId?: string; amount?: number };
  if (!body.runId) {
    return NextResponse.json({ ok: false, error: "run_id_required" }, { status: 400 });
  }

  const run = await runMarketingResearch(body.runId, body.amount ?? 25);
  return NextResponse.json({ ok: true, run });
}
