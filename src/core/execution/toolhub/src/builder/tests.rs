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
}
impl ApprovalGates for Gates {
    fn open(&self, _r: &GateRequest) -> Result<String> {
        let id = format!("gate-{}", self.states.borrow().len() + 1);
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
}
impl Gates {
    /// What the progressive-autonomy sweep does: close without a decision.
    fn force_close(&self, id: &str) {
        self.states.borrow_mut().insert(id.into(), "closed".into());
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

fn approved_svc(e: &Env, decision: Decision) {
    let s = e.b(AgentRole::Supervisor, &Allow);
    s.draft_spec(SVC).unwrap();
    let probes = vec![
        Probe { tool: "TESTSVC_GET_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] },
        Probe { tool: "TESTSVC_CREATE_ITEM".into(), args: json!({"name": "probe"}), cleanup: vec![Probe { tool: "TESTSVC_DELETE_ITEM".into(), args: json!({"id": "1"}), cleanup: vec![] }] },
    ];
    let t = s.live_test("testsvc", "1.0.0", &probes, None).unwrap();
    assert_eq!(t["passed"], json!(true), "{t}");
    let g = s.submit_for_approval("testsvc", "1.0.0").unwrap();
    s.approve(g["gate_id"].as_str().unwrap(), decision, "Ja, freigeben").unwrap();
    s.activate("testsvc", "1.0.0").unwrap();
}

#[test]
fn end_to_end_draft_validate_test_submit_approve_activate_execute() {
    let e = env();
    let worker = e.b(AgentRole::Worker, &Allow);
    let d = worker.draft_from_catalog("zippopotam").unwrap();
    assert_eq!(d["slug"], json!("zippopotam"));
    let v = worker.validate("zippopotam", "1.0.0").unwrap();
    assert_eq!(v["ok"], json!(true), "{v}");
    assert_eq!(v["hosts"], json!(["api.zippopotam.us"]));
    assert!(worker.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).is_err());
    let t = worker.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    assert_eq!(t["passed"], json!(true), "{t}");
    assert_eq!(t["review"]["untested"], json!(["ZIPPOPOTAM_FIND_POSTAL_CODES"]));
    let g = worker.submit_for_approval("zippopotam", "1.0.0").unwrap();
    let gate = g["gate_id"].as_str().unwrap().to_string();
    assert!(g["card_text"].as_str().unwrap().contains("api.zippopotam.us"));
    assert_eq!(worker.submit_for_approval("zippopotam", "1.0.0").unwrap()["gate_id"], json!(gate));
    assert!(worker.approve(&gate, Decision::Approve, "ok").unwrap_err().to_string().contains("delegate_to_supervisor"));
    assert!(worker.activate("zippopotam", "1.0.0").is_err());
    let sup = e.b(AgentRole::Supervisor, &Allow);
    assert!(sup.activate("zippopotam", "1.0.0").unwrap_err().to_string().contains("no user decision"));
    assert!(sup.approve(&gate, Decision::Approve, " ").is_err(), "quote required");
    sup.approve(&gate, Decision::Approve, "Ja, Postleitzahlen freigeben").unwrap();
    assert_eq!(e.gates.state(&gate).unwrap().as_deref(), Some("closed"));
    let a = sup.activate("zippopotam", "1.0.0").unwrap();
    assert_eq!(a["active"]["version"], json!("1.0.0"));
    let one = e.b(AgentRole::OneShot, &Allow);
    let s = one.search("postal code", false, 10).unwrap();
    assert!(s["tools"].as_array().unwrap().iter().any(|t| t["tool"] == "ZIPPOPOTAM_LOOKUP_POSTAL_CODE"));
    assert_eq!(one.get_schema("ZIPPOPOTAM_LOOKUP_POSTAL_CODE").unwrap()["risk"], json!("read"));
    let r = one.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).unwrap();
    assert_eq!(r["outcome"], json!("success"));
    assert_eq!(r["data"]["places"][0]["place name"], json!("Berlin"));
    let hist = std::fs::read_to_string(e.root.join("runtime/toolhub/adapter-specs/zippopotam/history.jsonl")).unwrap();
    for ev in ["draft", "live_test", "submit", "decision", "activate", "activate_gate", "execute"] {
        assert!(hist.contains(&format!("\"event\":\"{ev}\"")), "missing {ev} in {hist}");
    }
    assert!(!hist.contains("10115"), "no arguments in clear text");
    assert_eq!(sup.list().unwrap()["adapters"][0]["versions"][0]["gate"]["decision"], json!("approve"));
    sup.deactivate("zippopotam").unwrap();
    assert!(one.execute("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", &json!({"country": "de", "postal_code": "10115"}), None, None).is_err());
}

#[test]
fn gate_closed_without_decision_does_not_activate() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    let gate = sup.submit_for_approval("zippopotam", "1.0.0").unwrap()["gate_id"].as_str().unwrap().to_string();
    e.gates.force_close(&gate);
    assert!(sup.activate("zippopotam", "1.0.0").unwrap_err().to_string().contains("no user decision"));
}

#[test]
fn rejected_version_cannot_be_activated_or_resubmitted() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    let gate = sup.submit_for_approval("zippopotam", "1.0.0").unwrap()["gate_id"].as_str().unwrap().to_string();
    sup.approve(&gate, Decision::Reject, "Nein").unwrap();
    assert_eq!(e.gates.state(&gate).unwrap().as_deref(), Some("failed"));
    assert!(sup.activate("zippopotam", "1.0.0").is_err());
    assert!(sup.submit_for_approval("zippopotam", "1.0.0").unwrap_err().to_string().contains("rejected"));
    assert!(sup.approve(&gate, Decision::Approve, "doch").is_err(), "decided once");
}

#[test]
fn changed_spec_after_approval_is_refused() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    let gate = sup.submit_for_approval("zippopotam", "1.0.0").unwrap()["gate_id"].as_str().unwrap().to_string();
    sup.approve(&gate, Decision::Approve, "ja").unwrap();
    let p = e.root.join("runtime/toolhub/adapter-specs/zippopotam/versions/1.0.0.json");
    let tampered = std::fs::read_to_string(&p).unwrap().replace("Look up postal code", "Look up postal codes");
    std::fs::write(&p, tampered).unwrap();
    assert!(sup.activate("zippopotam", "1.0.0").unwrap_err().to_string().contains("changed after approval"));
}

#[test]
fn stale_or_missing_review_blocks_submit() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    assert!(sup.submit_for_approval("zippopotam", "1.0.0").is_err());
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    e.now.fetch_add(REVIEW_MAX_AGE_SECS + 1, Ordering::SeqCst);
    assert!(sup.submit_for_approval("zippopotam", "1.0.0").unwrap_err().to_string().contains("24 h"));
}

#[test]
fn read_only_approval_blocks_writes_and_destructive_calls_need_their_own_gate() {
    let e = env();
    approved_svc(&e, Decision::ReadOnly);
    let sup = e.b(AgentRole::Supervisor, &Allow);
    assert_eq!(sup.execute("TESTSVC_GET_ITEM", &json!({"id": "1"}), None, None).unwrap()["outcome"], json!("success"));
    assert!(sup.execute("TESTSVC_CREATE_ITEM", &json!({"name": "x"}), None, None).unwrap_err().to_string().contains("read_only_approval"));

    let e = env();
    approved_svc(&e, Decision::Approve);
    let sup = e.b(AgentRole::Supervisor, &Allow);
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
    sup.approve(&gate, Decision::Approve, "ja, löschen").unwrap();
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &json!({"id": "8"}), None, Some(&gate)).is_err(), "other args");
    assert_eq!(sup.execute("TESTSVC_DELETE_ITEM", &args, None, Some(&gate)).unwrap()["outcome"], json!("success"));
    assert!(sup.execute("TESTSVC_DELETE_ITEM", &args, None, Some(&gate)).unwrap_err().to_string().contains("already used"));
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
    assert!(e.b(AgentRole::OneShot, &Allow).submit_for_approval("testsvc", "1.0.0").is_err());
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

#[test]
fn rollback_needs_approved_previous_version() {
    let e = env();
    let sup = e.b(AgentRole::Supervisor, &Allow);
    sup.draft_from_catalog("zippopotam").unwrap();
    sup.live_test("zippopotam", "1.0.0", &zip_probe(), None).unwrap();
    let g = sup.submit_for_approval("zippopotam", "1.0.0").unwrap()["gate_id"].as_str().unwrap().to_string();
    sup.approve(&g, Decision::Approve, "ja").unwrap();
    sup.activate("zippopotam", "1.0.0").unwrap();
    let v2 = catalog::get("zippopotam").unwrap().replace("\"version\": \"1.0.0\"", "\"version\": \"1.0.1\"");
    sup.draft_spec(&v2).unwrap();
    sup.live_test("zippopotam", "1.0.1", &zip_probe(), None).unwrap();
    let g2 = sup.submit_for_approval("zippopotam", "1.0.1").unwrap()["gate_id"].as_str().unwrap().to_string();
    sup.approve(&g2, Decision::Approve, "ja").unwrap();
    sup.activate("zippopotam", "1.0.1").unwrap();
    assert_eq!(sup.rollback("zippopotam").unwrap()["active"]["version"], json!("1.0.0"));
    assert!(e.b(AgentRole::Worker, &Allow).rollback("zippopotam").is_err());
}

#[test]
fn role_rules_come_from_one_source() {
    for r in [AgentRole::Supervisor, AgentRole::Worker, AgentRole::OneShot, AgentRole::Mcp] {
        let t = roles::role_rules(r);
        assert!(t.len() > 40, "{r:?}");
        assert!(!t.contains("## role:"));
    }
    assert!(roles::role_rules(AgentRole::OneShot).contains("Zugang fehlt"));
    assert!(roles::role_rules(AgentRole::Supervisor).contains("even for read-only"));
}

#[test]
fn catalog_specs_all_validate() {
    for (slug, text) in catalog::CATALOG {
        let a = SpecAdapter::from_json(text).unwrap_or_else(|e| panic!("{slug}: {e:#}"));
        assert_eq!(a.spec().slug, *slug);
    }
}
