CREATE TABLE "operations_milestones" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"project_external_id" text NOT NULL,
	"title" text NOT NULL,
	"due_date" text NOT NULL,
	"status" text NOT NULL,
	"ctox_sync_key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "operations_milestones_external_id_unique" UNIQUE("external_id")
);
