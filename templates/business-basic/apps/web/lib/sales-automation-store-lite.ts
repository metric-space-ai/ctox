import { sql } from "drizzle-orm";

const SALES_AUTOMATION_STORE_KEY = "sales_automation";

export type LiteSalesAutomationStore = {
  campaigns: unknown[];
  rows: unknown[];
  pipelineRuns: unknown[];
};

export async function loadSalesAutomationStoreLite(): Promise<LiteSalesAutomationStore> {
  const databaseStore = await loadFromDatabase();
  if (databaseStore) return databaseStore;
  return { campaigns: [], rows: [], pipelineRuns: [] };
}

async function loadFromDatabase() {
  if (!process.env.DATABASE_URL) return null;

  try {
    const { createBusinessDb } = await import("@ctox-business/db");
    const db = createBusinessDb();
    await db.execute(sql`
      CREATE TABLE IF NOT EXISTS business_runtime_stores (
        store_key text PRIMARY KEY NOT NULL,
        payload_json text NOT NULL DEFAULT '{}',
        created_at timestamp with time zone NOT NULL DEFAULT now(),
        updated_at timestamp with time zone NOT NULL DEFAULT now()
      )
    `);
    const result = await db.execute(sql`
      SELECT payload_json
      FROM business_runtime_stores
      WHERE store_key = ${SALES_AUTOMATION_STORE_KEY}
      LIMIT 1
    `);
    const payload = sqlRows(result)[0]?.payload_json;
    if (typeof payload !== "string" || !payload) return { campaigns: [], rows: [], pipelineRuns: [] };
    return normalizeStore(JSON.parse(payload));
  } catch {
    return null;
  }
}

function normalizeStore(value: any): LiteSalesAutomationStore {
  return {
    campaigns: Array.isArray(value?.campaigns) ? value.campaigns : [],
    rows: Array.isArray(value?.rows) ? value.rows : [],
    pipelineRuns: Array.isArray(value?.pipelineRuns) ? value.pipelineRuns : []
  };
}

function sqlRows(result: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(result)) return result as Array<Record<string, unknown>>;
  const maybeRows = (result as { rows?: unknown }).rows;
  return Array.isArray(maybeRows) ? maybeRows as Array<Record<string, unknown>> : [];
}
