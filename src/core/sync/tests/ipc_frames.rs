use ctox_sync::authority::Command;
use ctox_sync::contracts::SyncIpcRequest;

#[test]
fn checkpoint_protection_rejects_the_retired_unsigned_replica_contract() {
    let unsigned = r#"{"type":"protectCheckpoint","jobId":"job","ownership":{"nodeId":1,"generation":1},"checkpoint":{"digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sequence":1,"replicas":[1,2]}}"#;
    assert!(serde_json::from_str::<Command>(unsigned).is_err());
}

#[test]
fn ipc_contract_rejects_impersonation_unknown_operations_and_unsafe_integers() {
    for invalid in [
        r#"{"version":1,"requestId":"x","operation":{"type":"create","actor":2,"spec":{}}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"notAnOperation"}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"validate","jobId":"job","ownership":{"nodeId":9007199254740992,"generation":1}}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"hello","unexpected":true}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"workerMembership","nodeId":9007199254740992}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"workerMembership","nodeId":4,"actor":1}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"takeOver","jobId":"job","expected":{"nodeId":1,"generation":1},"checkpointDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","owner":2}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"protectCheckpoint","jobId":"job","ownership":{"nodeId":1,"generation":1},"receipts":[],"actor":1}}"#,
        r#"{"version":1,"requestId":"x","operation":{"type":"takeOver","jobId":"job","expected":{"nodeId":1,"generation":9007199254740992},"checkpointDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
    ] {
        assert!(
            serde_json::from_str::<SyncIpcRequest>(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}
