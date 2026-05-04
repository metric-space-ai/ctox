import { businessDeepLink } from "@ctox-business/ui";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";

export type OperationsMutationRequest = {
  action: "create" | "update" | "delete" | "sync" | "extract" | "reschedule";
  resource: string;
  recordId?: string;
  title?: string;
  instruction?: string;
  payload?: Record<string, unknown>;
  source?: string;
  locale?: string;
  theme?: string;
};

const resourceToSubmodule: Record<string, string> = {
  "action-items": "meetings",
  decisions: "meetings",
  documents: "knowledge",
  "document-templates": "knowledge",
  knowledge: "knowledge",
  meetings: "meetings",
  milestones: "planning",
  projects: "projects",
  "work-items": "work-items"
};

const resourceToPanel: Record<string, string> = {
  "action-items": "meeting",
  decisions: "meeting",
  documents: "document",
  "document-templates": "document-template",
  knowledge: "knowledge",
  meetings: "meeting",
  milestones: "project",
  projects: "project",
  "work-items": "work-item"
};

export async function queueOperationsMutation(request: OperationsMutationRequest, origin?: string) {
  const submodule = resourceToSubmodule[request.resource] ?? "projects";
  const recordId = request.recordId ?? `${request.action}-${request.resource}-${crypto.randomUUID()}`;
  const panel = resourceToPanel[request.resource] ?? "record";
  const link = businessDeepLink({
    baseUrl: origin,
    module: "operations",
    submodule,
    recordId,
    panel,
    drawer: "right",
    locale: request.locale,
    theme: request.theme
  });
  const title = request.title ?? `${capitalize(request.action)} ${request.resource}`;
  const prompt = request.instruction ?? defaultInstruction(request, link?.url ?? link?.href ?? null);
  const context = {
    moduleId: "operations",
    submoduleId: submodule,
    recordType: request.resource,
    recordId,
    action: request.action,
    payload: request.payload ?? {},
    deepLink: link
  };

  const core = await createCtoxCoreTask({
    title,
    prompt,
    source: request.source ?? "operations-api",
    context,
    priority: request.action === "delete" ? "high" : "normal",
    skill: "product_engineering/business-stack",
    threadKey: `business/operations/${submodule}`
  });

  const event = await emitCtoxCoreEvent({
    type: `operations.${request.resource}.${request.action}`,
    module: "operations",
    recordType: request.resource,
    recordId,
    payload: { ...context, core }
  });

  return {
    ok: true,
    queued: true,
    mutation: {
      id: crypto.randomUUID(),
      status: "queued",
      action: request.action,
      resource: request.resource,
      recordId,
      title,
      deepLink: link
    },
    core,
    event
  };
}

function defaultInstruction(request: OperationsMutationRequest, deepLink: string | null) {
  return [
    `${capitalize(request.action)} operations ${request.resource}${request.recordId ? ` record ${request.recordId}` : ""}.`,
    request.payload ? `Payload: ${JSON.stringify(request.payload, null, 2)}` : null,
    deepLink ? `Business OS deep link: ${deepLink}` : null,
    "Keep the Operations module synchronized with CTOX core, bug reports, prompts, knowledge records, and cross-module business links."
  ].filter(Boolean).join("\n\n");
}

function capitalize(value: string) {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}
