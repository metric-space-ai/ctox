//! Brevo port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (`brevoCommon.apiCall` against api.brevo.com/v3).
mod support;

use ctox_toolhub::toolkits::brevo::Brevo;
use serde_json::{json, Value};
use support::{live, run, Case};

const B: &str = "https://api.brevo.com/v3";

fn adapter() -> Brevo {
    Brevo::default()
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Contact record as the v3 API returns it (GET /contacts/... and list items).
fn contact(email: &str, first: &str, last: &str) -> Value {
    json!({
        "id": 194, "email": email, "first_name": first, "last_name": last,
        "attributes": {"FIRSTNAME": first, "LASTNAME": last, "EMAIL": email},
        "emailBlacklisted": false, "smsBlacklisted": false,
        "createdAt": "2026-05-04T10:12:31Z", "modifiedAt": "2026-06-02T08:41:02Z",
        "listIds": [5]
    })
}

#[test]
fn brevo_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // ---------------------------------------------------------------- read
        Case {
            tool: "BREVO_GET_ACCOUNT_INFO",
            args: json!({}),
            method: "GET",
            url_contains: &[B, "/account"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "plan": [{"type": "free", "credits": 4999, "contactLimit": 500}],
                "owner": {"id": 194, "first_name": "Alice", "last_name": "Martin",
                          "email": "alice@acme.de",
                          "company": {"name": "Acme GmbH", "timezone": "Europe/Berlin"}},
                "address": {"street": "Hauptstr. 1", "city": "Berlin", "zip_code": "10115", "country": "DE"},
                "relay": {"data": {"user_name": "alice@acme.de", "relay_url": "smtp-relay.brevo.com",
                                   "port": 587, "relay_status": "active"}}
            }),
            result_eq: ptrs([
                ("/owner/email", json!("alice@acme.de")),
                ("/relay/data/port", json!(587)),
            ]),
        },
        Case {
            tool: "BREVO_FIND_CONTACT",
            args: json!({"identifier": "jane@example.com"}),
            method: "GET",
            url_contains: &[B, "/contacts/jane%40example.com?identifierType=email_id"],
            body_eq: ptrs([]),
            response_status: 200,
            response: contact("jane@example.com", "Jane", "Doe"),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/data/email", json!("jane@example.com")),
                ("/data/listIds/0", json!(5)),
            ]),
        },
        Case {
            // find-contact.ts: 404 becomes {found: false} instead of an error.
            tool: "BREVO_FIND_CONTACT",
            args: json!({"identifier": "gone@example.com"}),
            method: "GET",
            url_contains: &[B, "/contacts/gone%40example.com?identifierType=email_id"],
            body_eq: ptrs([]),
            response_status: 404,
            response: json!({"code": "document_not_found", "message": "Contact not found"}),
            result_eq: ptrs([("/found", json!(false)), ("/data", json!({}))]),
        },
        Case {
            tool: "BREVO_LIST_CONTACTS",
            args: json!({"limit": 2, "offset": 10, "sort": "asc",
                         "modified_since": "2026-05-01T00:00:00Z"}),
            method: "GET",
            url_contains: &[B, "/contacts?",
                "modifiedSince=2026-05-01T00%3A00%3A00Z", "limit=2", "offset=10", "sort=asc"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "contacts": [contact("jane@example.com", "Jane", "Doe"),
                             contact("john@example.com", "John", "Roe")],
                "count": 2, "total": 42
            }),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/total", json!(42)),
                ("/contacts/0/email", json!("jane@example.com")),
            ]),
        },
        Case {
            tool: "BREVO_GET_CONTACT_CAMPAIGN_STATS",
            args: json!({"identifier": "jane@example.com",
                         "start_date": "2026-05-01", "end_date": "2026-05-31"}),
            method: "GET",
            url_contains: &[B, "/contacts/jane%40example.com/campaignStats",
                "startDate=2026-05-01", "endDate=2026-05-31"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "statistics": {
                    "messagesSent": [{"campaignId": 42, "links": []}],
                    "opened": [{"campaignId": 42, "date": "2026-05-12T09:14:00Z", "ip": "77.20.1.2"}],
                    "clicked": [{"campaignId": 42, "date": "2026-05-12T09:15:10Z", "ip": "77.20.1.2",
                                 "links": [{"link": "/sale", "count": 2}]}],
                    "hardBounces": [], "softBounces": [], "unsubscribed": [], "spam": []
                }
            }),
            result_eq: ptrs([
                ("/statistics/opened/0/campaignId", json!(42)),
                ("/statistics/clicked/0/links/0/count", json!(2)),
            ]),
        },
        Case {
            tool: "BREVO_LIST_CONTACT_ATTRIBUTES",
            args: json!({}),
            method: "GET",
            url_contains: &[B, "/contacts/attributes"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "attributes": [
                    {"name": "FIRSTNAME", "type": "text", "category": "normal",
                     "isReadOnly": false, "options": []},
                    {"name": "LASTNAME", "type": "text", "category": "normal",
                     "isReadOnly": false, "options": []}
                ],
                "count": 2
            }),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/attributes/0/name", json!("FIRSTNAME")),
            ]),
        },
        Case {
            tool: "BREVO_LIST_CONTACT_LISTS",
            args: json!({}),
            method: "GET",
            url_contains: &[B, "/contacts/lists?limit=10&offset=0"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "lists": [{"id": 5, "name": "Newsletter", "folderId": 1,
                           "totalBlacklisted": 0, "totalSubscribers": 1240,
                           "createdAt": "2026-01-12T09:00:00Z"}],
                "count": 1
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/lists/0/id", json!(5)),
                ("/lists/0/totalSubscribers", json!(1240)),
            ]),
        },
        Case {
            tool: "BREVO_GET_CONTACT_LIST",
            args: json!({"list_id": 5, "start_date": "2026-05-01", "end_date": "2026-05-31"}),
            method: "GET",
            url_contains: &[B, "/contacts/lists/5", "startDate=2026-05-01", "endDate=2026-05-31"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 5, "name": "Newsletter", "folderId": 1,
                "totalBlacklisted": 3, "totalSubscribers": 1240,
                "campaignStats": {"clicked": 12, "opened": 410, "unsubscribed": 2},
                "createdAt": "2026-01-12T09:00:00Z"
            }),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/data/name", json!("Newsletter")),
                ("/data/campaignStats/opened", json!(410)),
            ]),
        },
        Case {
            tool: "BREVO_LIST_COMPANIES",
            args: json!({"page": 1, "sort": "desc", "sort_by": "createdAt"}),
            method: "GET",
            url_contains: &[B, "/companies?", "page=1", "limit=50", "sort=desc", "sortBy=createdAt"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "items": [{"id": "61a5cd07ca1347c82306ad06", "name": "Acme GmbH",
                           "attributes": {"domain": "acme.de", "phone_number": "+4930123456"},
                           "linked_contacts_count": 2, "deals_count": 1,
                           "created_at": "2026-02-03T12:00:00Z"}],
                "count": 1, "limit": 50, "offset": 0, "page": 1, "total": 27
            }),
            result_eq: ptrs([("/items/0/name", json!("Acme GmbH")), ("/total", json!(27))]),
        },
        Case {
            tool: "BREVO_GET_COMPANY",
            args: json!({"company_id": "61a5cd07ca1347c82306ad06"}),
            method: "GET",
            url_contains: &[B, "/companies/61a5cd07ca1347c82306ad06"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": "61a5cd07ca1347c82306ad06", "name": "Acme GmbH",
                "attributes": {"domain": "acme.de", "phone_number": "+4930123456",
                               "industry": "SaaS", "country_code": "DE"},
                "linked_contacts_ids": ["194"],
                "created_at": "2026-02-03T12:00:00Z", "updated_at": "2026-05-11T08:00:00Z"
            }),
            result_eq: ptrs([("/found", json!(true)), ("/data/attributes/domain", json!("acme.de"))]),
        },
        Case {
            tool: "BREVO_LIST_CRM_NOTES",
            args: json!({"entity": "deals"}),
            method: "GET",
            url_contains: &[B, "/crm/notes?", "entity=deals", "limit=50", "offset=0"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "items": [{"id": "6652f1a2b3c4d5e6f7a8b9c0", "entity": "deals",
                           "entity_id": "6652f99a2b3c4d5e6f7a8b9c1",
                           "text": "Follow-up call scheduled", "author_id": 194,
                           "created_at": "2026-03-14T09:30:00Z"}],
                "count": 1, "limit": 50, "offset": 0
            }),
            result_eq: ptrs([("/items/0/text", json!("Follow-up call scheduled"))]),
        },
        Case {
            tool: "BREVO_LIST_EMAIL_CAMPAIGNS",
            args: json!({"type": "classic", "status": "sent"}),
            method: "GET",
            url_contains: &[B, "/emailCampaigns?", "type=classic", "status=sent", "limit=50", "offset=0"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "campaigns": [{"id": 42, "name": "Frühjahrsaktion",
                               "subject": "Frühlingsrabatt 20 %", "status": "sent", "type": "classic",
                               "scheduledAt": "2026-05-10T09:00:00+02:00",
                               "recipients": {"listIds": [5], "exclusionListIds": []},
                               "statistics": {"globalStats": {"recipients": 1240, "delivered": 1211,
                                                              "opened": 487, "clicked": 95,
                                                              "unsubscribed": 4}}}],
                "count": 1
            }),
            result_eq: ptrs([("/campaigns/0/id", json!(42)), ("/count", json!(1))]),
        },
        Case {
            tool: "BREVO_GET_EMAIL_CAMPAIGN",
            args: json!({"campaign_id": 42, "statistics": "globalStats"}),
            method: "GET",
            url_contains: &[B, "/emailCampaigns/42", "statistics=globalStats"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 42, "name": "Frühjahrsaktion", "subject": "Frühlingsrabatt 20 %",
                "status": "sent", "type": "classic",
                "sender": {"email": "news@acme.de", "name": "Acme"},
                "recipients": {"listIds": [5]},
                "statistics": {"globalStats": {"recipients": 1240, "delivered": 1211,
                                               "opened": 487, "clicked": 95, "unsubscribed": 4}},
                "createdAt": "2026-05-02T10:00:00Z"
            }),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/data/statistics/globalStats/opened", json!(487)),
                ("/data/sender/email", json!("news@acme.de")),
            ]),
        },
        Case {
            tool: "BREVO_LIST_EMAIL_TEMPLATES",
            args: json!({}),
            method: "GET",
            url_contains: &[B, "/smtp/templates?limit=50&offset=0"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "templates": [{"id": 12, "name": "Willkommen", "subject": "Willkommen bei Acme",
                               "isActive": true,
                               "htmlContent": "<html><body>Hallo {{contact.FIRSTNAME}}</body></html>",
                               "createdAt": "2026-01-20T11:00:00Z"}],
                "count": 1
            }),
            result_eq: ptrs([("/templates/0/id", json!(12)), ("/count", json!(1))]),
        },
        Case {
            tool: "BREVO_GET_EMAIL_TEMPLATE",
            args: json!({"template_id": "12"}),
            method: "GET",
            url_contains: &[B, "/smtp/templates/12"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 12, "name": "Willkommen", "subject": "Willkommen bei Acme", "isActive": true,
                "htmlContent": "<html><body><h1>Hallo {{contact.FIRSTNAME}}</h1></body></html>",
                "sender": {"email": "news@acme.de", "name": "Acme"},
                "replyTo": "support@acme.de", "toField": "{{contact.FIRSTNAME}}"
            }),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/data/subject", json!("Willkommen bei Acme")),
            ]),
        },
        Case {
            tool: "BREVO_LIST_SMS_CAMPAIGNS",
            args: json!({"status": "sent"}),
            method: "GET",
            url_contains: &[B, "/smsCampaigns?", "status=sent", "limit=500", "offset=0"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "campaigns": [{"id": 9, "name": "Rabatt-SMS", "status": "sent",
                               "content": "20 % auf alles bis Sonntag: ACME20",
                               "sender": "ACMEDE",
                               "recipients": {"listIds": [5], "exclusionListIds": []},
                               "statistics": {"sent": 980, "delivered": 965, "unsubscribed": 4}}],
                "count": 1
            }),
            result_eq: ptrs([("/campaigns/0/id", json!(9)), ("/count", json!(1))]),
        },
        Case {
            tool: "BREVO_GET_SMS_CAMPAIGN",
            args: json!({"campaign_id": 9}),
            method: "GET",
            url_contains: &[B, "/smsCampaigns/9"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 9, "name": "Rabatt-SMS", "status": "sent",
                "content": "20 % auf alles bis Sonntag: ACME20", "sender": "ACMEDE",
                "recipients": {"listIds": [5], "exclusionListIds": []},
                "statistics": {"sent": 980, "delivered": 965, "unsubscribed": 4, "remaining": 0},
                "createdAt": "2026-03-30T08:00:00Z", "scheduledAt": "2026-04-02T10:00:00+02:00"
            }),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/data/statistics/delivered", json!(965)),
            ]),
        },
        Case {
            tool: "BREVO_LIST_SENDERS",
            args: json!({"domain": "acme.de"}),
            method: "GET",
            url_contains: &[B, "/senders?", "domain=acme.de"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "senders": [{"id": 3, "name": "Acme Newsletter", "email": "news@acme.de",
                             "ip": "94.130.10.20",
                             "dkim": {"status": "active", "data": {"key_size": 2048}}, "spf": true}],
                "count": 1
            }),
            result_eq: ptrs([("/senders/0/email", json!("news@acme.de"))]),
        },
        Case {
            tool: "BREVO_LIST_SENDER_DOMAINS",
            args: json!({}),
            method: "GET",
            url_contains: &[B, "/senders/domains"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "domains": [{"name": "acme.de",
                             "dns_records": {"dkim": {"status": "active", "data": {"key_size": 2048}},
                                             "spf": {"status": "active", "data": {}}}}],
                "count": 1
            }),
            result_eq: ptrs([("/domains/0/name", json!("acme.de"))]),
        },
        Case {
            tool: "BREVO_LIST_TRANSACTIONAL_EMAIL_EVENTS",
            args: json!({"email": "jane@example.com", "event": "delivered"}),
            method: "GET",
            url_contains: &[B, "/smtp/statistics/events?", "limit=50", "offset=0",
                "email=jane%40example.com", "event=delivered"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "events": [{"email": "jane@example.com", "date": "2026-06-01T10:00:00Z",
                            "event": "delivered",
                            "messageId": "<20260601.1234@smtp-relay.mailin.fr>",
                            "subject": "Willkommen bei Acme", "templateId": 12}],
                "count": 1
            }),
            result_eq: ptrs([("/events/0/event", json!("delivered")), ("/count", json!(1))]),
        },
        Case {
            tool: "BREVO_GET_IMPORT_PROCESS",
            args: json!({"process_id": 175}),
            method: "GET",
            url_contains: &[B, "/processes/175"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": 175, "status": "completed", "name": "Import contacts from file",
                "export_url": "https://export.brevo.com/175/invalid_emails.csv",
                "info": "Import finished: 2 created, 1 invalid",
                "created_at": "2026-06-01T09:00:00Z"
            }),
            result_eq: ptrs([
                ("/id", json!(175)),
                ("/status", json!("completed")),
                ("/finished", json!(true)),
                ("/export_url", json!("https://export.brevo.com/175/invalid_emails.csv")),
            ]),
        },
        // --------------------------------------------------------------- write
        Case {
            tool: "BREVO_CREATE_OR_UPDATE_CONTACT",
            args: json!({"email": "jane@example.com", "ext_id": "crm-4411",
                         "attributes": {"FIRSTNAME": "Jane", "LASTNAME": "Doe"}, "list_ids": [5]}),
            method: "POST",
            url_contains: &[B, "/contacts"],
            body_eq: ptrs([
                ("/email", json!("jane@example.com")),
                ("/ext_id", json!("crm-4411")),
                ("/attributes/FIRSTNAME", json!("Jane")),
                ("/listIds", json!([5])),
                ("/updateEnabled", json!(true)),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_UPDATE_CONTACT",
            args: json!({"identifier": "jane@example.com", "attributes": {"FIRSTNAME": "Janet"},
                         "list_ids": [5], "unlink_list_ids": [7], "email_blacklisted": false}),
            method: "PUT",
            url_contains: &[B, "/contacts/jane%40example.com?identifierType=email_id"],
            body_eq: ptrs([
                ("/attributes/FIRSTNAME", json!("Janet")),
                ("/listIds", json!([5])),
                ("/unlinkListIds", json!([7])),
                ("/emailBlacklisted", json!(false)),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_UNSUBSCRIBE_CONTACT",
            args: json!({"email": "jane@example.com", "unlink_list_ids": [5]}),
            method: "PUT",
            url_contains: &[B, "/contacts/jane%40example.com"],
            body_eq: ptrs([
                ("/emailBlacklisted", json!(true)),
                ("/unlinkListIds", json!([5])),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_DELETE_CONTACT",
            args: json!({"identifier": "+491701234567", "identifier_type": "phone_id"}),
            method: "DELETE",
            url_contains: &[B, "/contacts/%2B491701234567?identifierType=phone_id"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_CREATE_CONTACT_LIST",
            args: json!({"name": "Newsletter DE", "folder_id": 1}),
            method: "POST",
            url_contains: &[B, "/contacts/lists"],
            body_eq: ptrs([("/name", json!("Newsletter DE")), ("/folderId", json!(1))]),
            response_status: 201,
            response: json!({"id": 8, "name": "Newsletter DE", "folderId": 1,
                             "totalBlacklisted": 0, "totalSubscribers": 0,
                             "createdAt": "2026-06-10T10:00:00Z"}),
            result_eq: ptrs([("/id", json!(8)), ("/name", json!("Newsletter DE"))]),
        },
        Case {
            tool: "BREVO_CREATE_COMPANY",
            args: json!({"name": "Acme GmbH", "attributes": {"domain": "acme.de", "industry": "SaaS"},
                         "country_code": 49, "linked_contacts_ids": [194]}),
            method: "POST",
            url_contains: &[B, "/companies"],
            body_eq: ptrs([
                ("/name", json!("Acme GmbH")),
                ("/attributes/domain", json!("acme.de")),
                ("/countryCode", json!(49)),
                ("/linkedContactsIds", json!([194])),
            ]),
            response_status: 201,
            response: json!({"id": "6652f1a2b3c4d5e6f7a8b9c1", "name": "Acme GmbH",
                             "attributes": {"domain": "acme.de", "industry": "SaaS"},
                             "linked_contacts_ids": ["194"],
                             "created_at": "2026-06-10T10:12:00Z"}),
            result_eq: ptrs([("/name", json!("Acme GmbH"))]),
        },
        Case {
            tool: "BREVO_DELETE_COMPANY",
            args: json!({"company_id": "6652f1a2b3c4d5e6f7a8b9c1"}),
            method: "DELETE",
            url_contains: &[B, "/companies/6652f1a2b3c4d5e6f7a8b9c1"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_CREATE_EVENT",
            args: json!({"event_name": "order_completed", "email": "jane@example.com",
                         "event_date": "2026-06-10T12:00:00Z",
                         "event_properties": {"order_id": "A-1042", "amount": 99.5},
                         "contact_properties": {"FIRSTNAME": "Jane"}}),
            method: "POST",
            url_contains: &[B, "/events"],
            body_eq: ptrs([
                ("/event_name", json!("order_completed")),
                ("/identifiers/email_id", json!("jane@example.com")),
                ("/event_date", json!("2026-06-10T12:00:00Z")),
                ("/event_properties/order_id", json!("A-1042")),
                ("/contact_properties/FIRSTNAME", json!("Jane")),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_IMPORT_CONTACTS",
            args: json!({"json_body": [{"email": "new1@example.de"},
                                       {"email": "new2@example.de", "attributes": {"FIRSTNAME": "Max"}}],
                         "list_ids": [5], "email_blacklist": false, "sms_blacklist": false,
                         "update_existing_contacts": true, "empty_contacts_attributes": false}),
            method: "POST",
            url_contains: &[B, "/contacts/import"],
            body_eq: ptrs([
                ("/jsonBody/0/email", json!("new1@example.de")),
                ("/jsonBody/1/attributes/FIRSTNAME", json!("Max")),
                ("/listIds", json!([5])),
                ("/emailBlacklist", json!(false)),
                ("/smsBlacklist", json!(false)),
                ("/updateExistingContacts", json!(true)),
                ("/emptyContactsAttributes", json!(false)),
            ]),
            response_status: 202,
            response: json!({"processId": 175}),
            result_eq: ptrs([("/processId", json!(175))]),
        },
        Case {
            tool: "BREVO_SEND_TRANSACTIONAL_EMAIL",
            args: json!({"to": [{"email": "jane@example.com", "name": "Jane Doe"}],
                         "cc": [{"email": "orders@acme.de"}],
                         "sender_email": "news@acme.de", "sender_name": "Acme",
                         "subject": "Deine Bestellung A-1042",
                         "html_content": "<p>Vielen Dank für Ihre Bestellung.</p>",
                         "params": {"ORDER": "A-1042"}, "reply_to": "support@acme.de",
                         "tags": ["order"]}),
            method: "POST",
            url_contains: &[B, "/smtp/email"],
            body_eq: ptrs([
                ("/to/0/email", json!("jane@example.com")),
                ("/to/0/name", json!("Jane Doe")),
                ("/cc/0/email", json!("orders@acme.de")),
                ("/sender/email", json!("news@acme.de")),
                ("/sender/name", json!("Acme")),
                ("/subject", json!("Deine Bestellung A-1042")),
                ("/params/ORDER", json!("A-1042")),
                ("/replyTo/email", json!("support@acme.de")),
                ("/tags/0", json!("order")),
            ]),
            response_status: 201,
            response: json!({"messageId": "<20260610.1200.a1b2@smtp-relay.mailin.fr>"}),
            result_eq: ptrs([("/messageId", json!("<20260610.1200.a1b2@smtp-relay.mailin.fr>"))]),
        },
        Case {
            tool: "BREVO_SEND_TRANSACTIONAL_SMS",
            args: json!({"sender": "ACMEDE", "recipient": "491701234567",
                         "content": "Dein Code: 4821", "type": "transactional",
                         "unicode_enabled": false}),
            method: "POST",
            url_contains: &[B, "/transactionalSMS/send"],
            body_eq: ptrs([
                ("/sender", json!("ACMEDE")),
                ("/recipient", json!("491701234567")),
                ("/content", json!("Dein Code: 4821")),
                ("/type", json!("transactional")),
                ("/unicodeEnabled", json!(false)),
            ]),
            response_status: 201,
            response: json!({"reference": "sms-5f1e2d", "messageId": "20260610.0930.4821@sms",
                             "smsCount": 1}),
            result_eq: ptrs([("/reference", json!("sms-5f1e2d"))]),
        },
        Case {
            tool: "BREVO_CREATE_EMAIL_CAMPAIGN",
            args: json!({"name": "Frühjahrsaktion", "sender_email": "news@acme.de",
                         "sender_name": "Acme", "subject": "Frühlingsrabatt 20 %",
                         "html_content": "<html><body><h1>20 % auf alles</h1></body></html>",
                         "list_ids": [5], "scheduled_at": "2026-06-20T09:00:00+02:00",
                         "reply_to": "support@acme.de", "utm_campaign": "fruehling"}),
            method: "POST",
            url_contains: &[B, "/emailCampaigns"],
            body_eq: ptrs([
                ("/name", json!("Frühjahrsaktion")),
                ("/sender/email", json!("news@acme.de")),
                ("/sender/name", json!("Acme")),
                ("/subject", json!("Frühlingsrabatt 20 %")),
                ("/htmlContent", json!("<html><body><h1>20 % auf alles</h1></body></html>")),
                ("/recipients/listIds", json!([5])),
                ("/scheduledAt", json!("2026-06-20T09:00:00+02:00")),
                ("/replyTo", json!("support@acme.de")),
                ("/utmCampaign", json!("fruehling")),
            ]),
            response_status: 201,
            response: json!({"id": 43, "name": "Frühjahrsaktion", "subject": "Frühlingsrabatt 20 %",
                             "status": "draft", "type": "classic",
                             "sender": {"id": 3, "email": "news@acme.de", "name": "Acme"},
                             "recipients": {"listIds": [5]},
                             "createdAt": "2026-06-10T11:00:00Z"}),
            result_eq: ptrs([("/id", json!(43)), ("/status", json!("draft"))]),
        },
        Case {
            tool: "BREVO_UPDATE_EMAIL_CAMPAIGN",
            args: json!({"campaign_id": 43, "name": "Frühjahrsaktion 2026",
                         "subject": "Frühlingsrabatt 25 %"}),
            method: "PUT",
            url_contains: &[B, "/emailCampaigns/43"],
            body_eq: ptrs([
                ("/name", json!("Frühjahrsaktion 2026")),
                ("/subject", json!("Frühlingsrabatt 25 %")),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_SEND_EMAIL_CAMPAIGN_NOW",
            args: json!({"campaign_id": 43}),
            method: "POST",
            url_contains: &[B, "/emailCampaigns/43/sendNow"],
            body_eq: ptrs([]),
            response_status: 201,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            tool: "BREVO_CREATE_SMS_CAMPAIGN",
            args: json!({"name": "Rabatt-SMS", "sender": "ACMEDE",
                         "content": "20 % auf alles bis Sonntag: ACME20", "list_ids": [5],
                         "unicode_enabled": false, "organisation_prefix": "ACME",
                         "unsubscribe_instruction": "Antworte mit STOP, um keine Nachrichten mehr zu erhalten."}),
            method: "POST",
            url_contains: &[B, "/smsCampaigns"],
            body_eq: ptrs([
                ("/name", json!("Rabatt-SMS")),
                ("/sender", json!("ACMEDE")),
                ("/content", json!("20 % auf alles bis Sonntag: ACME20")),
                ("/recipients/listIds", json!([5])),
                ("/unicodeEnabled", json!(false)),
                ("/organisationPrefix", json!("ACME")),
            ]),
            response_status: 201,
            response: json!({"id": 9, "name": "Rabatt-SMS", "status": "draft",
                             "content": "20 % auf alles bis Sonntag: ACME20", "sender": "ACMEDE",
                             "recipients": {"listIds": [5], "exclusionListIds": []},
                             "createdAt": "2026-06-10T12:00:00Z"}),
            result_eq: ptrs([("/id", json!(9))]),
        },
        Case {
            tool: "BREVO_DELETE_SMS_CAMPAIGN",
            args: json!({"campaign_id": 9}),
            method: "DELETE",
            url_contains: &[B, "/smsCampaigns/9"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            // Update branch: Brevo answers 204 with an empty body.
            tool: "BREVO_CREATE_OR_UPDATE_EMAIL_TEMPLATE",
            args: json!({"template_id": 12, "template_name": "Willkommen v2",
                         "subject": "Willkommen bei Acme", "sender_email": "news@acme.de",
                         "html_content": "<html><body><h1>Willkommen</h1></body></html>",
                         "is_active": true, "tag": "onboarding"}),
            method: "PUT",
            url_contains: &[B, "/smtp/templates/12"],
            body_eq: ptrs([
                ("/templateName", json!("Willkommen v2")),
                ("/subject", json!("Willkommen bei Acme")),
                ("/sender/email", json!("news@acme.de")),
                ("/htmlContent", json!("<html><body><h1>Willkommen</h1></body></html>")),
                ("/isActive", json!(true)),
                ("/tag", json!("onboarding")),
            ]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
        Case {
            // Create branch: same action, POST when no template_id is given.
            tool: "BREVO_CREATE_OR_UPDATE_EMAIL_TEMPLATE",
            args: json!({"template_name": "Willkommen", "subject": "Willkommen bei Acme",
                         "sender_email": "news@acme.de",
                         "html_content": "<html><body><h1>Hallo</h1></body></html>"}),
            method: "POST",
            url_contains: &[B, "/smtp/templates"],
            body_eq: ptrs([
                ("/templateName", json!("Willkommen")),
                ("/subject", json!("Willkommen bei Acme")),
                ("/sender/email", json!("news@acme.de")),
            ]),
            response_status: 201,
            response: json!({"id": 14, "name": "Willkommen", "subject": "Willkommen bei Acme",
                             "isActive": false, "htmlContent": "<html><body><h1>Hallo</h1></body></html>",
                             "sender": {"email": "news@acme.de", "name": "Acme"}}),
            result_eq: ptrs([("/id", json!(14))]),
        },
        Case {
            tool: "BREVO_DELETE_EMAIL_TEMPLATE",
            args: json!({"template_id": 12}),
            method: "DELETE",
            url_contains: &[B, "/smtp/templates/12"],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/success", json!(true))]),
        },
    ];

    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// The auth scheme replicates the piece: bare `api-key` header, fixed v3 base URL.
#[test]
fn auth_header_matches_the_piece() {
    use ctox_toolhub::toolkits::ToolkitAdapter;
    use ctox_toolhub::AuthSchemeKind;
    let a = adapter();
    let spec = a.spec();
    assert_eq!(spec.slug, "brevo");
    assert_eq!(spec.auth_schemes.len(), 1);
    assert_eq!(spec.auth_schemes[0].kind(), AuthSchemeKind::ApiKey);
    let req = a
        .build_request("BREVO_GET_ACCOUNT_INFO", &json!({}))
        .unwrap();
    assert_eq!(req.url, format!("{B}/account"));
    // Auth is injected by the hub, never by the adapter.
    assert!(!req.headers.contains_key("api-key"));
}

/// Live read-only calls through the hub exec path (bare api-key header).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("BREVO_GET_ACCOUNT_INFO", json!({})),
        ("BREVO_LIST_CONTACT_LISTS", json!({"limit": 1})),
        ("BREVO_LIST_SENDERS", json!({})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
