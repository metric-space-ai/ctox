use super::*;

// Component contract, not genuine model/sidecar E2E acceptance. Real writer,
// policy and CodingPanel; only the reply uses the existing SidecarRunner seam.
fn exercise_inflight_reply(revoke: bool) {
    use crate::business_os::policy::{self, sync_rights};
    let env = env();
    let authority = env._rt.block_on(sync_rights::StoreRightsAuthority::attach(
        env.fixture.writer(), "p6-r13-instance".into())).unwrap();
    env._rt.block_on(authority.replace(policy::base_release_rights(&[]), vec![])).unwrap();
    let _registration = policy::bind_policy_authority(authority.clone()).unwrap();
    let before = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    let runner = ScriptedRunner::new(&[("a.js", "agent edit\n")]);
    let runtime = env._rt.handle().clone();
    *runner.during.lock().unwrap() = Some(Box::new(move || {
        if revoke {
            // The reply cannot return before the sequenced revocation commits.
            runtime.block_on(authority.replace(vec![], vec![])).unwrap();
            assert!(!policy::evaluate(&actor("alice", "admin"),
                BusinessOsPermission::AppsSourceEdit,
                &BusinessOsScope::module("notes", false)).allowed,
                "revocation fixture did not deny the current authority");
        }
    }));
    let admitted = env.panel(runner).execute(actor("alice", "admin"),
        turn("r13-command", turn_payload(&env, "alice", "r13-turn")), 10).unwrap();
    assert_eq!(admitted.result["status"], "running", "admission must be allowed before revocation");
    let terminal = admitted.worker.unwrap().join().unwrap();
    let after = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    let original_command = env.row("business_commands", "r13-command").unwrap();
    assert_eq!(original_command.1["payload"]["prompt"], "rename", "original input must remain inspectable");
    if !revoke {
        assert_eq!(terminal["state"], "done", "positive control must apply the genuine completion path");
        assert_ne!(after.0, before.0, "positive control did not patch the draft");
        eprintln!("R13 positive control: allowed reply changed the native draft");
        return;
    }
    assert_eq!(after, before, "R13: draft changed after the current native rights were revoked; terminal={terminal}");
    assert!(matches!(terminal["state"].as_str(), Some("failed" | "rejected" | "revoked")), "{terminal}");
    assert_eq!(terminal["error"]["code"], "revoked", "{terminal}");
}
#[test]
fn r13_inflight_reply_cannot_patch_draft_after_committed_rights_revocation() {
    exercise_inflight_reply(false);
    exercise_inflight_reply(true);
}
