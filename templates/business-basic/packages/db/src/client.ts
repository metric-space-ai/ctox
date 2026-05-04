import { drizzle } from "drizzle-orm/node-postgres";
import pg from "pg";
import * as schema from "./schema";

let pool: pg.Pool | null = null;

export function createBusinessDb(databaseUrl = process.env.DATABASE_URL) {
  if (!databaseUrl) {
    throw new Error("DATABASE_URL is required for the CTOX business database.");
  }

  pool ??= new pg.Pool({ connectionString: databaseUrl });
  return drizzle(pool, { schema });
}

export async function closeBusinessDb() {
  if (!pool) return;
  await pool.end();
  pool = null;
}
