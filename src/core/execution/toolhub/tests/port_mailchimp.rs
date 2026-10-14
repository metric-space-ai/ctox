//! Mailchimp port: one recorded case per action. Every expected URL comes from the
//! Activepieces `run()` code (`src/lib/actions/*.ts`) or from the REST endpoint that the
//! `@mailchimp/mailchimp_marketing` method it calls wraps.
mod support;

use ctox_toolhub::toolkits::mailchimp::Mailchimp;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://us21.api.mailchimp.test/3.0";

const LIST: &str = "a1b2c3d4e5";
const STORE: &str = "f1e2d3c4b5";
const CAMPAIGN: &str = "c9f8e7d6a5";

fn adapter() -> Mailchimp {
    Mailchimp::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn member() -> Value {
    json!({
        "id": "m1a2b3c4d5e6", "email_address": "a@b.com", "unique_email_id": "u1",
        "email_type": "html", "status": "subscribed", "full_name": "Ada Lovelace",
        "contact_id": "c0ffee1234", "web_id": 991234, "unsubscribe_reason": null,
        "merge_fields": {"FNAME": "Ada", "LNAME": "Lovelace"}, "interests": {"HTML": true},
        "stats": {"avg_open_rate": 0.42, "avg_click_rate": 0.11}, "ip_signup": "192.0.2.10",
        "timestamp_signup": "2026-09-01T10:12:00+00:00", "ip_opt": "192.0.2.10",
        "timestamp_opt": "2026-09-01T10:13:00+00:00", "member_rating": 4,
        "last_changed": "2026-09-20T08:30:00+00:00", "language": "en", "vip": true,
        "email_client": "Apple Mail", "location": {"country_code": "DE", "region": "Berlin"},
        "marketing_permissions": [], "source": "API", "tags_count": 2,
        "tags": [{"id": 123, "name": "vip"}], "list_id": LIST,
        "_links": [{"rel": "self", "href": "https://us21.api.mailchimp.com/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8"}]
    })
}

#[test]
fn mailchimp_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // mailchimpCommon.getUserLists -> GET /lists
        Case {
            tool: "MAILCHIMP_LIST_AUDIENCES",
            args: json!({}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists?",
                "fields=lists.id%2Clists.name%2Ctotal_items",
                "count=1000",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"lists": [{"id": LIST, "name": "Newsletter"}],
                             "total_items": 1,
                             "_links": [{"rel": "self", "href": "https://us21.api.mailchimp.com/3.0/lists"}]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/count", json!(1)),
                ("/total_items", json!(1)),
                ("/lists/0/id", json!(LIST)),
                ("/lists/0/name", json!("Newsletter")),
            ]),
        },
        // mailchimpCommon.getUserCampaigns -> GET /campaigns
        Case {
            tool: "MAILCHIMP_LIST_CAMPAIGNS",
            args: json!({"status": "save", "count": 25}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/campaigns?",
                "fields=campaigns.id%2Ccampaigns.settings.title%2Ccampaigns.status%2Ctotal_items",
                "count=25",
                "status=save",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"campaigns": [{"id": CAMPAIGN, "status": "save",
                                           "settings": {"title": "October digest"}}],
                             "total_items": 1}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/campaigns/0/settings/title", json!("October digest")),
                ("/campaigns/0/status", json!("save")),
            ]),
        },
        // mailchimpCommon.getUserStores -> GET /ecommerce/stores
        Case {
            tool: "MAILCHIMP_LIST_STORES",
            args: json!({}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/ecommerce/stores?",
                "fields=stores.id%2Cstores.name%2Ctotal_items",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"stores": [{"id": STORE, "name": "Acme Shop", "list_id": LIST,
                                         "platform": "Shopify"}],
                             "total_items": 1}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/stores/0/id", json!(STORE)),
                ("/stores/0/name", json!("Acme Shop")),
            ]),
        },
        // find-subscriber.ts -> lists.getListMember -> GET /lists/{id}/members/{hash}
        Case {
            tool: "MAILCHIMP_FIND_SUBSCRIBER",
            args: json!({"list_id": LIST, "email": "a@b.com",
                         "include_fields": ["email_address", "status", "merge_fields.FNAME"],
                         "exclude_fields": ["stats", "interests"]}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8?",
                "fields=email_address%2Cstatus%2Cmerge_fields.FNAME",
                "exclude_fields=stats%2Cinterests",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: member(),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/match_type", json!("exact")),
                ("/primary_subscriber/email_address", json!("a@b.com")),
                ("/subscribers/0/status", json!("subscribed")),
                ("/subscribers/0/merge_fields/FNAME", json!("Ada")),
            ]),
        },
        // find-tag.ts -> lists.tagSearch -> GET /lists/{id}/tag-search
        Case {
            tool: "MAILCHIMP_FIND_TAG",
            args: json!({"list_id": LIST, "name": "vip"}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/tag-search?name=vip",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"tags": [{"id": 123, "name": "vip", "type": "static"}],
                             "total_items": 1}),
            result_eq: ptrs([
                ("/found_tags", json!(1)),
                ("/total_items", json!(1)),
                ("/tags/0/name", json!("vip")),
            ]),
        },
        // find-campaign.ts -> searchCampaigns.search -> GET /search-campaigns
        Case {
            tool: "MAILCHIMP_FIND_CAMPAIGN",
            args: json!({"query": "newsletter & digest", "fields": "campaigns.id, campaigns.settings.title"}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/search-campaigns?",
                "query=newsletter+%26+digest",
                "fields=campaigns.id%2C+campaigns.settings.title",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"results": [{"id": CAMPAIGN, "type": "campaign",
                                          "campaign_title": "October digest"}],
                             "total_items": 1}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/results/0/campaign_title", json!("October digest")),
            ]),
        },
        // find-customer.ts -> ecommerce.getAllStoreCustomers -> GET /ecommerce/stores/{id}/customers
        Case {
            tool: "MAILCHIMP_FIND_CUSTOMER",
            args: json!({"store_id": STORE, "email_address": "a@b.com", "count": 5, "offset": 10}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/ecommerce/stores/f1e2d3c4b5/customers?",
                "email_address=a%40b.com",
                "count=5",
                "offset=10",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"store_id": STORE,
                             "customers": [{"id": "cu1", "email_address": "a@b.com",
                                            "first_name": "Ada", "last_name": "Lovelace",
                                            "orders_count": 3}],
                             "total_items": 1}),
            result_eq: ptrs([
                ("/store_id", json!(STORE)),
                ("/found_customers", json!(1)),
                ("/customers/0/email_address", json!("a@b.com")),
            ]),
        },
        // get-campaign-report.ts -> reports.getCampaignReport -> GET /reports/{id}
        Case {
            tool: "MAILCHIMP_GET_CAMPAIGN_REPORT",
            args: json!({"campaign_id": CAMPAIGN, "exclude_fields": "timeseries,_links"}),
            method: "GET",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/reports/c9f8e7d6a5?",
                "exclude_fields=timeseries%2C_links",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": CAMPAIGN, "campaign_title": "October digest", "type": "regular",
                "list_id": LIST, "list_name": "Newsletter", "subject_line": "This month at Acme",
                "emails_sent": 1000, "abuse_reports": 1, "unsubscribed": 20,
                "send_time": "2026-10-01T08:00:00+00:00",
                "opens": {"opens_total": 700, "unique_opens": 420, "open_rate": 0.42,
                          "last_open": "2026-10-03T19:00:00+00:00", "proxy_excluded_opens": 10},
                "clicks": {"clicks_total": 300, "unique_clicks": 150, "click_rate": 0.15,
                           "unique_subscriber_clicks": 140,
                           "last_click": "2026-10-03T20:00:00+00:00"},
                "bounces": {"hard_bounces": 4, "soft_bounces": 8, "syntax_errors": 1},
                "forwards": {"forwards_count": 5, "forwards_opens": 6},
                "facebook_likes": {"recipient_likes": 3, "unique_likes": 2, "facebook_likes": 2},
                "industry_stats": {"type": "all", "open_rate": 0.21, "click_rate": 0.02},
                "list_stats": {"sub_rate": 0.5, "open_rate": 0.4, "click_rate": 0.1},
                "ecommerce": {"total_orders": 25, "total_spent": 1234.5, "currency_code": "EUR"},
                "share_report": {"share_url": "https://mailchimp.com/share/x", "share_password": "abc"}
            }),
            result_eq: ptrs([
                ("/report/campaign_title", json!("October digest")),
                ("/summary/unique_opens", json!(420)),
                ("/summary/click_rate", json!(0.15)),
                ("/performance_metrics/bounces/total", json!(13)),
                ("/ecommerce_data/total_orders", json!(25)),
            ]),
        },
        // add-member-to-list.ts -> lists.setListMember -> PUT /lists/{id}/members/{hash}
        Case {
            tool: "MAILCHIMP_ADD_MEMBER_TO_LIST",
            args: json!({"list_id": LIST, "email_address": "a@b.com", "status_if_new": "subscribed",
                         "email_type": "html", "first_name": "Ada", "last_name": "Lovelace",
                         "language": "en", "vip": true, "skip_merge_validation": true}),
            method: "PUT",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8?",
                "skip_merge_validation=true",
            ],
            body_eq: ptrs([
                ("/email_address", json!("a@b.com")),
                ("/status_if_new", json!("subscribed")),
                ("/email_type", json!("html")),
                ("/merge_fields/FNAME", json!("Ada")),
                ("/merge_fields/LNAME", json!("Lovelace")),
                ("/language", json!("en")),
                ("/vip", json!(true)),
            ]),
            response_status: 200,
            response: member(),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/id", json!("m1a2b3c4d5e6")),
                ("/status", json!("subscribed")),
                ("/contact_id", json!("c0ffee1234")),
                ("/vip", json!(true)),
            ]),
        },
        // update-subscriber-status.ts -> lists.updateListMember -> PATCH /lists/{id}/members/{hash}
        Case {
            tool: "MAILCHIMP_UPDATE_MEMBER_IN_LIST",
            args: json!({"list_id": LIST, "email": "a@b.com", "status": "unsubscribed"}),
            method: "PATCH",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8",
            ],
            body_eq: ptrs([("/status", json!("unsubscribed"))]),
            response_status: 200,
            response: member(),
            result_eq: ptrs([("/status", json!("subscribed")), ("/email_address", json!("a@b.com"))]),
        },
        // archive-subscriber.ts -> lists.deleteListMember -> DELETE /lists/{id}/members/{hash}
        Case {
            tool: "MAILCHIMP_ARCHIVE_SUBSCRIBER",
            args: json!({"list_id": LIST, "subscriber_hash": "a@b.com"}),
            method: "DELETE",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8",
            ],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // add-subscriber-to-tag.ts -> lists.updateListMemberTags -> POST .../tags
        Case {
            tool: "MAILCHIMP_ADD_SUBSCRIBER_TO_TAG",
            args: json!({"list_id": LIST, "email": "a@b.com", "tag_name": "vip & co"}),
            method: "POST",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8/tags",
            ],
            body_eq: ptrs([
                ("/tags/0/name", json!("vip & co")),
                ("/tags/0/status", json!("active")),
            ]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // remove-subscriber-from-tag.ts (explicit fetch) -> POST .../tags
        Case {
            tool: "MAILCHIMP_REMOVE_SUBSCRIBER_FROM_TAG",
            args: json!({"list_id": LIST, "email": "a@b.com", "tag_name": "vip"}),
            method: "POST",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8/tags",
            ],
            body_eq: ptrs([
                ("/tags/0/name", json!("vip")),
                ("/tags/0/status", json!("inactive")),
            ]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // add-note-to-subscriber.ts (explicit fetch) -> POST .../notes
        Case {
            tool: "MAILCHIMP_ADD_NOTE_TO_SUBSCRIBER",
            args: json!({"list_id": LIST, "email": "a@b.com", "note": "Called about the invoice."}),
            method: "POST",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8/notes",
            ],
            body_eq: ptrs([("/note", json!("Called about the invoice."))]),
            response_status: 200,
            response: json!({"id": 987, "note": "Called about the invoice.",
                             "created_at": "2026-10-01T09:30:00+00:00",
                             "_links": [{"rel": "self", "href": "https://us21.api.mailchimp.com/3.0/notes/987"}]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/note_id", json!(987)),
                ("/note_content", json!("Called about the invoice.")),
            ]),
        },
        // create-custom-event.ts -> lists.createListMemberEvent -> POST .../events
        Case {
            tool: "MAILCHIMP_CREATE_CUSTOM_EVENT",
            args: json!({"list_id": LIST, "email": "a@b.com", "name": "purchased_item",
                         "properties": {"sku": "ABC-1", "price": 19.99}}),
            method: "POST",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/members/357a20e8c56e69d6f9734d23ef9517e8/events",
            ],
            body_eq: ptrs([
                ("/name", json!("purchased_item")),
                ("/properties/sku", json!("ABC-1")),
                ("/properties/price", json!(19.99)),
            ]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true))]),
        },
        // unsubscribe-email.ts -> lists.batchListMembers -> POST /lists/{id}
        Case {
            tool: "MAILCHIMP_UNSUBSCRIBE_EMAIL",
            args: json!({"list_id": LIST, "email_address": "a@b.com", "skip_duplicate_check": true,
                         "update_existing": true}),
            method: "POST",
            url_contains: &[
                "https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5?",
                "skip_duplicate_check=true",
            ],
            body_eq: ptrs([
                ("/members/0/email_address", json!("a@b.com")),
                ("/members/0/status", json!("unsubscribed")),
                ("/update_existing", json!(true)),
            ]),
            response_status: 200,
            response: json!({"new_members": 0, "updated_members": 1, "errors": [],
                             "total_created": 0, "total_updated": 1, "error_count": 0,
                             "_links": [{"rel": "self", "href": "https://us21.api.mailchimp.com/3.0/lists/a1b2c3d4e5"}]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/updated_members", json!(1)),
                ("/total_updated", json!(1)),
                ("/error_count", json!(0)),
            ]),
        },
        // create-audience.ts -> lists.createList -> POST /lists
        Case {
            tool: "MAILCHIMP_CREATE_AUDIENCE",
            args: json!({"name": "Product updates", "company": "Acme GmbH", "address1": "Hauptstr. 1",
                         "city": "Berlin", "country": "DE",
                         "permission_reminder": "You signed up for updates from us.",
                         "from_name": "Acme", "from_email": "news@acme.example",
                         "subject": "What is new at Acme", "language": "en",
                         "double_optin": true}),
            method: "POST",
            url_contains: &["https://us21.api.mailchimp.test/3.0/lists"],
            body_eq: ptrs([
                ("/name", json!("Product updates")),
                ("/contact/company", json!("Acme GmbH")),
                ("/contact/address1", json!("Hauptstr. 1")),
                ("/contact/address2", json!("")),
                ("/contact/country", json!("DE")),
                ("/campaign_defaults/from_email", json!("news@acme.example")),
                ("/campaign_defaults/language", json!("en")),
                ("/permission_reminder", json!("You signed up for updates from us.")),
                ("/double_optin", json!(true)),
            ]),
            response_status: 200,
            response: json!({"id": "newlist01", "web_id": 5511223, "name": "Product updates",
                             "contact": {"company": "Acme GmbH", "address1": "Hauptstr. 1",
                                         "city": "Berlin", "country": "DE"},
                             "permission_reminder": "You signed up for updates from us.",
                             "campaign_defaults": {"from_name": "Acme",
                                                   "from_email": "news@acme.example",
                                                   "subject": "What is new at Acme",
                                                   "language": "en"},
                             "email_type_option": true, "use_archive_bar": false,
                             "double_optin": true, "has_welcome": false,
                             "date_created": "2026-10-01T08:00:00+00:00",
                             "subscribe_url_short": "https://acme.us21.list-manage.com/s?u=x"}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/id", json!("newlist01")),
                ("/name", json!("Product updates")),
                ("/double_optin", json!(true)),
                ("/subscribe_url_short", json!("https://acme.us21.list-manage.com/s?u=x")),
            ]),
        },
        // create-tag.ts -> lists.createSegment -> POST /lists/{id}/segments
        Case {
            tool: "MAILCHIMP_CREATE_TAG",
            args: json!({"list_id": LIST, "name": "Beta testers"}),
            method: "POST",
            url_contains: &["https://us21.api.mailchimp.test/3.0/lists/a1b2c3d4e5/segments"],
            body_eq: ptrs([
                ("/name", json!("Beta testers")),
                ("/static_segment", json!([])),
            ]),
            response_status: 200,
            response: json!({"id": 4242, "name": "Beta testers", "type": "static",
                             "static_segment": [], "member_count": 0, "list_id": LIST}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/tag/id", json!(4242)),
                ("/tag/name", json!("Beta testers")),
            ]),
        },
        // create-campaign.ts -> campaigns.create -> POST /campaigns
        Case {
            tool: "MAILCHIMP_CREATE_CAMPAIGN",
            args: json!({"campaign_type": "regular", "list_id": LIST,
                         "subject_line": "What is new at Acme", "title": "October digest",
                         "from_name": "Acme", "from_email": "news@acme.example",
                         "reply_to": "reply@acme.example"}),
            method: "POST",
            url_contains: &["https://us21.api.mailchimp.test/3.0/campaigns"],
            body_eq: ptrs([
                ("/type", json!("regular")),
                ("/recipients/list_id", json!(LIST)),
                ("/settings/subject_line", json!("What is new at Acme")),
                ("/settings/title", json!("October digest")),
                ("/settings/from_email", json!("news@acme.example")),
                ("/settings/reply_to", json!("reply@acme.example")),
                ("/settings/to_name", json!("")),
            ]),
            response_status: 200,
            response: json!({"id": CAMPAIGN, "web_id": 8812, "type": "regular", "status": "save",
                             "create_time": "2026-10-01T07:55:00+00:00",
                             "archive_url": "https://mailchimp.com/campaigns/x",
                             "settings": {"title": "October digest",
                                          "subject_line": "What is new at Acme"}}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/campaign_id", json!(CAMPAIGN)),
                ("/campaign_title", json!("October digest")),
                ("/campaign_status", json!("save")),
                ("/campaign_web_id", json!(8812)),
            ]),
        },
        // create-campaign.ts -> campaigns.setContent -> PUT /campaigns/{id}/content
        Case {
            tool: "MAILCHIMP_SET_CAMPAIGN_CONTENT",
            args: json!({"campaign_id": CAMPAIGN, "template_id": "tpl42"}),
            method: "PUT",
            url_contains: &["https://us21.api.mailchimp.test/3.0/campaigns/c9f8e7d6a5/content"],
            body_eq: ptrs([("/template/id", json!("tpl42"))]),
            response_status: 200,
            response: json!({"section": 0, "html": "<html><body>Newsletter</body></html>"}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/content/html", json!("<html><body>Newsletter</body></html>")),
            ]),
        },
        // send-campaign.ts -> campaigns.send -> POST /campaigns/{id}/actions/send
        Case {
            tool: "MAILCHIMP_SEND_CAMPAIGN",
            args: json!({"campaign_id": CAMPAIGN}),
            method: "POST",
            url_contains: &["https://us21.api.mailchimp.test/3.0/campaigns/c9f8e7d6a5/actions/send"],
            body_eq: ptrs([]),
            response_status: 204,
            response: json!({}),
            result_eq: ptrs([("/success", json!(true)), ("/sent", json!(true))]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn unknown_tools_and_empty_bodies_are_rejected() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    assert!(a.build_request("MAILCHIMP_DOES_NOT_EXIST", &json!({})).is_err());
    assert!(a.build_request("MAILCHIMP_SEND_CAMPAIGN", &json!({})).is_err());
    assert!(a
        .build_request(
            "MAILCHIMP_ARCHIVE_SUBSCRIBER",
            &json!({"list_id": LIST, "subscriber_hash": "../../secrets"})
        )
        .is_err());

    // An empty body must not panic the parsers that index into it.
    let resp = HttpResponse { status: 204, body: Vec::new(), session_id: None };
    let out = a.parse_response("MAILCHIMP_SEND_CAMPAIGN", &resp).expect("parse_response");
    assert_eq!(out["sent"], json!(true));
}

/// Live read-only calls through the hub exec path (OAuth bearer token).
/// Full live coverage lives in tests/live_mailchimp.rs.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("MAILCHIMP_LIST_AUDIENCES", json!({})),
        ("MAILCHIMP_LIST_CAMPAIGNS", json!({"count": 5})),
        ("MAILCHIMP_LIST_STORES", json!({})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}