//! Calendly port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (`calendlyCommon.calendlyRequest` against
//! `https://api.calendly.com`).
mod support;

use ctox_toolhub::toolkits::ToolkitAdapter;
use ctox_toolhub::toolkits::calendly::Calendly;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://api.calendly.test";

const USER_ID: &str = "8e7a4b3c-2d1e-4f5a-9b8c-7d6e5f4a3b2c";
const ORG_ID: &str = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";
const MEMBER_ID: &str = "9b8a7c6d-1e2f-4a3b-8c5d-6e7f8a9b0c1d";
const SCHEDULE_ID: &str = "4d3c2b1a-0f9e-48d7-8b6a-5c4b3a2b1c0d";
const ET: &str = "GBGBDCAADAIDCXQL";
const EVENT: &str = "GBGBDCAADAIDCXZS";
const INVITEE: &str = "GBGBDCAADAIDCXZT";
const USER: &str = "https://api.calendly.com/users/8e7a4b3c-2d1e-4f5a-9b8c-7d6e5f4a3b2c";
const ORG: &str = "https://api.calendly.com/organizations/1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";
const ET_URI: &str = "https://api.calendly.com/event_types/GBGBDCAADAIDCXQL";

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// URL fragment built at runtime: leaked so `Case` can borrow it.
fn ss(v: String) -> &'static str {
    Box::leak(v.into_boxed_str())
}

/// `url_contains` list: leaked so the slice outlives the `let cases` statement.
fn frag(parts: &[&'static str]) -> &'static [&'static str] {
    Box::leak(parts.to_vec().into_boxed_slice())
}

/// `application/x-www-form-urlencoded` form of a query value, as the adapter sends it.
fn urlencoded(v: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("k", v)
        .finish()
        .trim_start_matches("k=")
        .to_string()
}

fn adapter() -> Calendly {
    Calendly::with_api_base(BASE)
}

fn user() -> Value {
    json!({
        "uri": USER, "name": "Ada Lovelace", "slug": "ada", "email": "ada@example.com",
        "scheduling_url": "https://calendly.com/ada", "timezone": "Europe/Berlin",
        "avatar_url": null, "created_at": "2023-02-01T09:00:00.000000Z",
        "updated_at": "2026-09-01T09:00:00.000000Z", "current_organization": ORG
    })
}

fn membership() -> Value {
    json!({
        "uri": ss(format!("https://api.calendly.com/organization_memberships/{MEMBER_ID}")),
        "role": "admin", "user": user(), "organization": ORG,
        "updated_at": "2026-09-01T09:00:00.000000Z"
    })
}

fn event_type() -> Value {
    json!({
        "uri": ET_URI, "name": "15 Minute Meeting", "active": true, "deleted": false,
        "type": "solo", "slug": "15min", "scheduling_url": "https://calendly.com/ada/15min",
        "duration": 15, "description_plain": "A quick chat.",
        "profile": {"type": "user", "name": "Ada Lovelace", "owner": USER},
        "custom_questions": [], "secret": false,
        "created_at": "2023-02-01T09:00:00.000000Z", "updated_at": "2026-09-01T09:00:00.000000Z"
    })
}

fn scheduled_event() -> Value {
    json!({
        "uri": ss(format!("https://api.calendly.com/scheduled_events/{EVENT}")),
        "name": "15 Minute Meeting", "status": "active",
        "start_time": "2026-10-20T09:00:00.000000Z", "end_time": "2026-10-20T09:15:00.000000Z",
        "event_type": ET_URI,
        "location": {"type": "physical", "location": "Berlin Office"},
        "invitees_counter": {"total": 1, "active": 1, "canceled": 0, "limit_total": 1},
        "event_memberships": [{"user": USER}],
        "cancellation": null,
        "created_at": "2026-10-01T12:00:00.000000Z", "updated_at": "2026-10-01T12:00:00.000000Z"
    })
}

fn invitee() -> Value {
    json!({
        "uri": ss(format!("https://api.calendly.com/scheduled_events/{EVENT}/invitees/{INVITEE}")),
        "email": "grace@example.com", "name": "Grace Hopper", "first_name": "Grace",
        "status": "active", "timezone": "Europe/Berlin", "is_rescheduled": false,
        "event": ss(format!("https://api.calendly.com/scheduled_events/{EVENT}")),
        "start_time": "2026-10-20T09:00:00.000000Z",
        "questions_and_answers": [{"question": "What is your phone number?", "answer": "+49 30 123456"}],
        "cancel_url": "https://calendly.com/cancellations/abc",
        "reschedule_url": "https://calendly.com/reschedulings/abc",
        "created_at": "2026-10-01T12:00:00.000000Z", "updated_at": "2026-10-01T12:00:00.000000Z"
    })
}

fn availability_schedule() -> Value {
    json!({
        "uri": ss(format!("https://api.calendly.com/user_availability_schedules/{SCHEDULE_ID}")),
        "name": "Working Hours", "default": true, "timezone": "Europe/Berlin",
        "rules": [{"type": "wday", "wday": "monday", "intervals": [{"from": "09:00", "to": "17:00"}]}]
    })
}

fn page(collection: Value, next: &str) -> Value {
    json!({"collection": collection, "pagination": {"next_page_token": if next.is_empty() { Value::Null } else { json!(next) }}})
}

fn rules() -> Value {
    json!([
        {"type": "wday", "wday": "monday", "intervals": [{"from": "09:00", "to": "12:00"}, {"from": "13:00", "to": "17:00"}]},
        {"type": "date", "date": "2026-12-24", "intervals": []}
    ])
}

#[test]
fn calendly_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "CALENDLY_GET_USER",
            args: json!({}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/users/me"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": user()}),
            result_eq: ptrs([
                ("/email", json!("ada@example.com")),
                ("/current_organization", json!(ORG)),
                ("/timezone", json!("Europe/Berlin")),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_USER",
            args: json!({"user": USER}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/users/{USER_ID}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": user()}),
            result_eq: ptrs([("/uri", json!(USER)), ("/slug", json!("ada"))]),
        },
        Case {
            tool: "CALENDLY_GET_ORGANIZATION",
            args: json!({"organization": ORG_ID}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/organizations/{ORG_ID}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": {
                "uri": ORG, "name": "Acme", "plan": "standard", "stage": "paid",
                "created_at": "2023-02-01T09:00:00.000000Z", "updated_at": "2026-09-01T09:00:00.000000Z"
            }}),
            result_eq: ptrs([
                ("/name", json!("Acme")),
                ("/plan", json!("standard")),
                ("/stage", json!("paid")),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_ORGANIZATION_MEMBERSHIP",
            args: json!({"membership": MEMBER_ID}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/organization_memberships/{MEMBER_ID}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": membership()}),
            result_eq: ptrs([
                ("/role", json!("admin")),
                ("/user/email", json!("ada@example.com")),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_ORGANIZATION_MEMBERS",
            args: json!({"organization": ORG, "role": "admin", "count": 50}),
            method: "GET",
            url_contains: frag(&[
                "/organization_memberships?",
                ss(format!("organization={}", urlencoded(ORG))),
                "role=admin",
                "count=50",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([membership()]), "sNEXTTOKEN"),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/role", json!("admin")),
                ("/items/0/user/name", json!("Ada Lovelace")),
                ("/next_page_token", json!("sNEXTTOKEN")),
            ]),
        },
        Case {
            tool: "CALENDLY_FIND_USER",
            args: json!({"organization": ORG, "email": "grace@example.com"}),
            method: "GET",
            url_contains: frag(&[
                "/organization_memberships?",
                ss(format!("organization={}", urlencoded(ORG))),
                "email=grace%40example.com",
                "count=1",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([{
                    "uri": ss(format!("https://api.calendly.com/organization_memberships/{MEMBER_ID}")),
                    "role": "user",
                    "user": {"uri": USER, "name": "Grace Hopper", "email": "grace@example.com",
                             "scheduling_url": "https://calendly.com/grace", "timezone": "Europe/Berlin"}
                }]),
                "",
            ),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/role", json!("user")),
                ("/user/email", json!("grace@example.com")),
                ("/membership_uri", json!(ss(format!("https://api.calendly.com/organization_memberships/{MEMBER_ID}")))),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_EVENT_TYPE",
            args: json!({"event_type": ET}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/event_types/{ET}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": event_type()}),
            result_eq: ptrs([
                ("/name", json!("15 Minute Meeting")),
                ("/duration", json!(15)),
                ("/type", json!("solo")),
                ("/scheduling_url", json!("https://calendly.com/ada/15min")),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_EVENT_TYPES",
            args: json!({"user": USER, "active": "true"}),
            method: "GET",
            url_contains: frag(&[
                "/event_types?",
                ss(format!("user={}", urlencoded(USER))),
                "active=true",
                "count=20",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([event_type()]), ""),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/uri", json!(ET_URI)),
                ("/next_page_token", Value::Null),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_EVENT_TYPE_HOSTS",
            args: json!({"event_type": ET}),
            method: "GET",
            url_contains: frag(&[
                "/event_type_memberships?",
                ss(format!("event_type={}", urlencoded(&format!("{BASE}/event_types/{ET}")))),
                "count=20",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([{
                    "uri": "https://api.calendly.com/event_type_memberships/6f5e4d3c-2b1a-4c9d-8e7f-0a1b2c3d4e5f",
                    "event_type": ET_URI, "user": user(),
                    "created_at": "2023-02-01T09:00:00.000000Z"
                }]),
                "",
            ),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/user/email", json!("ada@example.com")),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_EVENT_TYPE_AVAILABILITY",
            args: json!({"event_type": ET}),
            method: "GET",
            url_contains: frag(&[
                "/event_type_availability_schedules?",
                ss(format!("event_type={}", urlencoded(&format!("{BASE}/event_types/{ET}")))),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([{
                    "event_type": ET_URI,
                    "default_schedule": ss(format!("https://api.calendly.com/user_availability_schedules/{SCHEDULE_ID}")),
                    "timezone": "Europe/Berlin",
                    "rules": [{"type": "wday", "wday": "monday", "intervals": [{"from": "09:00", "to": "17:00"}]}]
                }]),
                "",
            ),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/timezone", json!("Europe/Berlin")),
                ("/items/0/rules/0/wday", json!("monday")),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_SCHEDULED_EVENT",
            args: json!({"scheduled_event": EVENT}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/scheduled_events/{EVENT}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": scheduled_event()}),
            result_eq: ptrs([
                ("/status", json!("active")),
                ("/location/type", json!("physical")),
                ("/invitees_counter/active", json!(1)),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_SCHEDULED_EVENTS",
            args: json!({"user": USER, "status": "active", "sort": "start_time:desc",
                         "min_start_time": "2026-10-20T00:00:00Z", "max_start_time": "2026-10-27T00:00:00Z"}),
            method: "GET",
            url_contains: frag(&[
                "/scheduled_events?",
                ss(format!("user={}", urlencoded(USER))),
                "status=active",
                "min_start_time=2026-10-20T00%3A00%3A00Z",
                "max_start_time=2026-10-27T00%3A00%3A00Z",
                "sort=start_time%3Adesc",
                "count=20",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([scheduled_event()]), ""),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/uri", json!(ss(format!("https://api.calendly.com/scheduled_events/{EVENT}")))),
            ]),
        },
        Case {
            tool: "CALENDLY_FIND_INVITEE_BOOKINGS",
            args: json!({"user": USER, "invitee_email": "grace@example.com", "status": "active"}),
            method: "GET",
            url_contains: frag(&[
                "/scheduled_events?",
                ss(format!("user={}", urlencoded(USER))),
                "invitee_email=grace%40example.com",
                "status=active",
                "sort=start_time%3Adesc",
                "count=100",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([scheduled_event()]), ""),
            result_eq: ptrs([
                ("/found", json!(true)),
                ("/count", json!(1)),
                ("/events/0/status", json!("active")),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_EVENT_INVITEES",
            args: json!({"scheduled_event": EVENT, "email": "grace@example.com"}),
            method: "GET",
            url_contains: frag(&[
                ss(format!("/scheduled_events/{EVENT}/invitees?")),
                "email=grace%40example.com",
                "count=20",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([invitee()]), ""),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Grace Hopper")),
                ("/items/0/status", json!("active")),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_EVENT_INVITEE",
            args: json!({"scheduled_event": EVENT,
                         "invitee": ss(format!("https://api.calendly.com/scheduled_events/{EVENT}/invitees/{INVITEE}"))}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/scheduled_events/{EVENT}/invitees/{INVITEE}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": invitee()}),
            result_eq: ptrs([
                ("/email", json!("grace@example.com")),
                ("/questions_and_answers/0/answer", json!("+49 30 123456")),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_USER_AVAILABILITY_SCHEDULES",
            args: json!({"user": USER}),
            method: "GET",
            url_contains: frag(&[
                "/user_availability_schedules?",
                ss(format!("user={}", urlencoded(USER))),
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(json!([availability_schedule()]), ""),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("Working Hours")),
                ("/items/0/default", json!(true)),
            ]),
        },
        Case {
            tool: "CALENDLY_GET_USER_AVAILABILITY_SCHEDULE",
            args: json!({"schedule": SCHEDULE_ID}),
            method: "GET",
            url_contains: frag(&[ss(format!("{BASE}/user_availability_schedules/{SCHEDULE_ID}"))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({"resource": availability_schedule()}),
            result_eq: ptrs([
                ("/name", json!("Working Hours")),
                ("/rules/0/intervals/0/from", json!("09:00")),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_USER_BUSY_TIMES",
            args: json!({"user": USER, "start_time": "2026-10-20T08:00:00Z", "end_time": "2026-10-20T18:00:00Z"}),
            method: "GET",
            url_contains: frag(&[
                "/user_busy_times?",
                ss(format!("user={}", urlencoded(USER))),
                "start_time=2026-10-20T08%3A00%3A00Z",
                "end_time=2026-10-20T18%3A00%3A00Z",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([
                    {"type": "calendly", "buffered_start_time": "2026-10-20T09:00:00.000000Z",
                     "buffered_end_time": "2026-10-20T09:15:00.000000Z",
                     "start_time": "2026-10-20T09:00:00.000000Z", "end_time": "2026-10-20T09:15:00.000000Z"}
                ]),
                "",
            ),
            result_eq: ptrs([("/count", json!(1)), ("/items/0/type", json!("calendly"))]),
        },
        Case {
            tool: "CALENDLY_LIST_AVAILABLE_TIMES",
            args: json!({"event_type": ET, "start_time": "2026-10-20T08:00:00Z", "end_time": "2026-10-21T08:00:00Z"}),
            method: "GET",
            url_contains: frag(&[
                "/event_type_available_times?",
                ss(format!("event_type={}", urlencoded(&format!("{BASE}/event_types/{ET}")))),
                "start_time=2026-10-20T08%3A00%3A00Z",
                "end_time=2026-10-21T08%3A00%3A00Z",
            ]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([
                    {"status": "available", "start_time": "2026-10-20T09:00:00.000000Z",
                     "scheduling_url": "https://calendly.com/ada/15min/2026-10-20T09:00:00Z", "invitees_remaining": 1},
                    {"status": "available", "start_time": "2026-10-20T09:30:00.000000Z",
                     "scheduling_url": "https://calendly.com/ada/15min/2026-10-20T09:30:00Z", "invitees_remaining": 1}
                ]),
                "",
            ),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/invitees_remaining", json!(1)),
            ]),
        },
        Case {
            tool: "CALENDLY_LIST_USER_LOCATIONS",
            args: json!({"user": USER}),
            method: "GET",
            url_contains: frag(&["/locations?", ss(format!("user={}", urlencoded(USER)))]),
            body_eq: ptrs([]),
            response_status: 200,
            response: page(
                json!([
                    {"kind": "physical", "name": "In person meeting"},
                    {"kind": "zoom_conference", "name": "Zoom", "conference_meeting_type": "zoom"}
                ]),
                "",
            ),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/1/kind", json!("zoom_conference")),
            ]),
        },
        Case {
            tool: "CALENDLY_CANCEL_EVENT",
            args: json!({"scheduled_event": EVENT, "reason": "Moved to next week."}),
            method: "POST",
            url_contains: frag(&[ss(format!("{BASE}/scheduled_events/{EVENT}/cancellation"))]),
            body_eq: ptrs([("/reason", json!("Moved to next week."))]),
            response_status: 200,
            response: json!({"resource": {
                "uri": ss(format!("https://api.calendly.com/scheduled_events/{EVENT}")),
                "name": "15 Minute Meeting", "status": "canceled",
                "cancellation": {"canceled_by": "Ada Lovelace", "reason": "Moved to next week.",
                                 "canceler_type": "host", "created_at": "2026-10-14T08:00:00.000000Z"}
            }}),
            result_eq: ptrs([
                ("/status", json!("canceled")),
                ("/cancellation/reason", json!("Moved to next week.")),
            ]),
        },
        Case {
            tool: "CALENDLY_CREATE_EVENT_TYPE",
            args: json!({"owner": USER, "name": "Deep Dive", "duration": 45,
                         "description": "A longer session.", "color": "#ff5733", "active": "true"}),
            method: "POST",
            url_contains: frag(&[ss(format!("{BASE}/event_types"))]),
            body_eq: ptrs([
                ("/owner", json!(USER)),
                ("/name", json!("Deep Dive")),
                ("/duration", json!(45)),
                ("/description", json!("A longer session.")),
                ("/color", json!("#ff5733")),
                ("/active", json!(true)),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "uri": "https://api.calendly.com/event_types/GBGBDCAADAIDCXQM",
                "name": "Deep Dive", "active": true, "type": "solo", "duration": 45,
                "scheduling_url": "https://calendly.com/ada/deep-dive",
                "profile": {"type": "user", "name": "Ada Lovelace", "owner": USER}
            }}),
            result_eq: ptrs([("/name", json!("Deep Dive")), ("/duration", json!(45))]),
        },
        Case {
            tool: "CALENDLY_UPDATE_EVENT_TYPE",
            args: json!({"event_type": ET, "name": "Deep Dive 60", "duration": 60}),
            method: "PATCH",
            url_contains: frag(&[ss(format!("{BASE}/event_types/{ET}"))]),
            body_eq: ptrs([
                ("/name", json!("Deep Dive 60")),
                ("/duration", json!(60)),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "uri": ET_URI, "name": "Deep Dive 60", "active": true, "type": "solo",
                "duration": 60, "scheduling_url": "https://calendly.com/ada/deep-dive-60"
            }}),
            result_eq: ptrs([("/name", json!("Deep Dive 60")), ("/duration", json!(60))]),
        },
        Case {
            tool: "CALENDLY_UPDATE_EVENT_TYPE_AVAILABILITY",
            args: json!({"event_type": ET, "timezone": "Europe/Berlin", "rules": rules()}),
            method: "PATCH",
            url_contains: frag(&[
                "/event_type_availability_schedules?",
                ss(format!("event_type={}", urlencoded(&format!("{BASE}/event_types/{ET}")))),
            ]),
            body_eq: ptrs([
                ("/availability_rule/timezone", json!("Europe/Berlin")),
                ("/availability_rule/rules/0/type", json!("wday")),
                ("/availability_rule/rules/0/intervals/1/to", json!("17:00")),
                ("/availability_rule/rules/1/date", json!("2026-12-24")),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "event_type": ET_URI, "timezone": "Europe/Berlin",
                "default_schedule": ss(format!("https://api.calendly.com/user_availability_schedules/{SCHEDULE_ID}")),
                "rules": rules()
            }}),
            result_eq: ptrs([
                ("/timezone", json!("Europe/Berlin")),
                ("/rules/1/date", json!("2026-12-24")),
            ]),
        },
        Case {
            tool: "CALENDLY_CREATE_SCHEDULING_LINK",
            args: json!({"event_type": ET}),
            method: "POST",
            url_contains: frag(&[ss(format!("{BASE}/scheduling_links"))]),
            body_eq: ptrs([
                ("/max_event_count", json!(1)),
                ("/owner", json!(format!("{BASE}/event_types/{ET}"))),
                ("/owner_type", json!("EventType")),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "booking_url": "https://calendly.com/d/cp7v-k2m/single-use",
                "owner": ET_URI, "owner_type": "EventType", "max_event_count": 1,
                "created_at": "2026-10-14T08:00:00.000000Z", "updated_at": "2026-10-14T08:00:00.000000Z"
            }}),
            result_eq: ptrs([
                ("/booking_url", json!("https://calendly.com/d/cp7v-k2m/single-use")),
                ("/max_event_count", json!(1)),
            ]),
        },
        Case {
            tool: "CALENDLY_CREATE_ONE_OFF_EVENT",
            args: json!({"name": "Quick chat", "host": USER, "duration": 20,
                         "start_date": "2026-10-20", "end_date": "2026-10-24",
                         "timezone": "Europe/Berlin", "location_kind": "physical",
                         "location_details": "Berlin Office"}),
            method: "POST",
            url_contains: frag(&[ss(format!("{BASE}/one_off_event_types"))]),
            body_eq: ptrs([
                ("/name", json!("Quick chat")),
                ("/host", json!(USER)),
                ("/duration", json!(20)),
                ("/timezone", json!("Europe/Berlin")),
                ("/date_setting/type", json!("date_range")),
                ("/date_setting/start_date", json!("2026-10-20")),
                ("/date_setting/end_date", json!("2026-10-24")),
                ("/location/kind", json!("physical")),
                ("/location/location", json!("Berlin Office")),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "uri": "https://api.calendly.com/event_types/GBGBDCAADAIDCXQO",
                "name": "Quick chat", "active": true, "type": "solo", "duration": 20,
                "scheduling_url": "https://calendly.com/ada/quick-chat",
                "date_setting": {"type": "date_range", "start_date": "2026-10-20", "end_date": "2026-10-24"}
            }}),
            result_eq: ptrs([
                ("/name", json!("Quick chat")),
                ("/scheduling_url", json!("https://calendly.com/ada/quick-chat")),
                ("/date_setting/start_date", json!("2026-10-20")),
            ]),
        },
        Case {
            tool: "CALENDLY_CREATE_SHARE",
            args: json!({"event_type": ET, "name": "Interview", "duration": 30,
                         "start_date": "2026-10-20", "end_date": "2026-10-30", "hide_location": true}),
            method: "POST",
            url_contains: frag(&[ss(format!("{BASE}/shares"))]),
            body_eq: ptrs([
                ("/event_type", json!(format!("{BASE}/event_types/{ET}"))),
                ("/name", json!("Interview")),
                ("/duration", json!(30)),
                ("/period_type", json!("fixed")),
                ("/start_date", json!("2026-10-20")),
                ("/end_date", json!("2026-10-30")),
                ("/hide_location", json!(true)),
            ]),
            response_status: 200,
            response: json!({"resource": {
                "uri": "https://api.calendly.com/shares/a1b2c3d4-5e6f-4a7b-8c9d-0e1f2a3b4c5d",
                "event_type": ET_URI, "name": "Interview", "duration": 30, "period_type": "fixed",
                "start_date": "2026-10-20", "end_date": "2026-10-30", "hide_location": true,
                "scheduling_links": [{"booking_url": "https://calendly.com/shares/i9j8k7l6"}],
                "created_at": "2026-10-14T08:00:00.000000Z", "updated_at": "2026-10-14T08:00:00.000000Z"
            }}),
            result_eq: ptrs([
                ("/name", json!("Interview")),
                ("/scheduling_links/0/booking_url", json!("https://calendly.com/shares/i9j8k7l6")),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn found_false_when_nothing_matches() {
    use ctox_toolhub::http::HttpResponse;

    let a = adapter();
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({"collection": []})).unwrap(),
        session_id: None,
    };
    let out = a.parse_response("CALENDLY_FIND_USER", &resp).unwrap();
    assert_eq!(out["found"], json!(false));
    assert_eq!(out["user"], Value::Null);
    let out = a
        .parse_response("CALENDLY_FIND_INVITEE_BOOKINGS", &resp)
        .unwrap();
    assert_eq!(out["found"], json!(false));
    assert_eq!(out["count"], json!(0));
}

#[test]
fn owner_and_window_validation() {
    let a = adapter();
    // List endpoints need exactly one owner (organization or user).
    let err = a
        .build_request("CALENDLY_LIST_EVENT_TYPES", &json!({}))
        .unwrap_err();
    assert!(err.to_string().contains("organization"), "{err}");
    let req = a
        .build_request(
            "CALENDLY_LIST_SCHEDULED_EVENTS",
            &json!({"organization": ORG_ID}),
        )
        .unwrap();
    let org = format!("{BASE}/organizations/{ORG_ID}");
    assert!(req.url.contains(&format!("organization={}", urlencoded(&org))));

    // A UUID passed as user is expanded against the API base.
    let req = a
        .build_request("CALENDLY_LIST_USER_LOCATIONS", &json!({"user": USER_ID}))
        .unwrap();
    let user = format!("{BASE}/users/{USER_ID}");
    assert!(req.url.contains(&format!("user={}", urlencoded(&user))));

    // Time windows: end before start, spans beyond the cap and unparseable input fail.
    for args in [
        json!({"user": USER, "start_time": "2026-10-21T09:00:00Z", "end_time": "2026-10-20T09:00:00Z"}),
        json!({"user": USER, "start_time": "2026-10-01T09:00:00Z", "end_time": "2026-10-20T09:00:00Z"}),
        json!({"user": USER, "start_time": "tomorrow", "end_time": "2026-10-20T09:00:00Z"}),
    ] {
        assert!(
            a.build_request("CALENDLY_LIST_USER_BUSY_TIMES", &args).is_err(),
            "{args}"
        );
    }
    // The 31-day cap of List Available Times accepts what busy times reject.
    assert!(a
        .build_request(
            "CALENDLY_LIST_AVAILABLE_TIMES",
            &json!({"event_type": ET, "start_time": "2026-10-01T09:00:00Z", "end_time": "2026-10-20T09:00:00Z"})
        )
        .is_ok());

    // Share windows need both dates.
    let err = a
        .build_request(
            "CALENDLY_CREATE_SHARE",
            &json!({"event_type": ET, "start_date": "2026-10-20"}),
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("both Start Date and End Date"),
        "{err}"
    );

    // Availability rules must be a JSON array.
    assert!(a
        .build_request(
            "CALENDLY_UPDATE_EVENT_TYPE_AVAILABILITY",
            &json!({"event_type": ET, "timezone": "Europe/Berlin", "rules": {"type": "wday"}})
        )
        .is_err());

    // Location kinds are validated against the piece's LOCATION_KINDS.
    assert!(a
        .build_request(
            "CALENDLY_CREATE_ONE_OFF_EVENT",
            &json!({"name": "X", "host": USER, "duration": 20, "start_date": "2026-10-20",
                    "end_date": "2026-10-24", "location_kind": "outer_space"})
        )
        .is_err());

    // Update without any field is rejected, like the TS.
    assert!(a
        .build_request("CALENDLY_UPDATE_EVENT_TYPE", &json!({"event_type": ET}))
        .is_err());
}

#[test]
fn scope_errors_surface_the_missing_scopes() {
    use ctox_toolhub::http::HttpResponse;

    let a = adapter();
    let resp = HttpResponse {
        status: 403,
        body: serde_json::to_vec(&json!({
            "title": "Permission Denied",
            "message": "The requested scope(s) are missing",
            "required_scopes": ["scheduled_events:read"]
        }))
        .unwrap(),
        session_id: None,
    };
    let err = a
        .parse_response("CALENDLY_LIST_SCHEDULED_EVENTS", &resp)
        .unwrap_err();
    let err = err.to_string();
    assert!(err.contains("scheduled_events:read"), "{err}");
    assert!(err.contains("reconnect"), "{err}");
}

/// Live read-only calls through the hub exec path (`Authorization: Bearer <token>`):
/// `CTOX_TOOLHUB_LIVE_CALENDLY_TOKEN=… cargo test --test port_calendly -- --ignored`.
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    // Who am I: uri + current_organization feed the calls below.
    let Some(result) = live(&a, "CALENDLY_GET_USER", json!({})) else {
        return;
    };
    let me = result.unwrap_or_else(|e| panic!("CALENDLY_GET_USER: {e}"));
    let uri = support::find_str(&me, "uri").expect("user uri");
    let org = support::find_str(&me, "current_organization").expect("organization uri");

    for (tool, args) in [
        ("CALENDLY_LIST_EVENT_TYPES", json!({"user": uri, "count": 1})),
        (
            "CALENDLY_LIST_ORGANIZATION_MEMBERS",
            json!({"organization": org, "count": 1}),
        ),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}
