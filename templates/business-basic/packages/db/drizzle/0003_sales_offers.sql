CREATE TABLE "sales_offers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_offers_external_id_unique" UNIQUE("external_id")
);
