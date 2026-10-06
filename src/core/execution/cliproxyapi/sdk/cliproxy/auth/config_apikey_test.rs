// ref: sdk/cliproxy/auth/config_apikey_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{is_config_api_key_auth, Auth};

#[test]
fn config_api_key_classification_matches_upstream() {
    assert!(!is_config_api_key_auth(None));
    let mut missing = Auth::default();
    missing
        .attributes
        .insert("source".into(), "config:codex[x]".into());
    assert!(!is_config_api_key_auth(Some(&missing)));

    let mut oauth = Auth::default();
    oauth.provider = "codex".into();
    oauth.attributes.extend([
        ("auth_kind".into(), "oauth".into()),
        ("api_key".into(), "k".into()),
        ("source".into(), "config:codex[abc]".into()),
    ]);
    assert!(!is_config_api_key_auth(Some(&oauth)));

    let mut api_key = Auth::default();
    api_key.provider = "codex".into();
    api_key.attributes.extend([
        ("api_key".into(), "k".into()),
        ("source".into(), "config:codex[abc]".into()),
    ]);
    assert!(is_config_api_key_auth(Some(&api_key)));

    let mut secret_held_elsewhere = Auth::default();
    secret_held_elsewhere.provider = "codex".into();
    secret_held_elsewhere.attributes.extend([
        ("auth_kind".into(), "apikey".into()),
        ("source".into(), "config:codex[abc]".into()),
    ]);
    assert!(is_config_api_key_auth(Some(&secret_held_elsewhere)));
}
