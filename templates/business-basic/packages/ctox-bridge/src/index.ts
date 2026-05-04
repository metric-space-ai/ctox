export type CtoxBusinessEvent = {
  id: string;
  type: string;
  module: "sales" | "marketing" | "operations" | "business" | "ctox";
  recordType: string;
  recordId: string;
  occurredAt: string;
  payload: Record<string, unknown>;
};

export type CtoxSyncTarget = {
  url: string;
  token?: string;
};

export type CtoxPromptContextItem = {
  moduleId?: string;
  submoduleId?: string;
  recordType?: string;
  recordId?: string;
  label?: string;
};

export type CtoxPromptQueueTask = {
  instruction: string;
  context: {
    source: "context-menu" | "api" | string;
    items: CtoxPromptContextItem[];
  };
};

export async function sendCtoxEvent(target: CtoxSyncTarget, event: CtoxBusinessEvent) {
  const response = await fetch(`${target.url.replace(/\/$/, "")}/events`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(target.token ? { authorization: `Bearer ${target.token}` } : {})
    },
    body: JSON.stringify(event)
  });

  if (!response.ok) {
    throw new Error(`CTOX bridge rejected event ${event.id}: ${response.status}`);
  }

  return response.json() as Promise<{ ok: boolean }>;
}

export async function createCtoxQueueTask(target: CtoxSyncTarget, task: CtoxPromptQueueTask) {
  const response = await fetch(`${target.url.replace(/\/$/, "")}/queue-tasks`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(target.token ? { authorization: `Bearer ${target.token}` } : {})
    },
    body: JSON.stringify(task)
  });

  if (!response.ok) {
    throw new Error(`CTOX queue task rejected: ${response.status}`);
  }

  return response.json() as Promise<{ ok: boolean; queued: boolean; task?: unknown }>;
}
