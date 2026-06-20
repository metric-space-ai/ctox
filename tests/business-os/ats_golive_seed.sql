-- =====================================================================
-- ATS Tenant Go-Live Seed Template (G2)
-- =====================================================================
-- Operator-editable seed for a single Personalvermittler (staffing)
-- tenant. Run against the CTOX Business OS native store, e.g.:
--
--   sqlite3 "<CTOX_ROOT>/runtime/<business-os-store>.sqlite3" < ats_golive_seed.sql
--
-- Table/column shapes are taken VERBATIM from
--   src/core/business_os/store.rs
-- (business_users ~L25583, business_records ~L25487). Do NOT add columns
-- that are not in those CREATE TABLE statements.
--
-- Every value marked  -- EDIT THIS  is tenant-specific. Replace it before
-- go-live. Replace the literal millisecond timestamps too (they are
-- documented defaults, NOT a real clock).
--
-- Runtime FLAGS (REQUIRE_LOGIN, AUE_REQUIRED_CREDENTIALS, capability
-- token, signatures, allowlist, default role, signaling/ICE) live in a
-- DIFFERENT db/table: runtime/ctox-runtime.sqlite3 -> runtime_env_kv.
-- See docs/ats-golive/tenant-config.md. They are NOT seeded here.
-- =====================================================================

BEGIN TRANSACTION;

-- ---------------------------------------------------------------------
-- Block 1: business_users  (chef + admin bootstrap accounts)
-- Columns (store.rs ~L25583):
--   user_id TEXT PRIMARY KEY
--   display_name TEXT NOT NULL
--   role TEXT NOT NULL CHECK(role IN ('chef','admin','founder','user'))
--   active INTEGER NOT NULL DEFAULT 1
--   created_at_ms INTEGER NOT NULL
--   updated_at_ms INTEGER NOT NULL
-- Role meaning: 'chef' and 'admin' are the manage-all roles. Give the
-- owner 'chef'; give operational staff 'admin'. Day-to-day recruiters
-- should be plain 'user' (least privilege) + per-scope permission grants.
-- ---------------------------------------------------------------------
INSERT OR REPLACE INTO business_users
    (user_id, display_name, role, active, created_at_ms, updated_at_ms)
VALUES
    -- The owner / Geschäftsführer of the Personalvermittlung.
    ('user_chef',  'Tenant Owner',   'chef',  1, 1750000000000, 1750000000000),  -- EDIT THIS (id + display_name)
    -- An operational administrator (HR/ops lead).
    ('user_admin', 'Office Admin',   'admin', 1, 1750000000000, 1750000000000);  -- EDIT THIS (id + display_name)

-- Optional: a sample recruiter as a least-privilege 'user'. Uncomment +
-- edit. Grant scoped permissions separately (business_permission_grants).
-- INSERT OR REPLACE INTO business_users
--     (user_id, display_name, role, active, created_at_ms, updated_at_ms)
-- VALUES
--     ('user_recruiter1', 'Recruiter One', 'user', 1, 1750000000000, 1750000000000);  -- EDIT THIS

-- ---------------------------------------------------------------------
-- Block 2: Stammdaten via business_records
-- Columns (store.rs ~L25487):
--   collection TEXT NOT NULL
--   record_id TEXT NOT NULL
--   rev TEXT NOT NULL
--   deleted INTEGER NOT NULL DEFAULT 0
--   updated_at_ms INTEGER NOT NULL
--   payload_json TEXT NOT NULL
--   PRIMARY KEY (collection, record_id)
--
-- Convention (matches native upsert_business_record payloads): the
-- record's own id is repeated inside payload_json as "id", and soft-
-- delete is mirrored as "_deleted": false. Collection names below are
-- the ones the native ATS handlers read/write:
--   vacancies, candidates, applications, business_credentials,
--   submissions, planning_time_records, business_consents,
--   signature_requests.
-- ---------------------------------------------------------------------

-- 2a. Sample vacancy (Stelle / job order the tenant is filling).
INSERT OR REPLACE INTO business_records
    (collection, record_id, rev, deleted, updated_at_ms, payload_json)
VALUES
    ('vacancies', 'vac_sample_1', '1-seed', 0, 1750000000000,
     json('{
        "id": "vac_sample_1",
        "title": "Gabelstaplerfahrer (m/w/d)",
        "client_account_id": "acct_sample_1",
        "location": "Musterstadt",
        "placement_type": "aue",
        "tariff_group": "ZAG-E2",
        "open_positions": 3,
        "status": "open",
        "created_at_ms": 1750000000000,
        "updated_at_ms": 1750000000000,
        "_deleted": false
     }'));  -- EDIT THIS (title, client, location, placement_type, tariff)

-- 2b. Sample candidate (talent-pool entry).
INSERT OR REPLACE INTO business_records
    (collection, record_id, rev, deleted, updated_at_ms, payload_json)
VALUES
    ('candidates', 'cand_sample_1', '1-seed', 0, 1750000000000,
     json('{
        "id": "cand_sample_1",
        "first_name": "Max",
        "last_name": "Mustermann",
        "email": "max.mustermann@example.de",
        "phone": "+49 30 0000000",
        "skills": ["staplerschein", "lager"],
        "status": "active",
        "created_at_ms": 1750000000000,
        "updated_at_ms": 1750000000000,
        "_deleted": false
     }'));  -- EDIT THIS (real candidate data; mind DSGVO before seeding PII)

-- 2c. Sample application (links the candidate to the vacancy; this is the
--     pipeline record that carries the matching stage at
--     data.pipeline.stage -- see matching/core/pipeline.js CANDIDATE_STAGES).
INSERT OR REPLACE INTO business_records
    (collection, record_id, rev, deleted, updated_at_ms, payload_json)
VALUES
    ('applications', 'app_sample_1', '1-seed', 0, 1750000000000,
     json('{
        "id": "app_sample_1",
        "candidate_id": "cand_sample_1",
        "vacancy_id": "vac_sample_1",
        "status": "neu",
        "data": { "pipeline": { "stage": "neu" } },
        "created_at_ms": 1750000000000,
        "updated_at_ms": 1750000000000,
        "_deleted": false
     }'));  -- EDIT THIS (stage must be one of the configured pipeline keys)

-- 2d. Sample credential (AÜG deployment paper for the candidate). The AÜG
--     placement gate reads business_credentials by subject_id == candidate
--     and checks credential_type against CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS.
--     Fields used by the gate: credential_type, subject_id,
--     deployment_blocking, verified, valid_until_ms (store.rs ~L22566,
--     ats_gates.rs evaluate_deployment_readiness).
INSERT OR REPLACE INTO business_records
    (collection, record_id, rev, deleted, updated_at_ms, payload_json)
VALUES
    ('business_credentials', 'cred_sample_1', '1-seed', 0, 1750000000000,
     json('{
        "id": "cred_sample_1",
        "subject_id": "cand_sample_1",
        "credential_type": "staplerschein",
        "deployment_blocking": true,
        "verified": true,
        "valid_until_ms": 1900000000000,
        "created_at_ms": 1750000000000,
        "updated_at_ms": 1750000000000,
        "_deleted": false
     }'));  -- EDIT THIS (credential_type must match your AUE_REQUIRED list;
            -- valid_until_ms must be a real future expiry)

COMMIT;

-- ---------------------------------------------------------------------
-- Post-seed checklist (do these in runtime_env_kv, NOT here):
--   * CTOX_BUSINESS_OS_REQUIRE_LOGIN = 1
--   * CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN = 1
--   * CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS = <tenant legal list>
--   * CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE = 1
--   * CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE = 1
--   * CTOX_BUSINESS_OS_MODULE_ALLOWLIST = <recruiting modules>
-- See docs/ats-golive/tenant-config.md for the exact set commands.
-- ---------------------------------------------------------------------
