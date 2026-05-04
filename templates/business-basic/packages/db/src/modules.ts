import { createBusinessDb } from "./client";
import {
  businessBookkeepingExports,
  businessCustomers,
  businessInvoices,
  businessProducts,
  businessReports,
  ctoxBugReports,
  marketingAssets,
  marketingCampaigns,
  marketingCommerceItems,
  marketingResearchItems,
  marketingWebsitePages,
  organizations,
  salesAccounts,
  salesCampaigns,
  salesContacts,
  salesCustomers,
  salesLeads,
  salesOffers,
  salesOpportunities,
  salesTasks
} from "./schema";

type ModuleName = "sales" | "marketing" | "business";

type ResourceRecord = {
  id: string;
  label: string;
  status: string;
  ownerId?: string | null;
  payload: unknown;
};

type ModuleRecordRow = {
  externalId: string;
  label: string;
  status: string;
  ownerId: string | null;
  payloadJson: string;
  ctoxSyncKey: string;
};

type CtoxBugReportRecord = {
  id: string;
  title: string;
  moduleId: string;
  submoduleId: string;
  status: string;
  severity: string;
  tags?: string[];
  coreTaskId?: string | null;
};

type CtoxBugReportRow = {
  externalId: string;
  title: string;
  moduleId: string;
  submoduleId: string;
  status: string;
  severity: string;
  tagsJson: string;
  payloadJson: string;
  coreTaskId: string | null;
};

type OrganizationRecord = {
  name: string;
  slug: string;
};

const tableMap = {
  business: {
    bookkeeping: businessBookkeepingExports,
    customers: businessCustomers,
    invoices: businessInvoices,
    products: businessProducts,
    reports: businessReports
  },
  marketing: {
    assets: marketingAssets,
    campaigns: marketingCampaigns,
    commerce: marketingCommerceItems,
    research: marketingResearchItems,
    website: marketingWebsitePages
  },
  sales: {
    accounts: salesAccounts,
    campaigns: salesCampaigns,
    contacts: salesContacts,
    customers: salesCustomers,
    leads: salesLeads,
    offers: salesOffers,
    opportunities: salesOpportunities,
    tasks: salesTasks
  }
} as const;

export async function listModuleRecords(module: ModuleName, resource: string, databaseUrl?: string) {
  const table = resolveTable(module, resource);
  if (!table) return null;
  return createBusinessDb(databaseUrl).select().from(table) as Promise<ModuleRecordRow[]>;
}

export async function listCtoxBugReports(databaseUrl?: string) {
  return createBusinessDb(databaseUrl).select().from(ctoxBugReports) as Promise<CtoxBugReportRow[]>;
}

export async function upsertCtoxBugReport(record: CtoxBugReportRecord, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  const values = {
    externalId: record.id,
    title: record.title,
    moduleId: record.moduleId,
    submoduleId: record.submoduleId,
    status: record.status,
    severity: record.severity,
    tagsJson: JSON.stringify(record.tags ?? []),
    payloadJson: JSON.stringify(record),
    coreTaskId: record.coreTaskId ?? null,
    updatedAt: new Date()
  };

  await db.insert(ctoxBugReports).values(values).onConflictDoUpdate({
    target: ctoxBugReports.externalId,
    set: values
  });
}

export async function upsertOrganization(record: OrganizationRecord, databaseUrl?: string) {
  const db = createBusinessDb(databaseUrl);
  const values = {
    name: record.name,
    slug: record.slug,
    updatedAt: new Date()
  };

  await db.insert(organizations).values(values).onConflictDoUpdate({
    target: organizations.slug,
    set: values
  });
}

export async function seedModuleRecords(
  module: ModuleName,
  recordsByResource: Record<string, ResourceRecord[]>,
  databaseUrl?: string
) {
  const db = createBusinessDb(databaseUrl);

  await db.transaction(async (tx) => {
    for (const [resource, records] of Object.entries(recordsByResource)) {
      const table = resolveTable(module, resource);
      if (!table) continue;

      for (const record of records) {
        const values = {
          externalId: record.id,
          label: record.label,
          status: record.status,
          ownerId: record.ownerId ?? null,
          payloadJson: JSON.stringify(record.payload),
          ctoxSyncKey: `${module}:${resource}:${record.id}`,
          updatedAt: new Date()
        };

        await tx.insert(table).values(values).onConflictDoUpdate({
          target: table.externalId,
          set: values
        });
      }
    }
  });
}

function resolveTable(module: ModuleName, resource: string) {
  if (module === "business") {
    if (resource === "exports") return tableMap.business.bookkeeping;
    if (resource === "services") return tableMap.business.products;
    return tableMap.business[resource as keyof typeof tableMap.business] ?? null;
  }

  if (module === "marketing") {
    return tableMap.marketing[resource as keyof typeof tableMap.marketing] ?? null;
  }

  if (resource === "pipeline") return tableMap.sales.opportunities;
  return tableMap.sales[resource as keyof typeof tableMap.sales] ?? null;
}
