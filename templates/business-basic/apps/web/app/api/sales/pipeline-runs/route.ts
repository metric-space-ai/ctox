import type { SalesPipelineRunGateId, SalesPipelineRunMode } from "@/lib/sales-automation-runtime";
import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { NextResponse } from "next/server";

export async function GET(request: Request) {
  const url = new URL(request.url);
  const candidateId = url.searchParams.get("candidateId");
  const { loadSalesAutomationStore } = await loadSalesAutomationRuntime();
  const store = await loadSalesAutomationStore();
  const runs = (store.pipelineRuns ?? []).filter((run) => !candidateId || run.candidateId === candidateId);

  return NextResponse.json({ ok: true, runs });
}

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as {
    candidateId?: string;
    candidateIds?: string[];
    mode?: SalesPipelineRunMode;
    gate?: SalesPipelineRunGateId | "next";
  };
  const candidateIds = Array.isArray(body.candidateIds)
    ? body.candidateIds
    : body.candidateId
      ? [body.candidateId]
      : [];
  const { startSalesPipelineRuns } = await loadSalesAutomationRuntime();
  const result = await startSalesPipelineRuns({
    candidateIds,
    mode: body.mode,
    gate: body.gate
  });

  return NextResponse.json(result);
}
