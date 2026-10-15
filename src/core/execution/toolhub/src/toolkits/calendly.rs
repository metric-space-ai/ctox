//! Calendly toolkit (REST v2, `https://api.calendly.com`). Action set modelled on
//! Activepieces `piece-calendly` (MIT).
//!
//! Auth: a Calendly Personal Access Token sent as `Authorization: Bearer <token>`,
//! exactly the `AuthenticationType.BEARER_TOKEN` the piece uses.
//!
//! Deliberate deviations from the TypeScript, forced by the pure `build_request`
//! model (no credentials, no second round-trip):
//! - The TS `run()` resolves "leave empty for the connected user/organization"
//!   through an extra `GET /users/me`. Here `user`, `organization`, `owner` and
//!   `host` are ordinary inputs; `CALENDLY_GET_USER` without arguments returns the
//!   connected user's `uri` and `current_organization` to fill them.
//! - `validateTimeWindow` moves a past Start Time to `now + 60s`. That needs a
//!   clock, so a past start time is passed through and rejected by Calendly
//!   instead; the remaining checks (parseable, end after start, max span) are kept.
//! - Webhook triggers (`src/lib/trigger/…`) and `createCustomApiCallAction` are
//!   not ported.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "calendly";

// Scopes a Personal Access Token needs (the list from `src/lib/auth.ts`).
const USERS_R: &str = "users:read";
const ORG_R: &str = "organizations:read";
const EVENT_TYPES_R: &str = "event_types:read";
const EVENT_TYPES_W: &str = "event_types:write";
const AVAIL_R: &str = "availability:read";
const AVAIL_W: &str = "availability:write";
const EVENTS_R: &str = "scheduled_events:read";
const EVENTS_W: &str = "scheduled_events:write";
const LINKS_W: &str = "scheduling_links:write";
const SHARES_W: &str = "shares:write";
const LOCATIONS_R: &str = "locations:read";

/// `LOCATION_KINDS` of `src/lib/common/index.ts`.
const LOCATION_KINDS: &[&str] = &[
    "zoom_conference",
    "google_conference",
    "microsoft_teams_conference",
    "webex_conference",
    "gotomeeting_conference",
    "physical",
    "outbound_call",
    "inbound_call",
    "custom",
    "ask_invitee",
];

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn number(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn objects(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "object"}, "description": desc})
}

/// `calendlyCommon.count`: page size 1..=100, default 20.
fn count_prop() -> Value {
    json!({"type": "integer",
        "description": "How many items to return in this page, 1 to 100. Default 20.",
        "minimum": 1, "maximum": 100, "default": 20})
}

fn page_token() -> Value {
    s("Next Page Token from a previous call. Leave empty for the first page.")
}

/// Pagination props shared by the paginated list actions.
fn page_props() -> Vec<(&'static str, Value)> {
    vec![("count", count_prop()), ("page_token", page_token())]
}

fn with_page(extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = page_props();
    v.extend(extra);
    v
}

/// `calendlyCommon.user` — required here (the TS default `GET /users/me` is not
/// available without credentials); the connected user's URI comes from
/// `CALENDLY_GET_USER` without arguments.
fn user_prop() -> Value {
    s("Nutzer-URI (https://api.calendly.com/users/…) oder UUID. Die URI des verbundenen Accounts liefert CALENDLY_GET_USER ohne Argumente.")
}

fn org_prop() -> Value {
    s("Organisations-URI (https://api.calendly.com/organizations/…) oder UUID. Die URI des verbundenen Accounts liefert CALENDLY_GET_USER (Feld current_organization).")
}

fn event_type_prop() -> Value {
    s("Event-Typ-URI (https://api.calendly.com/event_types/…) oder UUID, z. B. aus CALENDLY_LIST_EVENT_TYPES.")
}

fn event_prop() -> Value {
    s("Termin-URI (https://api.calendly.com/scheduled_events/…) oder UUID, z. B. aus CALENDLY_LIST_SCHEDULED_EVENTS.")
}

fn datetime(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "format": "date-time"})
}

fn iso_desc(extra: &str) -> String {
    format!("ISO 8601 mit Zeitzone, z. B. 2026-10-20T09:00:00Z{extra}")
}

// ---------------------------------------------------------------- id helpers

fn non_blank<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// Same guard as `path_segment`, for the tail of a pasted URI.
fn segment(v: &str, key: &str) -> Result<String> {
    if v.is_empty()
        || v.len() > 200
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || v == "."
        || v == ".."
    {
        bail!("invalid argument `{key}`");
    }
    Ok(v.to_string())
}

/// `calendlyCommon.toUuid`: the last path segment of a URI, or the value itself.
fn uuid_ref(args: &Value, key: &str) -> Result<String> {
    let raw = req_str(args, key)?.trim();
    if raw.contains('/') {
        segment(raw.trim_end_matches('/').rsplit('/').next().unwrap_or(""), key)
    } else {
        Ok(path_segment(args, key)?.to_string())
    }
}

// ---------------------------------------------------------------- request helpers

impl Calendly {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    /// `calendlyCommon.toUri`: a full `https://api.calendly.com` URI passes through,
    /// a bare id is expanded to `<api_base>/<resource>/<id>`.
    fn to_uri(&self, resource: &str, args: &Value, key: &str) -> Result<String> {
        let raw = req_str(args, key)?.trim();
        match raw.strip_prefix("https://") {
            Some(rest) if rest.split('/').next() == Some("api.calendly.com") => {
                Ok(raw.to_string())
            }
            Some(_) => bail!("`{key}` must be a Calendly URI or a bare id, got \"{raw}\""),
            None => Ok(format!("{}/{}/{}", self.api_base, resource, uuid_ref(args, key)?)),
        }
    }

    /// `calendlyCommon.pageQuery`: `count` always, `page_token` only when set.
    fn page_query(args: &Value) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("count", Some(opt_u64(args, "count", 20, 1, 100).to_string())),
            (
                "page_token",
                opt_str(args, "page_token")
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(String::from),
            ),
        ]
    }

    /// `organization` wins over `user` (as in the TS); exactly one is required.
    fn owner_query(&self, args: &Value) -> Result<(&'static str, Option<String>)> {
        if non_blank(args, "organization").is_some() {
            return Ok((
                "organization",
                Some(self.to_uri("organizations", args, "organization")?),
            ));
        }
        if non_blank(args, "user").is_some() {
            return Ok(("user", Some(self.to_uri("users", args, "user")?)));
        }
        bail!(
            "pass `organization`, or `user` — the connected user's URI comes from \
CALENDLY_GET_USER without arguments"
        )
    }

    /// `calendlyCommon.validateTimeWindow` without the clock: end after start and
    /// at most `max_days` apart; the "past start moves to now" clamp is skipped.
    fn time_window(args: &Value, max_days: i64) -> Result<(String, String)> {
        let invalid = || anyhow!("Start Time and End Time must be valid ISO 8601 date-times.");
        let start = req_str(args, "start_time")?.trim();
        let end = req_str(args, "end_time")?.trim();
        let s = parse_iso(start).ok_or_else(invalid)?;
        let e = parse_iso(end).ok_or_else(invalid)?;
        if e <= s {
            bail!("End Time must be after Start Time and in the future.");
        }
        if e - s > max_days * 86_400 {
            bail!("The range between Start Time and End Time can be at most {max_days} days.");
        }
        Ok((start.to_string(), end.to_string()))
    }
}

/// `calendlyCommon.eventTypeBody`: duration, description, color, active, locations,
/// each only when set (`active` becomes a boolean, blank strings are dropped).
fn event_type_body(body: &mut Map<String, Value>, args: &Value) -> Result<()> {
    put_number(body, args, "duration")?;
    if let Some(v) = opt_str(args, "description") {
        body.insert("description".into(), json!(v));
    }
    if let Some(c) = opt_str(args, "color").map(str::trim).filter(|v| !v.is_empty()) {
        body.insert("color".into(), json!(c));
    }
    match opt_str(args, "active") {
        Some("true") => {
            body.insert("active".into(), json!(true));
        }
        Some("false") => {
            body.insert("active".into(), json!(false));
        }
        Some(_) => bail!("`active` must be \"true\" or \"false\""),
        None => {}
    }
    if let Some(locations) = args.get("locations").and_then(Value::as_array) {
        let clean: Vec<Value> = locations
            .iter()
            .filter_map(Value::as_object)
            .map(|m| {
                let mut out = Map::new();
                for (k, v) in m {
                    if let Some(sv) = v.as_str() {
                        let sv = sv.trim();
                        if !sv.is_empty() {
                            out.insert(k.clone(), json!(sv));
                        }
                    }
                }
                Value::Object(out)
            })
            .collect();
        if !clean.is_empty() {
            body.insert("locations".into(), Value::Array(clean));
        }
    }
    Ok(())
}

/// Optional JSON number -> body field; `false` when the argument is absent.
fn put_number(body: &mut Map<String, Value>, args: &Value, key: &str) -> Result<bool> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(v) if v.is_number() => {
            body.insert(key.into(), v.clone());
            Ok(true)
        }
        Some(_) => bail!("`{key}` must be a number"),
    }
}

// ---------------------------------------------------------------- date parsing

/// Epoch seconds for `YYYY-MM-DD[ T]HH:MM[:SS][.fff][Z|±HH[:MM]]`; `None` if the
/// string is not in this shape (Calendly would reject it as well).
fn parse_iso(v: &str) -> Option<i64> {
    let (date, rest) = v.trim().split_once(['T', 't', ' ']).unwrap_or((v, ""));
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (mut h, mut mi, mut s) = (0i64, 0i64, 0i64);
    let mut offset = 0i64;
    if !rest.is_empty() {
        let (time, zone) = match rest.find(['+', '-', 'Z', 'z']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let mut tp = time.split(':');
        h = tp.next()?.parse().ok()?;
        mi = tp.next()?.parse().ok()?;
        let secs = tp.next().unwrap_or("0");
        let secs = secs.split('.').next().unwrap_or(secs);
        s = secs.parse().ok()?;
        if tp.next().is_some() || h > 23 || mi > 59 || s > 59 {
            return None;
        }
        if !zone.is_empty() {
            let sign: i64 = match zone.as_bytes()[0] {
                b'Z' | b'z' => 0,
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if sign != 0 {
                let (zh, zm): (i64, i64) = match zone[1..].split_once(':') {
                    Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
                    None => (zone[1..].parse().ok()?, 0),
                };
                offset = sign * (zh * 60 + zm);
            }
        }
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s - offset * 60)
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ---------------------------------------------------------------- response helpers

/// `toCalendlyError`: scope hints on 403, `{title, message, details}` otherwise.
fn check(response: &HttpResponse) -> Result<Value> {
    let v = response.json().unwrap_or(Value::Null);
    if response.status >= 400 {
        if response.status == 403 {
            let scopes: Vec<&str> = v["required_scopes"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !scopes.is_empty() {
                bail!(
                    "Your Calendly Personal Access Token is missing the scope(s): {}. \
Create a new token with these scopes and reconnect.",
                    scopes.join(", ")
                );
            }
        }
        let title = v["title"].as_str().unwrap_or("Calendly error");
        let message = v["message"].as_str().unwrap_or("");
        let details = v["details"]
            .as_array()
            .map(|d| format!(" {}", serde_json::to_string(d).unwrap_or_default()))
            .unwrap_or_default();
        bail!("{title} ({}): {message}{details}", response.status);
    }
    Ok(v)
}

/// `{resource: …}` of the single-record endpoints.
fn resource(response: &HttpResponse) -> Result<Value> {
    let v = check(response)?;
    match v.get("resource") {
        Some(r) if r.is_object() => Ok(r.clone()),
        _ => bail!("Calendly returned no `resource`"),
    }
}

/// `CalendlyCollection`: the rows plus `pagination.next_page_token`.
fn collection(response: &HttpResponse) -> Result<(Vec<Value>, Option<String>)> {
    let v = check(response)?;
    let items = v["collection"].as_array().cloned().unwrap_or_default();
    let next = v["pagination"]["next_page_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(String::from);
    Ok((items, next))
}

/// `calendlyCommon.toPage` shape.
fn list_page(response: &HttpResponse) -> Result<Value> {
    let (items, next) = collection(response)?;
    Ok(json!({
        "items": items,
        "count": items.len(),
        "next_page_token": next,
    }))
}

/// Shape of the non-paginated collections (`{items, count}`).
fn items_only(response: &HttpResponse) -> Result<Value> {
    let (items, _) = collection(response)?;
    Ok(json!({"items": items, "count": items.len()}))
}

// ---------------------------------------------------------------- adapter

pub struct Calendly {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Calendly {
    fn default() -> Self {
        Self::with_api_base("https://api.calendly.com")
    }
}

impl Calendly {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Calendly".into(),
            description: LocalizedText::new(
                "Calendly-Terminplanung: Nutzer, Organisationen, Event-Typen, Termine, \
Teilnehmer und Verfügbarkeiten lesen; Event-Typen, Buchungslinks und Verfügbarkeiten ändern.",
                "Calendly scheduling: read users, organizations, event types, scheduled \
events, invitees and availability; change event types, booking links and availability.",
            ),
            auth_schemes: vec![AuthScheme::Bearer],
            tools: vec![
                t(
                    "CALENDLY_GET_USER",
                    ("Nutzer lesen", "Get user"),
                    (
                        "Liest einen Calendly-Nutzer; ohne Argumente den verbundenen Account.",
                        "Reads a Calendly user; with no arguments the connected account.",
                    ),
                    &[("user", s("Nutzer-URI oder UUID; leer lassen für den verbundenen Account."))],
                    &[],
                    &[USERS_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_ORGANIZATION",
                    ("Organisation lesen", "Get organization"),
                    (
                        "Liest eine Organisation samt Plan und Phase.",
                        "Reads one organization including plan and stage.",
                    ),
                    &[("organization", org_prop())],
                    &["organization"],
                    &[ORG_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_ORGANIZATION_MEMBERSHIP",
                    ("Mitgliedschaft lesen", "Get organization membership"),
                    (
                        "Liest eine Organisations-Mitgliedschaft mit Rolle und Profil.",
                        "Reads one organization membership with role and profile.",
                    ),
                    &[("membership", s("Mitgliedschafts-URI oder UUID."))],
                    &["membership"],
                    &[ORG_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_ORGANIZATION_MEMBERS",
                    ("Organisations-Mitglieder auflisten", "List organization members"),
                    (
                        "Listet die Mitglieder einer Organisation mit Rolle und Profil.",
                        "Lists the members of an organization with role and profile.",
                    ),
                    &with_page(vec![
                        ("organization", org_prop()),
                        ("email", s("Nur das Mitglied mit dieser E-Mail.")),
                        (
                            "role",
                            choice("Nur Mitglieder mit dieser Rolle", &["owner", "admin", "user"]),
                        ),
                    ]),
                    &["organization"],
                    &[ORG_R],
                    false,
                ),
                t(
                    "CALENDLY_FIND_USER",
                    ("Nutzer suchen", "Find user"),
                    (
                        "Findet ein Organisations-Mitglied per exakter E-Mail.",
                        "Finds an organization member by exact email.",
                    ),
                    &[
                        ("organization", org_prop()),
                        ("email", s("Exakte E-Mail des Mitglieds.")),
                    ],
                    &["organization", "email"],
                    &[ORG_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_EVENT_TYPE",
                    ("Event-Typ lesen", "Get event type"),
                    (
                        "Liest einen Event-Typ mit Dauer, Standorten und Buchungs-URL.",
                        "Reads one event type with duration, locations and scheduling URL.",
                    ),
                    &[("event_type", event_type_prop())],
                    &["event_type"],
                    &[EVENT_TYPES_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_EVENT_TYPES",
                    ("Event-Typen auflisten", "List event types"),
                    (
                        "Listet die Event-Typen eines Nutzers oder einer Organisation.",
                        "Lists the event types of one user or organization.",
                    ),
                    &with_page(vec![
                        ("organization", s("Organisations-URI oder UUID. Setzt man sie, werden alle Event-Typen der Organisation geliefert statt der eines Nutzers (nur Admin). Leer: `user` nötig.")),
                        ("user", user_prop()),
                        (
                            "active",
                            choice("Nur aktive (true) oder inaktive (false) Event-Typen", &["true", "false"]),
                        ),
                    ]),
                    &[],
                    &[EVENT_TYPES_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_EVENT_TYPE_HOSTS",
                    ("Event-Typ-Hosts auflisten", "List event type hosts"),
                    (
                        "Listet die Hosts eines Event-Typs.",
                        "Lists the hosts of an event type.",
                    ),
                    &with_page(vec![("event_type", event_type_prop())]),
                    &["event_type"],
                    &[EVENT_TYPES_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_EVENT_TYPE_AVAILABILITY",
                    ("Event-Typ-Verfügbarkeit lesen", "Get event type availability"),
                    (
                        "Liest den Verfügbarkeitsplan eines Event-Typs.",
                        "Reads the availability schedule of an event type.",
                    ),
                    &[("event_type", event_type_prop())],
                    &["event_type"],
                    &[AVAIL_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_SCHEDULED_EVENT",
                    ("Termin lesen", "Get scheduled event"),
                    (
                        "Liest einen gebuchten Termin per URI oder UUID.",
                        "Reads one booked event by URI or UUID.",
                    ),
                    &[("scheduled_event", event_prop())],
                    &["scheduled_event"],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_SCHEDULED_EVENTS",
                    ("Termine auflisten", "List scheduled events"),
                    (
                        "Listet gebuchte Termine, filterbar nach Teilnehmer, Status und Zeitraum.",
                        "Lists booked events, filterable by invitee, status and time range.",
                    ),
                    &with_page(vec![
                        ("organization", s("Organisations-URI oder UUID. Setzt man sie, werden die Termine der ganzen Organisation geliefert statt der eines Nutzers (nur Admin). Leer: `user` nötig.")),
                        ("user", user_prop()),
                        ("invitee_email", s("Nur Termine mit diesem Teilnehmer-Eintrag (E-Mail).")),
                        ("status", choice("Nur Termine mit diesem Status", &["active", "canceled"])),
                        ("min_start_time", datetime(&iso_desc(" (Starts After)."))),
                        ("max_start_time", datetime(&iso_desc(" (Starts Before)."))),
                        (
                            "sort",
                            choice("Sortierung", &["start_time:asc", "start_time:desc"]),
                        ),
                    ]),
                    &[],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_FIND_INVITEE_BOOKINGS",
                    ("Termine eines Teilnehmers suchen", "Find invitee bookings"),
                    (
                        "Findet die Termine, die ein Teilnehmer per E-Mail gebucht hat.",
                        "Finds the events an invitee booked, by email.",
                    ),
                    &[
                        ("user", user_prop()),
                        ("invitee_email", s("E-Mail des Teilnehmers.")),
                        ("status", choice("Nur Termine mit diesem Status", &["active", "canceled"])),
                    ],
                    &["user", "invitee_email"],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_EVENT_INVITEES",
                    ("Termin-Teilnehmer auflisten", "List event invitees"),
                    (
                        "Listet die Teilnehmer eines gebuchten Termins.",
                        "Lists the invitees of one booked event.",
                    ),
                    &with_page(vec![
                        ("scheduled_event", event_prop()),
                        ("email", s("Nur den Teilnehmer mit dieser E-Mail.")),
                        ("status", choice("Nur Teilnehmer mit diesem Status", &["active", "canceled"])),
                    ]),
                    &["scheduled_event"],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_EVENT_INVITEE",
                    ("Termin-Teilnehmer lesen", "Get event invitee"),
                    (
                        "Liest einen Teilnehmer eines gebuchten Termins.",
                        "Reads one invitee of a booked event.",
                    ),
                    &[
                        ("scheduled_event", s("Termin-URI oder UUID; nötig, wenn `invitee` nur eine UUID ist.")),
                        ("invitee", s("Teilnehmer-URI (…/scheduled_events/…/invitees/…) oder UUID; bei einer URI wird nur der letzte Abschnitt verwendet.")),
                    ],
                    &["scheduled_event", "invitee"],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_USER_AVAILABILITY_SCHEDULES",
                    ("Verfügbarkeitspläne auflisten", "List user availability schedules"),
                    (
                        "Listet die Verfügbarkeitspläne (Arbeitszeiten) eines Nutzers.",
                        "Lists a user's availability schedules (working hours).",
                    ),
                    &[("user", user_prop())],
                    &["user"],
                    &[AVAIL_R],
                    false,
                ),
                t(
                    "CALENDLY_GET_USER_AVAILABILITY_SCHEDULE",
                    ("Verfügbarkeitsplan lesen", "Get user availability schedule"),
                    (
                        "Liest einen Verfügbarkeitsplan mit Zeitzone und Regeln.",
                        "Reads one availability schedule with timezone and rules.",
                    ),
                    &[("schedule", s("Verfügbarkeitsplan-URI oder UUID, z. B. aus CALENDLY_LIST_USER_AVAILABILITY_SCHEDULES."))],
                    &["schedule"],
                    &[AVAIL_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_USER_BUSY_TIMES",
                    ("Belegte Zeiten auflisten", "List user busy times"),
                    (
                        "Listet belegte Zeitblöcke eines Nutzers (Calendly plus Kalender).",
                        "Lists a user's busy time blocks (Calendly plus calendars).",
                    ),
                    &[
                        ("user", user_prop()),
                        ("start_time", datetime(&iso_desc(" des Fensters."))),
                        ("end_time", datetime(&iso_desc(" des Fensters; höchstens 7 Tage nach Start Time."))),
                    ],
                    &["user", "start_time", "end_time"],
                    &[EVENTS_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_AVAILABLE_TIMES",
                    ("Buchbare Zeiten auflisten", "List available times"),
                    (
                        "Listet die freien Buchungsslots eines Event-Typs.",
                        "Lists the open booking slots of an event type.",
                    ),
                    &[
                        ("event_type", event_type_prop()),
                        ("start_time", datetime(&iso_desc(" des Fensters."))),
                        ("end_time", datetime(&iso_desc(" des Fensters; höchstens 31 Tage nach Start Time."))),
                    ],
                    &["event_type", "start_time", "end_time"],
                    &[EVENT_TYPES_R],
                    false,
                ),
                t(
                    "CALENDLY_LIST_USER_LOCATIONS",
                    ("Meeting-Standorte auflisten", "List user meeting locations"),
                    (
                        "Listet die eingerichteten Meeting-Standortarten eines Nutzers.",
                        "Lists the meeting location kinds a user has set up.",
                    ),
                    &[("user", user_prop())],
                    &["user"],
                    &[LOCATIONS_R],
                    false,
                ),
                t(
                    "CALENDLY_CANCEL_EVENT",
                    ("Termin absagen", "Cancel event"),
                    (
                        "Sagt einen gebuchten Termin ab und informiert die Teilnehmer; braucht --confirm.",
                        "Cancels a booked event and notifies the invitees; requires --confirm.",
                    ),
                    &[
                        ("scheduled_event", event_prop()),
                        ("reason", s("Grund, den die Teilnehmer sehen; bis 10.000 Zeichen.")),
                    ],
                    &["scheduled_event"],
                    &[EVENTS_W],
                    true,
                ),
                t(
                    "CALENDLY_CREATE_EVENT_TYPE",
                    ("Event-Typ anlegen", "Create event type"),
                    (
                        "Legt einen 1:1-Event-Typ an; braucht --confirm.",
                        "Creates a one-on-one event type; requires --confirm.",
                    ),
                    &[
                        ("owner", s("Nutzer-URI oder UUID des Hosts; die URI des verbundenen Accounts liefert CALENDLY_GET_USER.")),
                        ("name", s("Name des Event-Typs.")),
                        ("duration", number("Dauer in Minuten.")),
                        ("description", s("Beschreibung.")),
                        ("color", s("Hex-Farbe, z. B. #ff5733.")),
                        ("active", choice("Event-Typ aktivieren", &["true", "false"])),
                        (
                            "locations",
                            objects("Ersetzt die Standortliste. Objekte mit kind (zoom_conference, google_conference, microsoft_teams_conference, webex_conference, gotomeeting_conference, physical, outbound_call, inbound_call, custom, ask_invitee) plus location, phone_number und additional_info."),
                        ),
                    ],
                    &["owner", "name"],
                    &[EVENT_TYPES_W],
                    true,
                ),
                t(
                    "CALENDLY_UPDATE_EVENT_TYPE",
                    ("Event-Typ ändern", "Update event type"),
                    (
                        "Ändert nur die übergebenen Felder eines 1:1-Event-Typs; braucht --confirm.",
                        "Changes only the fields you pass on a one-on-one event type; requires --confirm.",
                    ),
                    &[
                        ("event_type", event_type_prop()),
                        ("name", s("Neuer Name.")),
                        ("duration", number("Neue Dauer in Minuten.")),
                        ("description", s("Neue Beschreibung.")),
                        ("color", s("Neue Hex-Farbe, z. B. #ff5733.")),
                        ("active", choice("Event-Typ aktivieren oder deaktivieren", &["true", "false"])),
                        (
                            "locations",
                            objects("Ersetzt die komplette Standortliste. Objekte mit kind plus location, phone_number und additional_info."),
                        ),
                    ],
                    &["event_type"],
                    &[EVENT_TYPES_W],
                    true,
                ),
                t(
                    "CALENDLY_UPDATE_EVENT_TYPE_AVAILABILITY",
                    ("Event-Typ-Verfügbarkeit setzen", "Update event type availability"),
                    (
                        "Ersetzt den Verfügbarkeitsplan eines Event-Typs komplett; braucht --confirm.",
                        "Replaces the availability schedule of an event type; requires --confirm.",
                    ),
                    &[
                        ("event_type", event_type_prop()),
                        ("timezone", s("IANA-Zeitzone, z. B. Europe/Berlin.")),
                        (
                            "rules",
                            objects("Komplette Regelliste, ersetzt die bisherigen. Regel: {\"type\":\"wday\",\"wday\":\"monday\",\"intervals\":[{\"from\":\"09:00\",\"to\":\"17:00\"}]} oder {\"type\":\"date\",\"date\":\"2026-12-24\",\"intervals\":[]} (leere Intervalle = nicht verfügbar). Vorher CALENDLY_GET_EVENT_TYPE_AVAILABILITY lesen."),
                        ),
                    ],
                    &["event_type", "timezone", "rules"],
                    &[AVAIL_W],
                    true,
                ),
                t(
                    "CALENDLY_CREATE_SCHEDULING_LINK",
                    ("Einmal-Buchungslink anlegen", "Create single-use scheduling link"),
                    (
                        "Legt einen Buchungslink an, der nach einer Buchung verfällt; braucht --confirm.",
                        "Creates a booking link that expires after one booking; requires --confirm.",
                    ),
                    &[("event_type", event_type_prop())],
                    &["event_type"],
                    &[LINKS_W],
                    true,
                ),
                t(
                    "CALENDLY_CREATE_ONE_OFF_EVENT",
                    ("Einmal-Termin anlegen", "Create one-off meeting link"),
                    (
                        "Legt einen einmal buchbaren Termin an und liefert den Buchungslink; braucht --confirm.",
                        "Creates a one-off meeting and returns its booking link; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des einmaligen Termins.")),
                        ("host", s("Nutzer-URI oder UUID des Hosts; die URI des verbundenen Accounts liefert CALENDLY_GET_USER.")),
                        ("duration", number("Dauer in Minuten.")),
                        ("start_date", s("Erstes buchbares Datum, YYYY-MM-DD.")),
                        ("end_date", s("Letztes buchbares Datum, YYYY-MM-DD.")),
                        ("timezone", s("IANA-Zeitzone wie America/New_York; leer = Zeitzone des Hosts.")),
                        (
                            "location_kind",
                            choice("Art des Meeting-Standorts", LOCATION_KINDS),
                        ),
                        ("location_details", s("Adresse bei physisch, Text bei benutzerdefiniert, Telefonnummer bei eingehenden Anrufen.")),
                    ],
                    &["name", "host", "duration", "start_date", "end_date"],
                    &[EVENT_TYPES_W],
                    true,
                ),
                t(
                    "CALENDLY_CREATE_SHARE",
                    ("Angepassten Share-Link anlegen", "Create customized share link"),
                    (
                        "Legt einen Buchungslink an, der Einstellungen eines 1:1-Event-Typs überschreibt; braucht --confirm.",
                        "Creates a booking link overriding settings of a one-on-one event type; requires --confirm.",
                    ),
                    &[
                        ("event_type", event_type_prop()),
                        ("name", s("Name, der nur für diesen Link gilt.")),
                        ("duration", number("Abweichende Dauer in Minuten.")),
                        ("start_date", s("Erstes buchbares Datum, YYYY-MM-DD; nur zusammen mit End Date.")),
                        ("end_date", s("Letztes buchbares Datum, YYYY-MM-DD; nur zusammen mit Start Date.")),
                        ("hide_location", flag("Standort erst nach der Buchung zeigen.")),
                    ],
                    &["event_type"],
                    &[SHARES_W],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }
}

impl ToolkitAdapter for Calendly {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/get-user.ts
            // path: `/users/me` or `/users/${toUuid(user)}`
            "CALENDLY_GET_USER" => match opt_str(args, "user") {
                Some(_) => HttpRequest::new(
                    "GET",
                    self.url(&format!("/users/{}", uuid_ref(args, "user")?)),
                ),
                None => HttpRequest::new("GET", self.url("/users/me")),
            },

            // AP: src/lib/actions/get-organization.ts
            // path: `/organizations/${toUuid(organization)}`
            "CALENDLY_GET_ORGANIZATION" => HttpRequest::new(
                "GET",
                self.url(&format!("/organizations/{}", uuid_ref(args, "organization")?)),
            ),

            // AP: src/lib/actions/get-organization-membership.ts
            // path: `/organization_memberships/${toUuid(membership)}`
            "CALENDLY_GET_ORGANIZATION_MEMBERSHIP" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/organization_memberships/{}",
                    uuid_ref(args, "membership")?
                )),
            ),

            // AP: src/lib/actions/list-organization-memberships.ts
            // path: /organization_memberships, query: organization, email, role, count, page_token
            "CALENDLY_LIST_ORGANIZATION_MEMBERS" => {
                let mut pairs: Vec<(&str, Option<String>)> = vec![
                    (
                        "organization",
                        Some(self.to_uri("organizations", args, "organization")?),
                    ),
                    ("email", opt_str(args, "email").map(|v| v.trim().to_string())),
                    ("role", opt_str(args, "role").map(String::from)),
                ];
                pairs.extend(Self::page_query(args));
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/organization_memberships{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/find-user.ts
            // path: /organization_memberships, query: organization, email, count=1
            "CALENDLY_FIND_USER" => {
                let pairs = [
                    (
                        "organization",
                        Some(self.to_uri("organizations", args, "organization")?),
                    ),
                    ("email", Some(req_str(args, "email")?.trim().to_string())),
                    ("count", Some("1".to_string())),
                ];
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/organization_memberships{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/get-event-type.ts
            // path: `/event_types/${toUuid(eventType)}`
            "CALENDLY_GET_EVENT_TYPE" => HttpRequest::new(
                "GET",
                self.url(&format!("/event_types/{}", uuid_ref(args, "event_type")?)),
            ),

            // AP: src/lib/actions/list-event-types.ts
            // path: /event_types, query: organization|user, active, count, page_token
            "CALENDLY_LIST_EVENT_TYPES" => {
                let owner = self.owner_query(args)?;
                let mut pairs: Vec<(&str, Option<String>)> =
                    vec![owner, ("active", opt_str(args, "active").map(String::from))];
                pairs.extend(Self::page_query(args));
                HttpRequest::new("GET", self.url(&format!("/event_types{}", query(&pairs))))
            }

            // AP: src/lib/actions/list-event-type-hosts.ts
            // path: /event_type_memberships, query: event_type, count, page_token
            "CALENDLY_LIST_EVENT_TYPE_HOSTS" => {
                let mut pairs: Vec<(&str, Option<String>)> = vec![(
                    "event_type",
                    Some(self.to_uri("event_types", args, "event_type")?),
                )];
                pairs.extend(Self::page_query(args));
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/event_type_memberships{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/get-event-type-availability.ts
            // path: /event_type_availability_schedules, query: event_type
            "CALENDLY_GET_EVENT_TYPE_AVAILABILITY" => {
                let pairs = [(
                    "event_type",
                    Some(self.to_uri("event_types", args, "event_type")?),
                )];
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/event_type_availability_schedules{}",
                        query(&pairs)
                    )),
                )
            }

            // AP: src/lib/actions/get-scheduled-event.ts
            // path: `/scheduled_events/${toUuid(scheduledEvent)}`
            "CALENDLY_GET_SCHEDULED_EVENT" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/scheduled_events/{}",
                    uuid_ref(args, "scheduled_event")?
                )),
            ),

            // AP: src/lib/actions/list-scheduled-events.ts
            // path: /scheduled_events, query: organization|user, invitee_email, status,
            //       min_start_time, max_start_time, sort, count, page_token
            "CALENDLY_LIST_SCHEDULED_EVENTS" => {
                let owner = self.owner_query(args)?;
                let mut pairs: Vec<(&str, Option<String>)> = vec![
                    owner,
                    (
                        "invitee_email",
                        opt_str(args, "invitee_email").map(|v| v.trim().to_string()),
                    ),
                    ("status", opt_str(args, "status").map(String::from)),
                    ("min_start_time", opt_str(args, "min_start_time").map(String::from)),
                    ("max_start_time", opt_str(args, "max_start_time").map(String::from)),
                    ("sort", opt_str(args, "sort").map(String::from)),
                ];
                pairs.extend(Self::page_query(args));
                HttpRequest::new("GET", self.url(&format!("/scheduled_events{}", query(&pairs))))
            }

            // AP: src/lib/actions/find-invitee-bookings.ts
            // path: /scheduled_events, query: user, invitee_email, status,
            //       sort=start_time:desc, count=100
            "CALENDLY_FIND_INVITEE_BOOKINGS" => {
                let pairs = [
                    ("user", Some(self.to_uri("users", args, "user")?)),
                    (
                        "invitee_email",
                        Some(req_str(args, "invitee_email")?.trim().to_string()),
                    ),
                    ("status", opt_str(args, "status").map(String::from)),
                    ("sort", Some("start_time:desc".to_string())),
                    ("count", Some("100".to_string())),
                ];
                HttpRequest::new("GET", self.url(&format!("/scheduled_events{}", query(&pairs))))
            }

            // AP: src/lib/actions/list-event-invitees.ts
            // path: `/scheduled_events/${toUuid(scheduledEvent)}/invitees`,
            // query: email, status, count, page_token
            "CALENDLY_LIST_EVENT_INVITEES" => {
                let mut pairs: Vec<(&str, Option<String>)> = vec![
                    ("email", opt_str(args, "email").map(|v| v.trim().to_string())),
                    ("status", opt_str(args, "status").map(String::from)),
                ];
                pairs.extend(Self::page_query(args));
                HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/scheduled_events/{}/invitees{}",
                        uuid_ref(args, "scheduled_event")?,
                        query(&pairs)
                    )),
                )
            }

            // AP: src/lib/actions/get-event-invitee.ts
            // path: pathname of toInviteeUri() =
            //       `/scheduled_events/${toUuid(scheduledEvent)}/invitees/${toUuid(invitee)}`
            "CALENDLY_GET_EVENT_INVITEE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/scheduled_events/{}/invitees/{}",
                    uuid_ref(args, "scheduled_event")?,
                    uuid_ref(args, "invitee")?
                )),
            ),

            // AP: src/lib/actions/list-user-availability-schedules.ts
            // path: /user_availability_schedules, query: user
            "CALENDLY_LIST_USER_AVAILABILITY_SCHEDULES" => {
                let pairs = [("user", Some(self.to_uri("users", args, "user")?))];
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/user_availability_schedules{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/get-user-availability-schedule.ts
            // path: `/user_availability_schedules/${toUuid(schedule)}`
            "CALENDLY_GET_USER_AVAILABILITY_SCHEDULE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/user_availability_schedules/{}",
                    uuid_ref(args, "schedule")?
                )),
            ),

            // AP: src/lib/actions/list-user-busy-times.ts
            // path: /user_busy_times, query: user, start_time, end_time (max 7 days)
            "CALENDLY_LIST_USER_BUSY_TIMES" => {
                let (start, end) = Self::time_window(args, 7)?;
                let pairs = [
                    ("user", Some(self.to_uri("users", args, "user")?)),
                    ("start_time", Some(start)),
                    ("end_time", Some(end)),
                ];
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/user_busy_times{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/list-available-times.ts
            // path: /event_type_available_times,
            // query: event_type, start_time, end_time (max 31 days)
            "CALENDLY_LIST_AVAILABLE_TIMES" => {
                let (start, end) = Self::time_window(args, 31)?;
                let pairs = [
                    (
                        "event_type",
                        Some(self.to_uri("event_types", args, "event_type")?),
                    ),
                    ("start_time", Some(start)),
                    ("end_time", Some(end)),
                ];
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/event_type_available_times{}", query(&pairs))),
                )
            }

            // AP: src/lib/actions/list-user-locations.ts
            // path: /locations, query: user
            "CALENDLY_LIST_USER_LOCATIONS" => {
                let pairs = [("user", Some(self.to_uri("users", args, "user")?))];
                HttpRequest::new("GET", self.url(&format!("/locations{}", query(&pairs))))
            }

            // AP: src/lib/actions/cancel-scheduled-event.ts
            // path: POST `/scheduled_events/${toUuid(scheduledEvent)}/cancellation`,
            // body: {reason} | {}
            "CALENDLY_CANCEL_EVENT" => {
                let mut body = Map::new();
                if let Some(r) = opt_str(args, "reason") {
                    body.insert("reason".into(), json!(r));
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/scheduled_events/{}/cancellation",
                        uuid_ref(args, "scheduled_event")?
                    )),
                )
                .json(&Value::Object(body))
            }

            // AP: src/lib/actions/create-event-type.ts
            // path: POST /event_types, body: {owner, name, ...eventTypeBody}
            "CALENDLY_CREATE_EVENT_TYPE" => {
                let mut body = Map::new();
                body.insert(
                    "owner".into(),
                    json!(self.to_uri("users", args, "owner")?),
                );
                body.insert("name".into(), json!(req_str(args, "name")?));
                event_type_body(&mut body, args)?;
                HttpRequest::new("POST", self.url("/event_types")).json(&Value::Object(body))
            }

            // AP: src/lib/actions/update-event-type.ts
            // path: PATCH `/event_types/${toUuid(eventType)}`,
            // body: {name?} + eventTypeBody (at least one field)
            "CALENDLY_UPDATE_EVENT_TYPE" => {
                let mut body = Map::new();
                if let Some(n) = opt_str(args, "name") {
                    body.insert("name".into(), json!(n));
                }
                event_type_body(&mut body, args)?;
                if body.is_empty() {
                    bail!("Set at least one field to update.");
                }
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!("/event_types/{}", uuid_ref(args, "event_type")?)),
                )
                .json(&Value::Object(body))
            }

            // AP: src/lib/actions/update-event-type-availability.ts
            // path: PATCH /event_type_availability_schedules, query: event_type,
            // body: {availability_rule: {timezone, rules}}
            "CALENDLY_UPDATE_EVENT_TYPE_AVAILABILITY" => {
                let rules = args
                    .get("rules")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("`rules` must be a JSON array of availability rules."))?;
                let body = json!({
                    "availability_rule": {
                        "timezone": req_str(args, "timezone")?.trim(),
                        "rules": rules,
                    }
                });
                let pairs = [(
                    "event_type",
                    Some(self.to_uri("event_types", args, "event_type")?),
                )];
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!(
                        "/event_type_availability_schedules{}",
                        query(&pairs)
                    )),
                )
                .json(&body)
            }

            // AP: src/lib/actions/create-scheduling-link.ts
            // path: POST /scheduling_links,
            // body: {max_event_count: 1, owner: toUri(event_types), owner_type: "EventType"}
            "CALENDLY_CREATE_SCHEDULING_LINK" => HttpRequest::new("POST", self.url("/scheduling_links"))
                .json(&json!({
                    "max_event_count": 1,
                    "owner": self.to_uri("event_types", args, "event_type")?,
                    "owner_type": "EventType",
                })),

            // AP: src/lib/actions/create-one-off-event-type.ts
            // path: POST /one_off_event_types,
            // body: {name, host, duration, timezone?, date_setting, location?}
            "CALENDLY_CREATE_ONE_OFF_EVENT" => {
                let mut body = Map::new();
                body.insert("name".into(), json!(req_str(args, "name")?.trim()));
                body.insert("host".into(), json!(self.to_uri("users", args, "host")?));
                if !put_number(&mut body, args, "duration")? {
                    bail!("missing argument `duration`");
                }
                if let Some(tz) = opt_str(args, "timezone").map(str::trim).filter(|v| !v.is_empty())
                {
                    body.insert("timezone".into(), json!(tz));
                }
                body.insert(
                    "date_setting".into(),
                    json!({
                        "type": "date_range",
                        "start_date": req_str(args, "start_date")?.trim(),
                        "end_date": req_str(args, "end_date")?.trim(),
                    }),
                );
                if let Some(kind) = opt_str(args, "location_kind") {
                    if !LOCATION_KINDS.contains(&kind) {
                        bail!("`location_kind` must be one of {}", LOCATION_KINDS.join(", "));
                    }
                    let mut location = Map::new();
                    location.insert("kind".into(), json!(kind));
                    if let Some(details) =
                        opt_str(args, "location_details").map(str::trim).filter(|v| !v.is_empty())
                    {
                        location.insert(
                            if kind == "inbound_call" {
                                "phone_number"
                            } else {
                                "location"
                            }
                            .into(),
                            json!(details),
                        );
                    }
                    body.insert("location".into(), Value::Object(location));
                }
                HttpRequest::new("POST", self.url("/one_off_event_types"))
                    .json(&Value::Object(body))
            }

            // AP: src/lib/actions/create-share.ts
            // path: POST /shares,
            // body: {event_type, name?, duration?, period_type/start_date/end_date?, hide_location?}
            "CALENDLY_CREATE_SHARE" => {
                let start = opt_str(args, "start_date").map(str::trim).filter(|v| !v.is_empty());
                let end = opt_str(args, "end_date").map(str::trim).filter(|v| !v.is_empty());
                if start.is_some() != end.is_some() {
                    bail!("Set both Start Date and End Date to limit the booking window, or leave both empty.");
                }
                let mut body = Map::new();
                body.insert(
                    "event_type".into(),
                    json!(self.to_uri("event_types", args, "event_type")?),
                );
                if let Some(n) = opt_str(args, "name") {
                    body.insert("name".into(), json!(n));
                }
                put_number(&mut body, args, "duration")?;
                if let (Some(s), Some(e)) = (start, end) {
                    body.insert("period_type".into(), json!("fixed"));
                    body.insert("start_date".into(), json!(s));
                    body.insert("end_date".into(), json!(e));
                }
                if args.get("hide_location").and_then(Value::as_bool) == Some(true) {
                    body.insert("hide_location".into(), json!(true));
                }
                HttpRequest::new("POST", self.url("/shares")).json(&Value::Object(body))
            }

            other => bail!("unknown Calendly tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        Ok(match tool {
            // AP: src/lib/actions/find-user.ts (`{found, user, role, membership_uri}`)
            "CALENDLY_FIND_USER" => {
                let (items, _) = collection(response)?;
                match items.first() {
                    Some(m) => json!({
                        "found": true,
                        "user": m["user"].clone(),
                        "role": m["role"].clone(),
                        "membership_uri": m["uri"].clone(),
                    }),
                    None => json!({
                        "found": false,
                        "user": Value::Null,
                        "role": Value::Null,
                        "membership_uri": Value::Null,
                    }),
                }
            }
            // AP: src/lib/actions/find-invitee-bookings.ts (`{found, count, events}`)
            "CALENDLY_FIND_INVITEE_BOOKINGS" => {
                let (events, _) = collection(response)?;
                json!({"found": !events.is_empty(), "count": events.len(), "events": events})
            }
            // AP: the paginated list actions (`calendlyCommon.toPage`)
            "CALENDLY_LIST_ORGANIZATION_MEMBERS"
            | "CALENDLY_LIST_EVENT_TYPES"
            | "CALENDLY_LIST_EVENT_TYPE_HOSTS"
            | "CALENDLY_LIST_SCHEDULED_EVENTS"
            | "CALENDLY_LIST_EVENT_INVITEES" => list_page(response)?,
            // AP: the non-paginated collections (`{items, count}`)
            "CALENDLY_GET_EVENT_TYPE_AVAILABILITY"
            | "CALENDLY_LIST_USER_AVAILABILITY_SCHEDULES"
            | "CALENDLY_LIST_USER_BUSY_TIMES"
            | "CALENDLY_LIST_AVAILABLE_TIMES"
            | "CALENDLY_LIST_USER_LOCATIONS" => items_only(response)?,
            // AP: every remaining action returns `response.resource`
            _ => resource(response)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_uris_and_windows() {
        let c = Calendly::with_api_base("https://api.calendly.test");
        // toUuid accepts full URIs and bare ids.
        let r = c
            .build_request(
                "CALENDLY_GET_SCHEDULED_EVENT",
                &json!({"scheduled_event": "https://api.calendly.com/scheduled_events/GBGBDCAADAIDCXZS/"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.calendly.test/scheduled_events/GBGBDCAADAIDCXZS"
        );
        // toUuid takes the last path segment of whatever it is given, so a
        // relative tail resolves against the segment guard; only a bad tail errs.
        let r = c
            .build_request(
                "CALENDLY_GET_SCHEDULED_EVENT",
                &json!({"scheduled_event": "../users/x"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://api.calendly.test/scheduled_events/x");
        assert!(c
            .build_request(
                "CALENDLY_GET_SCHEDULED_EVENT",
                &json!({"scheduled_event": "../users/.."}),
            )
            .is_err());

        // toUri passes full Calendly URIs through and rejects foreign hosts.
        let r = c
            .build_request("CALENDLY_LIST_USER_LOCATIONS", &json!({"user": "https://evil.example.com/users/x"}))
            .unwrap_err();
        assert!(r.to_string().contains("Calendly URI"), "{r}");

        // validateTimeWindow: span and ordering checks.
        let args = json!({"user": "u1", "start_time": "2026-10-20T09:00:00Z", "end_time": "2026-10-27T09:00:01Z"});
        assert!(c.build_request("CALENDLY_LIST_USER_BUSY_TIMES", &args).is_err());
        let args = json!({"user": "u1", "start_time": "2026-10-21T09:00:00Z", "end_time": "2026-10-20T09:00:00Z"});
        assert!(c.build_request("CALENDLY_LIST_USER_BUSY_TIMES", &args).is_err());
        let args = json!({"user": "u1", "start_time": "nonsense", "end_time": "2026-10-20T09:00:00Z"});
        assert!(c.build_request("CALENDLY_LIST_USER_BUSY_TIMES", &args).is_err());

        // Exactly one owner is required.
        assert!(c.build_request("CALENDLY_LIST_EVENT_TYPES", &json!({})).is_err());
        let r = c
            .build_request("CALENDLY_CREATE_SHARE", &json!({"event_type": "ET1", "start_date": "2026-10-20"}))
            .unwrap_err();
        assert!(r.to_string().contains("both Start Date and End Date"), "{r}");
    }

    #[test]
    fn provider_errors() {
        let resp = |status: u16, body: Value| HttpResponse {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            session_id: None,
        };
        let c = Calendly::default();
        let err = c
            .parse_response(
                "CALENDLY_GET_USER",
                &resp(403, json!({"title": "Permission Denied", "required_scopes": ["users:read"]})),
            )
            .unwrap_err();
        assert!(err.to_string().contains("users:read"), "{err}");
        let err = c
            .parse_response(
                "CALENDLY_GET_USER",
                &resp(404, json!({"title": "Resource Not Found", "message": "nope"})),
            )
            .unwrap_err();
        assert!(err.to_string().contains("Resource Not Found (404): nope"), "{err}");
        let v = c
            .parse_response("CALENDLY_GET_USER", &resp(200, json!({"resource": {"uri": "u"}})))
            .unwrap();
        assert_eq!(v["uri"], "u");
    }
}
