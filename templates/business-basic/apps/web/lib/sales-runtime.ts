import { businessDeepLink } from "@ctox-business/ui";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";

export type SalesMutationRequest = {
  action: "create" | "update" | "delete" | "sync" | "convert";
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
  accounts: "leads",
  campaigns: "campaigns",
  contacts: "contacts",
  customers: "customers",
  leads: "leads",
  onboarding_projects: "customers",
  offers: "offers",
  opportunities: "pipeline",
  pipeline: "pipeline",
  sales_activity: "leads",
  tasks: "tasks"
};

const resourceToPanel: Record<string, string> = {
  accounts: "account",
  campaigns: "campaign",
  contacts: "contact",
  customers: "customer",
  leads: "lead",
  onboarding_projects: "customer",
  offers: "offer",
  opportunities: "opportunity",
  pipeline: "opportunity",
  sales_activity: "account",
  tasks: "task"
};

export async function queueSalesMutation(request: SalesMutationRequest, origin?: string) {
  const submodule = resourceToSubmodule[request.resource] ?? "pipeline";
  const recordId = request.recordId ?? `${request.action}-${request.resource}-${crypto.randomUUID()}`;
  const panel = resourceToPanel[request.resource] ?? "record";
  const link = businessDeepLink({
    baseUrl: origin,
    module: "sales",
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
    moduleId: "sales",
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
    source: request.source ?? "sales-api",
    context,
    priority: request.action === "delete" ? "high" : "normal",
    skill: "product_engineering/business-stack",
    threadKey: `business/sales/${submodule}`
  });

  const event = await emitCtoxCoreEvent({
    type: `sales.${request.resource}.${request.action}`,
    module: "sales",
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

function defaultInstruction(request: SalesMutationRequest, deepLink: string | null) {
  return [
    `${capitalize(request.action)} sales ${request.resource}${request.recordId ? ` record ${request.recordId}` : ""}.`,
    request.payload ? `Payload: ${JSON.stringify(request.payload, null, 2)}` : null,
    deepLink ? `Business OS deep link: ${deepLink}` : null,
    "Keep Sales synchronized with CTOX core, right-click prompts, CRM records, Operations handoffs, Marketing signals, and Business reporting."
  ].filter(Boolean).join("\n\n");
}

function capitalize(value: string) {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}
