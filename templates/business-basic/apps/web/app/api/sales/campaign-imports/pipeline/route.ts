import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { NextResponse } from "next/server";

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as {
    campaignId?: string;
    rowId?: string;
    force?: boolean;
  };
  const { transferReadySalesCampaignRowsToPipeline } = await loadSalesAutomationRuntime();
  const result = await transferReadySalesCampaignRowsToPipeline({
    campaignId: body.campaignId,
    rowId: body.rowId,
    force: body.force
  });

  return NextResponse.json(result);
}
