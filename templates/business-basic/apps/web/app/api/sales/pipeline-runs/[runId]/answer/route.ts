import { loadSalesAutomationRuntime } from "@/lib/sales-automation-server-runtime";
import { NextResponse } from "next/server";

export async function POST(
  request: Request,
  { params }: { params: Promise<{ runId: string }> }
) {
  const { runId } = await params;
  const body = await request.json().catch(() => ({})) as {
    questionId?: string;
    choiceId?: string;
    text?: string;
  };
  if (!body.questionId) {
    return NextResponse.json({ ok: false, error: "missing_question_id" }, { status: 400 });
  }
  const { answerSalesPipelineRunQuestion } = await loadSalesAutomationRuntime();
  const result = await answerSalesPipelineRunQuestion({
    runId,
    questionId: body.questionId,
    choiceId: body.choiceId,
    text: body.text
  });

  return NextResponse.json(result);
}
