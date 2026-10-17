use super::*;
use crate::http::{HttpRequest, HttpResponse};
use crate::store::MemorySecretBackend;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

struct Mock {
    calls: Mutex<Vec<String>>,
}
impl HttpTransport for Mock {
    fn send(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.calls.lock().unwrap().push(format!("{} {}", r.method, r.url));
        if r.url.contains("/items/boom") {
            return Ok(HttpResponse { status: 500, body: b"{}".to_vec(), session_id: None });
        }
        let body = if r.url.contains("zippopotam.us/de/10115") {
            json!({"country": "Germany", "post code": "10115", "places": [{"place name": "Berlin"}]})
        } else {
            json!({"id": "1", "ok": true})
        };
        Ok(HttpResponse { status: 200, body: serde_json::to_vec(&body)?, session_id: None })
    }
}

#[derive(Default)]
struct Gates {
    states: RefCell<std::collections::BTreeMap<String, String>>,
    missions: RefCell<std::collections::BTreeMap<String, String>>,
}
impl ApprovalGates for Gates {
    fn open(&self, _r: &GateRequest) -> Result<String> {
        let id = format!("self-work:internal:{}", self.states.borrow().len() + 1);
        self.states.borrow_mut().insert(id.clone(), "open".into());
        Ok(id)
    }
    fn state(&self, id: &str) -> Result<Option<String>> {
        Ok(self.states.borrow().get(id).cloned())
    }
    fn decide(&self, id: &str, approved: bool) -> Result<()> {
        self.states.borrow_mut().insert(id.into(), if approved { "closed" } else { "failed" }.into());
        Ok(())
    }
    fn builder_mission_service(&self, work_id: &str) -> Result<Option<String>> {
        Ok(self.missions.borrow().get(work_id).cloned())
    }
}

struct Allow;
impl PolicyGate for Allow {
    fn check(&self, _p: Permission) -> Result<()> {
        Ok(())
    }
}
struct DenyWrites;
impl PolicyGate for DenyWrites {
    fn check(&self, p: Permission) -> Result<()> {
        if p == Permission::DataRead { Ok(()) } else { bail!("policy denied {p:?}") }
    }
}

struct Env {
    root: PathBuf,
    backend: Arc<dyn SecretBackend>,
    mock: Arc<Mock>,
    gates: Gates,
    now: Arc<AtomicU64>,
}

fn env() -> Env {
    static N: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!("toolhub-builder-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&root);
    Env { root, backend: Arc::new(MemorySecretBackend::default()), mock: Arc::new(Mock { calls: Mutex::new(vec![]) }), gates: Gates::default(), now: Arc::new(AtomicU64::new(1_800_000_000)) }
}

impl Env {
    fn b<'a>(&'a self, role: AgentRole, policy: &'a dyn PolicyGate) -> Builder<'a> {
        let now = self.now.clone();
        Builder::new(&self.root, self.backend.clone(), self.mock.clone(), &self.gates, policy, role, "test").with_clock(Arc::new(move || now.load(Ordering::SeqCst)))
    }
}

fn zip_probe() -> Vec<Probe> {
    vec![Probe { tool: "ZIPPOPOTAM_LOOKUP_POSTAL_CODE".into(), args: json!({"country": "de", "postal_code": "10115"}), cleanup: vec![] }]
}

const SVC: &str = r#"{
  "schema": "ctox.adapter-spec/v1", "slug": "testsvc", "version": "1.0.0",
  "api_base": "https://api.testsvc.example.org", "allowed_hosts": ["api.testsvc.example.org"],
  "name": "TestSvc", "description": {"de": "Test", "en": "Test"},
  "auth_schemes": [{"type": "none"}],
  "actions": [
    {"slug": "TESTSVC_GET_ITEM", "title": {"de": "Holen", "en": "Get"}, "description": {"de": "x", "en": "x"}, "risk": "read", "scopes": [],
     "input_schema": {"type": "object", "additionalProperties": false, "properties": {"id": {"type": "string"}}, "required": ["id"]},
     "request": {"method": "GET", "path": "/items/{id}"}, "response": {"mode": "body"}},
    {"slug": "TESTSVC_CREATE_ITEM", "title": {"de": "Anlegen", "en": "Create"}, "description": {"de": "x", "en": "x"}, "risk": "write", "scopes": [],
     "input_schema": {"type": "object", "additionalProperties": false, "properties": {"name": {"type": "string"}}, "required": ["name"]},
     "request": {"method": "POST", "path": "/items", "body": {"json": {"name": {"arg": "name"}}}}, "response": {"mode": "body"}},
    {"slug": "TESTSVC_DELETE_ITEM", "title": {"de": "Löschen", "en": "Delete"}, "description": {"de": "x", "en": "x"}, "risk": "destructive", "scopes": [],
     "input_schema": {"type": "object", "additionalProperties": false, "properties": {"id": {"type": "string"}}, "required": ["id"]},
     "request": {"method": "DELETE", "path": "/items/{id}"}, "response": {"mode": "body"}}
  ]
}"#;

fn active_svc(e: &Env) {
    let s = e.b(AgentRole::Supervisor, &Allow);
    s.draft_spec(SVC).unwrap();
    assert_eq!(s.validate("testsvc", "1.0.0").unwrap()["ok"], json!(true));
    let probes = vec![
        Probe { tool: "TESTSVC_GET_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] },
        Probe { tool: "TESTSVC_CREATE_ITEM".into(), args: json!({"name": "probe"}), cleanup: vec![Probe { tool: "TESTSVC_DELETE_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }] },
    ];
    let t = s.live_test("testsvc", "1.0.0", &probes, None).unwrap();
    assert_eq!(t["passed"], json!(true), "{t}");
    s.activate("testsvc", "1.0.0", None).unwrap();
}

fn zip_tested(e: &Env, role: AgentRole) {
    let b = e.b(role, &Allow);
    b.draft_from_catalog("zippopotam").unwrap();
    assert_eq!(b.validate("zippopotam", "1.0.0").unwrap()["ok"], json!(true));
    assert_eq!(b.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap()["passed"], json!(true));
}

#[test]
fn end_to_end_autonomous_draft_validate_test_activate_execute() {
    let e = env();
    let worker = e.b(AgentRole::Worker, &Allow);
    let d = worker.draft_from_catalog("zippopotam").unwrap();
    assert_eq!(d["slug"], json!("zippopotam"));
    let v = worker.validate("zippopotam", "1.0.0").unwrap();
    assert_eq!(v["ok"], json!(true), "{v}");
    assert_eq!(v["hosts"], json!(["api.zippopotam.us"]));
    assert!(worker.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).is_err());
    assert!(worker.activate("zippopotam", "1.0.0", None).is_err(), "no proof yet, no mission");
    let t = worker.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    assert_eq!(t["passed"], json!(true), "{t}");
    assert_eq!(t["review"]["untested"], json!(["ZIPPOPOTAM_FIND_POSTAL_CODES"]));
    // Worker: only inside a builder mission for this service.
    assert!(worker.activate("zippopotam", "1.0.0", None).unwrap_err().to_string().contains("builder mission"));
    assert!(worker.activate("zippopotam", "1.0.0", Some("self-work:internal:m1")).unwrap_err().to_string().contains("not a live builder mission"));
    e.gates.missions.borrow_mut().insert("self-work:internal:m2".into(), "other".into());
    assert!(worker.activate("zippopotam", "1.0.0", Some("self-work:internal:m2")).is_err());
    e.gates.missions.borrow_mut().insert("self-work:internal:m1".into(), "zippopotam".into());
    let a = worker.activate("zippopotam", "1.0.0", Some("self-work:internal:m1")).unwrap();
    assert_eq!(a["active"]["version"], json!("1.0.0"));
    assert!(e.gates.states.borrow().is_empty(), "no approval gate for adapter setup");
    let one = e.b(AgentRole::OneShot, &Allow);
    let s = one.search("postal code", false, 10).unwrap();
    assert!(s["tools"].as_array().unwrap().iter().any(|t| t["tool"] == "ZIPPOPOTAM_LOOKUP_POSTAL_CODE"));
    assert_eq!(one.get_schema("ZIPPOPOTAM_LOOKUP_POSTAL_CODE").unwrap()["risk"], json!("read"));
    let r = one.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).unwrap();
    assert_eq!(r["outcome"], json!("success"));
    assert_eq!(r["data"]["places"][0]["place name"], json!("Berlin"));
    let hist = std::fs::read_to_string(e.root.join("runtime/toolhub/adapter-specs/zippopotam/history.jsonl")).unwrap();
    for ev in ["draft", "live_test", "activate", "activate_proof", "execute"] {
        assert!(hist.contains(&format!("\"event\":\"{ev}\"")), "missing {ev} in {hist}");
    }
    assert!(hist.contains("self-work:internal:m1"), "mission recorded");
    assert!(!hist.contains("10115"), "no arguments in clear text");
    let sup = e.b(AgentRole::Supervisor, &Allow);
    assert_eq!(sup.list().unwrap()["adapters"][0]["versions"][0]["tested"]["passed"], json!(true));
    sup.deactivate("zippopotam").unwrap();
    assert!(one.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).is_err());
}

#[test]
fn oneshot_and_mcp_never_activate() {
    let e = env();
    zip_tested(&e, AgentRole::Supervisor);
    e.gates.missions.borrow_mut().insert("m".into(), "zippopotam".into());
    assert!(e.b(AgentRole::OneShot, &Allow).activate("zippopotam", "1.0.0", Some("m")).unwrap_err().to_string().contains("not_allowed"));
    assert!(e.b(AgentRole::Mcp, &Allow).activate("zippopotam", "1.0.0", Some("m")).is_err());
    assert!(e.b(AgentRole::Supervisor, &DenyWrites).activate("zippopotam", "1.0.0", None).unwrap_err().to_string().contains("policy denied"));
}

#[test]
fn changed_spec_after_test_is_refused() {
    let e = env();
    zip_tested(&e, AgentRole::Supervisor);
    let p = e.root.join("runtime/toolhub/adapter-specs/zippopotam/versions/1.0.0.json");
    let tampered = std::fs::read_to_string(&p).unwrap().replace("Look up postal code", "Look up postal codes");
    std::fs::write(&p, tampered).unwrap();
    assert!(e.b(AgentRole::Supervisor, &Allow).activate("zippopotam", "1.0.0", None).unwrap_err().to_string().contains("changed after the test"));
}

#[test]
fn stale_or_missing_proof_blocks_activation() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    assert!(sup.activate("zippopotam", "1.0.0", None).is_err(), "not tested");
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    e.now.fetch_add(REVIEW_MAX_AGE_SECS + 1, Ordering::SeqCst);
    assert!(sup.activate("zippopotam", "1.0.0", None).unwrap_err().to_string().contains("24 h"));
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    sup.activate("zippopotam", "1.0.0", None).unwrap();
}

#[test]
fn destructive_calls_still_need_their_own_gate() {
    let e = env();
    active_svc(&e);
    let sup = e.b(AgentRole::Supervisor, &Allow);
    assert_eq!(sup.execute("TESTSVC_GET_ITEM", &json!({"id": "1"}), None, None).unwrap()["outcome"], json!("success"));
    assert_eq!(sup.execute("TESTSVC_CREATE_ITEM", &json!({"name": "x"}), None, None).unwrap()["outcome"], json!("success"));
    assert!(e.b(AgentRole::Worker, &Allow).execute("TESTSVC_CREATE_ITEM", &json!({"name": "x"}), None, None).unwrap_err().to_string().contains("delegate_to_supervisor"));
    assert!(e.b(AgentRole::Supervisor, &DenyWrites).execute("TESTSVC_CREATE_ITEM", &json!({"name": "x"}), None, None).unwrap_err().to_string().contains("policy denied"));
    let args = json!({"id": "7"});
    let before = e.mock.calls.lock().unwrap().len();
    let r = sup.execute("TESTSVC_DELETE_ITEM", &args, None, None).unwrap();
    assert_eq!(r["outcome"], json!("approval_required"));
    assert_eq!(e.mock.calls.lock().unwrap().len(), before, "nothing sent before approval");
    let gate = r["gate_id"].as_str().unwrap().to_string();
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &args, None, Some(&gate)).is_err(), "undecided");
    assert!(e.b(AgentRole::Worker, &Allow).approve(&gate, Decision::Approve, "ja").is_err());
    assert!(sup.approve(&gate, Decision::Approve, " ").is_err(), "quote required");
    sup.approve(&gate, Decision::Approve, "ja, löschen").unwrap();
    assert!(sup.approve(&gate, Decision::Approve, "nochmal").is_err(), "decided once");
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &json!({"id": "8"}), None, Some(&gate)).is_err(), "other args");
    assert_eq!(sup.execute("TESTSVC_DELETE_ITEM", &args, None, Some(&gate)).unwrap()["outcome"], json!("success"));
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &args, None, Some(&gate)).unwrap_err().to_string().contains("already used"));
    // A rejected call gate never runs.
    let r = sup.execute("TESTSVC_DELETE_ITEM", &json!({"id": "9"}), None, None).unwrap();
    let g2 = r["gate_id"].as_str().unwrap().to_string();
    sup.approve(&g2, Decision::Reject, "nein").unwrap();
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &json!({"id": "9"}), None, Some(&g2)).is_err());
}

#[test]
fn live_test_probe_rules() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_spec(SVC).unwrap();
    let write_no_cleanup = vec![Probe { tool: "TESTSVC_CREATE_ITEM".into(), args: json!({"name": "x"}), cleanup: vec![] }];
    assert!(sup.live_test("testsvc", "1.0.0", &write_no_cleanup, None).unwrap_err().to_string().contains("cleanup"));
    let destructive = vec![Probe { tool: "TESTSVC_DELETE_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }];
    assert!(sup.live_test("testsvc", "1.0.0", &destructive, None).unwrap_err().to_string().contains("never run"));
    assert!(sup.live_test("testsvc", "1.0.0", &[], None).is_err());
    let write = vec![Probe { tool: "TESTSVC_CREATE_ITEM".into(), args: json!({"name": "x"}), cleanup: vec![Probe { tool: "TESTSVC_DELETE_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }] }];
    assert!(e.b(AgentRole::Worker, &Allow).live_test("testsvc", "1.0.0", &write, None).unwrap_err().to_string().contains("delegate_to_supervisor"));
    assert!(e.b(AgentRole::Mcp, &Allow).draft_spec(SVC).is_err());
}

#[test]
fn missing_access_answer_depends_on_role() {
    let e = env();
    let spec = SVC.replace(r#"[{"type": "none"}]"#, r#"[{"type": "bearer"}]"#).replace("testsvc", "authsvc").replace("TESTSVC", "AUTHSVC");
    let one = e.b(AgentRole::OneShot, &Allow);
    one.draft_spec(&spec).unwrap();
    let probes = vec![Probe { tool: "AUTHSVC_GET_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }];
    let t = one.live_test("authsvc", "1.0.0", &probes, None).unwrap();
    assert_eq!(t["passed"], json!(false));
    assert!(t["access_missing"].as_str().unwrap().starts_with("Zugang fehlt"));
    let t = e.b(AgentRole::Worker, &Allow).live_test("authsvc", "1.0.0", &probes, None).unwrap();
    assert!(t["access_missing"].as_str().unwrap().contains("Delegate to the supervisor"));
    assert!(e.mock.calls.lock().unwrap().is_empty(), "no request without credentials");
}

fn svc_v2(e: &Env) {
    let s = e.b(AgentRole::Supervisor, &Allow);
    let v2 = SVC.replace("\"version\": \"1.0.0\"", "\"version\": \"1.0.1\"");
    s.draft_spec(&v2).unwrap();
    s.live_test("testsvc", "1.0.1", &[Probe { tool: "TESTSVC_GET_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }], None).unwrap();
    s.activate("testsvc", "1.0.1", None).unwrap();
}

#[test]
fn rollback_needs_tested_previous_version() {
    let e = env();
    active_svc(&e);
    svc_v2(&e);
    let sup = e.b(AgentRole::Supervisor, &Allow);
    assert!(e.b(AgentRole::Worker, &Allow).rollback("testsvc").is_err());
    assert_eq!(sup.rollback("testsvc").unwrap()["active"]["version"], json!("1.0.0"));
}

#[test]
fn repeated_failures_roll_back_automatically() {
    let e = env();
    active_svc(&e);
    svc_v2(&e);
    let one = e.b(AgentRole::OneShot, &Allow);
    // A success in between resets the counter.
    for _ in 0..(AUTO_ROLLBACK_AFTER - 1) {
        assert_eq!(one.execute("TESTSVC_GET_ITEM", &json!({"id": "boom"}), None, None).unwrap()["outcome"], json!("provider_error"));
    }
    one.execute("TESTSVC_GET_ITEM", &json!({"id": "1"}), None, None).unwrap();
    for _ in 0..(AUTO_ROLLBACK_AFTER - 1) {
        one.execute("TESTSVC_GET_ITEM", &json!({"id": "boom"}), None, None).unwrap();
    }
    assert_eq!(e.b(AgentRole::Supervisor, &Allow).store().active("testsvc").unwrap().unwrap().version, "1.0.1");
    let r = one.execute("TESTSVC_GET_ITEM", &json!({"id": "boom"}), None, None).unwrap();
    assert_eq!(r["auto_rollback"], json!("1.0.0"));
    assert_eq!(e.b(AgentRole::Supervisor, &Allow).store().active("testsvc").unwrap().unwrap().version, "1.0.0");
    let hist = std::fs::read_to_string(e.root.join("runtime/toolhub/adapter-specs/testsvc/history.jsonl")).unwrap();
    assert!(hist.contains("\"event\":\"auto_rollback\""));
    // No ping-pong back to the broken version after an auto-rollback.
    for _ in 0..(AUTO_ROLLBACK_AFTER + 1) {
        assert!(one.execute("TESTSVC_GET_ITEM", &json!({"id": "boom"}), None, None).unwrap().get("auto_rollback").is_none());
    }
}

#[test]
fn role_rules_come_from_one_source() {
    for r in [AgentRole::Supervisor, AgentRole::Worker, AgentRole::OneShot, AgentRole::Mcp] {
        let t = roles::role_rules(r);
        assert!(t.len() > 40, "{r:?}");
        assert!(!t.contains("## role:"));
    }
    assert!(roles::role_rules(AgentRole::OneShot).contains("Zugang fehlt"));
    assert!(roles::role_rules(AgentRole::Supervisor).contains("no user approval to install or activate"));
    assert!(roles::role_rules(AgentRole::Worker).contains("builder mission"));
}

#[test]
fn catalog_specs_all_validate() {
    for (slug, text) in catalog::CATALOG {
        let a = SpecAdapter::from_json(text).unwrap_or_else(|e| panic!("{slug}: {e:#}"));
        assert_eq!(a.spec().slug, *slug);
    }
}

#[test]
fn mission_service_matches_dash_and_underscore_slugs() {
    // request-adapter stores `nager-date`; the toolkit slug is `nager_date` (dogfood 2026-10-17).
    let e = env();
    let worker = e.b(AgentRole::Worker, &Allow);
    worker.draft_from_catalog("nager_date").unwrap();
    e.gates.missions.borrow_mut().insert("m-nd".into(), "nager-date".into());
    let err = worker.activate("nager_date", "1.0.1", Some("m-nd")).unwrap_err().to_string();
    assert!(!err.contains("builder mission"), "mission accepted, proof still missing: {err}");
    e.gates.missions.borrow_mut().insert("m-x".into(), "nager-dates".into());
    assert!(worker.activate("nager_date", "1.0.1", Some("m-x")).unwrap_err().to_string().contains("builder mission"));
}
