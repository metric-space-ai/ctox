//! Port tests for the Zoom toolkit (Zoom API v2).
//! One recorded case per action; the expected requests are derived from the
//! TypeScript `run()` code of Activepieces `piece-zoom` (MIT).
mod support;

use ctox_toolhub::toolkits::zoom::Zoom;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

const MEETING: &str = "85746065432";

/// `GET /users/me` – recorded shape of a Zoom user profile.
fn current_user() -> Value {
    json!({
        "id": "KdYKjnimT4KPd8KKdQt9FQ",
        "first_name": "Jane",
        "last_name": "Doe",
        "email": "jane.doe@example.com",
        "type": 2,
        "role_name": "Owner",
        "pmi": 1234567890,
        "personal_meeting_url": "https://zoom.us/j/1234567890",
        "timezone": "America/Los_Angeles",
        "verified": 1,
        "dept": "Engineering",
        "created_time": "2019-02-01T09:00:00Z",
        "last_login_time": "2026-10-01T08:12:03Z",
        "last_client_version": "5.13.1",
        "host_key": "hqkO0pXQZ2Wa1jFPnQ8jMxUu0lZ2hBq",
        "pic_url": "",
        "account_id": "kAFu6CnrTbSKWqmQC7K3mU2hQpbnbxQ",
    })
}

/// `GET /meetings/{id}` / one entry of `GET /users/me/meetings`.
fn meeting(id: &str, topic: &str) -> Value {
    json!({
        "id": id.parse::<u64>().unwrap(),
        "uuid": "1tDpFFuZQ2qLbUYFTWLNy9RLxbAv8t3V",
        "host_id": "KdYKjnimT4KPd8KKdQt9FQ",
        "host_email": "jane.doe@example.com",
        "type": 2,
        "topic": topic,
        "agenda": "Weekly sync",
        "start_time": "2026-11-01T15:00:00Z",
        "duration": 45,
        "timezone": "UTC",
        "created_at": "2026-10-01T09:12:00Z",
        "join_url": format!("https://zoom.us/j/{id}?pwd=Zx9k3mQpR2"),
        "start_url": "https://zoom.us/s/85746065432?zak=zak_eyJ0Ijoi",
        "password": "k3m9xq",
        "pre_schedule": false,
        "status": "waiting",
        "occurrence_count": 1,
        "occurrences": [
            {"duration": 45, "occurrence_id": "1648194360000", "start_time": "2026-11-01T15:00:00Z", "status": "available"}
        ],
        "settings": {
            "host_video": true,
            "participant_video": false,
            "join_before_host": false,
            "mute_upon_entry": true,
            "waiting_room": false,
            "meeting_authentication": true,
            "audio": "both",
            "auto_recording": "cloud",
            "approval_type": 2,
            "registration_type": 1,
        },
    })
}

/// `POST /users/me/meetings` / `GET /users/me/recordings` share this recording list entry.
fn recording_files() -> Value {
    json!([
        {
            "id": "2d1b9b1f-2a5e-4a2f-9a1c-0f2a9b1f2a5e",
            "meeting_id": 85746065432u64,
            "recording_type": "shared_screen_with_speaker_view",
            "recording_start": "2026-09-01T16:04:12Z",
            "recording_end": "2026-09-01T16:49:30Z",
            "file_type": "MP4",
            "file_extension": "mp4",
            "file_size": 81234567,
            "play_url": "https://zoom.us/rec/play/2d1b9b1f-2a5e-4a2f-9a1c-0f2a9b1f2a5e",
            "download_url": "",
            "status": "completed",
        },
        {
            "id": "6f1c1c22-7b6d-4c3e-b2f4-11aa22bb33cc",
            "meeting_id": 85746065432u64,
            "recording_type": "audio_only",
            "recording_start": "2026-09-01T16:04:12Z",
            "recording_end": "2026-09-01T16:49:30Z",
            "file_type": "M4A",
            "file_extension": "m4a",
            "file_size": 8123456,
            "play_url": "https://zoom.us/rec/play/6f1c1c22-7b6d-4c3e-b2f4-11aa22bb33cc",
            "download_url": "",
            "status": "completed",
        },
    ])
}

#[test]
fn every_action_matches_the_typescript_request() {
    let zoom = Zoom::default();

    let e_user = [("/email", json!("jane.doe@example.com")), ("/timezone", json!("America/Los_Angeles")), ("/pmi", json!(1234567890))];
    let e_list = [
        ("/meetings/0/id", json!(85746065432u64)),
        ("/meetings/0/topic", json!("Launch webinar")),
        ("/next_page_token", json!("TWNuIQ9")),
        ("/has_more", json!(true)),
        ("/page_size", json!(30)),
    ];
    let e_get = [
        ("/id", json!(85746065432u64)),
        ("/join_url", json!("https://zoom.us/j/85746065432?pwd=Zx9k3mQpR2")),
        ("/settings/mute_upon_entry", json!(true)),
        ("/occurrences/0/occurrence_id", json!("1648194360000")),
    ];
    let e_find = [("/id", json!(85746065432u64)), ("/topic", json!("Launch webinar"))];
    let e_past = [("/meetings/0/uuid", json!("TFRR4WZT1oG2H8H0lMLpAlP8lEnwJkRb")), ("/meetings/0/duration", json!(45))];
    let e_recordings = [
        ("/meetings/0/topic", json!("Launch webinar")),
        ("/meetings/0/recording_files/0/file_type", json!("MP4")),
        ("/meetings/0/recording_files/1/play_url", json!("https://zoom.us/rec/play/6f1c1c22-7b6d-4c3e-b2f4-11aa22bb33cc")),
        ("/has_more", json!(false)),
        ("/next_page_token", Value::Null),
    ];
    let e_create = [
        ("/topic", json!("Launch webinar")),
        ("/type", json!(2)),
        ("/agenda", json!("My Meeting")),
        ("/duration", json!(30)),
        ("/timezone", json!("UTC")),
        ("/pre_schedule", json!(false)),
        ("/settings/audio", json!("telephony")),
        ("/settings/meeting_authentication", json!(true)),
        ("/settings/approval_type", json!(2)),
        ("/id", json!(85746065432u64)),
        ("/join_url", json!("https://zoom.us/j/85746065432?pwd=Zx9k3mQpR2")),
    ];
    let e_update = [
        ("/topic", json!("Launch webinar (moved)")),
        ("/start_time", json!("2026-11-02T15:00:00Z")),
        ("/settings/host_video", json!(true)),
        ("/success", json!(true)),
        ("/message", json!("Meeting updated successfully")),
    ];
    let e_update_by_id = [
        ("/agenda", json!("")),
        ("/timezone", json!("Europe/Berlin")),
        ("/settings/waiting_room", json!(false)),
        ("/success", json!(true)),
    ];
    let e_delete = [("/success", json!(true)), ("/status", json!(204))];
    let e_registrant = [
        ("/first_name", json!("Alex")),
        ("/email", json!("alex@example.com")),
        ("/custom_questions/0/title", json!("Company")),
        ("/custom_questions/0/value", json!("Contoso")),
        ("/registrant_id", json!("ZoQd0aZLQ4CBvwFb0hLcLjaAuQ3gTrIxLwLjlKWXA")),
        ("/join_url", json!("https://zoom.us/w/85746065432?tk=zoqd0azlq4cbvwfb0hlcljaauq3gtrixlwljlkwxa")),
        ("/participant_pin_code", json!(418220)),
    ];

    let cases = vec![
        // ---- read -------------------------------------------------------------
        Case {
            tool: "ZOOM_GET_USER",
            args: json!({}),
            method: "GET",
            url_contains: &["/users/me"],
            body_eq: &[],
            response_status: 200,
            response: current_user(),
            result_eq: &e_user,
        },
        Case {
            tool: "ZOOM_LIST_MEETINGS",
            args: json!({"type": "scheduled", "page_size": 30}),
            method: "GET",
            url_contains: &["/users/me/meetings", "type=scheduled", "page_size=30"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "page_size": 30,
                "total_records": 2,
                "next_page_token": "TWNuIQ9",
                "meetings": [
                    meeting(MEETING, "Launch webinar"),
                    meeting("87654321098", "Weekly sync"),
                ],
            }),
            result_eq: &e_list,
        },
        Case {
            tool: "ZOOM_GET_MEETING",
            args: json!({"meeting_id": MEETING, "show_previous_occurrences": true}),
            method: "GET",
            url_contains: &["/meetings/85746065432", "show_previous_occurrences=true"],
            body_eq: &[],
            response_status: 200,
            response: meeting(MEETING, "Launch webinar"),
            result_eq: &e_get,
        },
        Case {
            tool: "ZOOM_FIND_MEETING",
            args: json!({"meeting_id": "857-460-65432", "occurrence_id": "1648194360000"}),
            method: "GET",
            url_contains: &["/meetings/85746065432", "occurrence_id=1648194360000"],
            body_eq: &[],
            response_status: 200,
            response: meeting(MEETING, "Launch webinar"),
            result_eq: &e_find,
        },
        Case {
            tool: "ZOOM_LIST_PAST_MEETING_INSTANCES",
            args: json!({"meeting_id": MEETING}),
            method: "GET",
            url_contains: &["/past_meetings/85746065432/instances"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "meetings": [
                    {
                        "uuid": "TFRR4WZT1oG2H8H0lMLpAlP8lEnwJkRb",
                        "start_time": "2026-09-01T16:00:00Z",
                        "duration": 45,
                        "total_minutes": 45,
                        "type": 2,
                    },
                ],
            }),
            result_eq: &e_past,
        },
        Case {
            tool: "ZOOM_LIST_RECORDINGS",
            args: json!({"from": "2026-09-01", "to": "2026-09-30", "page_size": 30}),
            method: "GET",
            url_contains: &["/users/me/recordings", "from=2026-09-01", "to=2026-09-30", "page_size=30"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "page_size": 30,
                "total_records": 1,
                "meetings": [
                    {
                        "id": 85746065432u64,
                        "uuid": "1tDpFFuZQ2qLbUYFTWLNy9RLxbAv8t3V",
                        "topic": "Launch webinar",
                        "type": 2,
                        "start_time": "2026-09-01T16:00:00Z",
                        "duration": 45,
                        "timezone": "UTC",
                        "total_size": 89358023,
                        "recording_count": 2,
                        "share_url": "https://zoom.us/rec/share/1tDpFFuZQ2qLbUYFTWLNy9RLxbAv8t3V",
                        "host_id": "KdYKjnimT4KPd8KKdQt9FQ",
                        "recording_files": recording_files(),
                    },
                ],
            }),
            result_eq: &e_recordings,
        },
        // ---- write ------------------------------------------------------------
        Case {
            tool: "ZOOM_CREATE_MEETING",
            args: json!({
                "topic": "Launch webinar",
                "start_time": "2026-11-01T15:00:00",
                "timezone": "UTC",
                "audio": "telephony",
                "require_authentication": true,
            }),
            method: "POST",
            url_contains: &["/users/me/meetings"],
            body_eq: &e_create[..9],
            response_status: 201,
            response: meeting(MEETING, "Launch webinar"),
            result_eq: &e_create[9..],
        },
        Case {
            tool: "ZOOM_UPDATE_MEETING",
            args: json!({
                "meeting_id": MEETING,
                "topic": "Launch webinar (moved)",
                "start_time": "2026-11-02T15:00:00Z",
                "host_video": true,
            }),
            method: "PATCH",
            url_contains: &["/meetings/85746065432"],
            body_eq: &e_update[..3],
            response_status: 204,
            response: json!({}),
            result_eq: &e_update[3..],
        },
        Case {
            tool: "ZOOM_UPDATE_MEETING_BY_ID",
            args: json!({
                "meeting_id": MEETING,
                "occurrence_id": "1648194360000",
                "clear_agenda": true,
                "timezone": "Europe/Berlin",
                "waiting_room": false,
            }),
            method: "PATCH",
            url_contains: &["/meetings/85746065432", "occurrence_id=1648194360000"],
            body_eq: &e_update_by_id[..3],
            response_status: 204,
            response: json!({}),
            result_eq: &e_update_by_id[3..],
        },
        Case {
            tool: "ZOOM_DELETE_MEETING",
            args: json!({
                "meeting_id": MEETING,
                "schedule_for_reminder": false,
                "cancel_meeting_reminder": true,
            }),
            method: "DELETE",
            url_contains: &[
                "/meetings/85746065432",
                "schedule_for_reminder=false",
                "cancel_meeting_reminder=true",
            ],
            body_eq: &[],
            response_status: 204,
            response: json!({}),
            result_eq: &e_delete,
        },
        Case {
            tool: "ZOOM_CREATE_MEETING_REGISTRANT",
            args: json!({
                "meeting_id": MEETING,
                "first_name": "Alex",
                "last_name": "Rivera",
                "email": "alex@example.com",
                "country": "DE",
                "job_title": "Engineer",
                "custom_questions": {"Company": "Contoso"},
            }),
            method: "POST",
            url_contains: &["/meetings/85746065432/registrants"],
            body_eq: &e_registrant[..4],
            response_status: 201,
            response: json!({
                "id": 85746065432u64,
                "join_url": "https://zoom.us/w/85746065432?tk=zoqd0azlq4cbvwfb0hlcljaauq3gtrixlwljlkwxa",
                "registrant_id": "ZoQd0aZLQ4CBvwFb0hLcLjaAuQ3gTrIxLwLjlKWXA",
                "start_time": "2026-11-01T15:00:00Z",
                "topic": "Launch webinar",
                "occurrences": [
                    {"duration": 60, "occurrence_id": "1648194360000", "start_time": "2026-11-01T15:00:00Z", "status": "available"},
                ],
                "participant_pin_code": 418220,
            }),
            result_eq: &e_registrant[4..],
        },
    ];

    support::assert_full_coverage(&zoom, &cases);
    for c in &cases {
        support::run(&zoom, c);
    }
}

#[test]
fn api_base_is_overridable_for_tests() {
    let zoom = Zoom::with_api_base("https://api.zoom.test/v2");
    let req = zoom.build_request("ZOOM_GET_USER", &json!({})).unwrap();
    assert_eq!(req.url, "https://api.zoom.test/v2/users/me");
}

/// Live smoke test: read-only calls with `CTOX_TOOLHUB_LIVE_ZOOM_TOKEN`.
#[test]
#[ignore = "needs a real Zoom OAuth access token"]
fn live_read_only() {
    let zoom = Zoom::default();
    for (tool, args) in [
        ("ZOOM_GET_USER", json!({})),
        ("ZOOM_LIST_MEETINGS", json!({"type": "scheduled", "page_size": 3})),
        (
            "ZOOM_LIST_RECORDINGS",
            json!({"from": "2026-09-01", "to": "2026-09-30", "page_size": 3}),
        ),
    ] {
        let Some(res) = support::live(&zoom, tool, args) else {
            eprintln!("no live token set, skipping {tool}");
            return;
        };
        res.unwrap_or_else(|e| panic!("{tool} failed: {e}"));
    }
}