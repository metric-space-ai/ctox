CREATE TYPE "public"."business_module" AS ENUM('sales', 'marketing', 'operations', 'business');--> statement-breakpoint
CREATE TABLE "ctox_sync_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"module" "business_module" NOT NULL,
	"event_type" text NOT NULL,
	"record_type" text NOT NULL,
	"record_id" text NOT NULL,
	"payload_json" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "marketing_competitive_scrape_runs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"target_key" text NOT NULL,
	"trigger_kind" text NOT NULL,
	"status" text NOT NULL,
	"criterion" text,
	"ctox_run_id" text,
	"payload_json" text NOT NULL,
	"scheduled_for" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "marketing_competitor_watchlist" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"name" text NOT NULL,
	"url" text NOT NULL,
	"source" text DEFAULT 'manual' NOT NULL,
	"ctox_scrape_target_key" text DEFAULT 'marketing-competitive-analysis' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "operations_action_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"owner_id" text NOT NULL,
	"due_date" text NOT NULL,
	"text_json" text NOT NULL,
	"work_item_external_id" text,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_action_items_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "operations_decisions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"meeting_external_id" text NOT NULL,
	"project_external_id" text NOT NULL,
	"text_json" text NOT NULL,
	"linked_work_item_ids_json" text DEFAULT '[]' NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_decisions_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "operations_knowledge_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"project_external_id" text NOT NULL,
	"title" text NOT NULL,
	"kind" text NOT NULL,
	"owner_id" text NOT NULL,
	"sections_json" text NOT NULL,
	"linked_work_item_ids_json" text DEFAULT '[]' NOT NULL,
	"updated_on" text NOT NULL,
	"ctox_knowledge_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_knowledge_items_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "operations_meetings" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"project_external_id" text NOT NULL,
	"title" text NOT NULL,
	"starts_at" text NOT NULL,
	"facilitator_id" text NOT NULL,
	"agenda_json" text NOT NULL,
	"decision_ids_json" text DEFAULT '[]' NOT NULL,
	"action_item_ids_json" text DEFAULT '[]' NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_meetings_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "operations_projects" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"code" text NOT NULL,
	"name" text NOT NULL,
	"owner_id" text NOT NULL,
	"customer_id" text,
	"health" text NOT NULL,
	"progress" integer DEFAULT 0 NOT NULL,
	"start_date" text NOT NULL,
	"end_date" text NOT NULL,
	"next_milestone" text NOT NULL,
	"summary_json" text NOT NULL,
	"linked_modules_json" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_projects_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "operations_work_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"project_external_id" text NOT NULL,
	"subject" text NOT NULL,
	"type" text NOT NULL,
	"status" text NOT NULL,
	"priority" text NOT NULL,
	"assignee_id" text NOT NULL,
	"due_date" text NOT NULL,
	"estimate" integer DEFAULT 0 NOT NULL,
	"description_json" text NOT NULL,
	"linked_knowledge_ids_json" text DEFAULT '[]' NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_work_items_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "organizations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"name" text NOT NULL,
	"slug" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "organizations_slug_unique" UNIQUE("slug")
);
