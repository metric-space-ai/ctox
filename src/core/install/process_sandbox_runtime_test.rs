//! Opt-in kernel acceptance; the driver installs/removes only its private policy.
//! The OS process, native backend, runtime owner and persisted allowlist are real.
//! Unrelated backend ports panic if reached; no sandbox result is substituted.
use crate::app_capabilities::{
    cap2::blob::{BlobAccess, BlobRef},
    cap3::{
        self,
        browser::{BrowserEngine, CapturedPage},
        ingest::IngestRuntime,
    },
    Actor, CapCallContext, CapabilityBackend, CtoxError, ErrorCode, ExecutorFence,
};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Unused;
impl BlobAccess for Unused {
    fn read(&self, _: &CapCallContext, _: &BlobRef) -> Result<Vec<u8>, CtoxError> {
        unreachable!("unused blob port")
    }
    fn publish(&self, _: &CapCallContext, _: &[u8], _: &str) -> Result<BlobRef, CtoxError> {
        unreachable!("unused blob port")
    }
}
impl BrowserEngine for Unused {
    fn op(
        &self,
        _: &CapCallContext,
        _: &str,
        _: Option<&str>,
        _: &str,
        _: &Value,
    ) -> Result<Value, CtoxError> {
        unreachable!("unused browser port")
    }
    fn session_stop(&self, _: &CapCallContext, _: &str) -> Result<Value, CtoxError> {
        unreachable!("unused browser port")
    }
    fn auth_assist_complete(&self, _: &CapCallContext, _: &str) -> Result<Value, CtoxError> {
        unreachable!("unused browser port")
    }
    fn capture(&self, _: &CapCallContext, _: &str, _: u64) -> Result<CapturedPage, CtoxError> {
        unreachable!("unused browser port")
    }
}
impl IngestRuntime for Unused {
    fn public_origin(&self) -> Option<String> {
        unreachable!("unused ingest port")
    }
    fn has_handler(&self, _: &str, _: &str) -> Result<bool, CtoxError> {
        unreachable!("unused ingest port")
    }
    fn dispatch(
        &self,
        _: &str,
        _: &str,
        _: Value,
    ) -> Result<std::sync::mpsc::Receiver<Value>, CtoxError> {
        unreachable!("unused ingest port")
    }
    fn store_body(&self, _: &str, _: &[u8], _: &str) -> Result<String, CtoxError> {
        unreachable!("unused ingest port")
    }
    fn load_body(&self, _: &str, _: &str) -> Result<Vec<u8>, CtoxError> {
        unreachable!("unused ingest port")
    }
}

#[test]
#[ignore = "requires the CAP-NOBLE driver, a private bwrap and a restricted AppArmor kernel"]
fn installed_apparmor_process_run_contract() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "runtime must run as the lane user"
    );
    assert_eq!(
        std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
            .unwrap()
            .trim(),
        "1"
    );
    let bwrap =
        PathBuf::from(std::env::var("CTOX_E20_ACCEPTANCE_BWRAP").expect("private bwrap path"));
    let phase = std::env::var("CTOX_E20_ACCEPTANCE_PHASE").expect("available or unavailable");
    assert!(matches!(phase.as_str(), "available" | "unavailable"));
    let profile = std::env::var("CTOX_E20_ACCEPTANCE_PROFILE").expect("private profile name");
    let output = PathBuf::from(std::env::var("CTOX_E20_ACCEPTANCE_OUTPUT").expect("evidence file"));
    assert!(bwrap.is_absolute() && bwrap.is_file() && output.is_absolute());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    cap3::tests::run_with_core_owners(root.clone(), move || {
        let ctx = CapCallContext {
            app_id: "cap-noble-probe".into(),
            app_ver: "1".into(),
            actor: Actor {
                id: "cap-noble-actor".into(),
                role: "admin".into(),
            },
            command_id: "cap-noble-command".into(),
            call_id: "cap-noble-call".into(),
            fence: ExecutorFence {
                run_id: "cap-noble-run".into(),
                participant: "core".into(),
                fence: 1,
                expires_at_core_ms: u64::MAX,
            },
        };
        let workspace = cap3::process::workspace(&root, &ctx);
        let ctox = bwrap.with_file_name("ctox");
        assert!(
            ctox.is_file(),
            "regular CTOX binary must be installed beside bwrap"
        );
        crate::inference::runtime_env::set_runtime_env_value(
            &root,
            cap3::process::ALLOWLIST_KEY,
            &json!({
                "probe":{"program":"/bin/cat","env":{"PATH":"/usr/bin:/bin"}},
                "ctox-version":{"program":ctox,"env":{
                    "PATH":"/usr/bin:/bin","CTOX_ROOT":workspace,
                    "CTOX_STATE_ROOT":workspace.join("ctox-state")
                }}
            })
            .to_string(),
        )
        .unwrap();
        crate::inference::runtime_env::set_runtime_env_value(
            &root,
            cap3::sandbox::SANDBOX_KEY,
            &json!({"bwrap":bwrap}).to_string(),
        )
        .unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("marker"), "E20_PRIVATE_WORKSPACE\n").unwrap();
        let route = cap3::ROUTES
            .iter()
            .find(|r| r.capability == "process" && r.function == "run")
            .unwrap();
        let call = route.call(&json!({"payload":{
            "program":"probe", "args":["marker","/proc/self/attr/current","/proc/self/status"], "timeout_ms":5000
        }})).unwrap();
        let clock = || 1_000;
        let backend = cap3::Cap3Backend {
            root: &root,
            blobs: &Unused,
            intake: &cap3::secrets::CoreSecretIntake,
            browser: &Unused,
            ingest: &Unused,
            now_ms: &clock,
            files: None,
        };
        let result = backend.execute(&ctx, &call);
        let evidence = match &result {
            Ok(value) => json!({"phase":phase,"value":value}),
            Err(error) => json!({"phase":phase,"error":error}),
        };
        std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        println!("CAP_NOBLE_OBSERVED {}", evidence);
        if phase == "unavailable" {
            assert_eq!(
                result
                    .expect_err("missing policy must stop process.run")
                    .code,
                ErrorCode::Unavailable
            );
            return;
        }
        let value = result.expect("installed policy must permit the actual sandbox");
        assert_eq!(value["exit"], 0, "{value}");
        assert_eq!(value["timed_out"], false, "{value}");
        let text = value["stdout"].as_str().unwrap();
        assert!(text.contains("E20_PRIVATE_WORKSPACE"), "{value}");
        assert!(
            text.contains(&profile),
            "private AppArmor policy must label the payload: {value}"
        );
        assert!(
            text.contains("CapEff:\t0000000000000000"),
            "payload capabilities must be empty: {value}"
        );
        let call = route
            .call(&json!({"payload":{
                "program":"ctox-version","args":["version"],"timeout_ms":10000
            }}))
            .unwrap();
        let version = backend
            .execute(&ctx, &call)
            .expect("regular CTOX binary must run inside the sandbox");
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(&json!({
                "phase":phase,"value":value,"regular_ctox_version":version
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(version["exit"], 0, "{version}");
        assert!(
            !version["stdout"].as_str().unwrap().trim().is_empty(),
            "{version}"
        );
    });
    drop(temp);
}
