#!/usr/bin/env bash
# ============================================================================
# CTOX Business OS ATS — deterministic end-to-end go-live smoke test (item G1)
# ============================================================================
#
# Proves the full native ATS command pipeline works against a FRESH isolated
# store: builds the binary from the main checkout, creates a clean temp root,
# seeds the minimal records each step needs (grounded in store.rs + ats_gates),
# then dispatches the complete ATS sequence via
#   `ctox business-os commands dispatch --json '<command>'`
# covering BOTH positive and negative gate paths, asserting the ok/blocked
# decision and the key returned ids/reasons for every step.
#
# Re-runnable + deterministic: every run starts from a brand-new mktemp -d root,
# so there is no cross-run state. Exits non-zero if ANY step fails; prints
# "ALL STEPS PASS" only when every assertion holds.
#
# Grounding (all read from src/core/business_os/store.rs + ats_gates.rs):
#   - dispatch goes through accept_rxdb_business_command(); a command REQUIRES a
#     unique `command_id` (or `id`); a repeated id returns the stored outcome,
#     so every step uses a fresh id.
#   - the handler outcome is nested under `.result` of the dispatch response:
#       { ok, id, command_id, status:"completed", result:{ ...handler json... } }
#   - mutating ats.* commands require a chef/admin actor (rxdb_command_session →
#     require_manage_all). capability-token enforcement is OFF by default, so the
#     browser-asserted client_context.actor.id is honoured against an active
#     business_users row → we seed chef_t (role 'chef').
#   - business_users columns: user_id, display_name, role['chef'|'admin'|
#     'founder'|'user'], active, created_at_ms, updated_at_ms.
#   - business_records columns: collection, record_id, rev, deleted,
#     updated_at_ms, payload_json.
#   - gates load by JSON field: deployment/consent read business_credentials/
#     business_consents WHERE json_extract(payload,'$.subject_id')=candidate_id.
#   - credential is deployable only when verified:true + credential_type matches
#     + within validity window (Valid/Expiring).
#   - consent_valid: legal_basis 'consent' needs granted_at_ms>0, not withdrawn,
#     not expired; purpose must equal 'present_to_client' for submission.
#   - leistungsnachweis.signoff: with require-signature OFF (default) the handler
#     sets entleiher_signed=true, so the only remaining billing blockers are the
#     entry/charge_rate gates → we test missing_charge_rate then the ok path.
# ============================================================================

set -u
set -o pipefail

REPO="/Users/you/Documents/ctox.nosync"
TARGET_DIR="/tmp/ctox-ats-target"
BIN="${TARGET_DIR}/debug/ctox"

PASS_COUNT=0
FAIL_COUNT=0
declare -a FAILURES=()

pass() { PASS_COUNT=$((PASS_COUNT+1)); echo "STEP ${1}: PASS"; }
fail() {
  FAIL_COUNT=$((FAIL_COUNT+1))
  FAILURES+=("${1}: ${2}")
  echo "STEP ${1}: FAIL -- ${2}"
  echo "  response: ${3:-<none>}"
}

# ----------------------------------------------------------------------------
# 0. Preconditions
# ----------------------------------------------------------------------------
command -v jq >/dev/null 2>&1 || { echo "FATAL: jq is required"; exit 2; }
command -v sqlite3 >/dev/null 2>&1 || { echo "FATAL: sqlite3 is required"; exit 2; }

# ----------------------------------------------------------------------------
# 1. Build the binary from the MAIN checkout (warm build ~1-2 min).
# ----------------------------------------------------------------------------
echo "== building ctox binary from ${REPO} =="
( cd "${REPO}" && CARGO_TARGET_DIR="${TARGET_DIR}" cargo build -p ctox --bin ctox ) \
  || { echo "FATAL: build failed"; exit 2; }
[ -x "${BIN}" ] || { echo "FATAL: binary not found at ${BIN}"; exit 2; }

# ----------------------------------------------------------------------------
# 2. Fresh isolated temp root with a runtime/ subdir (clean + deterministic).
# ----------------------------------------------------------------------------
ROOT="$(mktemp -d -t ats_golive_smoke.XXXXXX)"
mkdir -p "${ROOT}/runtime"
DB="${ROOT}/runtime/business-os.sqlite3"
echo "== fresh root: ${ROOT} =="

# The CLI resolves its workspace root itself (NOT from cwd): it honours
# CTOX_ROOT only when the dir "looks like" a ctox root — Cargo.toml +
# src/core/main.rs + contracts/history/creation-ledger.md (looks_like_ctox_root
# in src/core/main.rs). Without this, the binary falls back to its exe-relative
# install root (~/.local/lib/ctox/current) and the smoke run would NOT be
# isolated. We lay down empty sentinel files so CTOX_ROOT validates and every
# store write lands in this throwaway root.
mkdir -p "${ROOT}/src/core" "${ROOT}/contracts/history"
: > "${ROOT}/Cargo.toml"
: > "${ROOT}/src/core/main.rs"
: > "${ROOT}/contracts/history/creation-ledger.md"
export CTOX_ROOT="${ROOT}"

cleanup() { rm -rf "${ROOT}"; }
trap cleanup EXIT

# Deterministic time anchor used by all seeded records (well in the past so
# windows are stable regardless of wall clock). now() in the handler uses real
# time; we just keep seeded validity windows far in the future / past.
NOW_MS="$(($(date +%s) * 1000))"
FUTURE_MS="$(( NOW_MS + 400 * 24 * 60 * 60 * 1000 ))"   # +400 days (Valid, not Expiring)
PAST_GRANT_MS="$(( NOW_MS - 10 * 24 * 60 * 60 * 1000 ))" # consent granted 10 days ago

# dispatch helper: run a command with a unique id, return the raw JSON on stdout.
CMD_SEQ=0
dispatch() {
  # $1 = command_type, $2 = module, $3 = payload JSON object
  CMD_SEQ=$((CMD_SEQ+1))
  local cid="cmd_${CMD_SEQ}_$(date +%s%N)"
  local doc
  doc=$(jq -nc \
    --arg id "${cid}" \
    --arg ct "${1}" \
    --arg mod "${2}" \
    --argjson payload "${3}" \
    '{command_id:$id, command_type:$ct, module:$mod, payload:$payload, client_context:{actor:{id:"chef_t"}}}')
  ( cd "${ROOT}" && "${BIN}" business-os commands dispatch --json "${doc}" ) 2>/dev/null
}

sql() { sqlite3 "${DB}" "$1"; }

seed_record() {
  # $1 = collection, $2 = record_id, $3 = payload JSON
  local payload
  payload=$(printf '%s' "$3" | sed "s/'/''/g")
  sql "INSERT INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json)
       VALUES('${1}','${2}','1-seed',0,${NOW_MS},'${payload}')
       ON CONFLICT(collection,record_id) DO UPDATE SET payload_json=excluded.payload_json, deleted=0;"
}

# ----------------------------------------------------------------------------
# 3a. Initialise the store: a first read-only dispatch creates business-os.sqlite3
#     and all tables (open_store runs the schema). retention.due is read-only and
#     only needs an authenticated (not manage-all) actor, so it works pre-seed.
# ----------------------------------------------------------------------------
echo "== init store =="
INIT=$(dispatch "ats.retention.due" "consent" '{"collection":"applications","retention_days":3650}')
[ -f "${DB}" ] || { echo "FATAL: store not created; init response: ${INIT}"; exit 2; }

# ----------------------------------------------------------------------------
# 3b. Seed the chef actor + sample records grounded in the handler code.
# ----------------------------------------------------------------------------
echo "== seed chef + sample records =="
sql "INSERT INTO business_users(user_id,display_name,role,active,created_at_ms,updated_at_ms)
     VALUES('chef_t','Chef Tester','chef',1,${NOW_MS},${NOW_MS})
     ON CONFLICT(user_id) DO UPDATE SET role='chef', active=1;"

CAND="cand_smoke_1"
CLIENT="client_smoke_1"
VACANCY="vac_smoke_1"

# A vacancy (referenced by intake/submission/placement; no gate reads it but it
# grounds the pipeline).
seed_record "vacancies" "${VACANCY}" \
  "{\"id\":\"${VACANCY}\",\"title\":\"Lagerhelfer (m/w/d)\",\"client_account_id\":\"${CLIENT}\",\"status\":\"open\",\"created_at_ms\":${NOW_MS},\"updated_at_ms\":${NOW_MS},\"_deleted\":false}"

# Valid consent for present_to_client (legal_basis 'consent', granted, not
# withdrawn, no expiry) keyed by subject_id = candidate_id.
seed_record "business_consents" "consent_${CAND}_present" \
  "{\"id\":\"consent_${CAND}_present\",\"subject_id\":\"${CAND}\",\"purpose\":\"present_to_client\",\"legal_basis\":\"consent\",\"granted_at_ms\":${PAST_GRANT_MS},\"withdrawn_at_ms\":0,\"expires_at_ms\":0,\"created_at_ms\":${NOW_MS},\"updated_at_ms\":${NOW_MS},\"_deleted\":false}"

# A Leistungsnachweis (planning_time_records) with positive billable entries +
# an entleiher account so the signoff ok-path can emit an invoice.
seed_record "planning_time_records" "nw_${CAND}_1" \
  "{\"id\":\"nw_${CAND}_1\",\"subject_id\":\"${CAND}\",\"candidate_id\":\"${CAND}\",\"entleiher_account_id\":\"${CLIENT}\",\"entries\":[{\"type\":\"regular\",\"hours\":40.0},{\"type\":\"nacht\",\"hours\":4.0}],\"surcharge_pct\":{\"nacht\":25.0},\"created_at_ms\":${NOW_MS},\"updated_at_ms\":${NOW_MS},\"_deleted\":false}"

# ----------------------------------------------------------------------------
# 4 + 5. Run the FULL ATS sequence with specific assertions.
# ----------------------------------------------------------------------------

# --- ats.intake.capture (ok -> application_id) ------------------------------
R=$(dispatch "ats.intake.capture" "intake" \
  "{\"channel\":\"email\",\"name\":\"Erika Mustermann\",\"email\":\"Erika@example.com\",\"vacancy_id\":\"${VACANCY}\"}")
APP_ID=$(echo "$R" | jq -r '.result.application_id // empty')
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] && [ -n "$APP_ID" ]; then
  pass "intake.capture (ok, application_id=${APP_ID})"
else
  fail "intake.capture" "expected ok=true + application_id" "$R"
fi

# --- ats.consent.check (ok, allowed=true) -----------------------------------
R=$(dispatch "ats.consent.check" "consent" \
  "{\"subject_id\":\"${CAND}\",\"purpose\":\"present_to_client\"}")
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] && [ "$(echo "$R" | jq -r '.result.allowed')" = "true" ]; then
  pass "consent.check (ok, allowed=true)"
else
  fail "consent.check" "expected ok=true + allowed=true" "$R"
fi

# --- ats.submission.present (ok -> submission_id) ---------------------------
R=$(dispatch "ats.submission.present" "submissions" \
  "{\"candidate_id\":\"${CAND}\",\"client_account_id\":\"${CLIENT}\",\"vacancy_id\":\"${VACANCY}\"}")
SUBM_ID=$(echo "$R" | jq -r '.result.submission_id // empty')
if [ "$(echo "$R" | jq -r '.result.allowed')" = "true" ] && [ -n "$SUBM_ID" ]; then
  pass "submission.present#1 (ok, submission_id=${SUBM_ID})"
else
  fail "submission.present#1" "expected allowed=true + submission_id" "$R"
fi

# --- ats.submission.present AGAIN -> BLOCKED on conflicting_submission_id ----
R=$(dispatch "ats.submission.present" "submissions" \
  "{\"candidate_id\":\"${CAND}\",\"client_account_id\":\"${CLIENT}\",\"vacancy_id\":\"${VACANCY}\"}")
ALLOWED=$(echo "$R" | jq -r '.result.allowed')
REASON=$(echo "$R" | jq -r '.result.blockers[]?.reason' | grep -c '^double_submission$')
CONFLICT_ID=$(echo "$R" | jq -r '.result.blockers[]? | select(.reason=="double_submission") | .conflicting_submission_id')
if [ "$ALLOWED" = "false" ] && [ "$REASON" -ge 1 ] && [ "$CONFLICT_ID" = "$SUBM_ID" ]; then
  pass "submission.present#2 (BLOCKED double_submission, conflicting_submission_id=${CONFLICT_ID})"
else
  fail "submission.present#2" "expected allowed=false + double_submission + conflicting_submission_id=${SUBM_ID}" "$R"
fi

# --- ats.placement.create AÜG with required_types but NO credential -> BLOCKED
R=$(dispatch "ats.placement.create" "placements" \
  "{\"candidate_id\":\"${CAND}\",\"client_account_id\":\"${CLIENT}\",\"vacancy_id\":\"${VACANCY}\",\"placement_type\":\"arbeitnehmerueberlassung\",\"required_types\":[\"aue_license\"],\"fee\":5000.0}")
ALLOWED=$(echo "$R" | jq -r '.result.allowed')
MISSING=$(echo "$R" | jq -r '.result.blockers[]? | select(.credential_type=="aue_license") | .reason')
if [ "$ALLOWED" = "false" ] && [ "$MISSING" = "missing" ]; then
  pass "placement.create#1 (BLOCKED AÜG gate, aue_license=missing)"
else
  fail "placement.create#1" "expected allowed=false + aue_license blocker reason=missing" "$R"
fi

# Seed a VALID, VERIFIED aue_license credential for the candidate.
seed_record "business_credentials" "cred_${CAND}_aue" \
  "{\"id\":\"cred_${CAND}_aue\",\"subject_id\":\"${CAND}\",\"credential_type\":\"aue_license\",\"verified\":true,\"deployment_blocking\":true,\"valid_from_ms\":${PAST_GRANT_MS},\"valid_until_ms\":${FUTURE_MS},\"created_at_ms\":${NOW_MS},\"updated_at_ms\":${NOW_MS},\"_deleted\":false}"

# --- ats.placement.create AÜG WITH valid credential -> ok -------------------
R=$(dispatch "ats.placement.create" "placements" \
  "{\"candidate_id\":\"${CAND}\",\"client_account_id\":\"${CLIENT}\",\"vacancy_id\":\"${VACANCY}\",\"placement_type\":\"arbeitnehmerueberlassung\",\"required_types\":[\"aue_license\"],\"fee\":5000.0}")
PLAC_ID=$(echo "$R" | jq -r '.result.placement_id // empty')
if [ "$(echo "$R" | jq -r '.result.allowed')" = "true" ] && [ -n "$PLAC_ID" ]; then
  pass "placement.create#2 (ok, placement_id=${PLAC_ID})"
else
  fail "placement.create#2" "expected allowed=true + placement_id" "$R"
fi

# --- ats.deployment.check with valid credential -> ready=true ---------------
R=$(dispatch "ats.deployment.check" "placements" \
  "{\"subject_id\":\"${CAND}\",\"required_types\":[\"aue_license\"]}")
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] && [ "$(echo "$R" | jq -r '.result.ready')" = "true" ]; then
  pass "deployment.check#1 (ok, ready=true)"
else
  fail "deployment.check#1" "expected ok=true + ready=true" "$R"
fi

# --- ats.deployment.check for a MISSING credential type -> ready=false ------
R=$(dispatch "ats.deployment.check" "placements" \
  "{\"subject_id\":\"${CAND}\",\"required_types\":[\"medical_clearance\"]}")
READY=$(echo "$R" | jq -r '.result.ready')
BLK=$(echo "$R" | jq -r '.result.blockers[]? | select(.credential_type=="medical_clearance") | .reason')
if [ "$READY" = "false" ] && [ "$BLK" = "missing" ]; then
  pass "deployment.check#2 (blocked, medical_clearance=missing)"
else
  fail "deployment.check#2" "expected ready=false + medical_clearance=missing" "$R"
fi

# --- ats.leistungsnachweis.signoff WITHOUT charge_rate -> BLOCKED -----------
R=$(dispatch "ats.leistungsnachweis.signoff" "shiftflow" \
  "{\"collection\":\"planning_time_records\",\"record_id\":\"nw_${CAND}_1\"}")
RELEASED=$(echo "$R" | jq -r '.result.billing_released')
HAS_MCR=$(echo "$R" | jq -r '.result.blockers[]?' | grep -c '^missing_charge_rate$')
if [ "$RELEASED" = "false" ] && [ "$HAS_MCR" -ge 1 ]; then
  pass "leistungsnachweis.signoff#1 (BLOCKED missing_charge_rate, billing_released=false)"
else
  fail "leistungsnachweis.signoff#1" "expected billing_released=false + missing_charge_rate blocker" "$R"
fi

# --- ats.leistungsnachweis.signoff WITH charge_rate -> ok + invoice_id ------
R=$(dispatch "ats.leistungsnachweis.signoff" "shiftflow" \
  "{\"collection\":\"planning_time_records\",\"record_id\":\"nw_${CAND}_1\",\"charge_rate\":35.0}")
INV_ID=$(echo "$R" | jq -r '.result.invoice_id // empty')
RELEASED=$(echo "$R" | jq -r '.result.billing_released')
NET=$(echo "$R" | jq -r '.result.net_total')
BLK_COUNT=$(echo "$R" | jq -r '.result.blockers | length')
# net = 40*35 + 4*(35*1.25) = 1400 + 175 = 1575
if [ "$RELEASED" = "true" ] && [ -n "$INV_ID" ] && [ "$BLK_COUNT" = "0" ] \
   && [ "$(echo "$NET > 0" | bc -l 2>/dev/null || echo 1)" != "0" ]; then
  pass "leistungsnachweis.signoff#2 (ok, invoice_id=${INV_ID}, net_total=${NET})"
else
  fail "leistungsnachweis.signoff#2" "expected billing_released=true + invoice_id + no blockers + net_total>0" "$R"
fi

# --- ats.signature.request -> request_id ------------------------------------
R=$(dispatch "ats.signature.request" "signatures" \
  "{\"document_id\":\"nw_${CAND}_1\",\"subject_kind\":\"leistungsnachweis\",\"signers\":[{\"id\":\"entleiher_1\",\"name\":\"Entleiher\",\"state\":\"pending\"}]}")
REQ_ID=$(echo "$R" | jq -r '.result.request_id // empty')
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] && [ -n "$REQ_ID" ] \
   && [ "$(echo "$R" | jq -r '.result.status')" = "sent" ]; then
  pass "signature.request (ok, request_id=${REQ_ID}, status=sent)"
else
  fail "signature.request" "expected ok=true + request_id + status=sent" "$R"
fi

# --- ats.signature.sign -> status=completed (only signer signs) -------------
R=$(dispatch "ats.signature.sign" "signatures" \
  "{\"request_id\":\"${REQ_ID}\",\"signer_id\":\"entleiher_1\"}")
SIG_STATUS=$(echo "$R" | jq -r '.result.status')
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] && [ "$SIG_STATUS" = "completed" ] \
   && [ "$(echo "$R" | jq -r '.result.request_id')" = "$REQ_ID" ]; then
  pass "signature.sign (ok, status=completed)"
else
  fail "signature.sign" "expected ok=true + status=completed for request ${REQ_ID}" "$R"
fi

# --- ats.subject.export (Art.15 -> record_count + collections) --------------
R=$(dispatch "ats.subject.export" "consent" \
  "{\"subject_id\":\"${CAND}\"}")
RC=$(echo "$R" | jq -r '.result.record_count')
COLL_COUNT=$(echo "$R" | jq -r '.result.collections | keys | length')
# expect the seeded consent + credential + the submission + placement + nachweis.
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] \
   && [ "$(echo "$RC > 0" | bc -l 2>/dev/null || echo 1)" != "0" ] \
   && [ "$COLL_COUNT" -ge 3 ]; then
  pass "subject.export (ok, record_count=${RC}, collections=${COLL_COUNT})"
else
  fail "subject.export" "expected ok=true + record_count>0 + >=3 collections" "$R"
fi

# --- ats.subject.erase (Art.17 -> erased_count) -----------------------------
R=$(dispatch "ats.subject.erase" "consent" \
  "{\"subject_id\":\"${CAND}\"}")
EC=$(echo "$R" | jq -r '.result.erased_count')
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] \
   && [ "$(echo "$EC > 0" | bc -l 2>/dev/null || echo 1)" != "0" ] \
   && [ "$EC" = "$RC" ]; then
  pass "subject.erase (ok, erased_count=${EC})"
else
  fail "subject.erase" "expected ok=true + erased_count>0 (== exported ${RC})" "$R"
fi

# --- ats.retention.due (ok) -------------------------------------------------
# applications received NOW with a 0-day window are due immediately; we seeded
# the intake above, so a 0-day retention window returns it as due.
R=$(dispatch "ats.retention.due" "consent" \
  "{\"collection\":\"applications\",\"retention_days\":0,\"reference_field\":\"received_at_ms\"}")
if [ "$(echo "$R" | jq -r '.result.ok')" = "true" ] \
   && [ "$(echo "$R" | jq -r '.result.count')" != "null" ]; then
  pass "retention.due (ok, count=$(echo "$R" | jq -r '.result.count'))"
else
  fail "retention.due" "expected ok=true + numeric count" "$R"
fi

# ----------------------------------------------------------------------------
# Summary
# ----------------------------------------------------------------------------
echo "----------------------------------------------------------------------"
echo "PASS=${PASS_COUNT}  FAIL=${FAIL_COUNT}"
if [ "${FAIL_COUNT}" -eq 0 ]; then
  echo "ALL STEPS PASS"
  exit 0
else
  echo "FAILURES:"
  for f in "${FAILURES[@]}"; do echo "  - ${f}"; done
  exit 1
fi
