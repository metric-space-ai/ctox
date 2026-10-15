//! Stripe port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (`GET/POST/DELETE https://api.stripe.com/v1/…` with
//! form-encoded bodies). Write bodies are asserted separately below, because the
//! shared `Case.body_eq` only understands JSON bodies.
mod support;

use ctox_toolhub::toolkits::stripe::Stripe;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{run, Case};

const BASE: &str = "https://stripe.test/v1";

fn adapter() -> Stripe {
    Stripe::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn list(url: &str, data: Vec<Value>) -> Value {
    json!({"object": "list", "url": url, "has_more": false, "data": data})
}

fn customer(id: &str, email: &str, name: &str) -> Value {
    json!({
        "id": id, "object": "customer", "created": 1767000000,
        "email": email, "name": name, "phone": "+1 555 123 4567",
        "description": "Onboarding 2026",
        "address": {"line1": "123 Main St", "city": "Springfield", "postal_code": "72701",
                    "state": "AR", "country": "US"},
        "balance": 0, "currency": null, "default_source": null, "metadata": {},
        "livemode": false
    })
}

fn invoice(id: &str, number: Value, status: &str) -> Value {
    json!({
        "id": id, "object": "invoice", "number": number, "status": status,
        "customer": "cus_QkR2s8", "customer_email": "jane@example.com",
        "currency": "usd", "total": 2500, "amount_due": 2500, "amount_paid": 0,
        "subtotal": 2500, "created": 1767200000, "due_date": 1769800000,
        "hosted_invoice_url": "https://invoice.stripe.com/i/acct_1/invst_1",
        "attempt_count": 0, "livemode": false
    })
}

fn payment_intent(id: &str, status: &str) -> Value {
    json!({
        "id": id, "object": "payment_intent", "amount": 2500, "currency": "usd",
        "status": status, "customer": "cus_QkR2s8", "description": "Premium plan",
        "amount_received": 2500, "created": 1767300000, "livemode": false,
        "latest_charge": "ch_3S8x2cABcd"
    })
}

fn charge(id: &str) -> Value {
    json!({
        "id": id, "object": "charge", "amount": 2500, "currency": "usd",
        "status": "succeeded", "paid": true, "captured": true,
        "customer": "cus_QkR2s8", "payment_intent": "pi_3S8x2cABcd",
        "created": 1767300010, "livemode": false
    })
}

fn refund(id: &str, amount: i64) -> Value {
    json!({
        "id": id, "object": "refund", "amount": amount, "currency": "usd",
        "payment_intent": "pi_3S8x2cABcd", "charge": "ch_3S8x2cABcd",
        "status": "succeeded", "reason": "requested_by_customer",
        "created": 1767400000, "metadata": {}
    })
}

fn payout(id: &str, status: &str) -> Value {
    json!({
        "id": id, "object": "payout", "amount": 125000, "currency": "usd",
        "status": status, "arrival_date": 1767600000, "created": 1767500000,
        "destination": "ba_1S8x2c", "type": "bank_account", "livemode": false
    })
}

fn product(id: &str, name: &str) -> Value {
    json!({
        "id": id, "object": "product", "active": true, "name": name,
        "description": "Everything unlocked", "created": 1766000000,
        "updated": 1766000000, "images": [], "metadata": {}, "livemode": false
    })
}

fn price(id: &str, unit_amount: i64) -> Value {
    json!({
        "id": id, "object": "price", "active": true, "billing_scheme": "per_unit",
        "currency": "usd", "unit_amount": unit_amount, "product": "prod_S8x2cABcd",
        "type": "recurring",
        "recurring": {"interval": "month", "interval_count": 1, "usage_type": "licensed"},
        "created": 1766000100, "livemode": false
    })
}

fn subscription(id: &str, status: &str) -> Value {
    json!({
        "id": id, "object": "subscription", "status": status, "customer": "cus_QkR2s8",
        "collection_method": "charge_automatically", "created": 1766000200,
        "current_period_start": 1766000200, "current_period_end": 1768678600,
        "cancel_at_period_end": false, "items": {
            "object": "list", "has_more": false,
            "data": [{"id": "si_S8x2c", "object": "subscription_item",
                      "price": price("price_1S8x2cABcd", 2500), "quantity": 1}]
        },
        "livemode": false
    })
}

#[test]
fn stripe_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "STRIPE_GET_CUSTOMER",
            args: json!({"customer_id": "cus_QkR2s8"}),
            method: "GET",
            url_contains: &["/v1/customers/cus_QkR2s8"],
            body_eq: ptrs([]),
            response_status: 200,
            response: customer("cus_QkR2s8", "jane@example.com", "Jane Doe"),
            result_eq: ptrs([
                ("/id", json!("cus_QkR2s8")),
                ("/email", json!("jane@example.com")),
                ("/address/city", json!("Springfield")),
                ("/object", json!("customer")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_CUSTOMERS",
            args: json!({"email": "jane@example.com", "limit": 3}),
            method: "GET",
            url_contains: &[
                "/v1/customers?",
                "email=jane%40example.com",
                "limit=3",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/customers",
                vec![
                    customer("cus_QkR2s8", "jane@example.com", "Jane Doe"),
                    customer("cus_OkR1s7", "bob@example.com", "Bob Blob"),
                ],
            ),
            result_eq: ptrs([
                ("/object", json!("list")),
                ("/has_more", json!(false)),
                ("/data/0/email", json!("jane@example.com")),
                ("/data/1/name", json!("Bob Blob")),
            ]),
        },
        Case {
            tool: "STRIPE_SEARCH_CUSTOMERS",
            args: json!({"email": "jane@example.com"}),
            method: "GET",
            url_contains: &[
                "/v1/customers/search?",
                "query=email%3A%27jane%40example.com%27",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/customers/search",
                vec![customer("cus_QkR2s8", "jane@example.com", "Jane Doe")],
            ),
            result_eq: ptrs([
                ("/data/0/id", json!("cus_QkR2s8")),
                ("/data/0/name", json!("Jane Doe")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_INVOICE",
            args: json!({"invoice_id": "in_1S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/invoices/in_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: invoice("in_1S8x2cABcd", json!("INV-2026-0042"), "open"),
            result_eq: ptrs([
                ("/id", json!("in_1S8x2cABcd")),
                ("/number", json!("INV-2026-0042")),
                ("/status", json!("open")),
                ("/total", json!(2500)),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_INVOICES",
            args: json!({"customer": "cus_QkR2s8", "status": "open", "limit": 10}),
            method: "GET",
            url_contains: &[
                "/v1/invoices?",
                "customer=cus_QkR2s8",
                "status=open",
                "limit=10",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/invoices",
                vec![
                    invoice("in_1S8x2cABcd", json!("INV-2026-0042"), "open"),
                    invoice("in_0S7x1bABcd", json!("INV-2026-0041"), "open"),
                ],
            ),
            result_eq: ptrs([
                ("/has_more", json!(false)),
                ("/data/0/number", json!("INV-2026-0042")),
                ("/data/1/id", json!("in_0S7x1bABcd")),
            ]),
        },
        Case {
            tool: "STRIPE_SEARCH_INVOICES",
            args: json!({"query": "status:'open' AND total>1000", "limit": 25}),
            method: "GET",
            url_contains: &[
                "/v1/invoices/search?",
                "query=status%3A%27open%27+AND+total%3E1000",
                "limit=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/invoices/search",
                vec![invoice("in_1S8x2cABcd", json!("INV-2026-0042"), "open")],
            ),
            result_eq: ptrs([
                ("/data/0/id", json!("in_1S8x2cABcd")),
                ("/data/0/total", json!(2500)),
            ]),
        },
        Case {
            tool: "STRIPE_GET_PAYMENT_INTENT",
            args: json!({"payment_intent_id": "pi_3S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/payment_intents/pi_3S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: payment_intent("pi_3S8x2cABcd", "succeeded"),
            result_eq: ptrs([
                ("/id", json!("pi_3S8x2cABcd")),
                ("/amount", json!(2500)),
                ("/status", json!("succeeded")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_PAYMENT_INTENTS",
            args: json!({"customer": "cus_QkR2s8", "created_after": "2026-01-31T00:00:00Z",
                         "limit": 5, "starting_after": "pi_2S8x2cABcd"}),
            method: "GET",
            url_contains: &[
                "/v1/payment_intents?",
                "customer=cus_QkR2s8",
                "limit=5",
                "starting_after=pi_2S8x2cABcd",
                "created%5Bgte%5D=1769817600",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/payment_intents",
                vec![payment_intent("pi_3S8x2cABcd", "succeeded")],
            ),
            result_eq: ptrs([
                ("/data/0/id", json!("pi_3S8x2cABcd")),
                ("/data/0/description", json!("Premium plan")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_CHARGE",
            args: json!({"charge_id": "ch_3S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/charges/ch_3S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: charge("ch_3S8x2cABcd"),
            result_eq: ptrs([
                ("/id", json!("ch_3S8x2cABcd")),
                ("/paid", json!(true)),
                ("/payment_intent", json!("pi_3S8x2cABcd")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_CHARGES",
            args: json!({"customer": "cus_QkR2s8", "created_before": "2026-12-31T23:59:59Z"}),
            method: "GET",
            url_contains: &[
                "/v1/charges?",
                "customer=cus_QkR2s8",
                "created%5Blte%5D=1798761599",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list("/v1/charges", vec![charge("ch_3S8x2cABcd")]),
            result_eq: ptrs([("/data/0/status", json!("succeeded"))]),
        },
        Case {
            tool: "STRIPE_GET_REFUND",
            args: json!({"refund_id": "re_1S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/refunds/re_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: refund("re_1S8x2cABcd", 1000),
            result_eq: ptrs([
                ("/id", json!("re_1S8x2cABcd")),
                ("/amount", json!(1000)),
                ("/reason", json!("requested_by_customer")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_REFUNDS",
            args: json!({"payment_intent": "pi_3S8x2cABcd", "limit": 1}),
            method: "GET",
            url_contains: &[
                "/v1/refunds?",
                "payment_intent=pi_3S8x2cABcd",
                "limit=1",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list("/v1/refunds", vec![refund("re_1S8x2cABcd", 1000)]),
            result_eq: ptrs([
                ("/has_more", json!(false)),
                ("/data/0/payment_intent", json!("pi_3S8x2cABcd")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_PAYOUT",
            args: json!({"payout_id": "po_1S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/payouts/po_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: payout("po_1S8x2cABcd", "paid"),
            result_eq: ptrs([
                ("/id", json!("po_1S8x2cABcd")),
                ("/amount", json!(125000)),
                ("/status", json!("paid")),
                ("/arrival_date", json!(1767600000)),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_PAYOUTS",
            args: json!({"status": "paid", "arrival_date_after": "2026-05-01",
                         "arrival_date_before": "2026-05-01T12:30:00+02:00"}),
            method: "GET",
            url_contains: &[
                "/v1/payouts?",
                "status=paid",
                "arrival_date%5Bgte%5D=1777593600",
                "arrival_date%5Blte%5D=1777631400",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list("/v1/payouts", vec![payout("po_1S8x2cABcd", "paid")]),
            result_eq: ptrs([
                ("/data/0/id", json!("po_1S8x2cABcd")),
                ("/data/0/type", json!("bank_account")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_BALANCE",
            args: json!({}),
            method: "GET",
            url_contains: &["/v1/balance"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "object": "balance", "livemode": false,
                "available": [{"amount": 42042, "currency": "usd",
                               "source_types": {"card": 42042}}],
                "pending": [{"amount": 1200, "currency": "usd",
                             "source_types": {"card": 1200}}]
            }),
            result_eq: ptrs([
                ("/object", json!("balance")),
                ("/available/0/amount", json!(42042)),
                ("/pending/0/currency", json!("usd")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_PRODUCT",
            args: json!({"product_id": "prod_S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/products/prod_S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: product("prod_S8x2cABcd", "Premium Plan"),
            result_eq: ptrs([
                ("/id", json!("prod_S8x2cABcd")),
                ("/name", json!("Premium Plan")),
                ("/active", json!(true)),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_PRODUCTS",
            args: json!({"active": "true", "limit": 2}),
            method: "GET",
            url_contains: &["/v1/products?", "active=true", "limit=2"],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/products",
                vec![
                    product("prod_S8x2cABcd", "Premium Plan"),
                    product("prod_R7x1bABcd", "Starter Plan"),
                ],
            ),
            result_eq: ptrs([
                ("/data/0/name", json!("Premium Plan")),
                ("/data/1/name", json!("Starter Plan")),
            ]),
        },
        Case {
            tool: "STRIPE_GET_PRICE",
            args: json!({"price_id": "price_1S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/prices/price_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: price("price_1S8x2cABcd", 2500),
            result_eq: ptrs([
                ("/id", json!("price_1S8x2cABcd")),
                ("/unit_amount", json!(2500)),
                ("/recurring/interval", json!("month")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_PRICES",
            args: json!({"product": "prod_S8x2cABcd", "type": "recurring", "currency": "usd"}),
            method: "GET",
            url_contains: &[
                "/v1/prices?",
                "product=prod_S8x2cABcd",
                "type=recurring",
                "currency=usd",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/prices",
                vec![price("price_1S8x2cABcd", 2500), price("price_0S7x1bABcd", 1200)],
            ),
            result_eq: ptrs([
                ("/data/0/unit_amount", json!(2500)),
                ("/data/1/unit_amount", json!(1200)),
            ]),
        },
        Case {
            tool: "STRIPE_GET_SUBSCRIPTION",
            args: json!({"subscription_id": "sub_1S8x2cABcd"}),
            method: "GET",
            url_contains: &["/v1/subscriptions/sub_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: subscription("sub_1S8x2cABcd", "active"),
            result_eq: ptrs([
                ("/id", json!("sub_1S8x2cABcd")),
                ("/status", json!("active")),
                ("/items/data/0/price/id", json!("price_1S8x2cABcd")),
            ]),
        },
        Case {
            tool: "STRIPE_LIST_SUBSCRIPTIONS",
            args: json!({"customer": "cus_QkR2s8", "status": "active", "price": "price_1S8x2cABcd"}),
            method: "GET",
            url_contains: &[
                "/v1/subscriptions?",
                "customer=cus_QkR2s8",
                "status=active",
                "price=price_1S8x2cABcd",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: list(
                "/v1/subscriptions",
                vec![subscription("sub_1S8x2cABcd", "active")],
            ),
            result_eq: ptrs([
                ("/data/0/customer", json!("cus_QkR2s8")),
                ("/data/0/items/data/0/quantity", json!(1)),
            ]),
        },
        Case {
            tool: "STRIPE_CREATE_CUSTOMER",
            args: json!({"email": "jane@example.com", "name": "Jane Doe",
                         "line1": "123 Main St", "city": "Springfield", "country": "US"}),
            method: "POST",
            url_contains: &["/v1/customers"],
            body_eq: ptrs([]),
            response_status: 200,
            response: customer("cus_NewCust1", "jane@example.com", "Jane Doe"),
            result_eq: ptrs([
                ("/id", json!("cus_NewCust1")),
                ("/email", json!("jane@example.com")),
            ]),
        },
        Case {
            tool: "STRIPE_UPDATE_CUSTOMER",
            args: json!({"customer_id": "cus_QkR2s8", "phone": "+1 555 987 6543",
                         "description": "Moved to Springfield"}),
            method: "POST",
            url_contains: &["/v1/customers/cus_QkR2s8"],
            body_eq: ptrs([]),
            response_status: 200,
            response: {
                let mut c = customer("cus_QkR2s8", "jane@example.com", "Jane Doe");
                c["phone"] = json!("+1 555 987 6543");
                c["description"] = json!("Moved to Springfield");
                c
            },
            result_eq: ptrs([("/id", json!("cus_QkR2s8")), ("/phone", json!("+1 555 987 6543"))]),
        },
        Case {
            tool: "STRIPE_CREATE_PRODUCT",
            args: json!({"name": "Premium Plan", "description": "Everything unlocked",
                         "active": true, "url": "https://example.com/premium",
                         "images": ["https://example.com/premium.png"],
                         "metadata": {"team": "growth"}}),
            method: "POST",
            url_contains: &["/v1/products"],
            body_eq: ptrs([]),
            response_status: 200,
            response: product("prod_NewProd1", "Premium Plan"),
            result_eq: ptrs([("/id", json!("prod_NewProd1")), ("/name", json!("Premium Plan"))]),
        },
        Case {
            tool: "STRIPE_CREATE_PRICE",
            args: json!({"product_id": "prod_S8x2cABcd", "unit_amount": 10.50,
                         "currency": "usd", "recurring_interval": "month",
                         "recurring_interval_count": 1}),
            method: "POST",
            url_contains: &["/v1/prices"],
            body_eq: ptrs([]),
            response_status: 200,
            response: price("price_NewPrice1", 1050),
            result_eq: ptrs([
                ("/id", json!("price_NewPrice1")),
                ("/unit_amount", json!(1050)),
            ]),
        },
        Case {
            tool: "STRIPE_CREATE_INVOICE",
            args: json!({"customer_id": "cus_QkR2s8", "currency": "usd",
                         "description": "September platform fee"}),
            method: "POST",
            url_contains: &["/v1/invoices"],
            body_eq: ptrs([]),
            response_status: 200,
            response: invoice("in_NewDraft1", Value::Null, "draft"),
            result_eq: ptrs([
                ("/id", json!("in_NewDraft1")),
                ("/status", json!("draft")),
                ("/customer", json!("cus_QkR2s8")),
            ]),
        },
        Case {
            tool: "STRIPE_CREATE_PAYMENT_INTENT",
            args: json!({"amount": 25.00, "currency": "usd", "customer_id": "cus_QkR2s8",
                         "description": "Premium plan"}),
            method: "POST",
            url_contains: &["/v1/payment_intents"],
            body_eq: ptrs([]),
            response_status: 200,
            response: payment_intent("pi_NewPay1", "requires_payment_method"),
            result_eq: ptrs([
                ("/id", json!("pi_NewPay1")),
                ("/amount", json!(2500)),
                ("/status", json!("requires_payment_method")),
            ]),
        },
        Case {
            tool: "STRIPE_CREATE_REFUND",
            args: json!({"payment_intent_id": "pi_3S8x2cABcd", "amount": 10.00,
                         "reason": "requested_by_customer"}),
            method: "POST",
            url_contains: &["/v1/refunds"],
            body_eq: ptrs([]),
            response_status: 200,
            response: refund("re_NewRefund1", 1000),
            result_eq: ptrs([
                ("/id", json!("re_NewRefund1")),
                ("/amount", json!(1000)),
                ("/status", json!("succeeded")),
            ]),
        },
        Case {
            tool: "STRIPE_FINALIZE_INVOICE",
            args: json!({"invoice_id": "in_NewDraft1", "auto_advance": false}),
            method: "POST",
            url_contains: &["/v1/invoices/in_NewDraft1/finalize"],
            body_eq: ptrs([]),
            response_status: 200,
            response: invoice("in_NewDraft1", json!("INV-2026-0043"), "open"),
            result_eq: ptrs([
                ("/id", json!("in_NewDraft1")),
                ("/status", json!("open")),
                ("/number", json!("INV-2026-0043")),
            ]),
        },
        Case {
            tool: "STRIPE_CANCEL_SUBSCRIPTION",
            args: json!({"subscription_id": "sub_1S8x2cABcd", "cancel_at_period_end": true}),
            method: "POST",
            url_contains: &["/v1/subscriptions/sub_1S8x2cABcd"],
            body_eq: ptrs([]),
            response_status: 200,
            response: {
                let mut sub = subscription("sub_1S8x2cABcd", "active");
                sub["cancel_at_period_end"] = json!(true);
                sub
            },
            result_eq: ptrs([
                ("/id", json!("sub_1S8x2cABcd")),
                ("/cancel_at_period_end", json!(true)),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }

    // Mutating flags: everything that writes needs --confirm, reads do not.
    for tool in &a.spec().tools {
        let mutating = tool.slug.starts_with("STRIPE_CREATE_")
            || matches!(
                tool.slug.as_str(),
                "STRIPE_UPDATE_CUSTOMER" | "STRIPE_FINALIZE_INVOICE" | "STRIPE_CANCEL_SUBSCRIPTION"
            );
        assert_eq!(tool.mutating, mutating, "{} mutating flag", tool.slug);
    }

    write_bodies(&a);
    catalog_validates(&a);
}

/// Form-encoded write bodies, exactly the fields the Activepieces `run()` sends.
fn write_bodies(a: &Stripe) {
    let form = |tool: &str, args: Value| {
        let req = a
            .build_request(tool, &args)
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
        let body = req.body.as_deref().expect("form body");
        url::form_urlencoded::parse(body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<(String, String)>>()
    };

    // create-customer: flat contact fields + address[...] nesting, empties dropped.
    assert_eq!(
        form(
            "STRIPE_CREATE_CUSTOMER",
            json!({"email": "jane@example.com", "name": "Jane Doe", "city": "Springfield"})
        ),
        vec![
            ("email".to_string(), "jane@example.com".to_string()),
            ("name".to_string(), "Jane Doe".to_string()),
            ("address[city]".to_string(), "Springfield".to_string()),
        ]
    );

    // update-customer only sends the fields that are set.
    let f = form(
        "STRIPE_UPDATE_CUSTOMER",
        json!({"customer_id": "cus_QkR2s8", "email": "new@example.com", "city": "Berlin"}),
    );
    assert_eq!(
        f,
        vec![
            ("email".to_string(), "new@example.com".to_string()),
            ("address[city]".to_string(), "Berlin".to_string()),
        ]
    );
    assert!(a
        .build_request("STRIPE_UPDATE_CUSTOMER", &json!({"customer_id": "cus_QkR2s8"}))
        .is_err());

    // create-product: images[...] and metadata[k] flattening.
    let f = form(
        "STRIPE_CREATE_PRODUCT",
        json!({"name": "Premium Plan", "active": false, "images": ["https://a/1.png"],
               "metadata": {"team": "growth"}}),
    );
    for pair in [
        ("name", "Premium Plan"),
        ("active", "false"),
        ("images[0]", "https://a/1.png"),
        ("metadata[team]", "growth"),
    ] {
        assert!(f.contains(&(pair.0.to_string(), pair.1.to_string())), "{f:?}");
    }

    // create-price: main units -> cents; recurring fields only for subscriptions.
    let f = form(
        "STRIPE_CREATE_PRICE",
        json!({"product_id": "prod_S8x2cABcd", "unit_amount": 12.99, "currency": "usd",
               "recurring_interval": "year"}),
    );
    assert!(f.contains(&("unit_amount".to_string(), "1299".to_string())), "{f:?}");
    assert!(f.contains(&("recurring[interval]".to_string(), "year".to_string())), "{f:?}");
    let f = form(
        "STRIPE_CREATE_PRICE",
        json!({"product_id": "prod_S8x2cABcd", "unit_amount": 5, "currency": "usd",
               "recurring_interval": "one_time"}),
    );
    assert!(!f.iter().any(|(k, _)| k.starts_with("recurring")), "{f:?}");

    // create-invoice: customer + currency (+ optional description).
    let f = form(
        "STRIPE_CREATE_INVOICE",
        json!({"customer_id": "cus_QkR2s8", "currency": "eur"}),
    );
    assert_eq!(
        f,
        vec![
            ("customer".to_string(), "cus_QkR2s8".to_string()),
            ("currency".to_string(), "eur".to_string()),
        ]
    );

    // create-payment-intent: confirm needs payment method + return URL.
    assert!(a
        .build_request(
            "STRIPE_CREATE_PAYMENT_INTENT",
            &json!({"amount": 10, "currency": "usd", "confirm": true})
        )
        .is_err());
    assert!(a
        .build_request(
            "STRIPE_CREATE_PAYMENT_INTENT",
            &json!({"amount": 10, "currency": "usd", "payment_method_id": "pm_1",
                    "confirm": true})
        )
        .is_err());
    let f = form(
        "STRIPE_CREATE_PAYMENT_INTENT",
        json!({"amount": 25.01, "currency": "usd", "customer_id": "cus_QkR2s8",
               "payment_method_id": "pm_1", "confirm": true,
               "return_url": "https://example.com/back", "receipt_email": "jane@example.com"}),
    );
    for pair in [
        ("amount", "2501"),
        ("currency", "usd"),
        ("customer", "cus_QkR2s8"),
        ("payment_method", "pm_1"),
        ("confirm", "true"),
        ("return_url", "https://example.com/back"),
        ("receipt_email", "jane@example.com"),
    ] {
        assert!(f.contains(&(pair.0.to_string(), pair.1.to_string())), "{f:?}");
    }

    // create-refund: metadata flattens, bad reasons are rejected.
    let f = form(
        "STRIPE_CREATE_REFUND",
        json!({"payment_intent_id": "pi_3S8x2cABcd",
               "reason": "duplicate", "metadata": {"order": "6735"}}),
    );
    assert_eq!(
        f,
        vec![
            ("payment_intent".to_string(), "pi_3S8x2cABcd".to_string()),
            ("reason".to_string(), "duplicate".to_string()),
            ("metadata[order]".to_string(), "6735".to_string()),
        ]
    );
    assert!(a
        .build_request(
            "STRIPE_CREATE_REFUND",
            &json!({"payment_intent_id": "pi_3S8x2cABcd", "reason": "because"})
        )
        .is_err());

    // finalize-invoice: auto_advance only when set.
    let req = a
        .build_request(
            "STRIPE_FINALIZE_INVOICE",
            &json!({"invoice_id": "in_NewDraft1", "auto_advance": true}),
        )
        .unwrap();
    assert_eq!(req.method, "POST");
    assert_eq!(
        req.url,
        format!("{BASE}/invoices/in_NewDraft1/finalize")
    );
    let f: Vec<(String, String)> = url::form_urlencoded::parse(req.body.as_deref().unwrap())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(f, vec![("auto_advance".to_string(), "true".to_string())]);
}

/// Schema and validation contract: required fields, ids, enums, search inputs.
fn catalog_validates(a: &Stripe) {
    let req = |tool: &str, args: Value| {
        a.build_request(tool, &args)
            .unwrap_or_else(|e| panic!("{tool}: {e}"))
    };
    // Missing required props fail at schema level…
    let spec = a.spec();
    let schema = |tool: &str| {
        &spec
            .tools
            .iter()
            .find(|t| t.slug == tool)
            .expect("tool")
            .input_schema
    };
    assert!(support::validate(schema("STRIPE_GET_CUSTOMER"), &json!({})).is_err());
    assert!(support::validate(schema("STRIPE_CREATE_INVOICE"), &json!({"customer_id": "cus_1"})).is_err());
    // …wrong types too.
    assert!(support::validate(
        schema("STRIPE_LIST_CUSTOMERS"),
        &json!({"limit": "many"})
    )
    .is_err());

    // Id prefixes are enforced before interpolation.
    for (tool, args) in [
        ("STRIPE_GET_CUSTOMER", json!({"customer_id": "pi_1"})),
        ("STRIPE_GET_INVOICE", json!({"invoice_id": "cus_1"})),
        ("STRIPE_GET_CHARGE", json!({"charge_id": "re_1"})),
        ("STRIPE_GET_REFUND", json!({"refund_id": "ch_1"})),
        ("STRIPE_GET_PAYOUT", json!({"payout_id": "po"})),
        ("STRIPE_GET_PRODUCT", json!({"product_id": "price_1"})),
        ("STRIPE_GET_PRICE", json!({"price_id": "prod_1"})),
        ("STRIPE_GET_SUBSCRIPTION", json!({"subscription_id": "sub"})),
        ("STRIPE_CREATE_PRICE", json!({"product_id": "cus_1", "unit_amount": 1,
             "currency": "usd", "recurring_interval": "one_time"})),
    ] {
        assert!(a.build_request(tool, &args).is_err(), "{tool} must reject {args}");
    }
    assert!(req("STRIPE_GET_PRODUCT", json!({"product_id": "prod_A-b_9.c"})).url.ends_with("/products/prod_A-b_9.c"));
    assert!(a.build_request("STRIPE_GET_CUSTOMER", &json!({"customer_id": "cus_a/b"})).is_err());

    // Search tools need their query input.
    assert!(a.build_request("STRIPE_SEARCH_INVOICES", &json!({})).is_err());
    assert!(a.build_request("STRIPE_SEARCH_CUSTOMERS", &json!({})).is_err());

    // Malformed timestamps are rejected before a request is built.
    assert!(a
        .build_request("STRIPE_LIST_PAYOUTS", &json!({"arrival_date_after": "next week"}))
        .is_err());
    assert!(req("STRIPE_LIST_PAYOUTS", json!({"arrival_date_after": "2026-05-01"}))
        .url
        .contains("arrival_date%5Bgte%5D=1777593600"));

    // Unknown tools fail.
    assert!(a.build_request("STRIPE_DELETE_EVERYTHING", &json!({})).is_err());
}

/// Live read-only calls through the hub exec path (Bearer secret key).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("STRIPE_GET_BALANCE", json!({})),
        ("STRIPE_LIST_CUSTOMERS", json!({"limit": 1})),
        ("STRIPE_LIST_PRODUCTS", json!({"limit": 1})),
    ] {
        let Some(result) = support::live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
