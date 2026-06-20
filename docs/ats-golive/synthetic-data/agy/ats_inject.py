#!/usr/bin/env python3
"""ATS synthetic-data INJECTOR.

The LLM (Antigravity / Gemini) AUTHORS the realistic content; this injector only
lands it correctly + gate-consistently into the live ATS. It encapsulates the
hard-won mechanics so the model never re-hits them:
  - resolve the native store at runtime (it moves across releases),
  - seed stammdaten directly (robust JSON: umlauts / newlines in CVs survive),
  - dispatch the gated flow via `ctox business-os commands dispatch` with a
    UNIQUE command_id per call (the accept path deduplicates by command_id),
  - keep referential + gate order (consent before submission, credentials before
    an AÜG placement, a Leistungsnachweis before its sign-off).

Input: a JSON file (authored by the model) — see GENERATE.md for the schema.
Usage: python3 ats_inject.py records.json
"""
import json, subprocess, sys, time, uuid, os

CTOX = os.environ.get("CTOX", os.path.expanduser("~/.local/bin/ctox"))

def resolve_store():
    out = subprocess.run([CTOX, "business-os", "status"], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if "Native store:" in line:
            return line.split("Native store:")[1].strip()
    raise SystemExit("could not resolve native store from `ctox business-os status`")

STORE = resolve_store()
NOW = int(time.time() * 1000)
DAY = 86400000

def sql(stmt, params=()):
    # parameterised via a tiny here-doc to keep umlauts/quotes intact
    p = subprocess.run(["sqlite3", STORE, stmt], capture_output=True, text=True)
    if p.returncode != 0:
        sys.stderr.write("SQL ERR: " + p.stderr[:300] + "\n")
    return p.stdout.strip()

def seed(collection, rec_id, payload):
    payload = dict(payload)
    payload.setdefault("id", rec_id)
    payload["_deleted"] = False
    js = json.dumps(payload, ensure_ascii=False).replace("'", "''")
    sql(f"INSERT OR REPLACE INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json) "
        f"VALUES('{collection}','{rec_id}','1-syn',0,{NOW},json('{js}'));")

def dispatch(ctype, module, payload, actor):
    cid = f"{ctype.replace('.','_')}_{uuid.uuid4().hex}"
    env = {"command_id": cid, "command_type": ctype, "module": module,
           "payload": payload, "client_context": {"actor": {"id": actor}}}
    p = subprocess.run([CTOX, "business-os", "commands", "dispatch", "--json", json.dumps(env, ensure_ascii=False)],
                       capture_output=True, text=True)
    try:
        return json.loads(p.stdout).get("result", {})
    except Exception:
        return {"_raw": p.stdout[:200], "_err": p.stderr[:200]}

def namespace(data, B):
    """Prefix every id + cross-reference with the batch, so ids never collide
    across hundreds of independently-generated batches."""
    def p(x):
        return (B + x) if x else x
    for a in data.get("accounts", []):
        a["id"] = p(a.get("id", ""))
    for v in data.get("vacancies", []):
        v["id"] = p(v.get("id", ""))
        if v.get("client_account_id"):
            v["client_account_id"] = p(v["client_account_id"])
    for c in data.get("candidates", []):
        c["id"] = p(c.get("id", ""))
        if c.get("vacancy_id"):
            c["vacancy_id"] = p(c["vacancy_id"])
        if c.get("client_account_id"):
            c["client_account_id"] = p(c["client_account_id"])
    return data

def main(path):
    data = json.load(open(path, encoding="utf-8"))
    batch = data["batch"]
    actor = data.get("actor", "syn_chef")
    data = namespace(data, batch + "_")
    # vacancy -> client account, to resolve a candidate's client when the LLM set
    # only vacancy_id (common) and left client_account_id null.
    vac_account = {v["id"]: v.get("client_account_id", "") for v in data.get("vacancies", []) if v.get("id")}
    default_account = (data.get("accounts") or [{}])[0].get("id", "")  # each batch has one client
    sql(f"INSERT OR REPLACE INTO business_users(user_id,display_name,role,active,created_at_ms,updated_at_ms) "
        f"VALUES('{actor}','Synthetic Chef','chef',1,{NOW},{NOW});")

    def mark(p):
        p = dict(p); p["synthetic"] = True; p["batch_id"] = batch
        p.setdefault("created_at_ms", NOW); p.setdefault("updated_at_ms", NOW)
        return p

    counts = {k: 0 for k in ("accounts","vacancies","candidates","applications","consents",
                             "credentials","submissions_ok","submissions_blocked",
                             "placements_ok","placements_blocked","nachweise_billed",
                             "interviews","scorecards")}

    for a in data.get("accounts", []):
        seed("business_accounts", a["id"], mark(a)); counts["accounts"] += 1
    for v in data.get("vacancies", []):
        seed("vacancies", v["id"], mark(v)); counts["vacancies"] += 1

    for c in data["candidates"]:
        cid = c["id"]
        acct = c.get("client_account_id") or vac_account.get(c.get("vacancy_id", ""), "") or default_account
        # rich candidate record (CV / work history / skills authored by the model)
        cand = {k: c[k] for k in c if k not in ("stage","vacancy_id","consent","credentials",
                "submit","submit_note","client_account_id","placement","nachweis","interview")}
        cand["status"] = "active"
        seed("candidates", cid, mark(cand)); counts["candidates"] += 1
        # application carrying the funnel stage + a snapshot of the profile
        stage = c.get("stage", "neu")
        seed("applications", f"app_{cid}", mark({
            "candidate_id": cid, "vacancy_id": c.get("vacancy_id",""),
            "status": stage, "data": {"pipeline": {"stage": stage},
            "headline": c.get("headline",""), "source_channel": c.get("source_channel","")}}))
        counts["applications"] += 1
        # consent (model decides legal_basis / purpose / evidence)
        cons = c.get("consent")
        if cons:
            seed("business_consents", f"consent_{cid}", mark({
                "subject_id": cid, "purpose": cons.get("purpose","present_to_client"),
                "legal_basis": cons.get("legal_basis","consent"),
                "granted_at_ms": NOW - int(cons.get("granted_days_ago",1))*DAY,
                "withdrawn_at_ms": NOW - int(cons["withdrawn_days_ago"])*DAY if cons.get("withdrawn_days_ago") else 0,
                "expires_at_ms": NOW + int(cons["expires_in_days"])*DAY if cons.get("expires_in_days") else 0,
                "basis_evidence": cons.get("evidence","")}))
            counts["consents"] += 1
        # credentials (rich: issuer, expiry) — LLMs vary the type key (credential_type/type/name)
        for j, cr in enumerate(c.get("credentials", []), 1):
            if not isinstance(cr, dict):
                continue
            ctype = cr.get("credential_type") or cr.get("type") or cr.get("name")
            if not ctype:
                continue
            seed("business_credentials", f"cred_{cid}_{j}", mark({
                "subject_id": cid, "credential_type": ctype,
                "deployment_blocking": cr.get("deployment_blocking", True),
                "verified": cr.get("verified", True),
                "issuer": cr.get("issuer") or cr.get("issued_by", ""),
                "valid_until_ms": NOW + int(cr.get("valid_until_days", 365))*DAY}))
            counts["credentials"] += 1
        # interview meeting + rich scorecard
        iv = c.get("interview")
        if iv:
            seed("interview_meetings", f"iv_{cid}", mark({
                "candidate_id": cid, "vacancy_id": c.get("vacancy_id",""),
                "parties": iv.get("parties", [{"name": "Recruiter"}]),
                "start": NOW + int(iv.get("scheduled_in_days",3))*DAY,
                "end": NOW + int(iv.get("scheduled_in_days",3))*DAY + 3600000,
                "location_mode": iv.get("mode","video"), "state": iv.get("state","scheduled")}))
            counts["interviews"] += 1
            sc = iv.get("scorecard")
            if sc:
                scp = {"candidate_id": cid, "recommendation": iv.get("recommendation", ""),
                       "interviewer": iv.get("interviewer", "")}
                if isinstance(sc, dict):
                    scp.update(sc)
                else:  # LLMs often emit the scorecard as a list of competencies
                    scp["competencies"] = sc
                seed("interview_scorecards", f"sc_{cid}", mark(scp))
                counts["scorecards"] += 1
        # --- gated flow via dispatch (real gates) ---
        if c.get("submit"):
            r = dispatch("ats.submission.present", "submissions", {
                "candidate_id": cid, "client_account_id": acct,
                "vacancy_id": c.get("vacancy_id","")}, actor)
            counts["submissions_ok" if r.get("allowed") else "submissions_blocked"] += 1
        pl = c.get("placement")
        if pl:
            r = dispatch("ats.placement.create", "placements", {
                "candidate_id": cid, "client_account_id": acct,
                "placement_type": pl.get("placement_type","festanstellung"),
                "required_types": pl.get("required_types", []),
                "fee": pl.get("fee_eur"), "guarantee_days": pl.get("guarantee_days", 90)}, actor)
            ok = bool(r.get("allowed")) or bool(r.get("placement_id"))
            counts["placements_ok" if ok else "placements_blocked"] += 1
            nw = c.get("nachweis")
            if ok and nw:
                nwid = f"nw_{cid}"
                sur = nw.get("surcharge_pct")
                seed("planning_time_records", nwid, mark({
                    "subject_id": cid, "candidate_id": cid,
                    "entleiher_account_id": acct,
                    "entries": nw.get("entries", []),
                    "surcharge_pct": sur if isinstance(sur, dict) else {}}))
                r2 = dispatch("ats.leistungsnachweis.signoff", "nachweise", {
                    "collection": "planning_time_records", "record_id": nwid,
                    "charge_rate": nw.get("charge_rate_eur", 35.0),
                    "entleiher_account_id": acct}, actor)
                if r2.get("billing_released"):
                    counts["nachweise_billed"] += 1

    print(json.dumps({"batch": batch, "counts": counts}, ensure_ascii=False))

if __name__ == "__main__":
    main(sys.argv[1])
