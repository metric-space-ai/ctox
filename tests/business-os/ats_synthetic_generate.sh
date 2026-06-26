#!/usr/bin/env bash
# ATS synthetic test-data generator (see docs/ats-golive/synthetic-data/).
# Generates realistic, gate-consistent German Personalvermittler ATS data on a
# TEST instance, through the supported command-dispatch + direct-seed paths.
# Every record is tagged synthetic:true + batch_id for one-shot purge.
#
# Usage (on the test instance, with an ATS-capable ctox binary):
#   CTOX=~/.local/bin/ctox DB=~/runtime/business-os.sqlite3 \
#   SCALE=200 BATCH=syn-001 bash ats_synthetic_generate.sh
#   bash ats_synthetic_generate.sh --purge BATCH=syn-001     # remove a batch
set -uo pipefail

CTOX="${CTOX:-ctox}"
# Auto-detect the store the dispatch actually reads (the install layout / store
# path moves across releases, e.g. after `ctox upgrade --dev`); never hardcode it.
if [ -z "${DB:-}" ]; then
  DB="$("$CTOX" business-os status 2>/dev/null | grep -oE 'Native store: .*' | sed 's/Native store: //' | tr -d '[:space:]')"
  [ -z "$DB" ] && DB="$HOME/runtime/business-os.sqlite3"
fi
SCALE="${SCALE:-200}"
BATCH="${BATCH:-syn-001}"
AUE_SHARE="${AUE_SHARE:-55}"      # % of placements that are Arbeitnehmerüberlassung
EDGE_SHARE="${EDGE_SHARE:-12}"    # % of records deliberately in a blocked/edge state
ACTOR="${ACTOR:-syn_chef}"        # chef/admin actor for mutating dispatches
NOW=$(( $(date +%s) * 1000 ))
SEED="${SEED:-42}"

sqlx() { sqlite3 "$DB" "$@"; }

# ---- purge mode -------------------------------------------------------------
if [ "${1:-}" = "--purge" ]; then
  echo "Purging batch $BATCH from $DB ..."
  # placement-fee invoices first (linked via the placement's fee_invoice_id),
  # before the placements they reference are deleted.
  sqlx "DELETE FROM business_records WHERE collection='accounting_invoices' AND record_id IN (SELECT json_extract(payload_json,'\$.fee_invoice_id') FROM business_records WHERE collection='placements' AND json_extract(payload_json,'\$.candidate_id') LIKE 'syn_cand_${BATCH}_%');"
  # nachweis invoices carry the batch in their id; command-created flow records
  # (submissions/placements) carry no batch_id, so match by candidate prefix.
  sqlx "DELETE FROM business_records WHERE collection='accounting_invoices' AND record_id LIKE 'inv_nachweis_syn_nw_${BATCH}_%';"
  sqlx "DELETE FROM business_records WHERE collection IN ('submissions','placements') AND json_extract(payload_json,'\$.candidate_id') LIKE 'syn_cand_${BATCH}_%';"
  # direct-seeded stammdaten carry the batch_id marker.
  sqlx "DELETE FROM business_records WHERE json_extract(payload_json,'\$.batch_id')='$BATCH';"
  echo "done."; exit 0
fi

command -v "$CTOX" >/dev/null 2>&1 || { echo "ctox not found: $CTOX"; exit 1; }
[ -f "$DB" ] || { echo "store not found: $DB"; exit 1; }

# Deterministic pseudo-random in [0,mod) from an index + salt (reproducible).
rnd() { echo $(( ( (SEED + $1 * 2654435761 + $(printf '%d' "'${2:-x}")) % 1000003 ) % $3 )); }

FIRST=(Lukas Leon Felix Jonas Maximilian Elias Paul Ben Noah Luis Marie Sophie Emma Hannah Lena Laura Lea Mia Anna Julia Mehmet Ayse Ivan Oliwia Andrei Fatima Goran)
LAST=(Mueller Schmidt Schneider Fischer Weber Meyer Wagner Becker Schulz Hoffmann Koch Richter Klein Wolf Yilmaz Nowak Kowalski)
CITY=(Berlin Hamburg Muenchen Koeln Frankfurt Stuttgart Duesseldorf Dortmund Essen Leipzig Bremen Hannover Nuernberg Duisburg)
JOB=("Gabelstaplerfahrer (m/w/d)" "Produktionshelfer" "Lagerist" "Kommissionierer" "CNC-Fraeser" "Elektroniker" "SHK-Monteur" "Pflegefachkraft" "Altenpfleger" "Servicekraft" "LKW-Fahrer (CE)" "Schweisser" "Industriemechaniker" "Bueromkauffrau" "Buchhalter" "Maschinenbediener")
CRED=(staplerschein fuehrerschein_ce g25 gesundheitszeugnis sachkunde_34a schweisserpass ersthelfer a1_bescheinigung aufenthaltstitel)
TARIFF=(EG1 EG2 EG3 EG4 EG5 EG6 EG7 EG8 EG9)
CHANNEL=(indeed stepstone arbeitsagentur empfehlung website xing linkedin initiativbewerbung)
STAGES=(neu neu neu screening screening telefoninterview kundenvorstellung vertragsangebot eingestellt abgelehnt)
pick() { local -n arr=$1; echo "${arr[$(rnd "$2" "$3" ${#arr[@]})]}"; }

# AÜG required credential types for this tenant (mirror CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS)
AUE_REQ=(staplerschein g25)

dispatch() { # $1 type, $2 module, $3 payload-json  -> echoes the dispatch JSON
  # command_id MUST be unique per call: the accept path deduplicates by
  # command_id (a repeat returns the cached outcome WITHOUT creating a record).
  # A counter does not work here because dispatch() runs inside $(...) (a
  # subshell), so use a nanosecond timestamp + $RANDOM instead.
  local cid="${BATCH}_$(date +%s%N)_${RANDOM}"
  "$CTOX" business-os commands dispatch --json \
    "{\"command_id\":\"$cid\",\"command_type\":\"$1\",\"module\":\"$2\",\"payload\":$3,\"client_context\":{\"actor\":{\"id\":\"$ACTOR\"}}}" 2>/dev/null
}

# ---- 0. actor ---------------------------------------------------------------
sqlx "INSERT OR REPLACE INTO business_users(user_id,display_name,role,active,created_at_ms,updated_at_ms)
      VALUES('$ACTOR','Synthetic Chef','chef',1,$NOW,$NOW);" 2>/dev/null

# ---- 1. stammdaten (batched direct seed) ------------------------------------
echo "Seeding stammdaten (SCALE=$SCALE, batch=$BATCH) ..."
SQL="$(mktemp)"; echo "BEGIN;" > "$SQL"
seed() { # collection id payload
  printf "INSERT OR REPLACE INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json) VALUES('%s','%s','1-syn',0,%s,json('%s'));\n" \
    "$1" "$2" "$NOW" "$3" >> "$SQL"
}
NV=$(( SCALE/10 + 1 ))   # vacancies
NA=$(( SCALE/20 + 1 ))   # client accounts
for v in $(seq 1 "$NV"); do
  pt=$([ $(rnd "$v" v 100) -lt "$AUE_SHARE" ] && echo aue || echo festanstellung)
  seed vacancies "syn_vac_${BATCH}_$v" "{\"id\":\"syn_vac_${BATCH}_$v\",\"title\":\"$(pick JOB $v j)\",\"client_account_id\":\"syn_acct_${BATCH}_$(( v % NA + 1 ))\",\"location\":\"$(pick CITY $v c)\",\"placement_type\":\"$pt\",\"tariff_group\":\"$(pick TARIFF $v t)\",\"open_positions\":$(( $(rnd $v o 4) + 1 )),\"status\":\"open\",\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}"
done
for i in $(seq 1 "$SCALE"); do
  fn=$(pick FIRST $i f); ln=$(pick LAST $i l)
  cid="syn_cand_${BATCH}_$i"; vid="syn_vac_${BATCH}_$(( i % NV + 1 ))"
  stage="${STAGES[$(rnd $i s ${#STAGES[@]})]}"
  seed candidates "$cid" "{\"id\":\"$cid\",\"first_name\":\"$fn\",\"last_name\":\"$ln\",\"email\":\"${fn,,}.${ln,,}.$i@example.de\",\"phone\":\"+49 30 $(printf '%07d' $i)\",\"skills\":[\"$(pick CRED $i k)\"],\"status\":\"active\",\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}"
  seed applications "syn_app_${BATCH}_$i" "{\"id\":\"syn_app_${BATCH}_$i\",\"candidate_id\":\"$cid\",\"vacancy_id\":\"$vid\",\"status\":\"$stage\",\"data\":{\"pipeline\":{\"stage\":\"$stage\"}},\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}"
  # consent for candidates that reach submission (~stages past screening), unless EDGE
  edge=$([ $(rnd $i e 100) -lt "$EDGE_SHARE" ] && echo 1 || echo 0)
  case "$stage" in telefoninterview|kundenvorstellung|vertragsangebot|eingestellt)
    if [ "$edge" = 0 ]; then
      seed business_consents "syn_consent_${BATCH}_$i" "{\"id\":\"syn_consent_${BATCH}_$i\",\"subject_id\":\"$cid\",\"purpose\":\"present_to_client\",\"legal_basis\":\"consent\",\"granted_at_ms\":$(( NOW - 86400000 )),\"withdrawn_at_ms\":0,\"expires_at_ms\":0,\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}"
    fi
    # AÜG credentials for placed candidates (valid unless EDGE -> expired)
    until_ms=$([ "$edge" = 1 ] && echo $(( NOW - 86400000 )) || echo $(( NOW + 31536000000 )))
    ver=$([ "$edge" = 1 ] && echo false || echo true)
    j=0; for ct in "${AUE_REQ[@]}"; do j=$((j+1));
      seed business_credentials "syn_cred_${BATCH}_${i}_$j" "{\"id\":\"syn_cred_${BATCH}_${i}_$j\",\"subject_id\":\"$cid\",\"credential_type\":\"$ct\",\"deployment_blocking\":true,\"verified\":$ver,\"valid_until_ms\":$until_ms,\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}"
    done
  ;; esac
done
echo "COMMIT;" >> "$SQL"
sqlx < "$SQL"; rm -f "$SQL"

# ---- 2. flow via dispatch (exercises the real gates) ------------------------
echo "Dispatching flow (submissions / placements / sign-offs) ..."
sub_ok=0; sub_block=0; plc_ok=0; plc_block=0; bill_ok=0
for i in $(seq 1 "$SCALE"); do
  cid="syn_cand_${BATCH}_$i"; acct="syn_acct_${BATCH}_$(( i % NA + 1 ))"; vid="syn_vac_${BATCH}_$(( i % NV + 1 ))"
  stage="${STAGES[$(rnd $i s ${#STAGES[@]})]}"
  case "$stage" in telefoninterview|kundenvorstellung|vertragsangebot|eingestellt)
    r=$(dispatch ats.submission.present consent "{\"candidate_id\":\"$cid\",\"client_account_id\":\"$acct\",\"vacancy_id\":\"$vid\"}")
    echo "$r" | grep -qE '"allowed":[[:space:]]*true' && sub_ok=$((sub_ok+1)) || sub_block=$((sub_block+1))
  ;; esac
  case "$stage" in vertragsangebot|eingestellt)
    pt=$([ $(rnd "$(( i % NV + 1 ))" v 100) -lt "$AUE_SHARE" ] && echo arbeitnehmerueberlassung || echo festanstellung)
    req='[]'; [ "$pt" = arbeitnehmerueberlassung ] && req='["staplerschein","g25"]'
    fee=$(( $(rnd $i f 10000) + 4000 ))
    r=$(dispatch ats.placement.create placements "{\"candidate_id\":\"$cid\",\"client_account_id\":\"$acct\",\"placement_type\":\"$pt\",\"required_types\":$req,\"fee\":$fee,\"guarantee_days\":90}")
    if echo "$r" | grep -qE '"allowed":[[:space:]]*true|placement_id'; then
      plc_ok=$((plc_ok+1))
      if [ "$pt" = arbeitnehmerueberlassung ]; then
        nw="syn_nw_${BATCH}_$i"
        sqlx "INSERT OR REPLACE INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json) VALUES('planning_time_records','$nw','1-syn',0,$NOW,json('{\"id\":\"$nw\",\"subject_id\":\"$cid\",\"candidate_id\":\"$cid\",\"entleiher_account_id\":\"$acct\",\"entries\":[{\"type\":\"regular\",\"hours\":40.0},{\"type\":\"nacht\",\"hours\":4.0}],\"surcharge_pct\":{\"nacht\":25.0},\"synthetic\":true,\"batch_id\":\"$BATCH\",\"created_at_ms\":$NOW,\"updated_at_ms\":$NOW,\"_deleted\":false}'));" 2>/dev/null
        r=$(dispatch ats.leistungsnachweis.signoff nachweise "{\"collection\":\"planning_time_records\",\"record_id\":\"$nw\",\"charge_rate\":35.0,\"entleiher_account_id\":\"$acct\"}")
        echo "$r" | grep -qE '"billing_released":[[:space:]]*true' && bill_ok=$((bill_ok+1))
      fi
    else plc_block=$((plc_block+1)); fi
  ;; esac
done

echo "------------------------------------------------------------"
echo "batch=$BATCH  candidates=$SCALE"
echo "submissions: ok=$sub_ok blocked=$sub_block   placements: ok=$plc_ok blocked=$plc_block   nachweis-billed=$bill_ok"
echo "stammdaten counts:"
sqlx "SELECT collection, count(*) FROM business_records WHERE json_extract(payload_json,'\$.batch_id')='$BATCH' GROUP BY collection ORDER BY 1;"
echo "purge with:  bash $0 --purge   (BATCH=$BATCH)"
