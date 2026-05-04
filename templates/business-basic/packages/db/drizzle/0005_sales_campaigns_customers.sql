CREATE TABLE "sales_campaigns" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_campaigns_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_customers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_customers_external_id_unique" UNIQUE("external_id")
);
