//! Per-connection parameters (instance URL, subdomain, shop, data center):
//! stored with the account, applied to every request, asked for by the credential card.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use ctox_toolhub::oauth2::RedirectTarget;
use ctox_toolhub::*;
use serde_json::{json, Value};

#[derive(Default)]
struct Rec(Mutex<Vec<HttpRequest>>);
impl HttpTransport for Rec {
    fn send(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.0.lock().unwrap().push(r);
        Ok(HttpResponse { status: 200, body: b"{}".to_vec(), session_id: None })
    }
}

fn hub(rec: Arc<Rec>) -> ToolHub<MemorySecretBackend> {
    ToolHub::new(MemorySecretBackend::default(), rec, RedirectTarget::Loopback { port: 1 })
}

fn cfg(h: &ToolHub<MemorySecretBackend>, toolkit: &str, scheme: AuthSchemeKind) -> String {
    let id = format!("{toolkit}-cfg");
    h.upsert_auth_config(
        &AuthConfig { id: id.clone(), toolkit: toolkit.into(), scheme, source: ClientCredentialSource::None, client_id: None, scopes: vec![], enabled: true },
        None,
    )
    .unwrap();
    id
}

fn p(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Connect, run one read tool, return the URL the provider received.
fn url_for(toolkit: &str, scheme: AuthSchemeKind, cred: &str, params: &[(&str, &str)], tool: &str) -> String {
    let rec = Arc::new(Rec::default());
    let h = hub(rec.clone());
    let id = cfg(&h, toolkit, scheme);
    let acct = h.connect_with_credential_params(&id, "_instance", "t", cred, &p(params)).unwrap();
    assert!(!acct.params.is_empty(), "params stored with the account");
    let out = h.execute("_instance", tool, &json!({}), None).unwrap();
    assert!(matches!(out, ToolOutcome::Success { .. } | ToolOutcome::ProviderError { .. }), "{out:?}");
    let reqs = rec.0.lock().unwrap();
    reqs.last().expect("request sent").url.clone()
}

#[test]
fn zendesk_subdomain_from_full_url() {
    let u = url_for("zendesk", AuthSchemeKind::Basic, "a@b.c/token:x", &[("subdomain", "https://Acme.zendesk.com/agent")], "ZENDESK_LIST_TICKETS");
    assert!(u.starts_with("https://acme.zendesk.com/api/v2/"), "{u}");
}

#[test]
fn freshdesk_subdomain() {
    let u = url_for("freshdesk", AuthSchemeKind::ApiKey, "key", &[("subdomain", "acme")], "FRESHDESK_LIST_TICKETS");
    assert!(u.starts_with("https://acme.freshdesk.com/"), "{u}");
}

#[test]
fn jira_site() {
    let u = url_for("jira", AuthSchemeKind::Basic, "a@b.c:tok", &[("site", "acme.atlassian.net")], "JIRA_LIST_PROJECTS");
    assert!(u.starts_with("https://acme.atlassian.net/rest/"), "{u}");
}

#[test]
fn mailchimp_datacenter_derived_from_api_key() {
    let u = url_for("mailchimp", AuthSchemeKind::Bearer, "0123abcd-us21", &[], "MAILCHIMP_LIST_AUDIENCES");
    assert!(u.starts_with("https://us21.api.mailchimp.com/3.0/"), "{u}");
}

#[test]
fn shopify_shop_name_injected_as_argument() {
    let u = url_for("shopify", AuthSchemeKind::ApiKey, "shpat_x", &[("shop_name", "my-shop.myshopify.com")], "SHOPIFY_LIST_CUSTOMERS");
    assert!(u.starts_with("https://my-shop.myshopify.com/admin/api/"), "{u}");
}

#[test]
fn salesforce_instance_url() {
    let u = url_for("salesforce", AuthSchemeKind::Bearer, "tok", &[("instance_url", "https://acme.my.salesforce.com/")], "SALESFORCE_LIST_OBJECTS");
    assert!(u.starts_with("https://acme.my.salesforce.com/services/data/"), "{u}");
}

#[test]
fn salesforce_instance_url_from_oauth_token_response() {
    let a = ctox_toolhub::toolkits::builtin().into_iter().find(|a| a.spec().slug == "salesforce").unwrap();
    let extra = p(&[("instance_url", "https://acme.my.salesforce.com")]);
    assert_eq!(a.derive_connection_params(None, &extra).get("instance_url").map(String::as_str), Some("https://acme.my.salesforce.com"));
    let body = json!({"access_token": "t", "instance_url": "https://acme.my.salesforce.com", "token_type": "Bearer"});
    let t = ctox_toolhub::oauth2::parse_token_response(&body, " ").unwrap();
    assert_eq!(t.extra.get("instance_url").map(String::as_str), Some("https://acme.my.salesforce.com"));
}

#[test]
fn hosts_outside_provider_domain_are_rejected() {
    let h = hub(Arc::new(Rec::default()));
    for (tk, scheme, key, bad) in [
        ("zendesk", AuthSchemeKind::Basic, "subdomain", "evil.com/x"),
        ("zendesk", AuthSchemeKind::Basic, "subdomain", "a.b"),
        ("jira", AuthSchemeKind::Basic, "site", "https://evil.example"),
        ("salesforce", AuthSchemeKind::Bearer, "instance_url", "https://evil.example"),
        ("salesforce", AuthSchemeKind::Bearer, "instance_url", "http://acme.my.salesforce.com"),
        ("mailchimp", AuthSchemeKind::Bearer, "datacenter", "evil.com"),
        ("shopify", AuthSchemeKind::ApiKey, "shop_name", "a/b"),
    ] {
        let id = cfg(&h, tk, scheme);
        assert!(h.connect_with_credential_params(&id, "_instance", "t", "cred", &p(&[(key, bad)])).is_err(), "{tk} accepted {bad}");
    }
}

#[test]
fn missing_and_unknown_params_are_errors() {
    let h = hub(Arc::new(Rec::default()));
    let id = cfg(&h, "zendesk", AuthSchemeKind::Basic);
    let e = h.connect_with_credential(&id, "_instance", "t", "a/token:x").unwrap_err().to_string();
    assert!(e.contains("subdomain"), "{e}");
    assert!(h.connect_with_credential_params(&id, "_instance", "t", "x", &p(&[("subdomain", "acme"), ("foo", "1")])).is_err());
    // Toolkits without connection fields keep working unchanged.
    let gid = cfg(&h, "github", AuthSchemeKind::Bearer);
    assert!(h.connect_with_credential(&gid, "_instance", "t", "ghp_x").unwrap().params.is_empty());
}

#[test]
fn credential_card_lists_fields_in_de_and_en() {
    let h = hub(Arc::new(Rec::default()));
    let out = h.execute("_instance", "JIRA_LIST_PROJECTS", &json!({}), None).unwrap();
    let ToolOutcome::CredentialRequired { fields, .. } = &out else { panic!("{out:?}") };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].key, "site");
    assert!(!fields[0].label.de.is_empty() && !fields[0].label.en.is_empty());
    assert!(!fields[0].help.de.is_empty() && !fields[0].help.en.is_empty());
    let v: Value = serde_json::to_value(&out).unwrap();
    assert!(v.to_string().contains("\"site\""), "fields serialized for the card: {v}");
}

#[test]
fn account_without_required_params_asks_again() {
    // Accounts stored before params existed (or with params removed) get a card, no request.
    let rec = Arc::new(Rec::default());
    let h = hub(rec.clone());
    let id = cfg(&h, "jira", AuthSchemeKind::Basic);
    let mut acct = h.connect_with_credential_params(&id, "_instance", "t", "a:b", &p(&[("site", "acme")])).unwrap();
    acct.params.clear();
    h.store().save(&acct, None).unwrap();
    let out = h.execute("_instance", "JIRA_LIST_PROJECTS", &json!({}), None).unwrap();
    assert!(matches!(out, ToolOutcome::CredentialRequired { .. }), "{out:?}");
    assert!(rec.0.lock().unwrap().is_empty());
}
