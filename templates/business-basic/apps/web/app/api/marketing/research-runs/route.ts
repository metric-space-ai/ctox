import { NextRequest, NextResponse } from "next/server";
import { marketingSeed, type ResearchRun } from "../../../../lib/marketing-seed";
import { archiveMarketingResearchRun, getMarketingResearchRuns, upsertMarketingResearchRun } from "../../../../lib/marketing-research-store";

export async function GET() {
  const data = await getMarketingResearchRuns(marketingSeed.researchRuns);
  return NextResponse.json({ ok: true, data });
}

export async function POST(request: NextRequest) {
  const body = await request.json().catch(() => ({})) as { run?: ResearchRun };
  if (!body.run?.id || !body.run.title) {
    return NextResponse.json({ ok: false, error: "Expected a research run with id and title." }, { status: 400 });
  }

  const result = await upsertMarketingResearchRun(body.run, marketingSeed.researchRuns);
  return NextResponse.json({ ok: true, ...result });
}

export async function DELETE(request: NextRequest) {
  const runId = request.nextUrl.searchParams.get("id");
  if (!runId) {
    return NextResponse.json({ ok: false, error: "Expected research run id." }, { status: 400 });
  }

  const result = await archiveMarketingResearchRun(runId, marketingSeed.researchRuns);
  return NextResponse.json({ ok: true, ...result });
}
