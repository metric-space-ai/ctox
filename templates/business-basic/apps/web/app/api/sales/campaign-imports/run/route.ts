import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { NextResponse } from "next/server";

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as { campaignId?: string; rowId?: string; limit?: number; retryFailed?: boolean; rerunComplete?: boolean; useWebSearch?: boolean };
  const { runSalesResearchJobs } = await loadSalesAutomationRuntime();
  const result = await runSalesResearchJobs({
    campaignId: body.campaignId,
    rowId: body.rowId,
    retryFailed: body.retryFailed,
    rerunComplete: body.rerunComplete,
    useWebSearch: body.useWebSearch,
    limit: body.limit
  });

  return NextResponse.json(result);
}
