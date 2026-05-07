import { businessDeepLink } from "@ctox-business/ui";
import { assertPeriodOpen } from "@ctox-business/accounting";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";
import { getBusinessBundle, normalizeBusinessResource } from "./business-seed";
import { getDatabaseBackedBusinessBundle } from "./business-db-bundle";
import { buildFixedAssetRegister } from "./accounting-runtime";
import {
  prepareAssetDepreciationForAccounting,
  prepareAssetDisposalForAccounting,
  prepareBankMatchForAccounting,
  prepareDatevExportForAccounting,
  prepareExistingInvoiceForAccounting,
  prepareReceiptCapitalizationForAccounting,
  prepareReceiptForAccounting
} from "./business-accounting";

export type BusinessMutationRequest = {
  action: "capitalize" | "create" | "delete" | "depreciate" | "dispose" | "export" | "match" | "payment" | "post" | "send" | "sync" | "update";
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
  accounts: "ledger",
  "bank-transactions": "payments",
  banking: "payments",
  bookkeeping: "bookkeeping",
  customers: "customers",
  exports: "bookkeeping",
  "fixed-assets": "fixed-assets",
  invoices: "invoices",
  inventory: "warehouse",
  journal: "ledger",
  ledger: "ledger",
  payments: "payments",
  products: "products",
  receipts: "receipts",
  reports: "reports",
  services: "products",
  stock: "warehouse",
  warehouse: "warehouse"
};

const resourceToPanel: Record<string, string> = {
  accounts: "account",
  "bank-transactions": "bank-transaction",
  banking: "bank-transaction",
  bookkeeping: "export",
  customers: "customer",
  exports: "export",
  "fixed-assets": "asset",
  invoices: "invoice",
  inventory: "stock_balance",
  journal: "journal-entry",
  ledger: "journal-entry",
  payments: "bank-transaction",
  products: "product",
  receipts: "receipt",
  reports: "report",
  services: "product",
  stock: "stock_balance",
  warehouse: "warehouse_set"
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
  const accounting = await buildAccountingContext(request, normalizedResource, recordId);
  const accountingPersistence = accounting ? await persistAccountingContext(accounting) : undefined;
  const prompt = request.instruction ?? defaultInstruction(request, link?.url ?? link?.href ?? null, accounting);
  const context = {
    moduleId: "business",
    submoduleId: submodule,
    recordType: normalizedResource,
    recordId,
    action: request.action,
    payload: request.payload ?? {},
    accounting,
    accountingPersistence,
    deepLink: link
  };

  const core = await withTimeout(createCtoxCoreTask({
    title,
    prompt,
    source: request.source ?? "business-api",
    context,
    priority: request.action === "delete" || request.action === "export" || request.action === "send" ? "high" : "normal",
    skill: "product_engineering/business-stack",
    threadKey: `business/business/${submodule}`
  }), 5000, () => ({
    ok: true,
    mode: "planned_timeout",
    task: {
      id: crypto.randomUUID(),
      source: request.source ?? "business-api",
      status: "queued",
      title
    },
    taskId: null
  }));

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
    accounting,
    accountingPersistence,
    core,
    event
  };
}

async function buildAccountingContext(request: BusinessMutationRequest, normalizedResource: string, recordId: string) {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const locale = request.locale === "en" ? "en" : "de";

  if (normalizedResource === "invoices" && (request.action === "send" || request.action === "export" || request.action === "sync")) {
    const invoice = data.invoices.find((item) => item.id === recordId);
    if (!invoice) return undefined;

    const preview = prepareExistingInvoiceForAccounting({
      data,
      invoice,
      locale
    });

    return withPeriodValidation(data, {
      audit: preview.audit,
      command: preview.command,
      document: preview.document,
      invoiceProjection: preview.invoiceProjection,
      journalDraft: preview.journalDraft,
      outbox: preview.outbox,
      proposal: preview.proposal,
      validation: preview.validation,
      zugferdXml: preview.zugferdXml
    });
  }

  if (normalizedResource === "receipts" && request.action === "post") {
    const receipt = data.receipts.find((item) => item.id === recordId);
    if (!receipt) return undefined;
    return withPeriodValidation(data, prepareReceiptForAccounting({ receipt }));
  }

  if (normalizedResource === "receipts" && request.action === "capitalize") {
    const receipt = data.receipts.find((item) => item.id === recordId);
    if (!receipt) return undefined;
    return withPeriodValidation(data, prepareReceiptCapitalizationForAccounting({ receipt }));
  }

  if (normalizedResource === "fixedAssets" && request.action === "dispose") {
    const register = buildFixedAssetRegister(data);
    const asset = register.find((item) => item.id === recordId);
    if (!asset) return undefined;
    return withPeriodValidation(data, prepareAssetDisposalForAccounting({ asset }));
  }

  if (normalizedResource === "fixedAssets" && (request.action === "depreciate" || request.action === "post")) {
    const asset = data.fixedAssets.find((item) => item.id === recordId);
    if (!asset) return undefined;
    return withPeriodValidation(data, prepareAssetDepreciationForAccounting({ asset }));
  }

  if ((normalizedResource === "bankTransactions" || normalizedResource === "payments") && request.action === "match") {
    const transaction = data.bankTransactions.find((item) => item.id === recordId);
    if (!transaction) return undefined;
    return withPeriodValidation(data, prepareBankMatchForAccounting({ transaction }));
  }

  if ((normalizedResource === "bookkeeping" || normalizedResource === "exports") && request.action === "export") {
    const exportBatch = data.bookkeeping.find((item) => item.id === recordId) ?? data.bookkeeping[0];
    if (!exportBatch) return undefined;
    return prepareDatevExportForAccounting({ data, exportBatch });
  }

  return undefined;
}

function withPeriodValidation<T extends {
  audit?: unknown;
  journalDraft?: { postingDate: string } | null;
  outbox?: { payload?: unknown };
  validation?: { errors: string[]; warnings: string[] };
}>(data: Awaited<ReturnType<typeof getDatabaseBackedBusinessBundle>>, preview: T): T {
  if (!preview.journalDraft) return preview;

  try {
    assertPeriodOpen(data.fiscalPeriods.map((period) => ({
      closedAt: period.closedAt,
      endDate: period.endDate,
      id: period.id,
      startDate: period.startDate,
      status: period.status
    })), preview.journalDraft.postingDate);
    return preview;
  } catch (error) {
    const code = error instanceof Error ? error.message : "fiscal_period_validation_failed";
    return {
      ...preview,
      journalDraft: null,
      validation: {
        errors: [...(preview.validation?.errors ?? []), code],
        warnings: preview.validation?.warnings ?? []
      }
    };
  }
}

async function persistAccountingContext(accounting: NonNullable<Awaited<ReturnType<typeof buildAccountingContext>>>) {
  if (!process.env.DATABASE_URL) {
    return { persisted: false, reason: "DATABASE_URL not configured" };
  }

  try {
    await saveAccountingWorkflowSnapshot({
      audit: accounting.audit,
      invoice: "invoiceProjection" in accounting ? accounting.invoiceProjection : undefined,
      journalDraft: "journalDraft" in accounting ? accounting.journalDraft : undefined,
      outbox: accounting.outbox,
      payment: "paymentProjection" in accounting ? accounting.paymentProjection : undefined,
      proposal: accounting.proposal,
      receipt: "receiptProjection" in accounting ? accounting.receiptProjection : undefined
    });
    return { persisted: true };
  } catch (error) {
    return {
      error: error instanceof Error ? error.message : String(error),
      persisted: false
    };
  }
}

function defaultInstruction(request: BusinessMutationRequest, deepLink: string | null, accounting?: Awaited<ReturnType<typeof buildAccountingContext>>) {
  return [
    `${capitalize(request.action)} Business ${request.resource}${request.recordId ? ` record ${request.recordId}` : ""}.`,
    request.payload ? `Payload: ${JSON.stringify(request.payload, null, 2)}` : null,
    accounting ? `Accounting command: ${JSON.stringify(accounting.command, null, 2)}` : null,
    accounting?.validation?.errors?.length ? `Accounting blockers: ${accounting.validation.errors.join(", ")}` : null,
    accounting?.validation?.warnings?.length ? `Accounting warnings: ${accounting.validation.warnings.join(", ")}` : null,
    accounting?.proposal ? `Accounting proposal: ${JSON.stringify(accounting.proposal, null, 2)}` : null,
    deepLink ? `Business OS deep link: ${deepLink}` : null,
    "Keep ERP records synchronized with CTOX core context, bug reports, right-click prompts, Postgres business data, and the SQLite-held CTOX core queue.",
    "Preserve tax, due-date, export, and revenue-account context when changing Business records."
  ].filter(Boolean).join("\n\n");
}

function capitalize(value: string) {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, fallback: () => T): Promise<T> {
  return new Promise((resolve) => {
    const timeout = setTimeout(() => resolve(fallback()), timeoutMs);
    promise.then((value) => {
      clearTimeout(timeout);
      resolve(value);
    }).catch(() => {
      clearTimeout(timeout);
      resolve(fallback());
    });
  });
}
