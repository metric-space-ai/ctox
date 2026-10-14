//! Port tests for the Google Calendar toolkit.
//! DE: Ein aufgezeichneter Fall je portierter Aktion.
//! EN: one recorded case per ported action.
mod support;

use ctox_toolhub::toolkits::google_calendar::GoogleCalendar;
use ctox_toolhub::toolkits::ToolkitAdapter as _;
use serde_json::{json, Value};
use support::Case;

fn cal() -> GoogleCalendar {
    GoogleCalendar::with_api_base("https://gcal.test/calendar/v3")
}

fn event(id: &str, summary: &str) -> Value {
    json!({
        "kind": "calendar#event",
        "etag": "\"3499\"",
        "id": id,
        "status": "confirmed",
        "htmlLink": format!("https://calendar.google.com/event?eid={id}"),
        "created": "2026-05-02T09:12:33.000Z",
        "updated": "2026-05-20T16:04:11.000Z",
        "summary": summary,
        "location": "Berlin",
        "description": "Quarterly review with the team.",
        "colorId": "5",
        "start": {"dateTime": "2026-06-01T09:00:00+02:00", "timeZone": "Europe/Berlin"},
        "end": {"dateTime": "2026-06-01T10:00:00+02:00", "timeZone": "Europe/Berlin"},
        "organizer": {"email": "owner@example.com", "self": true},
        "attendees": [
            {"email": "owner@example.com", "self": true, "organizer": true, "responseStatus": "accepted"},
            {"email": "sam@example.com", "responseStatus": "needsAction"}
        ],
        "reminders": {"useDefault": true},
        "iCalUID": format!("{id}@google.com"),
        "sequence": 0
    })
}

#[test]
fn every_tool_has_a_recorded_case() {
    let adapter = cal();
    let mut cases: Vec<Case<'_>> = Vec::new();

    let r_list_cals: &[(&str, Value)] = &[
        ("/count", json!(2)),
        ("/calendars/0/id", json!("primary")),
        ("/calendars/0/access_role", json!("owner")),
        ("/calendars/0/time_zone", json!("Europe/Berlin")),
        ("/calendars/1/summary", json!("Team")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_LIST_CALENDARS",
        args: json!({}),
        method: "GET",
        url_contains: &["/users/me/calendarList?showDeleted=false"],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#calendarList", "etag": "\"p1\"", "nextPageToken": "CigK",
            "items": [
                {"kind": "calendar#calendarListEntry", "etag": "\"e1\"", "id": "primary",
                 "summary": "Michael Welsch", "timeZone": "Europe/Berlin", "primary": true,
                 "selected": true, "accessRole": "owner", "backgroundColor": "#16a765",
                 "foregroundColor": "#000000",
                 "conferenceProperties": {"allowedConferenceSolutionTypes": ["hangoutsMeet"]}},
                {"kind": "calendar#calendarListEntry", "etag": "\"e2\"", "id": "team@example.com",
                 "summary": "Team", "description": "Shared project calendar", "location": "Munich",
                 "timeZone": "Europe/Berlin", "accessRole": "writer", "selected": false,
                 "backgroundColor": "#7986cb", "foregroundColor": "#ffffff"}
            ]}),
        result_eq: r_list_cals,
    });

    let r_get_cal: &[(&str, Value)] = &[
        ("/id", json!("team@example.com")),
        ("/summary", json!("Team")),
        ("/time_zone", json!("Europe/Berlin")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_GET_CALENDAR",
        args: json!({"calendar_id": "team@example.com"}),
        method: "GET",
        url_contains: &["/calendars/team@example.com"],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#calendar", "etag": "\"c1\"", "id": "team@example.com",
            "summary": "Team", "description": "Shared project calendar", "location": "Munich",
            "timeZone": "Europe/Berlin",
            "conferenceProperties": {"allowedConferenceSolutionTypes": ["hangoutsMeet"]}}),
        result_eq: r_get_cal,
    });

    let r_list_events: &[(&str, Value)] = &[
        ("/count", json!(2)),
        ("/events/0/id", json!("ev0d1")),
        ("/events/0/summary", json!("Budget review")),
        ("/events/0/start/dateTime", json!("2026-06-01T09:00:00+02:00")),
        ("/next_page_token", json!("CkYI")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_LIST_EVENTS",
        args: json!({"calendar_id": "primary", "search": "budget review",
            "start_date": "2026-06-01T00:00:00Z", "end_date": "2026-06-30T00:00:00Z",
            "single_events": true, "event_types": ["default", "focusTime"], "max_results": 50}),
        method: "GET",
        url_contains: &[
            "/calendars/primary/events?showDeleted=false&singleEvents=true",
            "q=%22budget+review%22",
            "timeMin=2026-06-01T00%3A00%3A00.000Z",
            "timeMax=2026-06-30T00%3A00%3A00.000Z",
            "maxResults=50",
            "eventTypes=default&eventTypes=focusTime",
        ],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#events", "etag": "\"e\"",
            "summary": "owner@example.com", "timeZone": "Europe/Berlin", "nextPageToken": "CkYI",
            "items": [event("ev0d1", "Budget review"), event("ev1a2b", "Focus time")]}),
        result_eq: r_list_events,
    });

    let r_get_event: &[(&str, Value)] = &[
        ("/id", json!("ev0d1c3")),
        ("/html_link", json!("https://calendar.google.com/event?eid=ev0d1c3")),
        ("/organizer/email", json!("owner@example.com")),
        ("/attendees/1/email", json!("sam@example.com")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_GET_EVENT",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3", "max_attendees": 5,
            "time_zone": "Europe/Berlin"}),
        method: "GET",
        url_contains: &["/calendars/primary/events/ev0d1c3?maxAttendees=5",
            "timeZone=Europe%2FBerlin"],
        body_eq: &[],
        response_status: 200,
        response: event("ev0d1c3", "Budget review"),
        result_eq: r_get_event,
    });

    let r_search: &[(&str, Value)] = &[
        ("/count", json!(1)),
        ("/events/0/summary", json!("Budget review")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_SEARCH_EVENTS",
        args: json!({"query": "budget review", "start_date": "2026-06-01T00:00:00Z",
            "end_date": "2026-06-30T23:59:59Z", "max_results": 250}),
        method: "GET",
        url_contains: &["/calendars/primary/events?q=budget+review",
            "timeMin=2026-06-01T00%3A00%3A00Z", "timeMax=2026-06-30T23%3A59%3A59Z",
            "singleEvents=true", "showDeleted=false", "maxResults=250"],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#events", "summary": "owner@example.com",
            "timeZone": "Europe/Berlin", "accessRole": "owner",
            "items": [event("ev0d1", "Budget review")]}),
        result_eq: r_search,
    });

    let r_instances: &[(&str, Value)] = &[
        ("/count", json!(2)),
        ("/instances/0/id", json!("series01_20260601T070000Z")),
        ("/instances/0/recurring_event_id", json!("series01")),
        ("/instances/1/recurring_event_id", json!("series01")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_LIST_RECURRING_EVENT_INSTANCES",
        args: json!({"calendar_id": "primary", "event_id": "series01",
            "time_min": "2026-06-01T00:00:00Z", "time_max": "2026-06-05T00:00:00Z",
            "max_results": 10}),
        method: "GET",
        url_contains: &[
            "/calendars/primary/events/series01/instances?timeMin=2026-06-01T00%3A00%3A00.000Z",
            "timeMax=2026-06-05T00%3A00%3A00.000Z",
            "showDeleted=false",
            "maxResults=10",
        ],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#events", "etag": "\"s\"",
            "summary": "owner@example.com", "timeZone": "Europe/Berlin", "nextSyncToken": "CnY=",
            "items": [
                {"kind": "calendar#event", "etag": "\"1\"", "id": "series01_20260601T070000Z",
                 "recurringEventId": "series01", "status": "confirmed", "summary": "Standup",
                 "start": {"dateTime": "2026-06-01T09:00:00+02:00"},
                 "end": {"dateTime": "2026-06-01T09:15:00+02:00"}},
                {"kind": "calendar#event", "etag": "\"2\"", "id": "series01_20260602T070000Z",
                 "recurringEventId": "series01", "status": "confirmed", "summary": "Standup",
                 "start": {"dateTime": "2026-06-02T09:00:00+02:00"},
                 "end": {"dateTime": "2026-06-02T09:15:00+02:00"}}
            ]}),
        result_eq: r_instances,
    });

    let b_busy: &[(&str, Value)] = &[
        ("/timeMin", json!("2026-06-01T00:00:00.000Z")),
        ("/timeMax", json!("2026-06-02T00:00:00.000Z")),
        ("/items/0/id", json!("primary")),
        ("/items/1/id", json!("team@example.com")),
    ];
    let r_busy: &[(&str, Value)] = &[
        ("/time_min", json!("2026-06-01T00:00:00Z")),
        ("/calendars/primary/busy/0/start", json!("2026-06-01T07:00:00Z")),
        ("/calendars/team@example.com/busy/0/end", json!("2026-06-01T14:00:00Z")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_FIND_BUSY_PERIODS",
        args: json!({"calendar_ids": ["primary", "team@example.com"],
            "start_date": "2026-06-01T00:00:00Z", "end_date": "2026-06-02T00:00:00Z"}),
        method: "POST",
        url_contains: &["/freeBusy"],
        body_eq: b_busy,
        response_status: 200,
        response: json!({"kind": "calendar#freeBusy", "timeMin": "2026-06-01T00:00:00Z",
            "timeMax": "2026-06-02T00:00:00Z", "calendars": {
                "primary": {"busy": [{"start": "2026-06-01T07:00:00Z", "end": "2026-06-01T08:00:00Z"}]},
                "team@example.com": {"busy": [{"start": "2026-06-01T13:00:00Z", "end": "2026-06-01T14:00:00Z"}]}
            }}),
        result_eq: r_busy,
    });

    let b_free: &[(&str, Value)] = &[
        ("/timeMin", json!("2026-06-01T09:00:00.000Z")),
        ("/items/1/id", json!("team@example.com")),
    ];
    let r_free: &[(&str, Value)] = &[
        ("/count", json!(3)),
        ("/free_slots/0/start", json!("2026-06-01T09:00:00.000Z")),
        ("/free_slots/0/end", json!("2026-06-01T09:30:00.000Z")),
        ("/free_slots/0/duration_minutes", json!(30)),
        ("/free_slots/1/start", json!("2026-06-01T10:30:00.000Z")),
        ("/free_slots/1/end", json!("2026-06-01T11:00:00.000Z")),
        ("/free_slots/2/start", json!("2026-06-01T11:30:00.000Z")),
        ("/free_slots/2/end", json!("2026-06-01T12:00:00.000Z")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_FIND_FREE_SLOTS",
        args: json!({"calendar_ids": ["primary", "team@example.com"],
            "start_date": "2026-06-01T09:00:00Z", "end_date": "2026-06-01T12:00:00Z"}),
        method: "POST",
        url_contains: &["/freeBusy"],
        body_eq: b_free,
        response_status: 200,
        response: json!({"kind": "calendar#freeBusy", "timeMin": "2026-06-01T09:00:00Z",
            "timeMax": "2026-06-01T12:00:00Z", "calendars": {
                "primary": {"busy": [
                    {"start": "2026-06-01T09:30:00Z", "end": "2026-06-01T10:00:00Z"},
                    {"start": "2026-06-01T09:45:00Z", "end": "2026-06-01T10:30:00Z"}]},
                "team@example.com": {"busy": [{"start": "2026-06-01T11:00:00Z", "end": "2026-06-01T11:30:00Z"}]}
            }}),
        result_eq: r_free,
    });

    let r_settings: &[(&str, Value)] = &[
        ("/count", json!(3)),
        ("/settings/timezone", json!("Europe/Berlin")),
        ("/settings/weekStart", json!("monday")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_LIST_SETTINGS",
        args: json!({}),
        method: "GET",
        url_contains: &["/users/me/settings"],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#settings", "etag": "\"s\"", "items": [
            {"kind": "calendar#setting", "etag": "\"1\"", "id": "timezone", "value": "Europe/Berlin"},
            {"kind": "calendar#setting", "etag": "\"2\"", "id": "weekStart", "value": "monday"},
            {"kind": "calendar#setting", "etag": "\"3\"", "id": "format24HourTime", "value": "true"}
        ]}),
        result_eq: r_settings,
    });

    let r_colors: &[(&str, Value)] = &[
        ("/event/5/background", json!("#fbd75b")),
        ("/calendar/1/background", json!("#ac725e")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_GET_COLORS",
        args: json!({}),
        method: "GET",
        url_contains: &["/colors"],
        body_eq: &[],
        response_status: 200,
        response: json!({"kind": "calendar#colors", "event": {
            "1": {"background": "#a4bdfc", "foreground": "#1d1d1d", "access": "reader"},
            "5": {"background": "#fbd75b", "foreground": "#1d1d1d", "access": "reader"}},
            "calendar": {"1": {"background": "#ac725e", "foreground": "#1d1d1d", "access": "reader"}}}),
        result_eq: r_colors,
    });

    let b_create: &[(&str, Value)] = &[
        ("/summary", json!("Budget review")),
        ("/start/dateTime", json!("2026-06-01T09:00:00.000Z")),
        ("/end/dateTime", json!("2026-06-01T10:00:00.000Z")),
        ("/location", json!("Berlin")),
        ("/colorId", json!("5")),
        ("/attendees/0/email", json!("sam@example.com")),
        ("/guestsCanModify", json!(true)),
        ("/guestsCanSeeOtherGuests", json!(true)),
        ("/conferenceData/createRequest/conferenceSolutionKey/type", json!("hangoutsMeet")),
    ];
    let r_create: &[(&str, Value)] = &[
        ("/id", json!("ev0d1")),
        ("/summary", json!("Budget review")),
        ("/status", json!("confirmed")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_CREATE_EVENT",
        args: json!({"calendar_id": "primary", "title": "Budget review",
            "start_date_time": "2026-06-01T09:00:00Z", "end_date_time": "2026-06-01T10:00:00Z",
            "location": "Berlin", "description": "Quarterly review", "color_id": "5",
            "attendees": ["sam@example.com"], "guests_can_modify": true,
            "guests_can_invite_others": false, "guests_can_see_other_guests": true,
            "send_notifications": "all", "create_meet_link": true}),
        method: "POST",
        url_contains: &["/calendars/primary/events?sendUpdates=all&conferenceDataVersion=1"],
        body_eq: b_create,
        response_status: 200,
        response: event("ev0d1", "Budget review"),
        result_eq: r_create,
    });

    let r_quick: &[(&str, Value)] = &[
        ("/id", json!("evquick")),
        ("/summary", json!("Lunch with Sam")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_CREATE_QUICK_EVENT",
        args: json!({"calendar_id": "primary", "text": "Lunch with Sam tomorrow at 1pm",
            "send_updates": "none"}),
        method: "POST",
        url_contains: &["/calendars/primary/events/quickAdd?text=Lunch+with+Sam+tomorrow+at+1pm",
            "sendUpdates=none"],
        body_eq: &[],
        response_status: 200,
        response: event("evquick", "Lunch with Sam"),
        result_eq: r_quick,
    });

    let b_update: &[(&str, Value)] = &[
        ("/summary", json!("Budget review v2")),
        ("/start/dateTime", json!("2026-06-01T11:00:00.000Z")),
        ("/attendees/0/email", json!("sam@example.com")),
        ("/guestsCanModify", json!(false)),
    ];
    let r_update: &[(&str, Value)] = &[
        ("/id", json!("ev0d1c3")),
        ("/summary", json!("Budget review v2")),
        ("/etag", json!("\"3499\"")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_UPDATE_EVENT",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3", "title": "Budget review v2",
            "start_date_time": "2026-06-01T11:00:00Z", "attendees": ["sam@example.com"],
            "guests_can_modify": false, "send_updates": "externalOnly"}),
        method: "PATCH",
        url_contains: &["/calendars/primary/events/ev0d1c3?sendUpdates=externalOnly",
            "conferenceDataVersion=1"],
        body_eq: b_update,
        response_status: 200,
        response: event("ev0d1c3", "Budget review v2"),
        result_eq: r_update,
    });

    let b_add: &[(&str, Value)] = &[
        ("/attendees/0/email", json!("sam@example.com")),
        ("/attendees/1/email", json!("lee@example.com")),
    ];
    let r_add: &[(&str, Value)] = &[
        ("/id", json!("ev0d1c3")),
        ("/attendees/1/email", json!("lee@example.com")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_ADD_ATTENDEES",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3",
            "attendees": ["sam@example.com", "lee@example.com"], "send_updates": "all"}),
        method: "PATCH",
        url_contains: &["/calendars/primary/events/ev0d1c3?sendUpdates=all"],
        body_eq: b_add,
        response_status: 200,
        response: json!({"kind": "calendar#event", "etag": "\"3502\"", "id": "ev0d1c3",
            "status": "confirmed", "summary": "Budget review",
            "start": {"dateTime": "2026-06-01T09:00:00+02:00"},
            "end": {"dateTime": "2026-06-01T10:00:00+02:00"},
            "attendees": [{"email": "sam@example.com", "responseStatus": "needsAction"},
                          {"email": "lee@example.com", "responseStatus": "needsAction"}]}),
        result_eq: r_add,
    });

    let b_remove: &[(&str, Value)] = &[
        ("/attendees/0/email", json!("owner@example.com")),
    ];
    let r_remove: &[(&str, Value)] = &[
        ("/id", json!("ev0d1c3")),
        ("/attendees/0/email", json!("owner@example.com")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_REMOVE_ATTENDEE",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3",
            "attendees": ["owner@example.com"], "send_updates": "none"}),
        method: "PATCH",
        url_contains: &["/calendars/primary/events/ev0d1c3?sendUpdates=none"],
        body_eq: b_remove,
        response_status: 200,
        response: json!({"kind": "calendar#event", "etag": "\"3501\"", "id": "ev0d1c3",
            "status": "confirmed", "summary": "Budget review",
            "start": {"dateTime": "2026-06-01T09:00:00+02:00"},
            "end": {"dateTime": "2026-06-01T10:00:00+02:00"},
            "attendees": [{"email": "owner@example.com", "self": true, "responseStatus": "accepted"}]}),
        result_eq: r_remove,
    });

    cases.push(Case {
        tool: "GOOGLECALENDAR_DELETE_EVENT",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3", "send_updates": "all"}),
        method: "DELETE",
        url_contains: &["/calendars/primary/events/ev0d1c3?sendUpdates=all"],
        body_eq: &[],
        response_status: 204,
        response: json!({}),
        result_eq: &[("/deleted", json!(true))],
    });

    let r_move: &[(&str, Value)] = &[("/moved", json!(true)), ("/event/id", json!("ev0d1c3"))];
    cases.push(Case {
        tool: "GOOGLECALENDAR_MOVE_EVENT",
        args: json!({"calendar_id": "primary", "event_id": "ev0d1c3",
            "destination_calendar_id": "team@example.com", "send_updates": "all"}),
        method: "POST",
        url_contains: &["/calendars/primary/events/ev0d1c3/move?destination=team%40example.com",
            "sendUpdates=all"],
        body_eq: &[],
        response_status: 200,
        response: event("ev0d1c3", "Budget review"),
        result_eq: r_move,
    });

    let b_import: &[(&str, Value)] = &[
        ("/iCalUID", json!("abc123@google.com")),
        ("/summary", json!("Imported review")),
        ("/start/dateTime", json!("2026-06-01T09:00:00.000Z")),
        ("/end/dateTime", json!("2026-06-01T10:00:00.000Z")),
        ("/location", json!("Berlin")),
    ];
    let r_import: &[(&str, Value)] = &[
        ("/id", json!("imp0rt")),
        ("/summary", json!("Imported review")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_IMPORT_EVENT",
        args: json!({"calendar_id": "team@example.com", "ical_uid": "abc123@google.com",
            "summary": "Imported review", "start_date_time": "2026-06-01T09:00:00Z",
            "end_date_time": "2026-06-01T10:00:00Z", "location": "Berlin"}),
        method: "POST",
        url_contains: &["/calendars/team@example.com/events/import"],
        body_eq: b_import,
        response_status: 200,
        response: event("imp0rt", "Imported review"),
        result_eq: r_import,
    });

    let b_add_cal: &[(&str, Value)] = &[("/id", json!("team@example.com"))];
    let r_add_cal: &[(&str, Value)] = &[
        ("/id", json!("team@example.com")),
        ("/access_role", json!("reader")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_ADD_CALENDAR_TO_CALENDAR_LIST",
        args: json!({"id": "team@example.com"}),
        method: "POST",
        url_contains: &["/calendarList"],
        body_eq: b_add_cal,
        response_status: 200,
        response: json!({"kind": "calendar#calendarListEntry", "etag": "\"e9\"",
            "id": "team@example.com", "summary": "Team", "timeZone": "Europe/Berlin",
            "accessRole": "reader", "selected": false, "backgroundColor": "#7986cb",
            "foregroundColor": "#ffffff"}),
        result_eq: r_add_cal,
    });

    let b_watch: &[(&str, Value)] = &[
        ("/id", json!("chan-42")),
        ("/type", json!("web_hook")),
        ("/address", json!("https://hooks.ctox.io/cal")),
    ];
    let r_watch: &[(&str, Value)] = &[
        ("/id", json!("chan-42")),
        ("/resourceId", json!("res-99")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_WATCH_EVENTS",
        args: json!({"calendar_id": "primary", "webhook_url": "https://hooks.ctox.io/cal",
            "channel_id": "chan-42"}),
        method: "POST",
        url_contains: &["/calendars/primary/events/watch"],
        body_eq: b_watch,
        response_status: 200,
        response: json!({"kind": "api#channel", "id": "chan-42", "resourceId": "res-99",
            "resourceUri": "https://www.googleapis.com/calendar/v3/calendars/primary/events",
            "expiration": "1780000000000"}),
        result_eq: r_watch,
    });

    let b_stop: &[(&str, Value)] = &[
        ("/id", json!("chan-42")),
        ("/resourceId", json!("res-99")),
    ];
    cases.push(Case {
        tool: "GOOGLECALENDAR_STOP_WATCH",
        args: json!({"id": "chan-42", "resource_id": "res-99"}),
        method: "POST",
        url_contains: &["/channels/stop"],
        body_eq: b_stop,
        response_status: 204,
        response: json!({}),
        result_eq: &[("/stopped", json!(true))],
    });

    for c in &cases {
        support::run(&adapter, c);
    }
    support::assert_full_coverage(&adapter, &cases);
    assert_eq!(cases.len(), adapter.spec().tools.len());
}

/// Live read-only calls; skipped silently when no token is configured.
/// DE/EN: read-only tools only, token from CTOX_TOOLHUB_LIVE_GOOGLECALENDAR_TOKEN.
#[test]
#[ignore]
fn live_read_only_calls() {
    let adapter = GoogleCalendar::default();
    let calls: [(&str, Value); 3] = [
        ("GOOGLECALENDAR_LIST_CALENDARS", json!({})),
        (
            "GOOGLECALENDAR_LIST_EVENTS",
            json!({"calendar_id": "primary", "single_events": true, "max_results": 5}),
        ),
        ("GOOGLECALENDAR_GET_COLORS", json!({})),
    ];
    for (tool, args) in calls {
        let Some(result) = support::live(&adapter, tool, args) else {
            eprintln!("live test skipped (no CTOX_TOOLHUB_LIVE_GOOGLECALENDAR_TOKEN)");
            return;
        };
        let out = result.unwrap_or_else(|e| panic!("live {tool} failed: {e}"));
        assert!(out.is_object(), "live {tool}: expected an object, got {out}");
    }
}