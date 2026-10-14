//! Google Calendar toolkit (Calendar API v3). Action set modelled on
//! Activepieces `piece-google-calendar` (MIT).
//!
//! DE: Ereignisse in Google Kalendern suchen, lesen, anlegen, ändern, verschieben.
//! EN: search, read, create, update and move events in Google Calendars.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "googlecalendar";
const READ: &str = "https://www.googleapis.com/auth/calendar.readonly";
const EVENTS: &str = "https://www.googleapis.com/auth/calendar.events";

const EVENT_TYPES: &[&str] = &["default", "outOfOffice", "focusTime", "workingLocation"];
const SEND_UPDATES: &[&str] = &["all", "externalOnly", "none"];

pub struct GoogleCalendar {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for GoogleCalendar {
    fn default() -> Self {
        Self::with_api_base("https://www.googleapis.com/calendar/v3")
    }
}

/// `?a=b&c=d` with support for repeated keys (percent-encoded values).
fn qs(pairs: &[(&str, String)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        ser.append_pair(k, v);
    }
    format!("?{}", ser.finish())
}

/// Optional integer without a default (absent = parameter not sent).
fn int_opt(desc: &str, min: u64, max: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max})
}

/// Optional switch: absent = "keep the current value".
fn opt_bool(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn boolean(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn enum_str(desc: &str, values: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": values, "default": default})
}

/// Calendar ids are either `primary` or an address; kept strict so they cannot
/// escape into the path (`googleCalendarCommon.baseUrl/calendars/{id}/...`).
fn calendar_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = req_str(args, key)?;
    if !valid_calendar_id(v) {
        bail!("invalid argument `{key}`");
    }
    Ok(v)
}

fn valid_calendar_id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 200
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'@'))
}

/// `calendar_id` with the Google default (`primary`) when the caller omits it.
fn calendar_or_primary(args: &Value) -> &str {
    opt_str(args, "calendar_id").unwrap_or("primary")
}

/// RFC 3339 -> unix milliseconds (no timezone library in the crate).
pub fn iso_millis(v: &str) -> Result<i64> {
    let bad = || anyhow!("invalid ISO 8601 timestamp `{v}`");
    let b = v.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return Err(bad());
    }
    if !matches!(b[10], b'T' | b't' | b' ') {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| -> Result<i64> {
        v.get(r).ok_or_else(bad)?.parse::<i64>().map_err(|_| bad())
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return Err(bad());
    }
    let mut ms = days_from_civil(y, mo as u32, d as u32) * 86_400_000 + h * 3_600_000 + mi * 60_000
        + sec.min(59) * 1000;
    let mut i = 19;
    if v.as_bytes().get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while v.as_bytes().get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return Err(bad());
        }
        let frac = &v[start..i];
        let mut f = frac[..frac.len().min(3)].to_string();
        while f.len() < 3 {
            f.push('0');
        }
        ms += f.parse::<i64>().map_err(|_| bad())?;
    }
    match v.as_bytes().get(i) {
        None | Some(b'Z') | Some(b'z') => {}
        Some(c @ (b'+' | b'-')) => {
            let sign = if *c == b'-' { -1 } else { 1 };
            let rest = v.get(i + 1..).ok_or_else(bad)?;
            let (oh, om) = match rest.len() {
                5 if rest.as_bytes()[2] == b':' => (&rest[..2], &rest[3..]),
                4 => rest.split_at(2),
                _ => return Err(bad()),
            };
            let oh: i64 = oh.parse().map_err(|_| bad())?;
            let om: i64 = om.parse().map_err(|_| bad())?;
            ms -= sign * (oh * 3_600_000 + om * 60_000);
        }
        _ => return Err(bad()),
    }
    Ok(ms)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    era * 146_097 + (365 * yoe + yoe / 4 - yoe / 100 + doy)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Unix milliseconds -> `YYYY-MM-DDTHH:mm:ss.sssZ` (dayjs format in the TS code).
pub fn millis_to_iso(ms: i64) -> String {
    let (y, mo, d) = civil_from_days(ms.div_euclid(86_400_000));
    let rem = ms.rem_euclid(86_400_000);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

fn norm_iso(args: &Value, key: &str) -> Result<String> {
    Ok(millis_to_iso(iso_millis(req_str(args, key)?)?))
}

fn opt_iso(args: &Value, key: &str) -> Result<Option<String>> {
    match opt_str(args, key) {
        Some(v) => Ok(Some(millis_to_iso(iso_millis(v)?))),
        None => Ok(None),
    }
}

fn attendees(emails: Vec<String>) -> Value {
    Value::Array(
        emails
            .into_iter()
            .filter(|e| !e.trim().is_empty())
            .map(|email| json!({"email": email.trim()}))
            .collect(),
    )
}

fn send_updates(args: &Value, key: &str, default: &str) -> String {
    opt_str(args, key).unwrap_or(default).to_string()
}

fn event_times(args: &Value) -> Result<(String, String)> {
    let start = norm_iso(args, "start_date_time")?;
    let end = match opt_str(args, "end_date_time") {
        Some(_) => norm_iso(args, "end_date_time")?,
        // TS: end defaults to 30 minutes after the start.
        None => millis_to_iso(iso_millis(req_str(args, "start_date_time")?)? + 30 * 60_000),
    };
    Ok((start, end))
}

fn brief_event(e: &Value) -> Value {
    json!({
        "id": e["id"],
        "summary": e["summary"],
        "status": e["status"],
        "html_link": e["htmlLink"],
        "location": e["location"],
        "description": e["description"],
        "start": e["start"],
        "end": e["end"],
        "organizer": e["organizer"],
        "attendees": e["attendees"],
        "recurring_event_id": e["recurringEventId"],
        "color_id": e["colorId"],
        "updated": e["updated"],
        "etag": e["etag"],
    })
}

fn brief_calendar(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "summary": c["summary"],
        "description": c["description"],
        "location": c["location"],
        "time_zone": c["timeZone"],
        "access_role": c["accessRole"],
        "primary": c["primary"],
        "selected": c["selected"],
        "background_color": c["backgroundColor"],
        "foreground_color": c["foregroundColor"],
    })
}

fn random_id() -> String {
    use rand::Rng as _;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

impl GoogleCalendar {
    pub fn with_api_base(api_base: &str) -> Self {
        let cal = s("Calendar ID, e.g. `primary` or the calendar address");
        let ev = s("Event ID from a previous step");
        let t = |slug: &str,
                 title: (&str, &str),
                 desc: (&str, &str),
                 props: &[(&str, Value)],
                 req: &[&str],
                 scopes: &[&str],
                 m: bool| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Google Calendar".into(),
            description: LocalizedText::new(
                "Termine in Google Kalendern suchen, lesen, anlegen, ändern und verschieben.",
                "Search, read, create, update and move Google Calendar events.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://accounts.google.com/o/oauth2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    default_scopes: vec![READ.into(), EVENTS.into(), "email".into()],
                    pkce: true,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![
                        ("access_type".into(), "offline".into()),
                        ("prompt".into(), "consent".into()),
                    ],
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "GOOGLECALENDAR_LIST_CALENDARS",
                    ("Kalender auflisten", "List calendars"),
                    (
                        "Listet alle Kalender mit ID, Titel, Zugriffsrolle und Zeitzone.",
                        "Lists every calendar with id, summary, access role and time zone.",
                    ),
                    &[(
                        "min_access_role",
                        enum_str("Optional access-role filter", &["writer"], "writer"),
                    )],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_GET_CALENDAR",
                    ("Kalender abrufen", "Get calendar"),
                    (
                        "Liest Metadaten eines Kalenders (Titel, Zeitzone, Ort).",
                        "Reads a calendar's metadata (summary, time zone, location).",
                    ),
                    &[("calendar_id", cal.clone())],
                    &["calendar_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_LIST_EVENTS",
                    ("Termine auflisten", "List events"),
                    (
                        "Listet Termine eines Kalenders, optional gefiltert nach Zeitraum, Suchbegriff und Termintyp.",
                        "Lists events of a calendar, optionally filtered by time range, search term and event type.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("search", s("Exact phrase matched against title, description, location and guests")),
                        ("start_date", s("ISO 8601: only events that end after this time")),
                        ("end_date", s("ISO 8601: only events starting before this; needs Start Time")),
                        ("single_events", boolean("Expand recurring events into single occurrences", false)),
                        ("event_types", str_list("Event types to return (default, outOfOffice, focusTime, workingLocation)")),
                        ("max_results", int("1-2500", 1, 2500, 250)),
                    ],
                    &["calendar_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_GET_EVENT",
                    ("Termin lesen", "Get event"),
                    (
                        "Liest einen einzelnen Termin über seine ID.",
                        "Reads a single event by its ID.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", ev.clone()),
                        ("max_attendees", int_opt("Above this count only the organizer is returned", 1, 1000)),
                        ("time_zone", s("IANA zone, e.g. America/New_York")),
                    ],
                    &["calendar_id", "event_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_SEARCH_EVENTS",
                    ("Termine durchsuchen", "Search events"),
                    (
                        "Durchsucht einen Kalender nach einem Suchbegriff in einem Zeitraum.",
                        "Searches one calendar for a text query within a time window.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("query", s("Free-text query, e.g. `budget review`")),
                        ("start_date", s("ISO 8601 lower bound")),
                        ("end_date", s("ISO 8601 upper bound")),
                        ("single_events", boolean("Expand recurring events", true)),
                        ("max_results", int("1-250", 1, 250, 250)),
                    ],
                    &["query"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_LIST_RECURRING_EVENT_INSTANCES",
                    ("Wiederholungen auflisten", "List recurring instances"),
                    (
                        "Listet die einzelnen Termine einer wiederholenden Serie.",
                        "Lists the individual occurrences of one recurring event series.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", s("ID of the recurring series (the parent event)")),
                        ("time_min", s("ISO 8601 lower bound")),
                        ("time_max", s("ISO 8601 upper bound")),
                        ("show_deleted", boolean("Include cancelled instances", false)),
                        ("max_results", int("1-2500", 1, 2500, 250)),
                    ],
                    &["calendar_id", "event_id"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_FIND_BUSY_PERIODS",
                    ("Belegte Zeiten finden", "Find busy periods"),
                    (
                        "Fragt die belegten Zeiträume mehrerer Kalender in einem Fenster ab.",
                        "Queries the busy blocks of one or more calendars within a window.",
                    ),
                    &[
                        ("calendar_ids", str_list("Calendars to check")),
                        ("start_date", s("ISO 8601 window start")),
                        ("end_date", s("ISO 8601 window end")),
                    ],
                    &["calendar_ids", "start_date", "end_date"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_FIND_FREE_SLOTS",
                    ("Freie Zeiten finden", "Find free slots"),
                    (
                        "Berechnet die freien Zeitfenster, die auf allen gewählten Kalendern offen sind.",
                        "Computes the open slots shared by all selected calendars in the window.",
                    ),
                    &[
                        ("calendar_ids", str_list("Calendars that must all be free")),
                        ("start_date", s("ISO 8601 window start")),
                        ("end_date", s("ISO 8601 window end")),
                    ],
                    &["calendar_ids", "start_date", "end_date"],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_LIST_SETTINGS",
                    ("Kontoeinstellungen auflisten", "List settings"),
                    (
                        "Listet die Kontoeinstellungen (Zeitzone, Wochenstart, 24-Stunden-Format).",
                        "Lists the account settings (time zone, week start, 24-hour format).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_GET_COLORS",
                    ("Farben abrufen", "Get colors"),
                    (
                        "Liest die Farbpalette (colorId -> Hex-Farben) für Termine und Kalender.",
                        "Reads the color palette (colorId -> hex colors) for events and calendars.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "GOOGLECALENDAR_CREATE_EVENT",
                    ("Termin anlegen", "Create event"),
                    (
                        "Legt einen Termin mit Titel, Zeiten, Gästen und Optionen an.",
                        "Creates an event with title, times, guests and options.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("title", s("Event title")),
                        ("start_date_time", s("ISO 8601 start; end defaults to +30 min")),
                        ("end_date_time", s("ISO 8601 end")),
                        ("location", s("Location")),
                        ("description", s("Description; HTML is allowed")),
                        ("color_id", s("colorId, see Get Colors")),
                        ("attendees", str_list("Guest emails")),
                        ("guests_can_modify", boolean("Guests may edit the event", false)),
                        ("guests_can_invite_others", boolean("Guests may invite others", false)),
                        ("guests_can_see_other_guests", boolean("Guests see the whole guest list", false)),
                        ("send_notifications", enum_str("Who gets the invitation email", SEND_UPDATES, "all")),
                        ("create_meet_link", boolean("Attach a Google Meet link", false)),
                    ],
                    &["calendar_id", "title", "start_date_time"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_CREATE_QUICK_EVENT",
                    ("Termin per Text anlegen", "Create quick event"),
                    (
                        "Legt einen Termin aus einem Satz in natürlicher Sprache an (z. B. \"Mittag mit Sam morgen um 13 Uhr\").",
                        "Creates an event from a plain-language phrase (e.g. \"Lunch with Sam tomorrow at 1pm\").",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("text", s("Event text")),
                        ("send_updates", enum_str("Who gets the invitation email", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "text"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_UPDATE_EVENT",
                    ("Termin ändern", "Update event"),
                    (
                        "Ändert Felder eines Termins; nicht gesetzte Felder bleiben unverändert.",
                        "Updates fields of an event; fields that are not set keep their value.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", ev.clone()),
                        ("title", s("New title")),
                        ("start_date_time", s("ISO 8601 new start")),
                        ("end_date_time", s("ISO 8601 new end")),
                        ("location", s("New location")),
                        ("description", s("New description")),
                        ("color_id", s("New colorId")),
                        ("attendees", str_list("Replaces the guest list")),
                        ("guests_can_modify", opt_bool("Guests may edit the event; unset keeps the current value")),
                        ("guests_can_invite_others", opt_bool("Guests may invite others; unset keeps the current value")),
                        ("guests_can_see_other_guests", opt_bool("Guests see the whole guest list; unset keeps the current value")),
                        ("send_updates", enum_str("Who gets notified", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "event_id"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_ADD_ATTENDEES",
                    ("Gäste hinzufügen", "Add attendees"),
                    (
                        "Setzt die Gästeliste eines Termins auf die angegebenen E-Mails (Google's patch ersetzt die Liste).",
                        "Sets the guest list of an event to the given emails (Google's patch replaces the list).",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", ev.clone()),
                        ("attendees", str_list("Guest emails")),
                        ("send_updates", enum_str("Who gets notified", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "event_id", "attendees"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_REMOVE_ATTENDEE",
                    ("Gast entfernen", "Remove attendee"),
                    (
                        "Entfernt einen Gast, indem die verbleibende Gästeliste gepatcht wird.",
                        "Removes one guest by patching the remaining guest list.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", ev.clone()),
                        ("attendees", str_list("Emails that stay on the event")),
                        ("send_updates", enum_str("Who gets notified", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "event_id", "attendees"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_DELETE_EVENT",
                    ("Termin löschen", "Delete event"),
                    (
                        "Löscht einen Termin dauerhaft; braucht --confirm.",
                        "Permanently deletes an event; requires --confirm.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("event_id", ev.clone()),
                        ("send_updates", enum_str("Who gets notified", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "event_id"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_MOVE_EVENT",
                    ("Termin verschieben", "Move event"),
                    (
                        "Verschiebt einen Termin in einen anderen Kalender und behält ID und Metadaten.",
                        "Moves an event to another calendar, preserving its ID and metadata.",
                    ),
                    &[
                        ("calendar_id", s("Source calendar ID")),
                        ("event_id", ev.clone()),
                        ("destination_calendar_id", s("Target calendar ID")),
                        ("send_updates", enum_str("Who gets notified", SEND_UPDATES, "none")),
                    ],
                    &["calendar_id", "event_id", "destination_calendar_id"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_IMPORT_EVENT",
                    ("Termin importieren", "Import event"),
                    (
                        "Importiert einen bestehenden Termin (per iCalUID) als private Kopie.",
                        "Imports an existing event (by iCalUID) as a private copy.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("ical_uid", s("The event's iCalUID, e.g. `abc123@google.com`")),
                        ("summary", s("Title")),
                        ("start_date_time", s("ISO 8601 start")),
                        ("end_date_time", s("ISO 8601 end")),
                        ("location", s("Location")),
                        ("description", s("Description")),
                    ],
                    &["calendar_id", "ical_uid", "summary", "start_date_time", "end_date_time"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_ADD_CALENDAR_TO_CALENDAR_LIST",
                    ("Fremden Kalender hinzufügen", "Add calendar to list"),
                    (
                        "Fügt den Kalender einer anderen Person zur eigenen Kalenderliste hinzu.",
                        "Adds another person's calendar to your calendar list.",
                    ),
                    &[("id", s("Calendar ID, found in the calendar settings"))],
                    &["id"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_WATCH_EVENTS",
                    ("Änderungskanal abonnieren", "Watch events"),
                    (
                        "Abonniert Änderungen eines Kalenders über einen Webhook-Kanal.",
                        "Subscribes to changes of a calendar via a webhook channel.",
                    ),
                    &[
                        ("calendar_id", cal.clone()),
                        ("webhook_url", s("HTTPS URL that receives the notifications")),
                        ("channel_id", s("Optional channel id; generated when empty")),
                    ],
                    &["calendar_id", "webhook_url"],
                    &[EVENTS],
                    true,
                ),
                t(
                    "GOOGLECALENDAR_STOP_WATCH",
                    ("Kanal beenden", "Stop watch"),
                    (
                        "Beendet einen zuvor angelegten Benachrichtigungskanal.",
                        "Stops a previously created notification channel.",
                    ),
                    &[
                        ("id", s("Channel id returned by Watch Events")),
                        ("resource_id", s("Resource id returned by Watch Events")),
                    ],
                    &["id", "resource_id"],
                    &[EVENTS],
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

    /// `calendars/{calendar}/events[/{event}[/instances|/move]]` and the
    /// collection sub-resources `quickAdd` / `import` / `watch`.
    fn events_url(&self, args: &Value, suffix: &str) -> Result<String> {
        let cal = calendar_or_primary(args);
        if !valid_calendar_id(cal) {
            bail!("invalid argument `calendar_id`");
        }
        let path = match suffix {
            "" => format!("/calendars/{cal}/events"),
            "quickAdd" | "import" | "watch" => format!("/calendars/{cal}/events/{suffix}"),
            other => {
                let id = path_segment(args, "event_id")?;
                match other {
                    "event" => format!("/calendars/{cal}/events/{id}"),
                    "instances" => format!("/calendars/{cal}/events/{id}/instances"),
                    "/move" => format!("/calendars/{cal}/events/{id}/move"),
                    _ => bail!("unknown event sub-resource `{other}`"),
                }
            }
        };
        Ok(self.url(&path))
    }

    fn free_busy(&self, args: &Value) -> Result<HttpRequest> {
        let ids = str_vec(args, "calendar_ids");
        if ids.is_empty() {
            bail!("missing argument `calendar_ids`");
        }
        Ok(HttpRequest::new("POST", self.url("/freeBusy")).json(&json!({
            "timeMin": norm_iso(args, "start_date")?,
            "timeMax": norm_iso(args, "end_date")?,
            "items": ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
        })))
    }
}

/// Merged busy intervals + the complement inside the window.
fn free_slots(busy: &[(i64, i64)], start: i64, end: i64) -> Vec<(i64, i64)> {
    let mut sorted = busy.to_vec();
    sorted.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (s, e) in sorted {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let mut free = Vec::new();
    let mut cursor = start;
    for (s, e) in merged {
        if cursor >= end {
            break;
        }
        if s > cursor {
            free.push((cursor, s.min(end)));
        }
        cursor = cursor.max(e);
    }
    if cursor < end {
        free.push((cursor, end));
    }
    free.retain(|(s, e)| e > s);
    free
}

impl ToolkitAdapter for GoogleCalendar {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "GOOGLECALENDAR_LIST_CALENDARS" => {
                let mut pairs = vec![("showDeleted", "false".to_string())];
                if let Some(role) = opt_str(args, "min_access_role") {
                    pairs.push(("minAccessRole", role.to_string()));
                }
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/users/me/calendarList{}", qs(&pairs))),
                )
            }
            "GOOGLECALENDAR_GET_CALENDAR" => HttpRequest::new(
                "GET",
                self.url(&format!("/calendars/{}", calendar_id(args, "calendar_id")?)),
            ),
            "GOOGLECALENDAR_LIST_EVENTS" => {
                let mut pairs = vec![("showDeleted", "false".to_string())];
                if let Some(b) = args.get("single_events").and_then(Value::as_bool) {
                    pairs.push(("singleEvents", b.to_string()));
                }
                if let Some(term) = opt_str(args, "search") {
                    pairs.push(("q", format!("\"{term}\"")));
                }
                if let Some(t) = opt_iso(args, "start_date")? {
                    pairs.push(("timeMin", t));
                    if let Some(t) = opt_iso(args, "end_date")? {
                        pairs.push(("timeMax", t));
                    }
                }
                pairs.push(("maxResults", opt_u64(args, "max_results", 250, 1, 2500).to_string()));
                for ty in str_vec(args, "event_types") {
                    if !EVENT_TYPES.contains(&ty.as_str()) {
                        bail!("invalid argument `event_types`");
                    }
                    pairs.push(("eventTypes", ty));
                }
                HttpRequest::new("GET", format!("{}{}", self.events_url(args, "")?, qs(&pairs)))
            }
            "GOOGLECALENDAR_GET_EVENT" => {
                // Both are optional; sending them unconditionally would clamp the
                // attendee list / change the calendar time zone.
                let mut pairs: Vec<(&str, String)> = Vec::new();
                if args.get("max_attendees").and_then(Value::as_u64).is_some() {
                    pairs.push((
                        "maxAttendees",
                        opt_u64(args, "max_attendees", 0, 1, 1000).to_string(),
                    ));
                }
                if let Some(tz) = opt_str(args, "time_zone") {
                    pairs.push(("timeZone", tz.to_string()));
                }
                HttpRequest::new("GET", format!("{}{}", self.events_url(args, "event")?, qs(&pairs)))
            }
            "GOOGLECALENDAR_SEARCH_EVENTS" => {
                let pairs = vec![
                    ("q", req_str(args, "query")?.to_string()),
                    ("timeMin", req_str(args, "start_date")?.to_string()),
                    ("timeMax", req_str(args, "end_date")?.to_string()),
                    (
                        "singleEvents",
                        args.get("single_events")
                            .and_then(Value::as_bool)
                            .unwrap_or(true)
                            .to_string(),
                    ),
                    ("showDeleted", "false".to_string()),
                    ("maxResults", opt_u64(args, "max_results", 250, 1, 250).to_string()),
                ];
                HttpRequest::new("GET", format!("{}{}", self.events_url(args, "")?, qs(&pairs)))
            }
            "GOOGLECALENDAR_LIST_RECURRING_EVENT_INSTANCES" => {
                let mut pairs = vec![
                    (
                        "timeMin",
                        opt_iso(args, "time_min")?.unwrap_or_default(),
                    ),
                    (
                        "timeMax",
                        opt_iso(args, "time_max")?.unwrap_or_default(),
                    ),
                    (
                        "showDeleted",
                        args.get("show_deleted")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                            .to_string(),
                    ),
                    ("maxResults", opt_u64(args, "max_results", 250, 1, 2500).to_string()),
                ];
                pairs.retain(|(_, v)| !v.is_empty());
                HttpRequest::new(
                    "GET",
                    format!("{}{}", self.events_url(args, "instances")?, qs(&pairs)),
                )
            }
            "GOOGLECALENDAR_FIND_BUSY_PERIODS" | "GOOGLECALENDAR_FIND_FREE_SLOTS" => {
                self.free_busy(args)?
            }
            "GOOGLECALENDAR_LIST_SETTINGS" => HttpRequest::new("GET", self.url("/users/me/settings")),
            "GOOGLECALENDAR_GET_COLORS" => HttpRequest::new("GET", self.url("/colors")),
            "GOOGLECALENDAR_CREATE_EVENT" => {
                let (start, end) = event_times(args)?;
                let mut body = Map::new();
                body.insert("summary".into(), json!(req_str(args, "title")?));
                body.insert("start".into(), json!({"dateTime": start}));
                body.insert("end".into(), json!({"dateTime": end}));
                body.insert("location".into(), json!(opt_str(args, "location").unwrap_or("")));
                body.insert(
                    "description".into(),
                    json!(opt_str(args, "description").unwrap_or("")),
                );
                body.insert("attendees".into(), attendees(str_vec(args, "attendees")));
                for (key, arg) in [
                    ("guestsCanModify", "guests_can_modify"),
                    ("guestsCanInviteOthers", "guests_can_invite_others"),
                    ("guestsCanSeeOtherGuests", "guests_can_see_other_guests"),
                ] {
                    if let Some(b) = args.get(arg).and_then(Value::as_bool) {
                        body.insert(key.into(), json!(b));
                    }
                }
                if let Some(color) = opt_str(args, "color_id") {
                    body.insert("colorId".into(), json!(color));
                }
                let meet = args
                    .get("create_meet_link")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if meet {
                    body.insert(
                        "conferenceData".into(),
                        json!({"createRequest": {
                            "conferenceSolutionKey": {"type": "hangoutsMeet"},
                            "requestId": random_id(),
                        }}),
                    );
                }
                let pairs = vec![
                    ("sendUpdates", send_updates(args, "send_notifications", "all")),
                    (
                        "conferenceDataVersion",
                        if meet { "1".to_string() } else { "0".to_string() },
                    ),
                ];
                HttpRequest::new(
                    "POST",
                    format!("{}{}", self.events_url(args, "")?, qs(&pairs)),
                )
                .json(&Value::Object(body))
            }
            "GOOGLECALENDAR_CREATE_QUICK_EVENT" => {
                let pairs = vec![
                    ("text", req_str(args, "text")?.to_string()),
                    ("sendUpdates", send_updates(args, "send_updates", "none")),
                ];
                HttpRequest::new(
                    "POST",
                    format!("{}{}", self.events_url(args, "quickAdd")?, qs(&pairs)),
                )
                .json(&json!({}))
            }
            "GOOGLECALENDAR_UPDATE_EVENT" => {
                let mut body = Map::new();
                if let Some(title) = opt_str(args, "title") {
                    body.insert("summary".into(), json!(title));
                }
                for (key, arg) in [("location", "location"), ("description", "description"), ("colorId", "color_id")] {
                    if let Some(v) = opt_str(args, arg) {
                        body.insert(key.into(), json!(v));
                    }
                }
                let emails = str_vec(args, "attendees");
                if !emails.is_empty() {
                    body.insert("attendees".into(), attendees(emails));
                }
                if opt_str(args, "start_date_time").is_some() || opt_str(args, "end_date_time").is_some() {
                    let (start, end) = event_times(args)?;
                    if opt_str(args, "start_date_time").is_some() {
                        body.insert("start".into(), json!({"dateTime": start}));
                    }
                    if opt_str(args, "end_date_time").is_some() {
                        body.insert("end".into(), json!({"dateTime": end}));
                    }
                }
                for (key, arg) in [
                    ("guestsCanModify", "guests_can_modify"),
                    ("guestsCanInviteOthers", "guests_can_invite_others"),
                    ("guestsCanSeeOtherGuests", "guests_can_see_other_guests"),
                ] {
                    if let Some(b) = args.get(arg).and_then(Value::as_bool) {
                        body.insert(key.into(), json!(b));
                    }
                }
                if body.is_empty() {
                    bail!("no field to update");
                }
                let pairs = vec![
                    ("sendUpdates", send_updates(args, "send_updates", "none")),
                    ("conferenceDataVersion", "1".to_string()),
                ];
                HttpRequest::new(
                    "PATCH",
                    format!("{}{}", self.events_url(args, "event")?, qs(&pairs)),
                )
                .json(&Value::Object(body))
            }
            "GOOGLECALENDAR_ADD_ATTENDEES" | "GOOGLECALENDAR_REMOVE_ATTENDEE" => {
                let emails = str_vec(args, "attendees");
                if emails.is_empty() {
                    bail!("missing argument `attendees`");
                }
                let pairs = vec![("sendUpdates", send_updates(args, "send_updates", "none"))];
                HttpRequest::new(
                    "PATCH",
                    format!("{}{}", self.events_url(args, "event")?, qs(&pairs)),
                )
                .json(&json!({"attendees": attendees(emails)}))
            }
            "GOOGLECALENDAR_DELETE_EVENT" => {
                let pairs = vec![("sendUpdates", send_updates(args, "send_updates", "none"))];
                HttpRequest::new(
                    "DELETE",
                    format!("{}{}", self.events_url(args, "event")?, qs(&pairs)),
                )
            }
            "GOOGLECALENDAR_MOVE_EVENT" => {
                let dest = args
                    .get("destination_calendar_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("missing argument `destination_calendar_id`"))?;
                if !valid_calendar_id(dest) {
                    bail!("invalid argument `destination_calendar_id`");
                }
                let pairs = vec![
                    ("destination", dest.to_string()),
                    ("sendUpdates", send_updates(args, "send_updates", "none")),
                ];
                HttpRequest::new(
                    "POST",
                    format!("{}{}", self.events_url(args, "/move")?, qs(&pairs)),
                )
            }
            "GOOGLECALENDAR_IMPORT_EVENT" => HttpRequest::new(
                "POST",
                self.events_url(args, "import")?,
            )
            .json(&{
                let (start, end) = event_times(args)?;
                let mut body = Map::new();
                body.insert("iCalUID".into(), json!(req_str(args, "ical_uid")?));
                body.insert("summary".into(), json!(req_str(args, "summary")?));
                body.insert("start".into(), json!({"dateTime": start}));
                body.insert("end".into(), json!({"dateTime": end}));
                for (key, arg) in [("location", "location"), ("description", "description")] {
                    if let Some(v) = opt_str(args, arg) {
                        body.insert(key.into(), json!(v));
                    }
                }
                Value::Object(body)
            }),
            "GOOGLECALENDAR_ADD_CALENDAR_TO_CALENDAR_LIST" => {
                HttpRequest::new("POST", self.url("/calendarList"))
                    .json(&json!({"id": req_str(args, "id")?}))
            }
            "GOOGLECALENDAR_WATCH_EVENTS" => {
                let url = req_str(args, "webhook_url")?;
                if !url.starts_with("https://") {
                    bail!("`webhook_url` must be an https URL");
                }
                let id = opt_str(args, "channel_id")
                    .map(String::from)
                    .unwrap_or_else(random_id);
                HttpRequest::new("POST", self.events_url(args, "watch")?).json(&json!({
                    "id": id,
                    "type": "web_hook",
                    "address": url,
                }))
            }
            "GOOGLECALENDAR_STOP_WATCH" => HttpRequest::new("POST", self.url("/channels/stop"))
                .json(&json!({
                    "id": req_str(args, "id")?,
                    "resourceId": req_str(args, "resource_id")?,
                })),
            other => bail!("unknown Google Calendar tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        let items = |key: &str| -> Vec<Value> {
            v[key].as_array().into_iter().flatten().cloned().collect()
        };
        Ok(match tool {
            "GOOGLECALENDAR_LIST_CALENDARS" => {
                let cals: Vec<Value> = items("items").iter().map(brief_calendar).collect();
                json!({"count": cals.len(), "calendars": cals})
            }
            "GOOGLECALENDAR_GET_CALENDAR" | "GOOGLECALENDAR_ADD_CALENDAR_TO_CALENDAR_LIST" => {
                brief_calendar(&v)
            }
            "GOOGLECALENDAR_LIST_EVENTS" | "GOOGLECALENDAR_SEARCH_EVENTS" => {
                let events: Vec<Value> = items("items").iter().map(brief_event).collect();
                json!({"count": events.len(), "events": events, "next_page_token": v["nextPageToken"]})
            }
            "GOOGLECALENDAR_GET_EVENT" => brief_event(&v),
            "GOOGLECALENDAR_LIST_RECURRING_EVENT_INSTANCES" => {
                let list: Vec<Value> = items("items").iter().map(brief_event).collect();
                json!({"count": list.len(), "instances": list, "next_page_token": v["nextPageToken"]})
            }
            "GOOGLECALENDAR_FIND_BUSY_PERIODS" => {
                let mut calendars = Map::new();
                let mut errors: Vec<Value> = Vec::new();
                for (id, entry) in v["calendars"].as_object().into_iter().flatten() {
                    if let Some(errs) = entry.get("errors").and_then(Value::as_array) {
                        for e in errs {
                            errors.push(json!({"calendar_id": id, "reason": e["reason"], "domain": e["domain"]}));
                        }
                    }
                    calendars.insert(id.clone(), json!({"busy": entry["busy"]}));
                }
                json!({"time_min": v["timeMin"], "time_max": v["timeMax"],
                    "calendars": Value::Object(calendars), "errors": errors})
            }
            "GOOGLECALENDAR_FIND_FREE_SLOTS" => {
                let start = iso_millis(v["timeMin"].as_str().unwrap_or_default())?;
                let end = iso_millis(v["timeMax"].as_str().unwrap_or_default())?;
                if end <= start {
                    bail!("`timeMax` must be after `timeMin`");
                }
                let mut busy: Vec<(i64, i64)> = Vec::new();
                for (id, entry) in v["calendars"].as_object().into_iter().flatten() {
                    if let Some(errs) = entry.get("errors").and_then(Value::as_array) {
                        let reasons: Vec<String> = errs
                            .iter()
                            .map(|e| {
                                format!(
                                    "{id} ({})",
                                    e["reason"].as_str().unwrap_or("unknown")
                                )
                            })
                            .collect();
                        bail!(
                            "could not read availability for: {}",
                            reasons.join("; ")
                        );
                    }
                    for block in entry["busy"].as_array().into_iter().flatten() {
                        busy.push((
                            iso_millis(block["start"].as_str().unwrap_or_default())?,
                            iso_millis(block["end"].as_str().unwrap_or_default())?,
                        ));
                    }
                }
                let slots: Vec<Value> = free_slots(&busy, start, end)
                    .into_iter()
                    .map(|(s, e)| {
                        json!({"start": millis_to_iso(s), "end": millis_to_iso(e),
                            "duration_minutes": (e - s) / 60_000})
                    })
                    .collect();
                json!({"count": slots.len(), "free_slots": slots})
            }
            "GOOGLECALENDAR_LIST_SETTINGS" => {
                let mut settings = Map::new();
                for item in items("items") {
                    if let (Some(k), Some(val)) = (item["id"].as_str(), item["value"].as_str()) {
                        settings.insert(k.to_string(), json!(val));
                    }
                }
                json!({"count": items("items").len(), "settings": Value::Object(settings)})
            }
            "GOOGLECALENDAR_GET_COLORS" => json!({"event": v["event"], "calendar": v["calendar"]}),
            "GOOGLECALENDAR_CREATE_QUICK_EVENT"
            | "GOOGLECALENDAR_CREATE_EVENT"
            | "GOOGLECALENDAR_UPDATE_EVENT"
            | "GOOGLECALENDAR_ADD_ATTENDEES"
            | "GOOGLECALENDAR_REMOVE_ATTENDEE"
            | "GOOGLECALENDAR_IMPORT_EVENT" => brief_event(&v),
            "GOOGLECALENDAR_MOVE_EVENT" => json!({"moved": true, "event": brief_event(&v)}),
            "GOOGLECALENDAR_DELETE_EVENT" => json!({"deleted": true}),
            "GOOGLECALENDAR_STOP_WATCH" => json!({"stopped": true}),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gc() -> GoogleCalendar {
        GoogleCalendar::with_api_base("https://gcal.test")
    }

    #[test]
    fn timestamps_round_trip() {
        let ms = iso_millis("2026-06-01T09:00:00.000Z").unwrap();
        assert_eq!(millis_to_iso(ms), "2026-06-01T09:00:00.000Z");
        assert_eq!(
            iso_millis("2026-06-01T11:00:00+02:00").unwrap(),
            iso_millis("2026-06-01T09:00:00Z").unwrap()
        );
        assert_eq!(millis_to_iso(iso_millis("2024-02-29T00:00:00Z").unwrap()), "2024-02-29T00:00:00.000Z");
        assert!(iso_millis("not-a-date").is_err());
    }

    #[test]
    fn requests_are_built_and_injection_is_rejected() {
        let g = gc();
        let r = g
            .build_request(
                "GOOGLECALENDAR_LIST_EVENTS",
                &json!({"calendar_id": "a@b.de", "search": "budget review",
                        "start_date": "2026-06-01T00:00:00Z", "end_date": "2026-06-30T00:00:00Z",
                        "single_events": true, "event_types": ["default"], "max_results": 999}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://gcal.test/calendars/a@b.de/events?showDeleted=false&singleEvents=true\
             &q=%22budget+review%22&timeMin=2026-06-01T00%3A00%3A00.000Z\
             &timeMax=2026-06-30T00%3A00%3A00.000Z&maxResults=999&eventTypes=default"
        );
        assert!(g
            .build_request("GOOGLECALENDAR_GET_EVENT", &json!({"calendar_id": "primary", "event_id": "../x"}))
            .is_err());
        assert!(g
            .build_request("GOOGLECALENDAR_GET_CALENDAR", &json!({"calendar_id": "a/b"}))
            .is_err());
        assert!(g
            .build_request("GOOGLECALENDAR_MOVE_EVENT",
                &json!({"calendar_id": "primary", "event_id": "e1", "destination_calendar_id": "x?y=1"}))
            .is_err());
        assert!(g
            .build_request("GOOGLECALENDAR_UPDATE_EVENT",
                &json!({"calendar_id": "primary", "event_id": "e1"}))
            .is_err());
        assert!(g
            .build_request("GOOGLECALENDAR_WATCH_EVENTS",
                &json!({"calendar_id": "primary", "webhook_url": "http://x"}))
            .is_err());
    }

    #[test]
    fn create_event_defaults_end_to_plus_30_minutes() {
        let g = gc();
        let r = g
            .build_request(
                "GOOGLECALENDAR_CREATE_EVENT",
                &json!({"calendar_id": "primary", "title": "T", "start_date_time": "2026-06-01T09:00:00Z"}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["end"]["dateTime"], "2026-06-01T09:30:00.000Z");
        assert!(r.url.contains("sendUpdates=all"));
        assert!(r.url.contains("conferenceDataVersion=0"));
    }

    #[test]
    fn free_slots_are_the_complement_of_busy() {
        let g = gc();
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"kind": "calendar#freeBusy",
                "timeMin": "2026-06-01T09:00:00Z", "timeMax": "2026-06-01T12:00:00Z",
                "calendars": {"primary": {"busy": [
                    {"start": "2026-06-01T09:30:00Z", "end": "2026-06-01T10:00:00Z"},
                    {"start": "2026-06-01T09:45:00Z", "end": "2026-06-01T10:30:00Z"}]}}})).unwrap(),
            session_id: None,
        };
        let out = g.parse_response("GOOGLECALENDAR_FIND_FREE_SLOTS", &resp).unwrap();
        assert_eq!(out["count"], 2);
        assert_eq!(out["free_slots"][0]["end"], "2026-06-01T09:30:00.000Z");
        assert_eq!(out["free_slots"][1]["start"], "2026-06-01T10:30:00.000Z");
    }

    #[test]
    fn catalog_is_complete() {
        let g = gc();
        for t in &g.spec().tools {
            assert!(!t.title.de.is_empty() && !t.title.en.is_empty(), "{}", t.slug);
            assert!(!t.description.de.is_empty() && !t.description.en.is_empty(), "{}", t.slug);
            assert!(!t.scopes.is_empty(), "{}", t.slug);
        }
    }
}