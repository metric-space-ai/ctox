//! Box port. The Activepieces `piece-box` has **no actions**: its only action is
//! `createCustomApiCallAction` (excluded by the port rules) plus three webhook
//! triggers (`new_file`, `new_folder`, `new_comment`, also excluded). There is
//! therefore no `support::Case` to record — every Case would need a method +
//! URL from the TypeScript `run()` code, and the only requests in the source
//! (`POST/DELETE …/webhooks` in `lib/common/index.ts`) serve those excluded
//! webhooks. Inventing Box REST paths (files, folders, search, …) is forbidden.
//!
//! What the source *does* pin down and these tests lock in: the exact OAuth2
//! auth of `boxAuth`, the `common.baseUrl`, and the empty tool catalog. The
//! harness stays wired (`assert_full_coverage`, live loop) so that actions of a
//! future `piece-box` version only need a tool plus one `Case` each.

// The shared harness imports `Case`/`run`/http types that this port cannot use
// (nothing to record); keep the shared module warning-free for this binary.
#![allow(unused_imports)]

mod support;

use ctox_toolhub::model::AuthScheme;
use ctox_toolhub::toolkits::box_storage::{BoxStorage, API_BASE, SLUG};
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::json;
use support::{assert_full_coverage, live};

fn adapter() -> BoxStorage {
    BoxStorage::with_api_base(API_BASE)
}

#[test]
fn auth_matches_boxauth_exactly() {
    let a = adapter();
    assert_eq!(a.spec().slug, SLUG);
    let [AuthScheme::OAuth2(o)] = a.spec().auth_schemes.as_slice() else {
        panic!("piece-box is OAuth2 only");
    };
    assert_eq!(o.authorize_url, "https://account.box.com/api/oauth2/authorize");
    assert_eq!(o.token_url, "https://api.box.com/oauth2/token");
    assert_eq!(
        o.default_scopes,
        ["manage_webhook", "root_readonly", "root_readwrite"]
    );
    assert!(!o.pkce);
    assert_eq!(o.scope_separator, " ");
}

#[test]
fn api_base_is_the_piece_common_base_url() {
    assert_eq!(API_BASE, "https://api.box.com/2.0");
    assert_eq!(adapter().api_base(), API_BASE);
}

#[test]
fn catalog_has_no_tools_because_the_piece_has_no_actions() {
    let a = adapter();
    assert!(
        a.spec().tools.is_empty(),
        "a tool appeared without an endpoint in the piece source — see the module docs"
    );
    // Tool calls must fail loudly instead of hitting an invented endpoint.
    for tool in ["BOX_GET_FILE", "BOX_LIST_FOLDER_ITEMS", "BOX_SEARCH"] {
        let err = a
            .build_request(tool, &json!({"id": "123"}))
            .expect_err("no endpoint exists for this tool");
        assert!(err.to_string().contains("no actions"), "{err}");
    }
}

/// Every catalog tool needs a recorded `Case`; the catalog is empty today, so
/// the case list is empty too. If a future `piece-box` action is ported, add a
/// `Case` with the method/URL from its TypeScript `run()` and this check starts
/// enforcing it.
#[test]
fn every_ported_tool_has_a_recorded_case() {
    let a = adapter();
    assert_full_coverage(&a, &[]);
}

/// Live read-only calls through the hub exec path, token from
/// `CTOX_TOOLHUB_LIVE_BOX_TOKEN` (never printed). `piece-box` exposes no
/// read-only tool, so the loop below has nothing to call today — it stays
/// generic so any future ported action is verified live automatically
/// (panics on Err, skips only when the token is unset).
#[test]
#[ignore]
fn live_read_only_tools() {
    let a = adapter();
    let read_only: Vec<String> = a
        .spec()
        .tools
        .iter()
        .filter(|t| !t.mutating)
        .map(|t| t.slug.clone())
        .collect();
    for tool in read_only {
        let Some(result) = live(&a, &tool, json!({})) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
