CREATE TABLE IF NOT EXISTS "workforce_people" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "number" text NOT NULL,
  "name" text NOT NULL,
  "role" text NOT NULL,
  "team" text NOT NULL,
  "active" integer DEFAULT 1 NOT NULL,
  "location_slot_external_id" text,
  "payroll_employee_external_id" text,
  "weekly_hours" integer DEFAULT 40 NOT NULL,
  "skills_json" text DEFAULT '[]' NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_people_external_id_unique" ON "workforce_people" ("external_id");

CREATE TABLE IF NOT EXISTS "workforce_shift_types" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "name" text NOT NULL,
  "role" text NOT NULL,
  "start_time" text NOT NULL,
  "end_time" text NOT NULL,
  "color" text NOT NULL,
  "billable" integer DEFAULT 1 NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_shift_types_external_id_unique" ON "workforce_shift_types" ("external_id");

CREATE TABLE IF NOT EXISTS "workforce_location_slots" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "name" text NOT NULL,
  "zone" text NOT NULL,
  "capacity" integer DEFAULT 1 NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_location_slots_external_id_unique" ON "workforce_location_slots" ("external_id");

CREATE TABLE IF NOT EXISTS "workforce_absences" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "person_external_id" text NOT NULL,
  "start_date" text NOT NULL,
  "end_date" text NOT NULL,
  "type" text NOT NULL,
  "status" text NOT NULL,
  "note" text,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_absences_external_id_unique" ON "workforce_absences" ("external_id");

CREATE TABLE IF NOT EXISTS "workforce_recurring_shift_patterns" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "title" text NOT NULL,
  "person_external_id" text NOT NULL,
  "shift_type_external_id" text NOT NULL,
  "location_slot_external_id" text NOT NULL,
  "weekday" integer NOT NULL,
  "start_date" text NOT NULL,
  "end_date" text,
  "active" integer DEFAULT 1 NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_recurring_shift_patterns_external_id_unique" ON "workforce_recurring_shift_patterns" ("external_id");

CREATE TABLE IF NOT EXISTS "workforce_assignments" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "title" text NOT NULL,
  "person_external_id" text NOT NULL,
  "shift_type_external_id" text NOT NULL,
  "location_slot_external_id" text NOT NULL,
  "date" text NOT NULL,
  "start_time" text NOT NULL,
  "end_time" text NOT NULL,
  "customer_external_id" text,
  "project_external_id" text,
  "status" text NOT NULL,
  "blocker" text,
  "notes" text,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_assignments_external_id_unique" ON "workforce_assignments" ("external_id");
CREATE UNIQUE INDEX IF NOT EXISTS "workforce_assignment_slot_unique" ON "workforce_assignments" ("company_id", "person_external_id", "date", "start_time", "end_time");

CREATE TABLE IF NOT EXISTS "workforce_time_entries" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "assignment_external_id" text NOT NULL,
  "person_external_id" text NOT NULL,
  "date" text NOT NULL,
  "start_time" text NOT NULL,
  "end_time" text NOT NULL,
  "break_minutes" integer DEFAULT 0 NOT NULL,
  "status" text NOT NULL,
  "evidence" text,
  "note" text,
  "approved_at" timestamp with time zone,
  "approved_by" text,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_time_entries_external_id_unique" ON "workforce_time_entries" ("external_id");
CREATE UNIQUE INDEX IF NOT EXISTS "workforce_time_entry_assignment_unique" ON "workforce_time_entries" ("company_id", "assignment_external_id");

CREATE TABLE IF NOT EXISTS "workforce_handoffs" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "external_id" text NOT NULL,
  "company_id" text NOT NULL,
  "handoff_type" text NOT NULL,
  "assignment_external_id" text NOT NULL,
  "source_external_id" text NOT NULL,
  "target_external_id" text,
  "amount_minor" integer DEFAULT 0 NOT NULL,
  "currency" text DEFAULT 'EUR' NOT NULL,
  "status" text NOT NULL,
  "payload_json" text DEFAULT '{}' NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "workforce_handoffs_external_id_unique" ON "workforce_handoffs" ("external_id");
