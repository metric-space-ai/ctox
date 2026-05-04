CREATE TABLE "ctox_bug_reports" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"title" text NOT NULL,
	"module_id" text NOT NULL,
	"submodule_id" text NOT NULL,
	"status" text NOT NULL,
	"severity" text NOT NULL,
	"tags_json" text DEFAULT '[]' NOT NULL,
	"payload_json" text NOT NULL,
	"core_task_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "ctox_bug_reports_external_id_unique" UNIQUE("external_id")
);
