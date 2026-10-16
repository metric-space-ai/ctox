//! Shopify port: one recorded case per action. Every expectation below is derived from the
//! Activepieces piece (`src/lib/common/index.ts` in packages/pieces/community/shopify):
//! path, HTTP method and body fields are copied from its `httpClient.sendRequest` calls, and
//! the responses are recorded Shopify Admin REST payloads.
mod support;

use ctox_toolhub::toolkits::shopify::Shopify;
use serde_json::{json, Value};
use support::{live, run, Case};

const SHOP: &str = "my-shop";
const BASE: &str = "https://my-shop.myshopify.com/admin/api/2026-10";

const CUSTOMER: u64 = 191167000111u64;
const PRODUCT: u64 = 788032119674292922u64;
const VARIANT: u64 = 642667041524721922u64;
const ORDER: u64 = 450789469u64;
const TRANSACTION: u64 = 5604259584889u64;
const FULFILLMENT: u64 = 255858076u64;
const LOCATION: u64 = 24826418u64;
const THEME: u64 = 828155753u64;
const COLLECTION: u64 = 17365492015u64;
const INVENTORY_ITEM: u64 = 808950810u64;

fn adapter() -> Shopify {
    Shopify::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn with_shop(v: Value) -> Value {
    let mut m = v.as_object().cloned().unwrap_or_default();
    m.insert("shop_name".into(), json!(SHOP));
    Value::Object(m)
}

fn product() -> Value {
    json!({
        "id": PRODUCT, "title": "Sailboat T-Shirt", "handle": "sailboat-t-shirt",
        "description": "A very comfortable shirt", "vendor": "CTOX",
        "product_type": "Shirts", "status": "active", "tags": "Featured, Summer",
        "published_at": "2026-03-04T09:12:00Z", "created_at": "2026-03-04T09:11:58Z",
        "updated_at": "2026-09-20T07:41:12Z", "template_suffix": null,
        "body_html": "<p>A very comfortable shirt</p>",
        "image": {"id": 850703190799388098u64, "product_id": PRODUCT,
                  "created_at": "2026-03-04T09:12:00Z", "position": 1,
                  "updated_at": "2026-03-04T09:12:00Z",
                  "src": "https://cdn.shopify.com/s/files/1/shirt.png",
                  "variant_ids": [VARIANT], "width": 123, "height": 456,
                  "variant_id": null},
        "images": [], "options": ["Size", "Color"],
        "variants": [{"id": VARIANT, "product_id": PRODUCT, "title": "S / M",
                      "sku": "SB-M", "position": 1, "price": "19.99",
                      "compare_at_price": null, "fulfillment_service": "manual",
                      "inventory_management": "shopify", "taxable": true,
                      "barcode": "1234567890123", "requires_shipping": true,
                      "weight": 0.2, "weight_unit": "kg", "inventory_item_id": INVENTORY_ITEM,
                      "inventory_quantity": 42, "option1": "S", "option2": "M",
                      "created_at": "2026-03-04T09:12:00Z", "updated_at": "2026-09-20T07:41:12Z"}]
    })
}

fn customer() -> Value {
    json!({
        "id": CUSTOMER, "email": "ada@example.com", "accepts_marketing": true,
        "created_at": "2026-02-01T08:00:00Z", "updated_at": "2026-09-18T12:30:00Z",
        "first_name": "Ada", "last_name": "Lovelace", "state": "enabled",
        "verified_email": true, "currency": "EUR", "tax_exempt": false,
        "phone": "+49 30 123456", "note": null, "tags": "vip, wholesale",
        "last_order_id": ORDER, "orders_count": 1,
        "default_address": {"id": 1, "address1": "Unter den Linden 1", "city": "Berlin",
                            "country_code": "DE", "zip": "10117"}
    })
}

fn order() -> Value {
    json!({
        "id": ORDER, "name": "#1001", "order_number": 1001, "email": "ada@example.com",
        "created_at": "2026-09-01T10:00:00Z", "updated_at": "2026-09-02T11:30:00Z",
        "processed_at": "2026-09-01T10:01:00Z", "currency": "EUR",
        "total_price": "39.98", "subtotal_price": "39.98", "total_tax": "0.0",
        "total_discounts": "0.00", "financial_status": "paid", "fulfillment_status": null,
        "cancelled_at": null, "closed_at": null, "tags": "priority",
        "note": "Bitte schnell liefern", "phone": "+49 30 123456",
        "customer": {"id": CUSTOMER, "email": "ada@example.com"},
        "billing_address": {"first_name": "Ada", "last_name": "Lovelace", "city": "Berlin",
                            "country_code": "DE", "zip": "10117"},
        "shipping_address": {"first_name": "Ada", "last_name": "Lovelace", "city": "Berlin",
                             "country_code": "DE", "zip": "10117"},
        "line_items": [{"id": 866550309766439010u64, "product_id": PRODUCT, "variant_id": VARIANT,
                        "title": "Sailboat T-Shirt", "quantity": 2, "price": "19.99",
                        "sku": "SB-M", "vendor": "CTOX", "fulfillable_quantity": 2,
                        "fulfillment_status": null, "taxable": true,
                        "requires_shipping": true, "total_discount": "0.00"}],
        "shipping_lines": [{"title": "Standard", "price": "0.00", "code": "Standard"}],
        "payment_details": [{"gateway": "shopify_payments", "amount": "39.98"}],
        "client_details": {"user_agent": null, "accepts_marketing": true, "country_code": "DE"}
    })
}

#[test]
fn shopify_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "SHOPIFY_GET_CUSTOMER",
            args: with_shop(json!({"customer_id": CUSTOMER})),
            method: "GET",
            url_contains: &[BASE, "/customers/191167000111.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"customer": customer()}),
            result_eq: ptrs([
                ("/id", json!(CUSTOMER)),
                ("/email", json!("ada@example.com")),
                ("/tags", json!("vip, wholesale")),
            ]),
        },
        Case {
            tool: "SHOPIFY_LIST_CUSTOMERS",
            args: with_shop(json!({"page_info": "eyJsYXN0X2lkIjo0MTE3NjkwMDExMSwibGFzdF92YWx1ZSI6IkFkYSJ9"})),
            method: "GET",
            url_contains: &[BASE, "/customers.json?page_info=eyJsYXN0X2lkIjo0MTE3NjkwMDExMSwibGFzdF92YWx1ZSI6IkFkYSJ9"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"customers": [customer()]}),
            result_eq: ptrs([("/count", json!(1)), ("/customers/0/id", json!(CUSTOMER))]),
        },
        Case {
            tool: "SHOPIFY_LIST_CUSTOMER_ORDERS",
            args: with_shop(json!({"customer_id": CUSTOMER})),
            method: "GET",
            url_contains: &[BASE, "/customers/191167000111/orders.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"orders": [order()]}),
            result_eq: ptrs([("/count", json!(1)), ("/orders/0/name", json!("#1001"))]),
        },
        Case {
            tool: "SHOPIFY_GET_PRODUCT",
            args: with_shop(json!({"product_id": PRODUCT})),
            method: "GET",
            url_contains: &[BASE, "/products/788032119674292922.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"product": product()}),
            result_eq: ptrs([
                ("/id", json!(PRODUCT)),
                ("/title", json!("Sailboat T-Shirt")),
                ("/variants/0/sku", json!("SB-M")),
            ]),
        },
        Case {
            tool: "SHOPIFY_LIST_PRODUCTS",
            args: with_shop(json!({"title": "Sailboat T-Shirt", "updated_at_min": "2026-09-01T00:00:00Z"})),
            method: "GET",
            url_contains: &[
                BASE,
                "/products.json?title=Sailboat+T-Shirt&updated_at_min=2026-09-01T00%3A00%3A00Z",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"products": [product()]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/products/0/handle", json!("sailboat-t-shirt")),
            ]),
        },
        Case {
            tool: "SHOPIFY_GET_PRODUCT_VARIANT",
            args: with_shop(json!({"variant_id": VARIANT})),
            method: "GET",
            url_contains: &[BASE, "/variants/642667041524721922.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"variant": {
                "id": VARIANT, "product_id": PRODUCT, "title": "S / M", "sku": "SB-M",
                "price": "19.99", "compare_at_price": null, "position": 1,
                "inventory_item_id": INVENTORY_ITEM, "inventory_quantity": 42,
                "fulfillment_service": "manual", "taxable": true,
                "requires_shipping": true, "weight": 0.2, "weight_unit": "kg",
                "barcode": "1234567890123", "option1": "S", "option2": "M",
                "created_at": "2026-03-04T09:12:00Z", "updated_at": "2026-09-20T07:41:12Z"}}),
            result_eq: ptrs([("/id", json!(VARIANT)), ("/price", json!("19.99"))]),
        },
        Case {
            tool: "SHOPIFY_GET_TRANSACTION",
            args: with_shop(json!({"order_id": ORDER, "transaction_id": TRANSACTION})),
            method: "GET",
            url_contains: &[BASE, "/orders/450789469/transactions/5604259584889.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"transaction": {
                "id": TRANSACTION, "order_id": ORDER, "kind": "capture",
                "status": "success", "amount": "39.98", "currency": "EUR",
                "rate": "0.90", "parent_id": null, "test": false, "source": "shopify_payments",
                "message": null, "processed_at": "2026-09-01T10:01:05Z",
                "created_at": "2026-09-01T10:01:05Z", "updated_at": "2026-09-01T10:01:05Z",
                "receipt": {}, "payment_details": {"gateway": "shopify_payments"}}}),
            result_eq: ptrs([("/kind", json!("capture")), ("/amount", json!("39.98"))]),
        },
        Case {
            tool: "SHOPIFY_LIST_TRANSACTIONS",
            args: with_shop(json!({"order_id": ORDER})),
            method: "GET",
            url_contains: &[BASE, "/orders/450789469/transactions.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"transactions": [{
                "id": TRANSACTION, "order_id": ORDER, "kind": "sale", "status": "success",
                "amount": "39.98", "currency": "EUR", "parent_id": null, "test": false,
                "created_at": "2026-09-01T10:01:00Z", "updated_at": "2026-09-01T10:01:00Z",
                "processed_at": "2026-09-01T10:01:00Z", "source": "shopify_payments"}]}),
            result_eq: ptrs([("/count", json!(1)), ("/transactions/0/kind", json!("sale"))]),
        },
        Case {
            tool: "SHOPIFY_GET_FULFILLMENT",
            args: with_shop(json!({"order_id": ORDER, "fulfillment_id": FULFILLMENT})),
            method: "GET",
            url_contains: &[BASE, "/orders/450789469/fulfillments/255858076.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"fulfillment": {
                "id": FULFILLMENT, "order_id": ORDER, "status": "success",
                "created_at": "2026-09-03T08:00:00Z", "service": "manual",
                "updated_at": "2026-09-03T08:30:00Z", "tracking_company": "DHL",
                "shipment_status": null, "location_id": LOCATION,
                "line_items": [{"id": 866550309766439010u64, "title": "Sailboat T-Shirt",
                                "quantity": 2, "sku": "SB-M"}]}}),
            result_eq: ptrs([("/id", json!(FULFILLMENT)), ("/status", json!("success"))]),
        },
        Case {
            tool: "SHOPIFY_LIST_FULFILLMENTS",
            args: with_shop(json!({"order_id": ORDER})),
            method: "GET",
            url_contains: &[BASE, "/orders/450789469/fulfillments.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"fulfillments": [{
                "id": FULFILLMENT, "order_id": ORDER, "status": "success",
                "created_at": "2026-09-03T08:00:00Z", "service": "manual",
                "location_id": LOCATION, "tracking_company": "DHL"}]}),
            result_eq: ptrs([("/count", json!(1)), ("/fulfillments/0/id", json!(FULFILLMENT))]),
        },
        Case {
            tool: "SHOPIFY_LIST_LOCATIONS",
            args: with_shop(json!({})),
            method: "GET",
            url_contains: &[BASE, "/locations.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"locations": [{
                "id": LOCATION, "name": "Berlin Warehouse", "address1": "Unter den Linden 1",
                "city": "Berlin", "province": "Berlin", "country": "Germany",
                "country_code": "DE", "zip": "10117", "phone": "+49 30 123456",
                "active": true, "localized_country_name": "Deutschland",
                "localized_province_name": "Berlin", "updated_at": "2026-01-02T09:00:00Z"}]}),
            result_eq: ptrs([("/count", json!(1)), ("/locations/0/name", json!("Berlin Warehouse"))]),
        },
        Case {
            tool: "SHOPIFY_GET_ASSET",
            args: with_shop(json!({"theme_id": THEME, "key": "assets/theme.liquid"})),
            method: "GET",
            url_contains: &[BASE, "/themes/828155753/assets.json?key=assets%2Ftheme.liquid"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"asset": {
                "key": "assets/theme.liquid", "theme_id": THEME, "public_url": null,
                "created_at": "2026-01-02T09:00:00Z", "updated_at": "2026-05-11T14:22:00Z",
                "content_type": "text/x-liquid", "size": 812,
                "checksum": "0d1f2e3a4b5c6d7e8f90a1b2c3d4e5f6",
                "attachment": "<style>body { color: #000; }</style>"}}),
            result_eq: ptrs([
                ("/key", json!("assets/theme.liquid")),
                ("/checksum", json!("0d1f2e3a4b5c6d7e8f90a1b2c3d4e5f6")),
            ]),
        },
        Case {
            tool: "SHOPIFY_CREATE_CUSTOMER",
            args: with_shop(json!({
                "email": "ada@example.com", "first_name": "Ada", "last_name": "Lovelace",
                "phone_number": "+49 30 123456", "tags": "vip, wholesale",
                "verified_email": true, "accepts_marketing": true})),
            method: "POST",
            url_contains: &[BASE, "/customers.json"],
            body_eq: ptrs([
                ("/customer/email", json!("ada@example.com")),
                ("/customer/first_name", json!("Ada")),
                ("/customer/last_name", json!("Lovelace")),
                ("/customer/phone", json!("+49 30 123456")),
                ("/customer/tags", json!("vip, wholesale")),
                ("/customer/verified_email", json!(true)),
                ("/customer/email_marketing_consent/state", json!("subscribed")),
                ("/customer/email_marketing_consent/opt_in_level", json!("unknown")),
            ]),
            response_status: 201,
            response: json!({"customer": customer()}),
            result_eq: ptrs([("/id", json!(CUSTOMER)), ("/email", json!("ada@example.com"))]),
        },
        Case {
            tool: "SHOPIFY_UPDATE_CUSTOMER",
            args: with_shop(json!({"customer_id": CUSTOMER, "tags": "vip", "accepts_marketing": true})),
            method: "PUT",
            url_contains: &[BASE, "/customers/191167000111.json"],
            body_eq: ptrs([
                ("/customer/tags", json!("vip")),
                ("/customer/email_marketing_consent/state", json!("subscribed")),
            ]),
            response_status: 200,
            response: json!({"customer": customer()}),
            result_eq: ptrs([("/id", json!(CUSTOMER)), ("/tags", json!("vip, wholesale"))]),
        },
        Case {
            tool: "SHOPIFY_CREATE_PRODUCT",
            args: with_shop(json!({
                "title": "Sailboat T-Shirt", "description_html": "<p>A very comfortable shirt</p>",
                "product_type": "Shirts", "vendor": "CTOX", "tags": "Featured, Summer"})),
            method: "POST",
            url_contains: &[BASE, "/products.json"],
            body_eq: ptrs([
                ("/product/title", json!("Sailboat T-Shirt")),
                ("/product/body_html", json!("<p>A very comfortable shirt</p>")),
                ("/product/product_type", json!("Shirts")),
                ("/product/vendor", json!("CTOX")),
                ("/product/tags", json!("Featured, Summer")),
                ("/product/images", json!([])),
            ]),
            response_status: 201,
            response: json!({"product": product()}),
            result_eq: ptrs([("/id", json!(PRODUCT)), ("/title", json!("Sailboat T-Shirt"))]),
        },
        Case {
            tool: "SHOPIFY_UPDATE_PRODUCT",
            args: with_shop(json!({"product_id": PRODUCT, "title": "Sailboat Tee"})),
            method: "PUT",
            url_contains: &[BASE, "/products/788032119674292922.json"],
            body_eq: ptrs([("/product/title", json!("Sailboat Tee")), ("/product/images", json!([]))]),
            response_status: 200,
            response: json!({"product": product()}),
            result_eq: ptrs([("/id", json!(PRODUCT)), ("/title", json!("Sailboat T-Shirt"))]),
        },
        Case {
            tool: "SHOPIFY_CREATE_ORDER",
            args: with_shop(json!({
                "product_id": PRODUCT, "variant_id": VARIANT, "quantity": 2, "price": "19.99",
                "customer_id": CUSTOMER, "email": "ada@example.com", "tags": "priority",
                "send_receipt": true})),
            method: "POST",
            url_contains: &[BASE, "/orders.json"],
            body_eq: ptrs([
                ("/order/line_items/0/product_id", json!(PRODUCT)),
                ("/order/line_items/0/variant_id", json!(VARIANT)),
                ("/order/line_items/0/quantity", json!(2)),
                ("/order/line_items/0/price", json!("19.99")),
                ("/order/customer/id", json!(CUSTOMER)),
                ("/order/email", json!("ada@example.com")),
                ("/order/send_receipt", json!(true)),
                ("/order/send_fulfillment_receipt", json!(false)),
                ("/order/tags", json!("priority")),
            ]),
            response_status: 201,
            response: json!({"order": order()}),
            result_eq: ptrs([("/id", json!(ORDER)), ("/total_price", json!("39.98"))]),
        },
        Case {
            tool: "SHOPIFY_UPDATE_ORDER",
            args: with_shop(json!({
                "order_id": ORDER, "email": "ada@example.com", "phone_number": "+49 30 123456",
                "tags": "priority", "note": "Bitte schnell liefern"})),
            method: "PUT",
            url_contains: &[BASE, "/orders/450789469.json"],
            body_eq: ptrs([
                ("/order/email", json!("ada@example.com")),
                ("/order/phone", json!("+49 30 123456")),
                ("/order/tags", json!("priority")),
                ("/order/note", json!("Bitte schnell liefern")),
            ]),
            response_status: 200,
            response: json!({"order": order()}),
            result_eq: ptrs([("/id", json!(ORDER)), ("/note", json!("Bitte schnell liefern"))]),
        },
        Case {
            tool: "SHOPIFY_CLOSE_ORDER",
            args: with_shop(json!({"order_id": ORDER})),
            method: "POST",
            url_contains: &[BASE, "/orders/450789469/close.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"order": order()}),
            result_eq: ptrs([("/id", json!(ORDER)), ("/closed_at", json!(null))]),
        },
        Case {
            tool: "SHOPIFY_CANCEL_ORDER",
            args: with_shop(json!({"order_id": ORDER})),
            method: "POST",
            url_contains: &[BASE, "/orders/450789469/cancel.json"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"order": {
                "id": ORDER, "name": "#1001", "email": "ada@example.com",
                "created_at": "2026-09-01T10:00:00Z", "updated_at": "2026-09-04T09:00:00Z",
                "processed_at": "2026-09-01T10:01:00Z", "currency": "EUR",
                "cancelled_at": "2026-09-04T09:00:00Z", "closed_at": "2026-09-04T09:00:00Z",
                "cancel_reason": "customer", "total_price": "39.98",
                "financial_status": "refunded", "fulfillment_status": null}}),
            result_eq: ptrs([
                ("/cancelled_at", json!("2026-09-04T09:00:00Z")),
                ("/cancel_reason", json!("customer")),
            ]),
        },
        Case {
            tool: "SHOPIFY_CREATE_DRAFT_ORDER",
            args: with_shop(json!({
                "product_id": PRODUCT, "variant_id": VARIANT, "title": "Sailboat T-Shirt",
                "price": "19.99", "customer_id": CUSTOMER})),
            method: "POST",
            url_contains: &[BASE, "/draft_orders.json"],
            body_eq: ptrs([
                ("/draft_order/line_items/0/product_id", json!(PRODUCT)),
                ("/draft_order/line_items/0/variant_id", json!(VARIANT)),
                ("/draft_order/line_items/0/title", json!("Sailboat T-Shirt")),
                ("/draft_order/line_items/0/quantity", json!(1)),
                ("/draft_order/line_items/0/price", json!("19.99")),
                ("/draft_order/customer/id", json!(CUSTOMER)),
            ]),
            response_status: 201,
            response: json!({"draft_order": {
                "id": 8997654321u64, "name": "#D1001", "status": "open",
                "order_id": null, "email": null, "currency": "EUR",
                "total_price": "19.99", "subtotal_price": "19.99", "total_tax": "0.0",
                "created_at": "2026-09-05T09:00:00Z", "updated_at": "2026-09-05T09:00:00Z",
                "completed_at": null, "invoice_sent_at": null,
                "customer": {"id": CUSTOMER, "email": "ada@example.com"},
                "line_items": [{"id": 1, "product_id": PRODUCT, "variant_id": VARIANT,
                                "title": "Sailboat T-Shirt", "quantity": 1, "price": "19.99",
                                "sku": "SB-M"}]}}),
            result_eq: ptrs([("/id", json!(8997654321u64)), ("/status", json!("open"))]),
        },
        Case {
            tool: "SHOPIFY_CREATE_TRANSACTION",
            args: with_shop(json!({
                "order_id": ORDER, "kind": "capture", "amount": "39.98", "currency": "EUR",
                "parent_id": TRANSACTION, "source": "external", "test": true})),
            method: "POST",
            url_contains: &[BASE, "/orders/450789469/transactions.json"],
            body_eq: ptrs([
                ("/transaction/kind", json!("capture")),
                ("/transaction/amount", json!("39.98")),
                ("/transaction/currency", json!("EUR")),
                ("/transaction/parent_id", json!(TRANSACTION)),
                ("/transaction/source", json!("external")),
                ("/transaction/test", json!(true)),
            ]),
            response_status: 201,
            response: json!({"transaction": {
                "id": 5604259584890u64, "order_id": ORDER, "kind": "capture",
                "status": "success", "amount": "39.98", "currency": "EUR",
                "parent_id": TRANSACTION, "test": true, "source": "external",
                "created_at": "2026-09-06T07:00:00Z", "updated_at": "2026-09-06T07:00:00Z",
                "processed_at": "2026-09-06T07:00:00Z", "message": null}}),
            result_eq: ptrs([("/id", json!(5604259584890u64)), ("/kind", json!("capture"))]),
        },
        Case {
            tool: "SHOPIFY_CREATE_FULFILLMENT_EVENT",
            args: with_shop(json!({
                "order_id": ORDER, "fulfillment_id": FULFILLMENT, "status": "delivered",
                "message": "Paket angekommen"})),
            method: "POST",
            url_contains: &[BASE, "/orders/450789469/fulfillments/255858076/events.json"],
            body_eq: ptrs([
                ("/event/status", json!("delivered")),
                ("/event/message", json!("Paket angekommen")),
            ]),
            response_status: 201,
            response: json!({"fulfillment_event": {
                "id": 9441448834201u64, "fulfillment_id": FULFILLMENT, "order_id": ORDER,
                "status": "delivered", "message": "Paket angekommen",
                "happened_at": "2026-09-07T10:00:00Z",
                "created_at": "2026-09-07T10:00:00Z", "updated_at": "2026-09-07T10:00:00Z",
                "estimated_delivery_at": null, "address1": null}}),
            result_eq: ptrs([
                ("/id", json!(9441448834201u64)),
                ("/status", json!("delivered")),
            ]),
        },
        Case {
            tool: "SHOPIFY_ADJUST_INVENTORY_LEVEL",
            args: with_shop(json!({
                "inventory_item_id": INVENTORY_ITEM, "location_id": LOCATION,
                "available_adjustment": -3})),
            method: "POST",
            url_contains: &[BASE, "/inventory_levels/adjust.json"],
            body_eq: ptrs([
                ("/inventory_item_id", json!("808950810")),
                ("/location_id", json!("24826418")),
                ("/available_adjustment", json!(-3)),
            ]),
            response_status: 200,
            response: json!({"inventory_level": {
                "inventory_item_id": INVENTORY_ITEM, "location_id": LOCATION,
                "available": 39, "updated_at": "2026-09-08T11:00:00Z"}}),
            result_eq: ptrs([
                ("/inventory_level/available", json!(39)),
                ("/inventory_level/location_id", json!(LOCATION)),
            ]),
        },
        Case {
            tool: "SHOPIFY_CREATE_COLLECT",
            args: with_shop(json!({"product_id": PRODUCT, "collection_id": COLLECTION})),
            method: "POST",
            url_contains: &[BASE, "/collects.json"],
            body_eq: ptrs([
                ("/collect/product_id", json!("788032119674292922")),
                ("/collect/collection_id", json!("17365492015")),
            ]),
            response_status: 201,
            response: json!({"collect": {
                "id": 7209113662016u64, "collection_id": COLLECTION, "product_id": PRODUCT,
                "position": 1, "sort_value": "manual", "created_at": "2026-09-09T09:00:00Z",
                "updated_at": "2026-09-09T09:00:00Z"}}),
            result_eq: ptrs([("/id", json!(7209113662016u64)), ("/collection_id", json!(COLLECTION))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn shop_name_is_required_when_no_base_is_pinned() {
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = Shopify::default();
    let err = a
        .build_request("SHOPIFY_LIST_PRODUCTS", &json!({}))
        .unwrap_err();
    assert!(err.to_string().contains("shop_name"), "{err}");

    // The default adapter derives the Admin API base from the shop name (piece: getBaseUrl).
    let r = a
        .build_request("SHOPIFY_LIST_LOCATIONS", &json!({"shop_name": "other-shop"}))
        .unwrap();
    assert_eq!(
        r.url,
        "https://other-shop.myshopify.com/admin/api/2026-10/locations.json"
    );

    // A missing resource is an error, not an empty object.
    use ctox_toolhub::http::HttpResponse;
    let resp = HttpResponse {
        status: 404,
        body: serde_json::to_vec(&json!({"errors": "Not Found"})).unwrap(),
        session_id: None,
    };
    assert!(a.parse_response("SHOPIFY_GET_CUSTOMER", &resp).is_err());
}

/// Live read-only calls through the hub exec path (X-Shopify-Access-Token header).
/// Needs `CTOX_TOOLHUB_LIVE_SHOPIFY_TOKEN` and `CTOX_TOOLHUB_LIVE_SHOPIFY_SHOP`.
/// Full live coverage lives in tests/live_linear.rs style scripts per toolkit.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    let Ok(shop) = std::env::var("CTOX_TOOLHUB_LIVE_SHOPIFY_SHOP") else {
        return;
    };
    for (tool, extra) in [
        ("SHOPIFY_LIST_LOCATIONS", json!({})),
        ("SHOPIFY_LIST_PRODUCTS", json!({"title": "a"})),
        ("SHOPIFY_LIST_CUSTOMERS", json!({})),
    ] {
        let mut args = extra;
        args["shop_name"] = json!(shop);
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}