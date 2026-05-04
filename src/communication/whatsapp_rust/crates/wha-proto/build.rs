//! Compile a curated subset of WhatsApp's protos with `protox` (pure-Rust)
//! and feed the resulting FileDescriptorSet into `prost-build`. Avoids needing
//! a `protoc` binary on the host.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let proto_root = workspace_root.join("_upstream/whatsmeow/proto");

    // Tell rustc that `wha_proto_stubbed` is a recognised cfg flag whether
    // we set it or not — silences `unexpected cfg` warnings in lib.rs.
    println!("cargo:rustc-check-cfg=cfg(wha_proto_stubbed)");

    if !proto_root.exists() {
        println!(
            "cargo:warning=upstream proto tree not found at {} — wha-proto will be empty",
            proto_root.display()
        );
        // Tell lib.rs (via cfg gate) that the upstream-derived types it
        // would otherwise reference (e.g. `MessageApplication`) are absent.
        // Without this signal, compile fails on `pub type EncryptedMessage =
        // MessageApplication;` even though the build script "succeeds" by
        // writing empty stubs.
        println!("cargo:rustc-cfg=wha_proto_stubbed");
        // Emit empty stubs so `include!(OUT_DIR)` still works.
        let out = std::env::var("OUT_DIR")?;
        for stub in [
            "wa_cert",
            "wa_web_protobufs_wa6",
            "wa_common",
            "wa_companion_reg",
            "wa_adv",
            "wa_mms_retry",
            "wa_status_attributions",
            "waai_common_deprecated",
            "wa_web_protobufs_ai_common",
            "wa_web_protobufs_e2e",
            "wa_web_protobufs_chat_lock_settings",
            "wa_web_protobufs_device_capabilities",
            "wa_web_protobufs_user_password",
            "wa_web_protobuf_sync_action",
            "wa_web_protobufs_web",
            "wa_web_protobufs_history_sync",
            "wa_server_sync",
            "wa_msg_application",
            "wa_msg_transport",
            "wa_consumer_application",
        ] {
            std::fs::write(format!("{out}/{stub}.rs"), "// upstream proto missing\n")?;
        }
        return Ok(());
    }

    let proto_files = [
        proto_root.join("waCert/WACert.proto"),
        proto_root.join("waWa6/WAWebProtobufsWa6.proto"),
        proto_root.join("waCommon/WACommon.proto"),
        proto_root.join("waCompanionReg/WACompanionReg.proto"),
        proto_root.join("waAdv/WAAdv.proto"),
        // waE2E and its dependencies — the message-body protocol.
        proto_root.join("waMmsRetry/WAMmsRetry.proto"),
        proto_root.join("waStatusAttributions/WAStatusAttributions.proto"),
        proto_root.join("waAICommonDeprecated/WAAICommonDeprecated.proto"),
        proto_root.join("waAICommon/WAWebProtobufsAICommon.proto"),
        proto_root.join("waE2E/WAWebProtobufsE2E.proto"),
        // History sync + transitive deps (waSyncAction, waChatLockSettings,
        // waWeb and their deps waUserPassword, waDeviceCapabilities).
        proto_root.join("waChatLockSettings/WAWebProtobufsChatLockSettings.proto"),
        proto_root.join("waDeviceCapabilities/WAWebProtobufsDeviceCapabilities.proto"),
        proto_root.join("waUserPassword/WAWebProtobufsUserPassword.proto"),
        proto_root.join("waSyncAction/WAWebProtobufSyncAction.proto"),
        proto_root.join("waWeb/WAWebProtobufsWeb.proto"),
        proto_root.join("waHistorySync/WAWebProtobufsHistorySync.proto"),
        // App state sync — patches, snapshots, mutations.
        proto_root.join("waServerSync/WAServerSync.proto"),
        // Messenger / Facebook E2EE wrapper protos. `MessageApplication` is
        // the outer envelope (`prepareFBMessage` upstream); `MessageTransport`
        // wraps it for the per-device Signal-encrypt step. Both are needed
        // for the `send_fb_message` port — see `crates/wha-client/src/send_fb.rs`.
        proto_root.join("waMsgApplication/WAMsgApplication.proto"),
        proto_root.join("waMsgTransport/WAMsgTransport.proto"),
        // Consumer application — the inner sub-protocol of an FB-side
        // `MessageApplication`. Required by `decode_armadillo_message` to
        // type-resolve the `consumerMessage` arm.
        proto_root.join("waConsumerApplication/WAConsumerApplication.proto"),
    ];

    for f in &proto_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }

    let fds = protox::compile(&proto_files, [proto_root.clone()])?;

    let mut cfg = prost_build::Config::new();
    cfg.compile_fds(fds)?;

    Ok(())
}
