import { NextResponse } from "next/server";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { getBusinessBundle } from "@/lib/business-seed";
import {
  accountingStoryWorkflows,
  buildStoryWorkflowExecution,
  storyWorkflowsForSubmodule
} from "@/lib/accounting-story-workflows";

type StoryWorkflowRequest = {
  locale?: string;
  storyId?: string;
};

export async function GET(request: Request) {
  const url = new URL(request.url);
  const submodule = url.searchParams.get("submodule");
  const workflows = submodule ? storyWorkflowsForSubmodule(submodule) : accountingStoryWorkflows;

  return NextResponse.json({
    ok: true,
    count: workflows.length,
    workflows
  });
}

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as StoryWorkflowRequest;
  const storyId = body.storyId;
  const locale = body.locale === "en" ? "en" : "de";

  if (!storyId) {
    return NextResponse.json({ error: "story_id_required" }, { status: 400 });
  }

  try {
    const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
    const workflow = buildStoryWorkflowExecution({ data, locale, storyId });
    const persisted = Boolean(process.env.DATABASE_URL);

    if (process.env.DATABASE_URL) {
      await saveAccountingWorkflowSnapshot({
        audit: workflow.audit,
        journalDraft: workflow.journalDraft,
        outbox: workflow.outbox,
        proposal: workflow.proposal
      });
    }

    return NextResponse.json({
      ok: true,
      persisted,
      workflow
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return NextResponse.json({ error: message }, { status: message.startsWith("unknown_story_workflow") ? 404 : 500 });
  }
}
