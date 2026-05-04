import { createCtoxCoreTask, emitCtoxCoreEvent } from "@/lib/ctox-core-bridge";
import { getCtoxResource } from "@/lib/ctox-seed";
import { NextResponse } from "next/server";

type QueueTaskRequest = {
  instruction?: string;
  context?: {
    currentUrl?: string;
    source?: string;
    items?: Array<{
      action?: string;
      currentUrl?: string;
      filePath?: string;
      group?: string;
      href?: string;
      moduleId?: string;
      submoduleId?: string;
      recordType?: string;
      recordId?: string;
      label?: string;
      selectedText?: string;
      skillId?: string;
      sourcePath?: string;
    }>;
  };
};

export async function GET() {
  return NextResponse.json({ ok: true, data: await getCtoxResource("queue") });
}

export async function POST(request: Request) {
  const body = await request.json() as QueueTaskRequest;
  const instruction = body.instruction?.trim();

  if (!instruction) {
    return NextResponse.json(
      { ok: false, error: "instruction_required" },
      { status: 400 }
    );
  }

  const task = {
    id: crypto.randomUUID(),
    type: "ctox.prompt",
    status: "queued",
    instruction,
    context: {
      source: body.context?.source ?? "unknown",
      currentUrl: body.context?.currentUrl,
      items: body.context?.items ?? []
    }
  };

  const firstItem = body.context?.items?.[0];
  const core = await createCtoxCoreTask({
    title: `Business prompt: ${firstItem?.label ?? firstItem?.recordId ?? "selected context"}`,
    prompt: instruction,
    source: body.context?.source ?? "business-ui",
    context: {
      task,
      currentUrl: body.context?.currentUrl,
      moduleId: firstItem?.moduleId,
      submoduleId: firstItem?.submoduleId,
      items: body.context?.items ?? []
    },
    priority: "normal",
    skill: "product_engineering/business-stack",
    threadKey: ["business", firstItem?.moduleId, firstItem?.submoduleId].filter(Boolean).join("/")
  });

  await emitCtoxCoreEvent({
    type: "business.prompt_queued",
    module: firstItem?.moduleId ?? "ctox",
    recordType: firstItem?.recordType ?? "prompt",
    recordId: firstItem?.recordId ?? task.id,
    payload: { task, core }
  });

  return NextResponse.json({ ok: true, queued: true, task: { ...task, coreTaskId: core.taskId }, core });
}
