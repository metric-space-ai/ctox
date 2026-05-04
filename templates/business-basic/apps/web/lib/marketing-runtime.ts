import { businessDeepLink } from "@ctox-business/ui";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";

export type MarketingMutationRequest = {
  action: "create" | "update" | "delete" | "sync" | "publish" | "schedule";
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
  assets: "assets",
  campaigns: "campaigns",
  commerce: "commerce",
  research: "research",
  website: "website"
};

const resourceToPanel: Record<string, string> = {
  assets: "asset",
  campaigns: "campaign",
  commerce: "commerce",
  research: "research",
  website: "page"
};

export async function queueMarketingMutation(request: MarketingMutationRequest, origin?: string) {
  const submodule = resourceToSubmodule[request.resource] ?? "website";
  const recordId = request.recordId ?? `${request.action}-${request.resource}-${crypto.randomUUID()}`;
  const panel = resourceToPanel[request.resource] ?? "record";
  const link = businessDeepLink({
    baseUrl: origin,
    module: "marketing",
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
    moduleId: "marketing",
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
    source: request.source ?? "marketing-api",
    context,
    priority: request.action === "publish" ? "high" : "normal",
    skill: "product_engineering/business-stack",
    threadKey: `business/marketing/${submodule}`
  });

  const event = await emitCtoxCoreEvent({
    type: `marketing.${request.resource}.${request.action}`,
    module: "marketing",
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

function defaultInstruction(request: MarketingMutationRequest, deepLink: string | null) {
  return [
    `${capitalize(request.action)} marketing ${request.resource}${request.recordId ? ` record ${request.recordId}` : ""}.`,
    request.payload ? `Payload: ${JSON.stringify(request.payload, null, 2)}` : null,
    deepLink ? `Business OS deep link: ${deepLink}` : null,
    "Keep website, assets, campaigns, research, commerce, CTOX prompts, bug reports, and cross-module sales/business links synchronized."
  ].filter(Boolean).join("\n\n");
}

function capitalize(value: string) {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}
