CREATE TABLE "warehouse_nodes" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"name" text NOT NULL,
	"kind" text NOT NULL,
	"status" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_nodes_external_id_unique" ON "warehouse_nodes" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "warehouse_integration_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"source" text NOT NULL,
	"provider" text NOT NULL,
	"event_type" text NOT NULL,
	"idempotency_key" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"received_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_integration_events_external_id_unique" ON "warehouse_integration_events" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_integration_events_company_idempotency_unique" ON "warehouse_integration_events" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "warehouse_robotics_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"robot_id" text NOT NULL,
	"event_type" text NOT NULL,
	"idempotency_key" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"occurred_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_robotics_events_external_id_unique" ON "warehouse_robotics_events" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_robotics_events_company_idempotency_unique" ON "warehouse_robotics_events" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "warehouse_wave_plans" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"priority" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_wave_plans_external_id_unique" ON "warehouse_wave_plans" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "warehouse_wave_plan_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"wave_plan_external_id" text NOT NULL,
	"reservation_external_id" text NOT NULL,
	"pick_list_external_id" text,
	"sequence" integer NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_wave_plan_lines_external_id_unique" ON "warehouse_wave_plan_lines" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "slotting_recommendations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"from_location_external_id" text NOT NULL,
	"to_location_external_id" text NOT NULL,
	"reason" text NOT NULL,
	"status" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "slotting_recommendations_external_id_unique" ON "slotting_recommendations" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "warehouse_transfers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"from_node_external_id" text NOT NULL,
	"to_node_external_id" text NOT NULL,
	"from_location_external_id" text NOT NULL,
	"to_location_external_id" text NOT NULL,
	"status" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_transfers_external_id_unique" ON "warehouse_transfers" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "warehouse_transfer_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"transfer_external_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"inventory_item_external_id" text NOT NULL,
	"lot_id" text,
	"serial_id" text,
	"quantity" integer NOT NULL,
	"shipped_quantity" integer DEFAULT 0 NOT NULL,
	"received_quantity" integer DEFAULT 0 NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "warehouse_transfer_lines_external_id_unique" ON "warehouse_transfer_lines" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "offline_sync_batches" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"device_id" text NOT NULL,
	"status" text NOT NULL,
	"received_at" timestamp with time zone DEFAULT now() NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "offline_sync_batches_external_id_unique" ON "offline_sync_batches" USING btree ("external_id");
--> statement-breakpoint
CREATE TABLE "offline_sync_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"batch_external_id" text NOT NULL,
	"idempotency_key" text NOT NULL,
	"action" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "offline_sync_events_external_id_unique" ON "offline_sync_events" USING btree ("external_id");
--> statement-breakpoint
CREATE UNIQUE INDEX "offline_sync_events_company_idempotency_unique" ON "offline_sync_events" USING btree ("company_id","idempotency_key");
--> statement-breakpoint
CREATE TABLE "three_pl_charges" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"inventory_owner_party_id" text NOT NULL,
	"metric" text NOT NULL,
	"quantity" integer NOT NULL,
	"amount_cents" integer NOT NULL,
	"currency" text NOT NULL,
	"source_type" text NOT NULL,
	"source_id" text NOT NULL,
	"payload_json" text DEFAULT '{}' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "three_pl_charges_external_id_unique" ON "three_pl_charges" USING btree ("external_id");
