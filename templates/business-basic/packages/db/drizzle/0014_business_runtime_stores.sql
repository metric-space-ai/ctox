CREATE TABLE IF NOT EXISTS "business_runtime_stores" (
  "store_key" text PRIMARY KEY NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
