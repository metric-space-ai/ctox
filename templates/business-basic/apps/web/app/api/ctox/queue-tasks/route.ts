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
  const syncContext = businessOsCodeSyncContext();
  const core = await createCtoxCoreTask({
    title: `Business prompt: ${firstItem?.label ?? firstItem?.recordId ?? "selected context"}`,
    prompt: instruction,
    source: body.context?.source ?? "business-ui",
    context: {
      task,
      currentUrl: body.context?.currentUrl,
      moduleId: firstItem?.moduleId,
      submoduleId: firstItem?.submoduleId,
      items: body.context?.items ?? [],
      businessOsCodeSync: syncContext
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

function businessOsCodeSyncContext() {
  return {
    app: "Kunstmen Business OS",
    mountedPath: "/business-os",
    canonicalTemplate: "templates/business-basic",
    codeSyncPolicy: [
      "If the requested change fixes or improves reusable Business OS code, implement it in the running Kunstmen Business OS and backport the generic code change to the CTOX Business OS template.",
      "Do not copy tenant data, customer records, screenshots, credentials, database rows, or .ctox-business runtime JSON into the template.",
      "Persistent Business OS data belongs in Postgres. If a module still persists durable state in files, treat that as a bug and migrate it to Postgres-backed storage.",
      "If immediate backport is unsafe, create a tracked follow-up with the changed file paths and exact migration notes."
    ]
  };
}
