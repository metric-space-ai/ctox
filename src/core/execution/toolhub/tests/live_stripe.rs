//! Live Stripe verification through the hot-plug spec `specs/stripe.json`, TEST MODE ONLY.
//! Refuses to run (before every single call) unless the key starts with `sk_test_`.
//! Creates throwaway objects and cleans up: refund the test payment, delete the draft
//! invoice and the customer, deactivate the payment link, archive price and product
//! (Stripe does not allow deleting products that have prices).
//! `CTOX_TOOLHUB_LIVE_STRIPE_TOKEN=sk_test_… cargo test --test live_stripe -- --ignored --nocapture`
mod support;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};

fn test_mode_key() -> bool {
    std::env::var("CTOX_TOOLHUB_LIVE_STRIPE_TOKEN").is_ok_and(|k| k.trim().starts_with("sk_test_"))
}

fn scenario() {
    if std::env::var("CTOX_TOOLHUB_LIVE_STRIPE_TOKEN").is_err() {
        return;
    }
    assert!(test_mode_key(), "refusing: CTOX_TOOLHUB_LIVE_STRIPE_TOKEN is not a Stripe test-mode key (sk_test_…)");
    let mut fails = vec![];
    let mut run = |tool: &str, args: Value| -> Value {
        assert!(test_mode_key(), "refusing {tool}: not a test-mode key");
        match support::live_exec("stripe", AuthSchemeKind::Bearer, tool, args) {
            None => Value::Null,
            Some(Ok(v)) => {
                println!("LIVE PASS {tool}");
                v
            }
            Some(Err(e)) => {
                println!("LIVE FAIL {tool}: {}", e.chars().take(400).collect::<String>());
                fails.push(tool.to_string());
                Value::Null
            }
        }
    };
    let tag = format!("ctox-live-{}", std::process::id());
    let email = format!("{tag}@example.com");
    let c = run("STRIPE_CREATE_CUSTOMER", json!({"email": email, "name": "CTOX Toolhub Test",
        "address": {"city": "Berlin", "country": "DE"}, "metadata": {"ctox_test": tag}}));
    let cus = c["id"].as_str().expect("customer id").to_string();
    assert_eq!(c["metadata"]["ctox_test"], json!(tag), "bracketed metadata arrived");
    run("STRIPE_GET_CUSTOMER", json!({"customer": cus}));
    let u = run("STRIPE_UPDATE_CUSTOMER", json!({"customer": cus, "description": "aktualisiert", "metadata": {"step": "updated"}}));
    assert_eq!(u["metadata"]["step"], json!("updated"));
    let l = run("STRIPE_LIST_CUSTOMERS", json!({"limit": 1}));
    if l["has_more"] == json!(true) {
        let p2 = run("STRIPE_LIST_CUSTOMERS", json!({"limit": 1, "starting_after": l["next_starting_after"]}));
        assert_ne!(p2["data"][0]["id"], l["data"][0]["id"], "second page differs");
    }
    run("STRIPE_LIST_CUSTOMERS", json!({"email": email}));
    run("STRIPE_SEARCH_CUSTOMERS", json!({"query": format!("email:'{email}'")})); // search index may lag; call must succeed

    let p = run("STRIPE_CREATE_PRODUCT", json!({"name": format!("CTOX Testprodukt {tag}"), "metadata": {"ctox_test": tag}}));
    let prod = p["id"].as_str().expect("product id").to_string();
    run("STRIPE_LIST_PRODUCTS", json!({"active": true, "limit": 3}));
    let pr = run("STRIPE_CREATE_PRICE", json!({"product": prod, "unit_amount": 500, "currency": "eur"}));
    let price = pr["id"].as_str().expect("price id").to_string();
    let rp = run("STRIPE_CREATE_PRICE", json!({"product": prod, "unit_amount": 900, "currency": "eur", "recurring": {"interval": "month"}}));
    run("STRIPE_LIST_PRICES", json!({"product": prod}));
    let link = run("STRIPE_CREATE_PAYMENT_LINK", json!({"line_items": [{"price": price, "quantity": 1}]}));
    let inv = run("STRIPE_CREATE_INVOICE", json!({"customer": cus, "collection_method": "send_invoice", "days_until_due": 14, "description": "CTOX Test (Entwurf)"}));
    assert_eq!(inv["status"], json!("draft"));
    run("STRIPE_LIST_INVOICES", json!({"customer": cus, "status": "draft"}));
    let pi = run("STRIPE_CREATE_PAYMENT_INTENT", json!({"amount": 500, "currency": "eur", "customer": cus, "payment_method": "pm_card_visa", "confirm": true, "metadata": {"ctox_test": tag}}));
    assert_eq!(pi["status"], json!("succeeded"));
    run("STRIPE_LIST_PAYMENT_INTENTS", json!({"customer": cus}));
    let rf = run("STRIPE_CREATE_REFUND", json!({"payment_intent": pi["id"], "reason": "requested_by_customer"}));
    assert!(matches!(rf["status"].as_str(), Some("succeeded" | "pending")));
    run("STRIPE_LIST_SUBSCRIPTIONS", json!({"status": "all", "limit": 3}));

    // Cleanup.
    if let Some(id) = link["id"].as_str() { run("STRIPE_DEACTIVATE_PAYMENT_LINK", json!({"payment_link": id})); }
    if let Some(id) = inv["id"].as_str() { run("STRIPE_DELETE_DRAFT_INVOICE", json!({"invoice": id})); }
    run("STRIPE_UPDATE_PRICE", json!({"price": price, "active": false}));
    if let Some(id) = rp["id"].as_str() { run("STRIPE_UPDATE_PRICE", json!({"price": id, "active": false})); }
    run("STRIPE_UPDATE_PRODUCT", json!({"product": prod, "active": false}));
    run("STRIPE_DELETE_CUSTOMER", json!({"customer": cus}));
    assert!(fails.is_empty(), "live failures: {fails:?}");
}

#[test]
#[ignore]
fn live_test_mode_actions_via_spec() {
    support::use_spec(include_str!("../specs/stripe.json"));
    scenario();
}
