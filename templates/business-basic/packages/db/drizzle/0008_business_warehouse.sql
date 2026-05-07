CREATE TABLE "inventory_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"sku" text NOT NULL,
	"name" text NOT NULL,
	"uom" text DEFAULT 'ea' NOT NULL,
	"tracking_mode" text DEFAULT 'none' NOT NULL,
	"inventory_owner_party_id" text,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "inventory_items_external_id_unique" ON "inventory_items" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "inventory_items_company_sku_unique" ON "inventory_items" USING btree ("company_id","sku");
--> statement-breakpoint
CREATE TABLE "warehouse_locations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"name" text NOT NULL,
	"kind" text NOT NULL,
	"parent_external_id" text,
	"default_owner_party_id" text,
	"pickable" integer DEFAULT 0 NOT NULL,
	"receivable" integer DEFAULT 0 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_locations_external_id_unique" ON "warehouse_locations" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "warehouse_policies" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"default_owner_party_id" text NOT NULL,
	"allow_negative_stock" integer DEFAULT 0 NOT NULL,
	"allow_backorder" integer DEFAULT 0 NOT NULL,
	"allocation_strategy" text DEFAULT 'fifo' NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_policies_external_id_unique" ON "warehouse_policies" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "stock_balances" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"company_id" text NOT NULL,
	"balance_key" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"warehouse_location_external_id" text NOT NULL,
	"stock_status" text NOT NULL,
	"lot_id" text,
	"serial_id" text,
	"quantity" integer DEFAULT 0 NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "stock_balances_company_balance_key_unique" ON "stock_balances" USING btree ("company_id","balance_key");
--> statement-breakpoint
CREATE TABLE "stock_movements" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"warehouse_location_external_id" text NOT NULL,
	"movement_type" text NOT NULL,
	"stock_status" text NOT NULL,
	"stock_status_from" text,
	"stock_status_to" text,
	"lot_id" text,
	"serial_id" text,
	"quantity" integer NOT NULL,
	"uom" text DEFAULT 'ea' NOT NULL,
	"source_type" text NOT NULL,
	"source_id" text NOT NULL,
	"source_line_id" text,
	"idempotency_key" text NOT NULL,
	"posted_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "stock_movements_external_id_unique" ON "stock_movements" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "stock_movements_company_idempotency_unique" ON "stock_movements" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "inventory_command_log" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"company_id" text NOT NULL,
	"idempotency_key" text NOT NULL,
	"type" text NOT NULL,
	"ref_type" text NOT NULL,
	"ref_id" text NOT NULL,
	"requested_by" text NOT NULL,
	"payload_json" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "inventory_command_log_company_idempotency_unique" ON "inventory_command_log" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "inventory_audit_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"company_id" text NOT NULL,
	"actor_type" text NOT NULL,
	"actor_id" text NOT NULL,
	"action" text NOT NULL,
	"ref_type" text NOT NULL,
	"ref_id" text NOT NULL,
	"before_json" text,
	"after_json" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "stock_reservations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"source_type" text NOT NULL,
	"source_id" text NOT NULL,
	"status" text NOT NULL,
	"allow_partial_reservation" integer DEFAULT 0 NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "stock_reservations_external_id_unique" ON "stock_reservations" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "stock_reservation_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"reservation_external_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"warehouse_location_external_id" text NOT NULL,
	"lot_id" text,
	"serial_id" text,
	"quantity" integer NOT NULL,
	"picked_quantity" integer DEFAULT 0 NOT NULL,
	"shipped_quantity" integer DEFAULT 0 NOT NULL,
	"released_quantity" integer DEFAULT 0 NOT NULL,
	"allow_backorder" integer DEFAULT 0 NOT NULL,
	"source_line_id" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "stock_reservation_lines_external_id_unique" ON "stock_reservation_lines" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "pick_lists" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"reservation_external_id" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "pick_lists_external_id_unique" ON "pick_lists" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "receipts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"source_type" text NOT NULL,
	"source_id" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "receipts_external_id_unique" ON "receipts" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "putaway_tasks" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"from_location_external_id" text NOT NULL,
	"to_location_external_id" text NOT NULL,
	"receipt_line_external_id" text NOT NULL,
	"quantity" integer NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "putaway_tasks_external_id_unique" ON "putaway_tasks" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "shipments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"reservation_external_id" text NOT NULL,
	"provider" text,
	"carrier" text,
	"tracking_number" text,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "shipments_external_id_unique" ON "shipments" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "return_authorizations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"source_shipment_external_id" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "return_authorizations_external_id_unique" ON "return_authorizations" USING btree ("external_id");
