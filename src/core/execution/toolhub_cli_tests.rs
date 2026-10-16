use super::*;
use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};

struct Mock;
impl HttpTransport for Mock {
    fn send(&self, r: HttpRequest) -> Result<HttpResponse> {
        assert!(r.url.starts_with("https://api.zippopotam.us/"), "{}", r.url);
        let body = json!({"country": "Germany", "places": [{"place name": "Berlin"}]});
        Ok(HttpResponse { status: 200, body: serde_json::to_vec(&body)?, session_id: None })
    }
}

fn root(name: &str) -> std::path::PathBuf {
    let r = std::env::temp_dir().join(format!("ctox-toolhub-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    r
}

fn cli(root: &Path, args: &[&str]) -> Result<Value> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    run(root, &args, Some(Arc::new(Mock)))
}

/// What the approval-reply sweep does for an authorized owner reply.
fn owner_approves(root: &Path, gate: &str) {
    let conn = rusqlite::Connection::open(crate::paths::core_db(root)).unwrap();
    conn.execute_batch("CREATE TABLE IF NOT EXISTS ticket_approval_reply_ledger (message_key TEXT PRIMARY KEY, work_id TEXT NOT NULL, action TEXT NOT NULL, sender_address TEXT NOT NULL, body_sha256 TEXT NOT NULL, residual_text TEXT NOT NULL, decision_status TEXT NOT NULL, followup_message_key TEXT, observed_at TEXT NOT NULL, applied_at TEXT);").unwrap();
    conn.execute("INSERT INTO ticket_approval_reply_ledger VALUES ('owner-reply-1', ?1, 'approve', 'owner@example.org', 'abcdef0123456789abcdef', '', 'observed', NULL, '2026-10-16T00:00:00Z', NULL)", [gate]).unwrap();
    tickets::set_ticket_approval_gate_state_from_authorized_reply(root, gate, "closed", "owner-reply-1").unwrap();
}

const PROBES: &str = r#"[{"tool":"ZIPPOPOTAM_LOOKUP_POSTAL_CODE","args":{"country":"de","postal_code":"10115"}}]"#;

#[test]
fn cli_end_to_end_with_real_approval_gate_work_items() {
    let r = root("e2e");
    cli(&r, &["--as", "worker", "draft-spec", "--from-catalog", "zippopotam"]).unwrap();
    assert_eq!(cli(&r, &["--as", "worker", "validate", "zippopotam", "1.0.0"]).unwrap()["ok"], json!(true));
    assert_eq!(cli(&r, &["--as", "worker", "live-test", "zippopotam", "1.0.0", "--probes-json", PROBES]).unwrap()["passed"], json!(true));
    let s = cli(&r, &["--as", "worker", "submit", "zippopotam", "1.0.0"]).unwrap();
    let gate = s["gate_id"].as_str().unwrap().to_string();
    let item = tickets::load_ticket_self_work_item(&r, &gate).unwrap().unwrap();
    assert_eq!(item.kind, "approval-gate");
    assert_eq!(item.state, "open");
    assert!(cli(&r, &["--as", "worker", "approve", "--gate", &gate, "--user-quote", "ja"]).is_err());
    // No owner reply yet: even a supervisor cannot approve.
    let err = cli(&r, &["--as", "supervisor", "approve", "--gate", &gate, "--user-quote", "ja"]).unwrap_err();
    assert!(err.to_string().contains("waiting_for_user"), "{err}");
    owner_approves(&r, &gate);
    cli(&r, &["--as", "supervisor", "approve", "--gate", &gate, "--decision", "approve", "--user-quote", "Ja, freigeben"]).unwrap();
    assert_eq!(tickets::load_ticket_self_work_item(&r, &gate).unwrap().unwrap().state, "closed");
    cli(&r, &["--as", "supervisor", "activate", "zippopotam", "1.0.0"]).unwrap();
    let out = cli(&r, &["execute", "ZIPPOPOTAM_LOOKUP_POSTAL_CODE", "--args-json", r#"{"country":"de","postal_code":"10115"}"#]).unwrap();
    assert_eq!(out["data"]["places"][0]["place name"], json!("Berlin"));
    let found = cli(&r, &["search", "postal"]).unwrap();
    assert!(!found["tools"].as_array().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn business_os_policy_gates_activation() {
    let p = CtoxPolicy { actor: BusinessOsActor::new(Some("u".into()), "user") };
    // Workspace-wide data read is chef/admin only in Business OS policy.
    assert!(p.check(Permission::DataRead).is_err());
    assert!(p.check(Permission::IntegrationsManage).unwrap_err().to_string().contains("policy_denied"));
    let chef = CtoxPolicy { actor: BusinessOsActor::new(Some("c".into()), "chef") };
    assert!(chef.check(Permission::IntegrationsManage).is_ok());
}

#[test]
fn default_role_is_least_privilege_and_rules_print() {
    let r = root("default");
    let rules = cli(&r, &["rules"]).unwrap();
    assert_eq!(rules["role"], json!("oneshot"));
    assert!(cli(&r, &["submit", "zippopotam", "1.0.0"]).is_err());
    assert!(cli(&r, &["--as", "root", "list"]).is_err());
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn toolhub_is_relayed_through_the_daemon() {
    let argv: Vec<String> = ["toolhub", "execute"].iter().map(|s| s.to_string()).collect();
    assert!(crate::service::sandboxed_cli_command_allowed(&argv));
}

#[test]
fn request_adapter_queues_one_builder_mission_per_service() {
    let r = root("mission");
    let a = cli(&r, &["--as", "worker", "request-adapter", "Pipedrive", "--reason", "CRM sync"]).unwrap();
    assert_eq!(a["existing"], json!(false));
    assert_eq!(a["skill"], json!("adapter-builder"));
    let b = cli(&r, &["--as", "supervisor", "request-adapter", "pipedrive"]).unwrap();
    assert_eq!(b["work_id"], a["work_id"]);
    let item = tickets::load_ticket_self_work_item(&r, a["work_id"].as_str().unwrap()).unwrap().unwrap();
    assert_eq!(item.kind, "adapter-build");
    assert!(cli(&r, &["request-adapter", "pipedrive"]).unwrap_err().to_string().contains("not_allowed"));
    let _ = std::fs::remove_dir_all(&r);
}
