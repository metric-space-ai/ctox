CREATE TABLE "business_bookkeeping_exports" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_bookkeeping_exports_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_customers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_customers_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_invoices" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_invoices_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_products" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_products_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_reports" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_reports_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "marketing_assets" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_assets_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "marketing_campaigns" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_campaigns_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "marketing_commerce_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_commerce_items_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "marketing_research_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_research_items_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "marketing_website_pages" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_website_pages_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_accounts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_accounts_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_contacts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_contacts_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_leads" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_leads_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_opportunities" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_opportunities_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "sales_tasks" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"label" text NOT NULL,
	"status" text NOT NULL,
	"owner_id" text,
	"payload_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "sales_tasks_external_id_unique" UNIQUE("external_id")
);
