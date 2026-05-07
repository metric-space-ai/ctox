CREATE TABLE "scanner_sessions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"device_id" text NOT NULL,
	"user_id" text NOT NULL,
	"location_external_id" text,
	"status" text NOT NULL,
	"scan_count" integer DEFAULT 0 NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"started_at" timestamp with time zone DEFAULT now() NOT NULL,
	"ended_at" timestamp with time zone,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "scanner_sessions_external_id_unique" ON "scanner_sessions" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "scan_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"session_external_id" text NOT NULL,
	"idempotency_key" text NOT NULL,
	"action" text NOT NULL,
	"barcode" text NOT NULL,
	"inventory_item_external_id" text,
	"location_external_id" text,
	"lot_id" text,
	"serial_id" text,
	"quantity" integer DEFAULT 1 NOT NULL,
	"occurred_at" timestamp with time zone NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "scan_events_external_id_unique" ON "scan_events" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "scan_events_company_idempotency_unique" ON "scan_events" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "cycle_counts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"location_external_id" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"opened_at" timestamp with time zone DEFAULT now() NOT NULL,
	"closed_at" timestamp with time zone,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "cycle_counts_external_id_unique" ON "cycle_counts" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "cycle_count_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"cycle_count_external_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"location_external_id" text NOT NULL,
	"stock_status" text NOT NULL,
	"lot_id" text,
	"serial_id" text,
	"expected_quantity" integer NOT NULL,
	"counted_quantity" integer,
	"variance_quantity" integer,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "cycle_count_lines_external_id_unique" ON "cycle_count_lines" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "inventory_adjustments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"cycle_count_external_id" text,
	"cycle_count_line_external_id" text NOT NULL,
	"stock_movement_external_id" text NOT NULL,
	"reason" text NOT NULL,
	"quantity" integer NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "inventory_adjustments_external_id_unique" ON "inventory_adjustments" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "shipment_packages" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"shipment_external_id" text NOT NULL,
	"carrier" text,
	"tracking_number" text,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "shipment_packages_external_id_unique" ON "shipment_packages" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "fulfillment_labels" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"package_external_id" text NOT NULL,
	"provider" text NOT NULL,
	"carrier" text NOT NULL,
	"tracking_number" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "fulfillment_labels_external_id_unique" ON "fulfillment_labels" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "shipment_tracking_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"shipment_external_id" text NOT NULL,
	"tracking_number" text NOT NULL,
	"carrier" text NOT NULL,
	"event_code" text NOT NULL,
	"event_time" timestamp with time zone NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "shipment_tracking_events_external_id_unique" ON "shipment_tracking_events" USING btree ("external_id");
