import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type CtoxCoreTaskRequest = {
  title: string;
  prompt: string;
  source: string;
  context?: Record<string, unknown>;
  priority?: "urgent" | "high" | "normal" | "low";
  skill?: string;
  threadKey?: string;
  workspaceRoot?: string;
};

export type CtoxCoreEventRequest = {
  type: string;
  module: string;
  recordType: string;
  recordId: string;
  payload: Record<string, unknown>;
};

export async function createCtoxCoreTask(request: CtoxCoreTaskRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/queue-tasks`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify({
        instruction: request.prompt,
        context: {
          source: request.source,
          title: request.title,
          priority: request.priority ?? "normal",
          skill: request.skill,
          threadKey: request.threadKey,
          workspaceRoot: request.workspaceRoot,
          ...request.context
        }
      })
    });

    if (response.ok) {
      const payload = await response.json().catch(() => null) as { task?: { id?: string; message_key?: string } } | null;
      return {
        ok: true,
        mode: "bridge_http",
        task: payload?.task ?? null,
        taskId: payload?.task?.id ?? payload?.task?.message_key ?? null
      };
    }
  }

  const command = buildQueueAddCommand(request);
  if (shouldExecuteCoreQueue()) {
    const [binary, ...args] = command;
    try {
      const { stdout } = await execFileAsync(binary, args, { cwd: process.env.CTOX_ROOT });
      const payload = JSON.parse(stdout) as { task?: { message_key?: string; title?: string } };
      return {
        ok: true,
        mode: "ctox_cli",
        task: payload.task ?? null,
        taskId: payload.task?.message_key ?? null
      };
    } catch (error) {
      return plannedTask(request, command, error instanceof Error ? error.message : String(error));
    }
  }

  return plannedTask(request, command);
}

export async function emitCtoxCoreEvent(request: CtoxCoreEventRequest) {
  const bridgeUrl = process.env.CTOX_BRIDGE_URL?.replace(/\/$/, "");
  const token = process.env.CTOX_BRIDGE_TOKEN;
  const event = {
    id: crypto.randomUUID(),
    type: request.type,
    module: request.module,
    recordType: request.recordType,
    recordId: request.recordId,
    occurredAt: new Date().toISOString(),
    payload: request.payload
  };

  if (bridgeUrl) {
    const response = await fetch(`${bridgeUrl}/events`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {})
      },
      body: JSON.stringify(event)
    });

    if (response.ok) return { ok: true, mode: "bridge_http", event };
  }

  return { ok: true, mode: "local_event", event };
}

function buildQueueAddCommand(request: CtoxCoreTaskRequest) {
  const prompt = [
    request.prompt,
    "",
    "Business stack context:",
    JSON.stringify(request.context ?? {}, null, 2)
  ].join("\n");

  const command = [
    resolveCtoxBinary(),
    "queue",
    "add",
    "--title",
    request.title,
    "--prompt",
    prompt,
    "--thread-key",
    request.threadKey ?? `business/${request.source}`,
    "--priority",
    request.priority ?? "normal"
  ];

  if (request.skill) command.push("--skill", request.skill);
  if (request.workspaceRoot) command.push("--workspace-root", request.workspaceRoot);

  return command;
}

function resolveCtoxBinary() {
  return process.env.CTOX_BIN ?? process.env.CTOX_WEB_BIN ?? "ctox";
}

function shouldExecuteCoreQueue() {
  return process.env.CTOX_BUSINESS_QUEUE_MODE !== "planned";
}

function plannedTask(request: CtoxCoreTaskRequest, command: string[], error?: string) {
  return {
    ok: true,
    mode: "planned",
    task: {
      id: crypto.randomUUID(),
      title: request.title,
      status: "queued",
      source: request.source
    },
    taskId: null,
    command,
    error
  };
}
