CREATE TABLE "business_accounting_proposals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"kind" text NOT NULL,
	"status" text NOT NULL,
	"ref_type" text NOT NULL,
	"ref_id" text NOT NULL,
	"proposed_command_json" text NOT NULL,
	"evidence_json" text NOT NULL,
	"confidence" integer DEFAULT 0 NOT NULL,
	"created_by_agent" text NOT NULL,
	"decided_by" text,
	"decided_at" timestamp with time zone,
	"resulting_journal_entry_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_accounting_proposals_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_outbox_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"topic" text NOT NULL,
	"payload_json" text NOT NULL,
	"status" text DEFAULT 'pending' NOT NULL,
	"attempts" integer DEFAULT 0 NOT NULL,
	"delivered_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "business_outbox_events_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "business_accounting_audit_events" (
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
CREATE TABLE "accounting_accounts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"code" text NOT NULL,
	"name" text NOT NULL,
	"root_type" text NOT NULL,
	"account_type" text NOT NULL,
	"parent_external_id" text,
	"is_group" integer DEFAULT 0 NOT NULL,
	"currency" text DEFAULT 'EUR' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_accounts_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_parties" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"kind" text NOT NULL,
	"name" text NOT NULL,
	"tax_id" text,
	"vat_id" text,
	"default_receivable_account_external_id" text,
	"default_payable_account_external_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_parties_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_tax_rates" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"code" text NOT NULL,
	"rate" integer DEFAULT 0 NOT NULL,
	"account_external_id" text,
	"type" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_tax_rates_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_fiscal_periods" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"start_date" text NOT NULL,
	"end_date" text NOT NULL,
	"status" text DEFAULT 'open' NOT NULL,
	"closed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_fiscal_periods_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_invoices" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"customer_external_id" text NOT NULL,
	"number" text NOT NULL,
	"status" text NOT NULL,
	"issue_date" text NOT NULL,
	"service_date" text,
	"due_date" text NOT NULL,
	"currency" text DEFAULT 'EUR' NOT NULL,
	"net_amount_minor" integer DEFAULT 0 NOT NULL,
	"tax_amount_minor" integer DEFAULT 0 NOT NULL,
	"total_amount_minor" integer DEFAULT 0 NOT NULL,
	"balance_due_minor" integer DEFAULT 0 NOT NULL,
	"pdf_blob_ref" text,
	"zugferd_xml" text,
	"posted_journal_entry_external_id" text,
	"sent_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_invoices_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_invoice_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"invoice_external_id" text NOT NULL,
	"line_no" integer NOT NULL,
	"product_external_id" text,
	"description" text NOT NULL,
	"quantity" integer DEFAULT 1 NOT NULL,
	"unit_price_minor" integer DEFAULT 0 NOT NULL,
	"line_net_minor" integer DEFAULT 0 NOT NULL,
	"tax_rate" integer DEFAULT 0 NOT NULL,
	"tax_amount_minor" integer DEFAULT 0 NOT NULL,
	"line_total_minor" integer DEFAULT 0 NOT NULL,
	"revenue_account_external_id" text
);
--> statement-breakpoint
CREATE TABLE "accounting_receipts" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"vendor_external_id" text,
	"number" text NOT NULL,
	"vendor_invoice_number" text,
	"status" text NOT NULL,
	"receipt_date" text NOT NULL,
	"due_date" text,
	"currency" text DEFAULT 'EUR' NOT NULL,
	"net_amount_minor" integer DEFAULT 0 NOT NULL,
	"tax_amount_minor" integer DEFAULT 0 NOT NULL,
	"total_amount_minor" integer DEFAULT 0 NOT NULL,
	"expense_account_external_id" text,
	"payable_account_external_id" text,
	"tax_code" text,
	"ocr_text" text,
	"extracted_json" text,
	"posted_journal_entry_external_id" text,
	"reviewed_at" timestamp with time zone,
	"posted_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_receipts_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_receipt_files" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"receipt_external_id" text NOT NULL,
	"blob_ref" text NOT NULL,
	"mime" text NOT NULL,
	"original_filename" text NOT NULL,
	"sha256" text NOT NULL,
	"uploaded_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "accounting_receipt_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"receipt_external_id" text NOT NULL,
	"line_no" integer NOT NULL,
	"description" text NOT NULL,
	"expense_account_external_id" text NOT NULL,
	"net_amount_minor" integer DEFAULT 0 NOT NULL,
	"tax_code" text,
	"tax_amount_minor" integer DEFAULT 0 NOT NULL,
	"total_amount_minor" integer DEFAULT 0 NOT NULL
);
--> statement-breakpoint
CREATE TABLE "accounting_payments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"party_external_id" text,
	"kind" text NOT NULL,
	"payment_date" text NOT NULL,
	"amount_minor" integer DEFAULT 0 NOT NULL,
	"currency" text DEFAULT 'EUR' NOT NULL,
	"bank_account_external_id" text NOT NULL,
	"bank_statement_line_external_id" text,
	"posted_journal_entry_external_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_payments_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_payment_allocations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"payment_external_id" text NOT NULL,
	"invoice_external_id" text,
	"receipt_external_id" text,
	"amount_minor" integer DEFAULT 0 NOT NULL
);
--> statement-breakpoint
CREATE TABLE "accounting_bank_statements" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"account_external_id" text NOT NULL,
	"format" text NOT NULL,
	"imported_by" text,
	"source_filename" text NOT NULL,
	"source_sha256" text NOT NULL,
	"start_date" text,
	"end_date" text,
	"opening_balance_minor" integer DEFAULT 0 NOT NULL,
	"closing_balance_minor" integer DEFAULT 0 NOT NULL,
	"imported_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_bank_statements_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_bank_statement_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"statement_external_id" text NOT NULL,
	"line_no" integer NOT NULL,
	"booking_date" text NOT NULL,
	"value_date" text,
	"amount_minor" integer DEFAULT 0 NOT NULL,
	"currency" text DEFAULT 'EUR' NOT NULL,
	"remitter_name" text,
	"remitter_iban" text,
	"purpose" text,
	"end_to_end_ref" text,
	"match_status" text DEFAULT 'unmatched' NOT NULL,
	"matched_journal_entry_external_id" text,
	"duplicate_of_line_external_id" text,
	CONSTRAINT "accounting_bank_statement_lines_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_number_series" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"key" text NOT NULL,
	"fiscal_year" integer NOT NULL,
	"prefix" text NOT NULL,
	"next_value" integer DEFAULT 1 NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_number_series_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_journal_entries" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"posting_date" text NOT NULL,
	"type" text NOT NULL,
	"ref_type" text NOT NULL,
	"ref_id" text NOT NULL,
	"number" text NOT NULL,
	"narration" text,
	"created_by" text NOT NULL,
	"reversed_by_external_id" text,
	"posted_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_journal_entries_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE TABLE "accounting_journal_entry_lines" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"journal_entry_external_id" text NOT NULL,
	"line_no" integer NOT NULL,
	"account_external_id" text NOT NULL,
	"party_external_id" text,
	"debit_minor" integer DEFAULT 0 NOT NULL,
	"credit_minor" integer DEFAULT 0 NOT NULL,
	"cost_center_external_id" text,
	"project_external_id" text
);
--> statement-breakpoint
CREATE TABLE "accounting_ledger_entries" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"external_id" text NOT NULL,
	"company_id" text NOT NULL,
	"posting_date" text NOT NULL,
	"account_external_id" text NOT NULL,
	"party_external_id" text,
	"debit_minor" integer DEFAULT 0 NOT NULL,
	"credit_minor" integer DEFAULT 0 NOT NULL,
	"ref_type" text NOT NULL,
	"ref_id" text NOT NULL,
	"journal_entry_external_id" text NOT NULL,
	"reverted" integer DEFAULT 0 NOT NULL,
	"reverts_external_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "accounting_ledger_entries_external_id_unique" UNIQUE("external_id")
);
--> statement-breakpoint
CREATE FUNCTION "prevent_posted_journal_entry_mutation"() RETURNS trigger AS $$
BEGIN
	IF OLD."posted_at" IS NOT NULL THEN
		RAISE EXCEPTION 'posted journal entries are immutable';
	END IF;
	RETURN NEW;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint
CREATE TRIGGER "accounting_journal_entries_lock_update"
BEFORE UPDATE ON "accounting_journal_entries"
FOR EACH ROW EXECUTE FUNCTION "prevent_posted_journal_entry_mutation"();
--> statement-breakpoint
CREATE TRIGGER "accounting_journal_entries_lock_delete"
BEFORE DELETE ON "accounting_journal_entries"
FOR EACH ROW EXECUTE FUNCTION "prevent_posted_journal_entry_mutation"();
--> statement-breakpoint
CREATE FUNCTION "prevent_posted_journal_line_mutation"() RETURNS trigger AS $$
BEGIN
	IF EXISTS (
		SELECT 1 FROM "accounting_journal_entries"
		WHERE "external_id" = OLD."journal_entry_external_id"
		AND "posted_at" IS NOT NULL
	) THEN
		RAISE EXCEPTION 'posted journal entry lines are immutable';
	END IF;
	RETURN NEW;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint
CREATE TRIGGER "accounting_journal_entry_lines_lock_update"
BEFORE UPDATE ON "accounting_journal_entry_lines"
FOR EACH ROW EXECUTE FUNCTION "prevent_posted_journal_line_mutation"();
--> statement-breakpoint
CREATE TRIGGER "accounting_journal_entry_lines_lock_delete"
BEFORE DELETE ON "accounting_journal_entry_lines"
FOR EACH ROW EXECUTE FUNCTION "prevent_posted_journal_line_mutation"();
--> statement-breakpoint
CREATE FUNCTION "prevent_ledger_entry_mutation"() RETURNS trigger AS $$
BEGIN
	RAISE EXCEPTION 'accounting ledger entries are append-only';
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint
CREATE TRIGGER "accounting_ledger_entries_lock_update"
BEFORE UPDATE ON "accounting_ledger_entries"
FOR EACH ROW EXECUTE FUNCTION "prevent_ledger_entry_mutation"();
--> statement-breakpoint
CREATE TRIGGER "accounting_ledger_entries_lock_delete"
BEFORE DELETE ON "accounting_ledger_entries"
FOR EACH ROW EXECUTE FUNCTION "prevent_ledger_entry_mutation"();
