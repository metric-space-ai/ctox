// ref: sdk/api/handlers/openai/openai_videos_handlers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

#[test]
fn supported_video_models_and_request_mapping_match_contract() {
    assert!(is_supported_videos_model("sora-2"));
    assert!(is_supported_videos_model("xai/grok-imagine-video"));
    assert!(!is_supported_videos_model("gpt-5"));
    let request: Value = serde_json::from_slice(
        &build_xai_videos_create_request(br#"{"prompt":"move","seconds":"8"}"#, "sora-2").unwrap(),
    )
    .unwrap();
    assert_eq!(request["model"], "grok-imagine-video");
    assert_eq!(request["prompt"], "move");
    assert_eq!(request["duration"], "8");
}

#[test]
fn auth_binding_is_instance_owned_and_expires() {
    let store = VideoAuthBindingStore::default();
    store.set("video-1", "auth-1", "sora-2", Duration::from_secs(1));
    assert_eq!(
        store.get("video-1"),
        Some(("auth-1".to_owned(), "sora-2".to_owned()))
    );
    store.set("video-2", "auth-2", "sora-2", Duration::ZERO);
    assert!(store.get("video-2").is_none());
}
