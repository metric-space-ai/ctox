import { sql, type SQLWrapper } from "drizzle-orm";

const PLACEHOLDER_DATABASE = "user:password@localhost";

export async function loadRuntimeJsonStore<T>(storeKey: string): Promise<T | null> {
  if (!canUseRuntimePostgres()) return null;

  const { createBusinessDb } = await import("@ctox-business/db");
  const db = createBusinessDb();
  await ensureBusinessRuntimeStoresTable(db);
  const rows = await db.execute(sql`
    SELECT payload_json
    FROM business_runtime_stores
    WHERE store_key = ${storeKey}
    LIMIT 1
  `);
  const payload = sqlRows(rows)[0]?.payload_json;
  if (typeof payload !== "string") return null;
  return JSON.parse(payload) as T;
}

export async function saveRuntimeJsonStore(storeKey: string, payload: unknown): Promise<boolean> {
  if (!canUseRuntimePostgres()) return false;

  const { createBusinessDb } = await import("@ctox-business/db");
  const db = createBusinessDb();
  await ensureBusinessRuntimeStoresTable(db);
  await db.execute(sql`
    INSERT INTO business_runtime_stores (store_key, payload_json, updated_at)
    VALUES (${storeKey}, ${JSON.stringify(payload)}, now())
    ON CONFLICT (store_key)
    DO UPDATE SET payload_json = EXCLUDED.payload_json, updated_at = now()
  `);
  return true;
}

async function ensureBusinessRuntimeStoresTable(db: { execute: (query: string | SQLWrapper) => Promise<unknown> | unknown }) {
  await db.execute(sql`
    CREATE TABLE IF NOT EXISTS business_runtime_stores (
      store_key text PRIMARY KEY,
      payload_json text NOT NULL DEFAULT '{}',
      created_at timestamptz NOT NULL DEFAULT now(),
      updated_at timestamptz NOT NULL DEFAULT now()
    )
  `);
}

function canUseRuntimePostgres() {
  const databaseUrl = process.env.DATABASE_URL;
  return Boolean(databaseUrl && !databaseUrl.includes(PLACEHOLDER_DATABASE));
}

function sqlRows(result: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(result)) return result as Array<Record<string, unknown>>;
  if (result && typeof result === "object" && "rows" in result && Array.isArray((result as { rows: unknown }).rows)) {
    return (result as { rows: Array<Record<string, unknown>> }).rows;
  }
  return [];
}
