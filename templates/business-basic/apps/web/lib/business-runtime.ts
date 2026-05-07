import { businessDeepLink } from "@ctox-business/ui";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { createCtoxCoreTask, emitCtoxCoreEvent } from "./ctox-core-bridge";
import { getBusinessBundle, normalizeBusinessResource } from "./business-seed";
import {
  prepareBankMatchForAccounting,
  prepareDatevExportForAccounting,
  prepareExistingInvoiceForAccounting,
  prepareReceiptForAccounting
} from "./business-accounting";

export type BusinessMutationRequest = {
  action: "create" | "update" | "delete" | "sync" | "export" | "payment" | "send" | "post" | "match";
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
  invoices: "invoices",
  journal: "ledger",
  ledger: "ledger",
  payments: "payments",
  products: "products",
  receipts: "receipts",
  reports: "reports",
  services: "products"
};

const resourceToPanel: Record<string, string> = {
  accounts: "account",
  "bank-transactions": "bank-transaction",
  banking: "bank-transaction",
  bookkeeping: "export",
  customers: "customer",
  exports: "export",
  invoices: "invoice",
  journal: "journal-entry",
  ledger: "journal-entry",
  payments: "bank-transaction",
  products: "product",
  receipts: "receipt",
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
  const data = await getBusinessBundle();
  const locale = request.locale === "en" ? "en" : "de";

  if (normalizedResource === "invoices" && (request.action === "send" || request.action === "export" || request.action === "sync")) {
    const invoice = data.invoices.find((item) => item.id === recordId);
    if (!invoice) return undefined;

    const preview = prepareExistingInvoiceForAccounting({
      data,
      invoice,
      locale
    });

    return {
      audit: preview.audit,
      command: preview.command,
      document: preview.document,
      invoiceProjection: preview.invoiceProjection,
      journalDraft: preview.journalDraft,
      outbox: preview.outbox,
      proposal: preview.proposal,
      validation: preview.validation,
      zugferdXml: preview.zugferdXml
    };
  }

  if (normalizedResource === "receipts" && request.action === "post") {
    const receipt = data.receipts.find((item) => item.id === recordId);
    if (!receipt) return undefined;
    return prepareReceiptForAccounting({ receipt });
  }

  if ((normalizedResource === "bankTransactions" || normalizedResource === "payments") && request.action === "match") {
    const transaction = data.bankTransactions.find((item) => item.id === recordId);
    if (!transaction) return undefined;
    return prepareBankMatchForAccounting({ transaction });
  }

  if ((normalizedResource === "bookkeeping" || normalizedResource === "exports") && request.action === "export") {
    const exportBatch = data.bookkeeping.find((item) => item.id === recordId) ?? data.bookkeeping[0];
    if (!exportBatch) return undefined;
    return prepareDatevExportForAccounting({ data, exportBatch });
  }

  return undefined;
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
