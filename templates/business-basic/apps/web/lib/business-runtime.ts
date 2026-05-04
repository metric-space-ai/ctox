import { businessDeepLink } from "@ctox-business/ui";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";
import { normalizeBusinessResource } from "./business-seed";

export type BusinessMutationRequest = {
  action: "create" | "update" | "delete" | "sync" | "export" | "payment";
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
  bookkeeping: "bookkeeping",
  customers: "customers",
  exports: "bookkeeping",
  invoices: "invoices",
  products: "products",
  reports: "reports",
  services: "products"
};

const resourceToPanel: Record<string, string> = {
  bookkeeping: "export",
  customers: "customer",
  exports: "export",
  invoices: "invoice",
  products: "product",
  reports: "report",
  services: "product"
};

export async function queueBusinessMutation(request: BusinessMutationRequest, origin?: string) {
  const normalizedResource = normalizeBusinessResource(request.resource);
  if (!normalizedResource) {
    return {
      ok: false,
      error: "unknown_business_resource"
    };
  }

  const submodule = resourceToSubmodule[request.resource] ?? normalizedResource;
  const recordId = request.recordId ?? `${request.action}-${request.resource}-${crypto.randomUUID()}`;
  const panel = resourceToPanel[request.resource] ?? "record";
  const link = businessDeepLink({
    baseUrl: origin,
    module: "business",
    submodule,
    recordId,
    panel,
    drawer: request.action === "create" ? "left-bottom" : "right",
    locale: request.locale,
    theme: request.theme
  });
  const title = request.title ?? `${capitalize(request.action)} ${request.resource}`;
  const prompt = request.instruction ?? defaultInstruction(request, link?.url ?? link?.href ?? null);
  const context = {
    moduleId: "business",
    submoduleId: submodule,
    recordType: normalizedResource,
    recordId,
    action: request.action,
    payload: request.payload ?? {},
    deepLink: link
  };

  const core = await createCtoxCoreTask({
    title,
    prompt,
    source: request.source ?? "business-api",
    context,
    priority: request.action === "delete" || request.action === "export" ? "high" : "normal",
    skill: "product_engineering/business-stack",
    threadKey: `business/business/${submodule}`
  });

  const event = await emitCtoxCoreEvent({
    type: `business.${normalizedResource}.${request.action}`,
    module: "business",
    recordType: normalizedResource,
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
      resource: normalizedResource,
      recordId,
      title,
      deepLink: link
    },
    core,
    event
  };
}

function defaultInstruction(request: BusinessMutationRequest, deepLink: string | null) {
  return [
    `${capitalize(request.action)} Business ${request.resource}${request.recordId ? ` record ${request.recordId}` : ""}.`,
    request.payload ? `Payload: ${JSON.stringify(request.payload, null, 2)}` : null,
    deepLink ? `Business OS deep link: ${deepLink}` : null,
    "Keep ERP records synchronized with CTOX core context, bug reports, right-click prompts, Postgres business data, and the SQLite-held CTOX core queue.",
    "Preserve tax, due-date, export, and revenue-account context when changing Business records."
  ].filter(Boolean).join("\n\n");
}

function capitalize(value: string) {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}
