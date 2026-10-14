//! Zoom toolkit (Zoom API v2). Action set modelled on Activepieces `piece-zoom` (MIT).
//!
//! Auth: the piece is an OAuth2 app (Zoom Marketplace "General App"). It sends
//! `Authorization: Bearer <access_token>` on every call (`lib/common/client.ts`),
//! which is exactly what the hub injects for `AuthScheme::Bearer`, so the same
//! `Zoom` adapter serves both the managed OAuth2 flow and a stored access token.
//! The authorize/token URLs are the piece's `zoomAuth` literals; the default
//! scopes are the ones the piece's auth description asks the app owner to add.
//!
//! The API base URL (`https://api.zoom.us/v2`) is account independent, so
//! `with_api_base` only exists for tests/mirrors.
//!
//! Every request below is one `zoomClient.request*` call from the TypeScript,
//! with the path, query keys and body fields copied verbatim:
//!
//! | tool                                | TS file                              | request                       |
//! |-------------------------------------|--------------------------------------|-------------------------------|
//! | `ZOOM_GET_USER`                     | `lib/actions/get-current-user.ts`    | `GET /users/me`               |
//! | `ZOOM_LIST_MEETINGS`                | `lib/actions/list-meetings.ts`       | `GET /users/me/meetings`      |
//! | `ZOOM_GET_MEETING`                  | `lib/common/meetings.ts`             | `GET /meetings/{id}`          |
//! | `ZOOM_FIND_MEETING`                 | `lib/actions/find-meeting.ts`        | `GET /meetings/{id}`          |
//! | `ZOOM_LIST_PAST_MEETING_INSTANCES`  | `lib/actions/list-past-meeting-instances.ts` | `GET /past_meetings/{id}/instances` |
//! | `ZOOM_LIST_RECORDINGS`              | `lib/actions/list-recordings.ts`     | `GET /users/me/recordings`    |
//! | `ZOOM_CREATE_MEETING`               | `lib/actions/create-meeting.ts`      | `POST /users/me/meetings`     |
//! | `ZOOM_UPDATE_MEETING`               | `lib/actions/update-meeting.ts`      | `PATCH /meetings/{id}`        |
//! | `ZOOM_UPDATE_MEETING_BY_ID`         | `lib/actions/update-meeting-by-id.ts`| `PATCH /meetings/{id}`        |
//! | `ZOOM_DELETE_MEETING`               | `lib/actions/delete-meeting.ts`      | `DELETE /meetings/{id}`       |
//! | `ZOOM_CREATE_MEETING_REGISTRANT`    | `lib/actions/create-meeting-registrant.ts` | `POST /meetings/{id}/registrants` |
//!
//! Not ported: `createCustomApiCallAction` (generic escape hatch) and the meeting
//! dropdown refetcher (`zoomMeetingDropdown` -> `GET /users/me/meetings`, already
//! covered by `ZOOM_LIST_MEETINGS`), which Activepieces runs at edit time only.
//! The piece's own error-message helpers, 30 s timeout and Zoom error mapping stay
//! in the HTTP/hub layer.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "zoom";

const MEETING_READ: &str = "meeting:read:meeting";
const MEETING_READ_LIST: &str = "meeting:read:list_meetings";
const MEETING_WRITE: &str = "meeting:write:meeting";
const MEETING_UPDATE: &str = "meeting:update:meeting";
const MEETING_DELETE: &str = "meeting:delete:meeting";
const MEETING_WRITE_REGISTRANT: &str = "meeting:write:registrant";
const MEETING_READ_PAST: &str = "meeting:read:list_past_instances";
const RECORDING_READ: &str = "cloud_recording:read:list_user_recordings";
const USER_READ: &str = "user:read:user";

const RECORDING_OPTIONS: &[&str] = &["local", "cloud", "none"];
const AUDIO_OPTIONS: &[&str] = &["both", "telephony", "voip", "thirdParty"];

// ---------------------------------------------------------------- schema helpers

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// Checkbox without a default (`Property.Checkbox` without `defaultValue`): the
/// TS update actions send the setting only when the caller sets the flag.
fn tri_flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn choice_default(desc: &str, options: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": options, "default": default})
}

fn date(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "pattern": "^\\d{4}-\\d{2}-\\d{2}$"})
}

/// Minutes, without a schema default: the update actions only send a duration
/// when the caller passes one.
fn minutes(desc: &str) -> Value {
    json!({"type": "integer", "description": desc, "minimum": 1, "maximum": 1440})
}

/// `zoomProps.pageSize()` – 1..300, default 30.
fn page_size() -> Value {
    json!({"type": "integer", "description": "Wie viele Datensätze diese Seite liefert (1-300). Default 30.",
        "minimum": 1, "maximum": 300, "default": 30})
}

/// `zoomProps.nextPageToken()`.
fn next_page_token() -> Value {
    s("Für die erste Seite leer lassen. Für die nächste Seite das Next Page Token des vorherigen Aufrufs eintragen.")
}

fn meeting_id_prop() -> Value {
    s("Numerische Zoom-Meeting-ID, z. B. 85746065432 (Leerzeichen und Bindestriche sind erlaubt).")
}

fn occurrence_id_prop() -> Value {
    s("Nur für wiederkehrende Meetings: Occurrence-ID eines konkreten Termins; leer = das ganze Meeting.")
}

fn recording_prop() -> Value {
    choice("Auto-Aufzeichnung", RECORDING_OPTIONS)
}

fn audio_prop() -> Value {
    choice("Audio", AUDIO_OPTIONS)
}

/// `page_size` + `next_page_token`, the pagination props of the TS list actions.
fn page_props() -> Vec<(&'static str, Value)> {
    vec![("page_size", page_size()), ("next_page_token", next_page_token())]
}

fn with_page(mut extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = page_props();
    v.append(&mut extra);
    v
}

// ---------------------------------------------------------------- meeting ids

/// `zoomClient.normalizeMeetingId`: strip spaces/dashes, then require 1..19 digits.
/// The normalized id is the path segment, so it additionally goes through the
/// shared `path_segment` guard before it is interpolated.
fn meeting_id(args: &Value, key: &str) -> Result<String> {
    let raw = req_str(args, key)?;
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if cleaned.is_empty()
        || cleaned.len() > 19
        || !cleaned.bytes().all(|b| b.is_ascii_digit())
    {
        bail!("`{key}` is not a Zoom meeting ID: a meeting ID is a number such as 85746065432.");
    }
    let guard = json!({"meeting_id": cleaned});
    Ok(path_segment(&guard, "meeting_id")?.to_string())
}

// ---------------------------------------------------------------- date helpers

/// Days from 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

fn format_day(epoch_day: i64) -> String {
    let (y, m, d) = civil_from_days(epoch_day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `parseDay` of `list-recordings.ts`: `YYYY-MM-DD`, and a real calendar date.
fn parse_day(value: &str, label: &str) -> Result<i64> {
    let shown: String = value.chars().take(40).collect();
    let malformed = || {
        anyhow::anyhow!("{label} must be a date in YYYY-MM-DD format with no time part, for example 2026-10-01 (got \"{shown}\").")
    };
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(malformed());
    }
    let num = |r: std::ops::Range<usize>| -> Result<i64> {
        if !r.clone().all(|i| b[i].is_ascii_digit()) {
            return Err(malformed());
        }
        value
            .get(r)
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(malformed)
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || d < 1 || d as u32 > days_in_month(y, m as u32) {
        bail!("{label} \"{value}\" is not a real calendar date.");
    }
    Ok(days_from_civil(y, m as u32, d as u32))
}

/// `addOneMonth` of `list-recordings.ts`: same day next month, clamped to its length.
fn add_one_month(from: i64) -> i64 {
    let (y, m, d) = civil_from_days(from);
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, d.min(days_in_month(ny, nm)))
}

fn today_utc() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64
}

/// `recordingRange` of `list-recordings.ts`: `from`/`to` (at most one month apart,
/// default `to` = today clamped to the one-month limit).
fn recording_range(args: &Value) -> Result<(String, String)> {
    let from = parse_day(req_str(args, "from")?.trim(), "From Date")?;
    let limit = add_one_month(from);
    let token = opt_str(args, "next_page_token");
    let given_to = match opt_str(args, "to") {
        Some(v) => Some(parse_day(v.trim(), "To Date")?),
        None => None,
    };
    if given_to.is_none() && token.is_some() {
        bail!("To Date is required when Next Page Token is set. Use the From and To values returned with that token, so the next page covers the same dates as the previous one.");
    }
    let to = given_to.unwrap_or_else(|| today_utc().min(limit));
    if to < from {
        bail!("To Date must be on or after From Date.");
    }
    if to > limit {
        bail!("Zoom returns at most one month of recordings per request: To Date must be on or before {}.", format_day(limit));
    }
    Ok((format_day(from), format_day(to)))
}

// ---------------------------------------------------------------- responses

/// `zoomClient.listPage`: normalize `next_page_token` and add `has_more`.
fn list_page(body: &Value, items_key: &str) -> Value {
    let Some(map) = body.as_object() else {
        return body.clone();
    };
    let items = body[items_key].as_array().cloned().unwrap_or_default();
    let token = body["next_page_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(String::from);
    let mut out = map.clone();
    out.insert(items_key.to_string(), Value::Array(items));
    out.insert(
        "next_page_token".into(),
        token.clone().map_or(Value::Null, Value::from),
    );
    out.insert("has_more".into(), json!(token.is_some()));
    Value::Object(out)
}

/// Tri-state lookup: `Some` only when the caller actually passed the boolean.
fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn put_opt_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

/// The `settings` sub-object of the two update actions: only values the caller
/// actually passed are sent (both TS actions drop undefined/null/empty entries).
fn update_settings(args: &Value) -> Option<Value> {
    let mut s = Map::new();
    for key in ["auto_recording", "audio"] {
        if let Some(v) = opt_str(args, key) {
            s.insert(key.into(), json!(v));
        }
    }
    for key in [
        "host_video",
        "participant_video",
        "join_before_host",
        "mute_upon_entry",
        "waiting_room",
    ] {
        if let Some(v) = opt_bool(args, key) {
            s.insert(key.into(), json!(v));
        }
    }
    (!s.is_empty()).then_some(Value::Object(s))
}

// ---------------------------------------------------------------- adapter

pub struct Zoom {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Zoom {
    fn default() -> Self {
        Self::with_api_base("https://api.zoom.us/v2")
    }
}

impl Zoom {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Zoom".into(),
            description: LocalizedText::new(
                "Zoom-Meetings, Teilnehmer und Cloud-Aufzeichnungen lesen und verwalten.",
                "Read and manage Zoom meetings, registrants and cloud recordings.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    // `zoomAuth` in src/index.ts.
                    authorize_url: "https://zoom.us/oauth/authorize".into(),
                    token_url: "https://zoom.us/oauth/token".into(),
                    // The piece ships `scope: []` and asks the app owner to add
                    // these per-action scopes (see the `zoomAuth` description).
                    default_scopes: [
                        MEETING_READ,
                        MEETING_READ_LIST,
                        MEETING_WRITE,
                        MEETING_UPDATE,
                        MEETING_DELETE,
                        MEETING_WRITE_REGISTRANT,
                        MEETING_READ_PAST,
                        RECORDING_READ,
                        USER_READ,
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                // `zoomClient.request` sends `Authorization: Bearer <token>`.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "ZOOM_GET_USER",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    (
                        "Liest das Profil des Zoom-Nutzers hinter dieser Verbindung.",
                        "Reads the profile of the Zoom user behind this connection.",
                    ),
                    &[],
                    &[],
                    &[USER_READ],
                    false,
                ),
                t(
                    "ZOOM_LIST_MEETINGS",
                    ("Meetings auflisten", "List meetings"),
                    (
                        "Listet die Meetings des verbundenen Nutzers, eine Seite pro Aufruf.",
                        "Lists the connected user's meetings, one page at a time.",
                    ),
                    &with_page(vec![(
                        "type",
                        choice_default(
                            "Welche Meetings: scheduled, live, upcoming, upcoming_meetings oder previous_meetings",
                            &["scheduled", "live", "upcoming", "upcoming_meetings", "previous_meetings"],
                            "scheduled",
                        ),
                    )]),
                    &[],
                    &[MEETING_READ_LIST],
                    false,
                ),
                t(
                    "ZOOM_GET_MEETING",
                    ("Meeting lesen", "Get meeting"),
                    (
                        "Liest ein Meeting über seine Meeting-ID, optional einen Termin.",
                        "Reads one meeting by its meeting ID, optionally one occurrence.",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("occurrence_id", occurrence_id_prop()),
                        (
                            "show_previous_occurrences",
                            flag(
                                "Für wiederkehrende Meetings: bereits vergangene Termine mitliefern.",
                                false,
                            ),
                        ),
                    ],
                    &["meeting_id"],
                    &[MEETING_READ],
                    false,
                ),
                t(
                    "ZOOM_FIND_MEETING",
                    ("Meeting finden", "Find meeting"),
                    (
                        "Sucht ein Meeting über die Meeting-ID (Zoom_GET_MEETING mit denselben Feldern).",
                        "Looks a meeting up by meeting ID (same fields as ZOOM_GET_MEETING).",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("occurrence_id", occurrence_id_prop()),
                        (
                            "show_previous_occurrences",
                            flag(
                                "Für wiederkehrende Meetings: alle bisherigen Termine anzeigen.",
                                false,
                            ),
                        ),
                    ],
                    &["meeting_id"],
                    &[MEETING_READ],
                    false,
                ),
                t(
                    "ZOOM_LIST_PAST_MEETING_INSTANCES",
                    ("Vergangene Meeting-Termine auflisten", "List past meeting instances"),
                    (
                        "Listet die beendeten Instanzen (UUID und Startzeit) eines Meetings.",
                        "Lists the ended instances (UUID and start time) of a meeting.",
                    ),
                    &[("meeting_id", meeting_id_prop())],
                    &["meeting_id"],
                    &[MEETING_READ_PAST],
                    false,
                ),
                t(
                    "ZOOM_LIST_RECORDINGS",
                    ("Cloud-Aufzeichnungen auflisten", "List cloud recordings"),
                    (
                        "Listet die Cloud-Aufzeichnungen zwischen zwei Daten (max. ein Monat Abstand).",
                        "Lists cloud recordings between two dates (at most one month apart).",
                    ),
                    &with_page(vec![
                        ("from", date("Startdatum YYYY-MM-DD (UTC), z. B. 2026-10-01.")),
                        ("to", date("Enddatum YYYY-MM-DD (UTC), höchstens ein Monat nach From Date. Mit Next Page Token erforderlich.")),
                    ]),
                    &["from"],
                    &[RECORDING_READ],
                    false,
                ),
                t(
                    "ZOOM_CREATE_MEETING",
                    ("Meeting anlegen", "Create meeting"),
                    (
                        "Legt ein neues Zoom-Meeting an; braucht --confirm.",
                        "Creates a new Zoom meeting; requires --confirm.",
                    ),
                    &[
                        ("topic", s("Thema des Meetings.")),
                        ("start_time", s("Startzeitpunkt, z. B. 2026-11-01T15:00:00 (in der Zeitzone unten) oder 2026-11-01T15:00:00Z.")),
                        ("timezone", s("IANA-Zeitzone, z. B. America/New_York. Default UTC.")),
                        ("duration", int("Dauer in Minuten (Default 30).", 1, 1440, 30)),
                        ("auto_recording", recording_prop()),
                        ("audio", audio_prop()),
                        ("agenda", s("Agenda des Meetings (Default \"My Meeting\").")),
                        ("password", s("Passcode zum Beitreten: max. 10 Zeichen, alphanumerisch plus @ - _ *")),
                        ("pre_schedule", flag("Das Meeting wurde über die Google-Suite-App vorgeplant.", false)),
                        ("schedule_for", s("E-Mail oder User-ID des Nutzers, für den geplant wird.")),
                        (
                            "require_authentication",
                            flag("Nur eingeloggte Zoom-Nutzer dürfen beitreten.", true),
                        ),
                    ],
                    &["topic"],
                    &[MEETING_WRITE],
                    true,
                ),
                t(
                    "ZOOM_UPDATE_MEETING",
                    ("Meeting ändern", "Update meeting"),
                    (
                        "Ändert ein Meeting; nur übergebene Felder werden geändert; braucht --confirm.",
                        "Changes a meeting; only the fields you pass are changed; requires --confirm.",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("topic", s("Neues Thema.")),
                        ("start_time", s("Neuer Startzeitpunkt (ISO 8601, z. B. 2026-11-01T15:00:00Z).")),
                        ("duration", minutes("Neue Dauer in Minuten; leer lassen, um sie unverändert zu lassen.")),
                        ("timezone", s("Zeitzone, z. B. UTC oder America/New_York.")),
                        ("auto_recording", recording_prop()),
                        ("audio", audio_prop()),
                        ("agenda", s("Neue Agenda.")),
                        ("password", s("Neuer Passcode (max. 10 Zeichen).")),
                        ("host_video", tri_flag("Video starten, wenn der Host beitritt.")),
                        ("participant_video", tri_flag("Video starten, wenn Teilnehmer beitreten.")),
                        ("join_before_host", tri_flag("Teilnehmer dürfen vor dem Host beitreten.")),
                        ("mute_upon_entry", tri_flag("Teilnehmer werden stummgeschaltet.")),
                        ("waiting_room", tri_flag("Warteraum ein- oder ausschalten.")),
                    ],
                    &["meeting_id"],
                    &[MEETING_UPDATE],
                    true,
                ),
                t(
                    "ZOOM_UPDATE_MEETING_BY_ID",
                    ("Meeting per ID ändern", "Update meeting by ID"),
                    (
                        "Ändert ein Meeting über seine ID, optional einen Termin davon; braucht --confirm.",
                        "Changes a meeting by ID, optionally one occurrence of it; requires --confirm.",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("occurrence_id", occurrence_id_prop()),
                        ("topic", s("Neues Thema.")),
                        ("start_time", s("Neuer Startzeitpunkt, z. B. 2026-11-01T15:00:00.")),
                        ("duration", minutes("Neue Dauer in Minuten; leer lassen, um sie unverändert zu lassen.")),
                        ("timezone", s("IANA-Zeitzone, z. B. America/New_York.")),
                        ("agenda", s("Neuer Agendatext.")),
                        (
                            "clear_agenda",
                            flag("Agenda entfernen; ignoriert, wenn agenda gefüllt ist.", false),
                        ),
                        ("password", s("Neuer Passcode (max. 10 Zeichen).")),
                        ("auto_recording", recording_prop()),
                        ("audio", audio_prop()),
                        ("host_video", tri_flag("Video starten, wenn der Host beitritt.")),
                        ("participant_video", tri_flag("Video starten, wenn Teilnehmer beitreten.")),
                        ("join_before_host", tri_flag("Teilnehmer dürfen vor dem Host beitreten.")),
                        ("mute_upon_entry", tri_flag("Teilnehmer werden stummgeschaltet.")),
                        ("waiting_room", tri_flag("Warteraum ein- oder ausschalten.")),
                    ],
                    &["meeting_id"],
                    &[MEETING_UPDATE],
                    true,
                ),
                t(
                    "ZOOM_DELETE_MEETING",
                    ("Meeting löschen", "Delete meeting"),
                    (
                        "Löscht ein Meeting oder einen Termin davon; braucht --confirm.",
                        "Deletes a meeting or one occurrence of it; requires --confirm.",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("occurrence_id", occurrence_id_prop()),
                        (
                            "schedule_for_reminder",
                            flag("Host und alternative Hosts über die Absage informieren.", false),
                        ),
                        ("cancel_meeting_reminder", flag("Teilnehmer über die Absage informieren.", false)),
                    ],
                    &["meeting_id"],
                    &[MEETING_DELETE],
                    true,
                ),
                t(
                    "ZOOM_CREATE_MEETING_REGISTRANT",
                    ("Meeting-Teilnehmer registrieren", "Create meeting registrant"),
                    (
                        "Registriert einen Teilnehmer für ein Meeting mit aktivierter Registrierung; braucht --confirm.",
                        "Registers an attendee for a meeting with registration enabled; requires --confirm.",
                    ),
                    &[
                        ("meeting_id", meeting_id_prop()),
                        ("first_name", s("Vorname des Teilnehmers.")),
                        ("last_name", s("Nachname des Teilnehmers.")),
                        ("email", s("E-Mail-Adresse des Teilnehmers.")),
                        ("address", s("Adresse des Teilnehmers.")),
                        ("city", s("Stadt des Teilnehmers.")),
                        ("state", s("Bundesland oder Provinz des Teilnehmers.")),
                        ("zip", s("Postleitzahl des Teilnehmers.")),
                        ("country", s("Zweibuchstaben-Ländercode, z. B. DE.")),
                        ("phone", s("Telefonnummer des Teilnehmers.")),
                        ("comments", s("Fragen und Kommentare des Teilnehmers.")),
                        (
                            "custom_questions",
                            json!({"type": "object", "description": "Antworten auf die Custom-Questions des Meetings: Question-Titel als Key, Antwort als Value."}),
                        ),
                        ("industry", s("Branche des Teilnehmers.")),
                        ("job_title", s("Position des Teilnehmers.")),
                        (
                            "no_of_employees",
                            choice("Anzahl Mitarbeiter", &["1-20", "21-50", "51-100", "101-500", "500-1,000", "1,001-5,000", "5,001-10,000", "More than 10,000"]),
                        ),
                        ("org", s("Organisation des Teilnehmers.")),
                        (
                            "purchasing_time_frame",
                            choice("Zeitrahmen des Kaufs", &["Within a month", "1-3 months", "4-6 months", "More than 6 months", "No timeframe"]),
                        ),
                        (
                            "role_in_purchase_process",
                            choice("Rolle im Kaufprozess", &["Decision Maker", "Evaluator/Recommender", "Influencer", "Not involved"]),
                        ),
                    ],
                    &["meeting_id", "first_name", "email"],
                    &[MEETING_WRITE_REGISTRANT],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api_base)
    }

    /// Body of `zoom_create_meeting` (`buildCreateMeetingBody`).
    fn create_meeting_body(args: &Value) -> Result<Value> {
        let mut settings = Map::new();
        for (k, v) in [
            ("allow_multiple_devices", json!(true)),
            ("approval_type", json!(2)),
            (
                "audio",
                json!(opt_str(args, "audio").unwrap_or("telephony")),
            ),
            ("calendar_type", json!(1)),
            ("close_registration", json!(false)),
            ("email_notification", json!(true)),
            ("host_video", json!(true)),
            ("join_before_host", json!(false)),
            (
                "meeting_authentication",
                json!(opt_bool(args, "require_authentication").unwrap_or(true)),
            ),
            ("mute_upon_entry", json!(false)),
            ("participant_video", json!(false)),
            ("private_meeting", json!(false)),
            ("registrants_confirmation_email", json!(true)),
            ("registrants_email_notification", json!(true)),
            ("registration_type", json!(1)),
            ("show_share_button", json!(true)),
            ("host_save_video_order", json!(true)),
        ] {
            settings.insert(k.into(), v);
        }
        if let Some(r) = opt_str(args, "auto_recording") {
            settings.insert("auto_recording".into(), json!(r));
        }
        let mut body = Map::new();
        body.insert("topic".into(), json!(req_str(args, "topic")?.trim()));
        body.insert("type".into(), json!(2));
        body.insert(
            "agenda".into(),
            json!(opt_str(args, "agenda").unwrap_or("My Meeting")),
        );
        body.insert("default_password".into(), json!(false));
        body.insert(
            "duration".into(),
            json!(args.get("duration").and_then(Value::as_u64).unwrap_or(30)),
        );
        body.insert(
            "pre_schedule".into(),
            json!(opt_bool(args, "pre_schedule").unwrap_or(false)),
        );
        body.insert(
            "timezone".into(),
            json!(opt_str(args, "timezone").unwrap_or("UTC")),
        );
        for (key, field) in [
            ("start_time", "start_time"),
            ("password", "password"),
            ("schedule_for", "schedule_for"),
        ] {
            put_opt_str(&mut body, args, key, field);
        }
        body.insert("settings".into(), Value::Object(settings));
        Ok(Value::Object(body))
    }

    /// Body of `zoom_update_meeting` (only truthy values are sent, as in the TS).
    fn update_meeting_body(args: &Value) -> Value {
        let mut body = Map::new();
        for key in ["topic", "start_time", "timezone", "agenda", "password"] {
            put_opt_str(&mut body, args, key, key);
        }
        if let Some(d) = args.get("duration").and_then(Value::as_u64).filter(|d| *d > 0) {
            body.insert("duration".into(), json!(d));
        }
        if let Some(settings) = update_settings(args) {
            body.insert("settings".into(), settings);
        }
        Value::Object(body)
    }

    /// Body of `zoom_update_meeting_by_id`; returns the body and the changed field
    /// names (`buildUpdateBody` + the `updatedFields` list of the TS action).
    fn update_meeting_by_id_body(args: &Value) -> Result<(Value, Vec<String>)> {
        let mut body = Map::new();
        for key in ["topic", "start_time", "timezone", "password"] {
            put_opt_str(&mut body, args, key, key);
        }
        if let Some(d) = args.get("duration") {
            let minutes = d
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("`duration` must be a whole number of minutes."))?;
            body.insert("duration".into(), json!(minutes));
        }
        // `agenda` wins over `clear_agenda`, exactly as in the TS spread.
        match args.get("agenda").and_then(Value::as_str).filter(|a| !a.is_empty()) {
            Some(a) => {
                body.insert("agenda".into(), json!(a));
            }
            None if opt_bool(args, "clear_agenda") == Some(true) => {
                body.insert("agenda".into(), json!(""));
            }
            None => {}
        }
        let settings = update_settings(args);
        let mut fields: Vec<String> = body.keys().cloned().collect();
        if let Some(s) = settings {
            if let Some(map) = s.as_object() {
                for k in map.keys() {
                    fields.push(format!("settings.{k}"));
                }
            }
            body.insert("settings".into(), s);
        }
        if fields.is_empty() {
            bail!("Nothing to update: fill in at least one field to change (or turn on Clear Agenda).");
        }
        Ok((Value::Object(body), fields))
    }

    /// Body of `zoom_create_meeting_registrant`.
    fn registrant_body(args: &Value) -> Result<Value> {
        let mut body = Map::new();
        body.insert("first_name".into(), json!(req_str(args, "first_name")?.trim()));
        body.insert("email".into(), json!(req_str(args, "email")?.trim()));
        for key in [
            "last_name",
            "address",
            "city",
            "state",
            "zip",
            "country",
            "phone",
            "comments",
            "industry",
            "job_title",
            "no_of_employees",
            "org",
            "purchasing_time_frame",
            "role_in_purchase_process",
        ] {
            put_opt_str(&mut body, args, key, key);
        }
        // `custom_questions` is an object in the props and an array on the wire.
        if let Some(q) = args.get("custom_questions").filter(|v| !v.is_null()) {
            let Some(answers) = q.as_object() else {
                bail!("`custom_questions` must be an object with the question titles as keys");
            };
            if !answers.is_empty() {
                body.insert(
                    "custom_questions".into(),
                    Value::Array(
                        answers
                            .iter()
                            .map(|(title, value)| {
                                json!({"title": title, "value": value.as_str().unwrap_or_default()})
                            })
                            .collect(),
                    ),
                );
            }
        }
        Ok(Value::Object(body))
    }
}

impl ToolkitAdapter for Zoom {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: lib/actions/get-current-user.ts
            "ZOOM_GET_USER" => HttpRequest::new("GET", self.url("/users/me")),

            // AP: lib/actions/list-meetings.ts
            "ZOOM_LIST_MEETINGS" => {
                let kind = opt_str(args, "type").unwrap_or("scheduled");
                if !["scheduled", "live", "upcoming", "upcoming_meetings", "previous_meetings"]
                    .contains(&kind)
                {
                    bail!("invalid argument `type`: {kind}");
                }
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/users/me/meetings{}",
                        query(&[
                            ("type", Some(kind.to_string())),
                            (
                                "page_size",
                                Some(opt_u64(args, "page_size", 30, 1, 300).to_string())
                            ),
                            ("next_page_token", opt_str(args, "next_page_token").map(String::from)),
                        ])
                    )),
                )
            }

            // AP: lib/common/meetings.ts (zoomMeetings.getMeeting, used by
            // lib/actions/get-meeting.ts and lib/actions/find-meeting.ts)
            "ZOOM_GET_MEETING" | "ZOOM_FIND_MEETING" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/meetings/{}{}",
                    meeting_id(args, "meeting_id")?,
                    query(&[
                        ("occurrence_id", opt_str(args, "occurrence_id").map(String::from)),
                        (
                            "show_previous_occurrences",
                            (opt_bool(args, "show_previous_occurrences") == Some(true))
                                .then(|| "true".to_string()),
                        ),
                    ])
                )),
            ),

            // AP: lib/actions/list-past-meeting-instances.ts
            "ZOOM_LIST_PAST_MEETING_INSTANCES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/past_meetings/{}/instances",
                    meeting_id(args, "meeting_id")?
                )),
            ),

            // AP: lib/actions/list-recordings.ts
            "ZOOM_LIST_RECORDINGS" => {
                let (from, to) = recording_range(args)?;
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/users/me/recordings{}",
                        query(&[
                            ("from", Some(from)),
                            ("to", Some(to)),
                            (
                                "page_size",
                                Some(opt_u64(args, "page_size", 30, 1, 300).to_string())
                            ),
                            (
                                "next_page_token",
                                opt_str(args, "next_page_token").map(String::from),
                            ),
                        ])
                    )),
                )
            }

            // AP: lib/actions/create-meeting.ts
            "ZOOM_CREATE_MEETING" => HttpRequest::new("POST", self.url("/users/me/meetings"))
                .json(&Self::create_meeting_body(args)?),

            // AP: lib/actions/update-meeting.ts
            "ZOOM_UPDATE_MEETING" => HttpRequest::new(
                "PATCH",
                self.url(&format!("/meetings/{}", meeting_id(args, "meeting_id")?)),
            )
            .json(&Self::update_meeting_body(args)),

            // AP: lib/actions/update-meeting-by-id.ts
            "ZOOM_UPDATE_MEETING_BY_ID" => {
                let (body, _) = Self::update_meeting_by_id_body(args)?;
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!(
                        "/meetings/{}{}",
                        meeting_id(args, "meeting_id")?,
                        query(&[(
                            "occurrence_id",
                            opt_str(args, "occurrence_id").map(String::from),
                        )])
                    )),
                )
                .json(&body)
            }

            // AP: lib/actions/delete-meeting.ts
            "ZOOM_DELETE_MEETING" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/meetings/{}{}",
                    meeting_id(args, "meeting_id")?,
                    query(&[
                        ("occurrence_id", opt_str(args, "occurrence_id").map(String::from)),
                        (
                            "schedule_for_reminder",
                            Some((opt_bool(args, "schedule_for_reminder") == Some(true)).to_string()),
                        ),
                        (
                            "cancel_meeting_reminder",
                            Some((opt_bool(args, "cancel_meeting_reminder") == Some(true)).to_string()),
                        ),
                    ])
                )),
            ),

            // AP: lib/actions/create-meeting-registrant.ts
            "ZOOM_CREATE_MEETING_REGISTRANT" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/meetings/{}/registrants",
                    meeting_id(args, "meeting_id")?
                )),
            )
            .json(&Self::registrant_body(args)?),

            other => bail!("unknown Zoom tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "ZOOM_LIST_MEETINGS" => list_page(&v, "meetings"),
            "ZOOM_LIST_RECORDINGS" => list_page(&v, "meetings"),
            // The three write actions answer `204 No Content`; the TS actions
            // acknowledge that with their own success object (the meeting id and
            // the changed fields are already known to the caller).
            "ZOOM_DELETE_MEETING" => json!({"success": true, "status": response.status}),
            "ZOOM_UPDATE_MEETING" => json!({
                "success": true,
                "message": "Meeting updated successfully",
                "status": response.status,
            }),
            "ZOOM_UPDATE_MEETING_BY_ID" => json!({"success": true, "status": response.status}),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_ids_are_normalised_and_guarded() {
        let z = Zoom::with_api_base("https://z.test");
        let r = z
            .build_request("ZOOM_GET_MEETING", &json!({"meeting_id": "857 460-65432"}))
            .unwrap();
        assert_eq!(r.url, "https://z.test/meetings/85746065432");
        for bad in ["../x", "abc", "", "1234/5678", "1;2"] {
            assert!(
                z.build_request("ZOOM_GET_MEETING", &json!({"meeting_id": bad}))
                    .is_err(),
                "accepted {bad}"
            );
        }
    }

    #[test]
    fn optional_query_values_are_omitted() {
        let z = Zoom::with_api_base("https://z.test");
        let r = z
            .build_request("ZOOM_LIST_MEETINGS", &json!({"type": "live", "page_size": 300}))
            .unwrap();
        assert_eq!(
            r.url,
            "https://z.test/users/me/meetings?type=live&page_size=300"
        );
        // The token is percent-encoded.
        let r = z
            .build_request(
                "ZOOM_LIST_MEETINGS",
                &json!({"next_page_token": "aB+c/d="}),
            )
            .unwrap();
        assert!(r.url.contains("next_page_token=aB%2Bc%2Fd%3D"), "{}", r.url);
    }

    #[test]
    fn recording_range_enforces_one_month() {
        assert_eq!(
            recording_range(&json!({"from": "2026-01-15", "to": "2026-02-15"})).unwrap(),
            ("2026-01-15".to_string(), "2026-02-15".to_string())
        );
        assert!(recording_range(&json!({"from": "2026-01-15", "to": "2026-03-01"})).is_err());
        assert!(recording_range(&json!({"from": "2026-02-30"})).is_err());
        assert!(recording_range(&json!({"from": "2026-13-01"})).is_err());
        assert!(recording_range(&json!({"from": "01/15/2026"})).is_err());
        assert!(recording_range(&json!({"from": "2026-01-15", "next_page_token": "t"})).is_err());
        // Default `to` is today clamped to the one-month limit.
        let (from, to) = recording_range(&json!({"from": "2020-01-15"})).unwrap();
        assert_eq!(from, "2020-01-15");
        assert_eq!(to, "2020-02-15");
    }

    #[test]
    fn update_bodies_only_carry_passed_fields() {
        let z = Zoom::with_api_base("https://z.test");
        let r = z
            .build_request(
                "ZOOM_UPDATE_MEETING_BY_ID",
                &json!({"meeting_id": "123", "clear_agenda": true, "waiting_room": false}),
            )
            .unwrap();
        assert_eq!(r.url, "https://z.test/meetings/123");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["agenda"], json!(""));
        assert_eq!(body["settings"], json!({"waiting_room": false}));
        assert!(z
            .build_request("ZOOM_UPDATE_MEETING_BY_ID", &json!({"meeting_id": "123"}))
            .is_err());
    }
}