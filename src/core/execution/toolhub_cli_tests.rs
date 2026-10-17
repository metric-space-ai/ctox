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

const PROBES: &str = r#"[{"tool":"ZIPPOPOTAM_LOOKUP_POSTAL_CODE","args":{"country":"de","postal_code":"10115"}}]"#;

#[test]
fn cli_end_to_end_autonomous_activation_inside_builder_mission() {
    let r = root("e2e");
    let m = cli(&r, &["--as", "worker", "request-adapter", "zippopotam", "--reason", "postal codes"]).unwrap();
    let mission = m["work_id"].as_str().unwrap().to_string();
    cli(&r, &["--as", "worker", "draft-spec", "--from-catalog", "zippopotam"]).unwrap();
    assert_eq!(cli(&r, &["--as", "worker", "validate", "zippopotam", "1.0.0"]).unwrap()["ok"], json!(true));
    assert_eq!(cli(&r, &["--as", "worker", "live-test", "zippopotam", "1.0.0", "--probes-json", PROBES, "--mission", &mission]).unwrap()["passed"], json!(true));
    // One-shot never activates; a worker only inside its builder mission.
    assert!(cli(&r, &["activate", "zippopotam", "1.0.0", "--mission", &mission]).unwrap_err().to_string().contains("not_allowed"));
    assert!(cli(&r, &["--as", "worker", "activate", "zippopotam", "1.0.0"]).is_err());
    assert!(cli(&r, &["--as", "worker", "submit", "zippopotam", "1.0.0"]).is_err(), "no submit step any more");
    let a = cli(&r, &["--as", "worker", "activate", "zippopotam", "1.0.0", "--mission", &mission]).unwrap();
    assert_eq!(a["active"]["version"], json!("1.0.0"));
    // The builder mission closes itself, with a note naming the tested tools.
    assert_eq!(a["mission"]["closed"], json!(true), "{a}");
    assert_eq!(tickets::load_ticket_self_work_item(&r, &mission).unwrap().unwrap().state, "closed");
    // The core grant re-checks the store: a fresh mission cannot be closed with
    // a made-up sha256 or without its own proof.
    let m2 = cli(&r, &["--as", "worker", "request-adapter", "zippopotam"]).unwrap();
    assert_eq!(m2["existing"], json!(false), "the closed mission is not reused");
    let m2 = m2["work_id"].as_str().unwrap().to_string();
    assert!(tickets::close_adapter_build_mission_after_activation(&r, &m2, "zippopotam", "deadbeef", "x").is_err());
    let sha = a["sha256"].as_str().unwrap();
    assert!(tickets::close_adapter_build_mission_after_activation(&r, &m2, "zippopotam", sha, "x").unwrap_err().to_string().contains("does not belong"));
    assert_eq!(tickets::load_ticket_self_work_item(&r, &m2).unwrap().unwrap().state, "open");
    let gates = tickets::list_ticket_self_work_items(&r, Some("internal"), None, 100).unwrap();
    assert!(gates.iter().all(|i| i.kind != "approval-gate"), "adapter setup opens no approval gate (no nag)");
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

#[test]
fn draft_from_openapi_file_validates_and_saves_probes() {
    let r = root("openapi");
    let doc = concat!(env!("CARGO_MANIFEST_DIR"), "/src/core/execution/toolhub/tests/fixtures/openapi/pegel.yaml");
    let out = cli(&r, &["--as", "worker", "draft-spec", "--from-openapi", doc]).unwrap();
    assert_eq!(out["slug"], json!("pegel_online"));
    assert_eq!(out["validate"]["ok"], json!(true), "{out}");
    assert_eq!(out["probes"].as_array().unwrap().len(), 5);
    assert!(std::path::Path::new(out["probes_file"].as_str().unwrap()).exists());
    // One-shot workers may draft from a file but never fetch URLs through core.
    assert!(cli(&r, &["draft-spec", "--from-openapi", "https://example.org/openapi.yaml"]).unwrap_err().to_string().contains("not_allowed"));
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn fake_ip_egress_is_a_typed_supervisor_setting() {
    let r = root("config");
    assert_eq!(cli(&r, &["config", "show"]).unwrap()["fake_ip_egress"], json!(false));
    assert!(cli(&r, &["config", "set", "fake-ip-egress", "on"]).unwrap_err().to_string().contains("not_allowed"));
    assert!(cli(&r, &["--as", "worker", "config", "set", "fake-ip-egress", "on"]).is_err());
    assert_eq!(cli(&r, &["--as", "supervisor", "config", "set", "fake-ip-egress", "on"]).unwrap()["fake_ip_egress"], json!(true));
    assert_eq!(cli(&r, &["config", "show"]).unwrap()["fake_ip_egress"], json!(true));
    assert_eq!(cli(&r, &["--as", "supervisor", "config", "set", "fake-ip-egress", "off"]).unwrap()["fake_ip_egress"], json!(false));
    assert!(cli(&r, &["--as", "supervisor", "config", "set", "fake-ip-egress", "maybe"]).is_err());
    let _ = std::fs::remove_dir_all(&r);
}
