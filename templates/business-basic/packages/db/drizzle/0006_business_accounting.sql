CREATE TABLE "business_accounts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_accounts_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_bank_transactions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_bank_transactions_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_journal_entries" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_journal_entries_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_receipts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_receipts_external_id_unique" UNIQUE("external_id")
);
