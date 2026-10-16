//! Twilio toolkit (REST API `2010-04-01` + Lookup v2).
//! Action set modelled on Activepieces `piece-twilio` (MIT).
//!
//! Auth: the piece declares `PieceAuth.BasicAuth` (username = Account SID,
//! password = Auth Token) and every `httpClient.sendRequest` sends
//! `authentication: {type: BASIC, username: account_sid, password: auth_token}`.
//! The hub stores that credential as `account_sid:auth_token` and injects the
//! `Authorization: Basic …` header, so `AuthScheme::Basic` is exactly right.
//!
//! The piece takes the Account SID from the credential and puts it into the URL
//! path (`/2010-04-01/Accounts/{account_sid}/…`). A credential-free adapter
//! cannot read the stored credential, so every `api.twilio.com` tool takes the
//! Account SID as a plain `account_sid` argument — the very same value the
//! Basic credential carries as its username. Phone numbers are *not* ids and
//! travel percent-encoded into the path (Lookup v2), as `encodeURIComponent`
//! does in `phone-number-lookup.ts`.
//!
//! Every tool's request arm cites the Activepieces source file it mirrors; the
//! list/get tools reuse the exact resource URIs the piece itself polls or ships
//! as sample data (`…/Calls/{sid}.json`, `…/Recordings.json?PageSize=…`, …).
//! `download_recording_media` is the piece's one binary action: the piece writes
//! the audio to the Activepieces file store, which this hub has no equivalent
//! for, so the tool performs the same `GET …/Recordings/{sid}.{mp3|wav}` and
//! reports the fetched byte count plus the format detected from the payload.
//!
//! Skipped on purpose: `custom_api_call` (an open-ended HTTP escape hatch) and
//! the five polling triggers (not actions).
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "twilio";

/// Twilio auth tokens carry no scopes: the whole token is either allowed or
/// not. Like the Linear adapter, every tool therefore names the access level
/// the call needs (`read` / `write`) instead of pretending there are scopes.
const READ: &str = "read";
const WRITE: &str = "write";

/// `static_dropdown` options copied from the actions.
const FORMATS: [&str; 2] = ["mp3", "wav"];
const VOICES: [&str; 3] = ["alice", "man", "woman"];
const LANGUAGES: [&str; 5] = ["en-US", "en-GB", "es-ES", "fr-FR", "de-DE"];

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// `channels` (`Property.StaticDropdown` with numeric options 1 / 2).
fn channels_prop() -> Value {
    json!({"type": "integer", "description":
        "Kanalzahl der Aufnahme: 1 Mono, 2 Dual (nur wenn getrennt aufgezeichnet). / Recording channels: 1 mono, 2 dual (only if recorded that way).",
        "enum": [1, 2]})
}

/// `timeout` (`Property.Number`, min 5, max 600, step 5, default 60).
fn timeout_prop() -> Value {
    json!({"type": "integer", "description":
        "Sekunden läuten, bevor aufgegeben wird (5-600). Leer: 60. / Seconds to ring before giving up (5-600). Empty: 60.",
        "minimum": 5, "maximum": 600, "default": 60})
}

fn page_size(default: u64, max: u64) -> Value {
    json!({"type": "integer",
        "description": format!("`PageSize` der Twilio-Liste (1-{max}). Standard {default}. / `PageSize` of the Twilio list (1-{max}). Default {default}."),
        "minimum": 1, "maximum": max, "default": default})
}

/// Shared props of every list tool: account + `PageSize`.
fn list_props(default: u64, max: u64, extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = vec![
        ("account_sid", s("SID des Twilio-Kontos (AC…) – identisch mit dem Benutzernamen der Basic-Zugangsdaten `account_sid:auth_token`.")),
        ("page_size", page_size(default, max)),
    ];
    v.extend(extra);
    v
}

// ---------------------------------------------------------------- encoding

/// `encodeURIComponent` for one path segment (Twilio Lookup phone numbers):
/// unreserved characters pass through, everything else is percent-encoded from
/// its UTF-8 bytes. `+` therefore becomes `%2B`, which is what
/// `phone-number-lookup.ts` sends.
fn encode_path_segment(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// TwiML attribute/text escaping exactly as `make-call.ts` does it.
fn twiml_escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn q_str(args: &Value, key: &str) -> Option<String> {
    opt_str(args, key).map(str::to_string)
}

fn q_u64(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_u64).map(|v| v.to_string())
}

/// Enforced dropdown (`format`, `voice`, `language`) so a typo never reaches
/// the API.
fn pick<'a>(args: &'a Value, key: &str, allowed: &[&str]) -> Result<Option<&'a str>> {
    match opt_str(args, key) {
        Some(v) if allowed.contains(&v) => Ok(Some(v)),
        Some(_) => bail!("`{key}` must be one of {}", allowed.join(", ")),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------- adapter

pub struct Twilio {
    spec: ToolkitSpec,
    /// `https://api.twilio.com/2010-04-01` — the piece's base for account calls.
    api_base: String,
    /// `https://lookups.twilio.com/v2` — a separate host for Lookup v2.
    lookup_base: String,
}

impl Default for Twilio {
    fn default() -> Self {
        Self::with_bases(
            "https://api.twilio.com/2010-04-01",
            "https://lookups.twilio.com/v2",
        )
    }
}

#[allow(clippy::too_many_lines)]
impl Twilio {
    pub fn with_api_base(api_base: &str) -> Self {
        Self::with_bases(api_base, "https://lookups.twilio.com/v2")
    }

    pub fn with_bases(api_base: &str, lookup_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let acct = "SID des Twilio-Kontos (AC…) – identisch mit dem Benutzernamen der Basic-Zugangsdaten `account_sid:auth_token`.";
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Twilio".into(),
            description: LocalizedText::new(
                "Nachrichten, Anrufe, Aufzeichnungen und Nummern in Twilio lesen, SMS senden und Anrufe tätigen.",
                "Read messages, calls, recordings and phone numbers in Twilio, send SMS and place calls.",
            ),
            auth_schemes: vec![AuthScheme::Basic],
            tools: vec![
                t(
                    "TWILIO_GET_MESSAGE",
                    ("Nachricht lesen", "Get message"),
                    (
                        "Liest Status und Details einer gesendeten oder empfangenen Nachricht.",
                        "Reads the status and details of a sent or received message.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("message_sid", s("SID der Nachricht (SM… oder MM…); steht in der Ausgabe von TWILIO_SEND_SMS und im Twilio-Log.")),
                    ],
                    &["account_sid", "message_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_MESSAGES",
                    ("Nachrichten auflisten", "List messages"),
                    (
                        "Listet Nachrichten des Kontos, optional auf eine Empfängernummer gefiltert.",
                        "Lists the account's messages, optionally filtered to one destination number.",
                    ),
                    &list_props(
                        20,
                        1000,
                        vec![("to", s("Nur Nachrichten an diese Nummer (E.164, z. B. +15558675310)."))],
                    ),
                    &["account_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_MESSAGE_MEDIA",
                    ("Nachrichtenmedien auflisten", "List message media"),
                    (
                        "Listet die Bilder einer MMS (über `subresource_uris.media` der Nachricht).",
                        "Lists the images attached to an MMS (via the message's `subresource_uris.media`).",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("message_sid", s("SID der Nachricht (MM… oder SM…)")),
                    ],
                    &["account_sid", "message_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_GET_CALL",
                    ("Anruf lesen", "Get call"),
                    (
                        "Liest Status, Richtung, Parteien und Preis eines Anrufs.",
                        "Reads the status, direction, parties and price of one call.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("call_sid", s("SID des Anrufs (CA…)")),
                    ],
                    &["account_sid", "call_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_CALLS",
                    ("Anrufe auflisten", "List calls"),
                    (
                        "Listet die Anrufe des Kontos, optional nach Status gefiltert.",
                        "Lists the account's calls, optionally filtered by status.",
                    ),
                    &list_props(
                        10,
                        1000,
                        vec![("status", s("Nur Anrufe mit diesem Status, z. B. completed."))],
                    ),
                    &["account_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_GET_RECORDING",
                    ("Aufzeichnung lesen", "Get recording"),
                    (
                        "Liest Metadaten einer Gesprächsaufzeichnung (Dauer, Status, Quell-URL).",
                        "Reads the metadata of a call recording (duration, status, media URL).",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("recording_sid", s("SID der Aufzeichnung (RE…); steht in der Ausgabe von TWILIO_LIST_RECORDINGS.")),
                    ],
                    &["account_sid", "recording_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_RECORDINGS",
                    ("Aufzeichnungen auflisten", "List recordings"),
                    (
                        "Listet die Gesprächsaufzeichnungen des Kontos, neueste zuerst.",
                        "Lists the account's call recordings, newest first.",
                    ),
                    &list_props(10, 1000, vec![]),
                    &["account_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_DOWNLOAD_RECORDING_MEDIA",
                    ("Aufzeichnung herunterladen", "Download recording media"),
                    (
                        "Lädt die Audiodatei einer Aufzeichnung (MP3 oder WAV).",
                        "Downloads the audio file of a recording (MP3 or WAV).",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("recording_sid", s("SID der Aufzeichnung (RE…)")),
                        ("format", choice("MP3 ist kleiner, WAV behält die volle Qualität. / MP3 is smaller, WAV keeps full quality.", &FORMATS)),
                        ("channels", channels_prop()),
                    ],
                    &["account_sid", "recording_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_GET_TRANSCRIPTION",
                    ("Transkription lesen", "Get transcription"),
                    (
                        "Liest das Transkript einer Gesprächsaufzeichnung.",
                        "Reads the transcript of a call recording.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("transcription_sid", s("SID der Transkription (TR…); steht in der Ausgabe von TWILIO_LIST_TRANSCRIPTIONS.")),
                    ],
                    &["account_sid", "transcription_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_TRANSCRIPTIONS",
                    ("Transkriptionen auflisten", "List transcriptions"),
                    (
                        "Listet die Transkriptionen des Kontos mit Status und Aufzeichnungs-SID.",
                        "Lists the account's transcriptions with status and recording SID.",
                    ),
                    &list_props(10, 1000, vec![]),
                    &["account_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_LIST_INCOMING_PHONE_NUMBERS",
                    ("Eigene Rufnummern auflisten", "List incoming phone numbers"),
                    (
                        "Listet die eigenen Twilio-Rufnummern mit Fähigkeiten (Voice/SMS/MMS).",
                        "Lists your own Twilio phone numbers with their capabilities (voice/SMS/MMS).",
                    ),
                    &list_props(1000, 1000, vec![]),
                    &["account_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_GET_INCOMING_PHONE_NUMBER",
                    ("Eigene Rufnummer lesen", "Get incoming phone number"),
                    (
                        "Liest eine eigene Rufnummer mit Sprach-/SMS-Weiterleitung und Fähigkeiten.",
                        "Reads one of your phone numbers with its voice/SMS URLs and capabilities.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("phone_number_sid", s("SID der Rufnummer (PN…); steht in der Ausgabe von TWILIO_LIST_INCOMING_PHONE_NUMBERS.")),
                    ],
                    &["account_sid", "phone_number_sid"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_PHONE_NUMBER_LOOKUP",
                    ("Telefonnummer nachschlagen", "Phone number lookup"),
                    (
                        "Ermittelt Betreiber und Leitungsart (mobil, Festnetz) einer Nummer.",
                        "Determines the carrier and line type (mobile, landline) of a number.",
                    ),
                    &[("phone_number", s("Telefonnummer mit Ländervorwahl, beginnend mit +, z. B. +15558675310."))],
                    &["phone_number"],
                    &[READ],
                    false,
                ),
                t(
                    "TWILIO_SEND_SMS",
                    ("SMS senden", "Send SMS"),
                    (
                        "Sendet eine SMS von einer eigenen Twilio-Nummer; braucht --confirm.",
                        "Sends an SMS from one of your Twilio numbers; requires --confirm.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("from", s("Absendernummer aus TWILIO_LIST_INCOMING_PHONE_NUMBERS (E.164).")),
                        ("to", s("Empfängernummer mit Ländervorwahl, beginnend mit +.")),
                        ("body", s("Nachrichtentext, bis 1.600 Zeichen; längere Texte werden geteilt und je Teil berechnet.")),
                    ],
                    &["account_sid", "from", "to", "body"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TWILIO_MAKE_CALL",
                    ("Telefon anrufen", "Call phone"),
                    (
                        "Ruft eine Nummer an und liest einen Text vor; braucht --confirm.",
                        "Calls a number and reads a message aloud; requires --confirm.",
                    ),
                    &[
                        ("account_sid", s(acct)),
                        ("from", s("Anrufernummer aus TWILIO_LIST_INCOMING_PHONE_NUMBERS (E.164).")),
                        ("to", s("Zu rufende Nummer mit Ländervorwahl, beginnend mit +.")),
                        ("message", s("Text, der nach Abnehmen vorgelesen wird.")),
                        ("voice", choice("Leer: Standardstimme des Kontos. / Empty: your Twilio account's default voice.", &VOICES)),
                        ("language", choice("Sprache der Vorlesung. Leer: Englisch (US). / Language the message is read in. Empty: English (US).", &LANGUAGES)),
                        ("send_digits", s("Tasten, die nach dem Abheben gedrückt werden; w pausiert 0,5 s, W 1 s.")),
                        ("timeout", timeout_prop()),
                    ],
                    &["account_sid", "from", "to", "message"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
            lookup_base: lookup_base.trim_end_matches('/').to_string(),
        }
    }

    /// `{api_base}/Accounts/{account_sid}/{tail}` — the exact URL shape
    /// `callTwilioApi` builds in `src/lib/common/index.ts`.
    fn account_url(&self, args: &Value, tail: &str) -> Result<String> {
        Ok(format!(
            "{}/Accounts/{}/{tail}",
            self.api_base,
            path_segment(args, "account_sid")?
        ))
    }

    /// `GET {account_url}{query}`.
    fn account_get(
        &self,
        args: &Value,
        tail: &str,
        pairs: &[(&str, Option<String>)],
    ) -> Result<HttpRequest> {
        Ok(HttpRequest::new(
            "GET",
            format!("{}{}", self.account_url(args, tail)?, query(pairs)),
        ))
    }
}

impl ToolkitAdapter for Twilio {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/action/get-message.ts — callTwilioApi(GET, `Messages/${message_sid}.json`)
            "TWILIO_GET_MESSAGE" => self.account_get(
                args,
                &format!("Messages/{}.json", path_segment(args, "message_sid")?),
                &[],
            )?,
            // AP: src/lib/trigger/new-incoming-sms.ts — `Messages.json?PageSize=20&To=${encodeURIComponent(phone_number)}`
            "TWILIO_LIST_MESSAGES" => self.account_get(
                args,
                "Messages.json",
                &[
                    ("PageSize", q_u64(args, "page_size")),
                    ("To", q_str(args, "to")),
                ],
            )?,
            // AP: src/lib/trigger/new-incoming-sms.ts — sampleData `subresource_uris.media`
            //   = `/2010-04-01/Accounts/{AccountSid}/Messages/{MessageSid}/Media.json`
            "TWILIO_LIST_MESSAGE_MEDIA" => self.account_get(
                args,
                &format!(
                    "Messages/{}/Media.json",
                    path_segment(args, "message_sid")?
                ),
                &[],
            )?,
            // AP: src/lib/trigger/new-call.ts — sampleData `uri`
            //   = `/2010-04-01/Accounts/{AccountSid}/Calls/{CallSid}.json`
            "TWILIO_GET_CALL" => self.account_get(
                args,
                &format!("Calls/{}.json", path_segment(args, "call_sid")?),
                &[],
            )?,
            // AP: src/lib/trigger/new-call.ts — `Calls.json?PageSize=10&Status=completed`
            "TWILIO_LIST_CALLS" => self.account_get(
                args,
                "Calls.json",
                &[
                    ("PageSize", q_u64(args, "page_size")),
                    ("Status", q_str(args, "status")),
                ],
            )?,
            // AP: src/lib/trigger/new-recording.ts — `Recordings.json?PageSize=…`
            "TWILIO_LIST_RECORDINGS" => {
                self.account_get(args, "Recordings.json", &[("PageSize", q_u64(args, "page_size"))])?
            }
            // AP: src/lib/trigger/new-recording.ts — sampleData `uri`
            //   = `/2010-04-01/Accounts/{AccountSid}/Recordings/{RecordingSid}.json`
            "TWILIO_GET_RECORDING" => self.account_get(
                args,
                &format!("Recordings/{}.json", path_segment(args, "recording_sid")?),
                &[],
            )?,
            // AP: src/lib/action/download-recording-media.ts — `Recordings/${recording_sid}.${fileFormat}`
            //   with the optional `RequestedChannels` query param
            "TWILIO_DOWNLOAD_RECORDING_MEDIA" => {
                let format = pick(args, "format", &FORMATS)?.unwrap_or("mp3");
                let sid = path_segment(args, "recording_sid")?;
                self.account_get(
                    args,
                    &format!("Recordings/{sid}.{format}"),
                    &[(
                        "RequestedChannels",
                        args.get("channels").and_then(Value::as_u64).map(|v| v.to_string()),
                    )],
                )?
            }
            // AP: src/lib/trigger/new-transcription.ts — `Transcriptions.json?PageSize=…`
            "TWILIO_LIST_TRANSCRIPTIONS" => self.account_get(
                args,
                "Transcriptions.json",
                &[("PageSize", q_u64(args, "page_size"))],
            )?,
            // AP: src/lib/trigger/new-transcription.ts — sampleData `uri`
            //   = `/2010-04-01/Accounts/{AccountSid}/Transcriptions/{TranscriptionSid}.json`
            "TWILIO_GET_TRANSCRIPTION" => self.account_get(
                args,
                &format!(
                    "Transcriptions/{}.json",
                    path_segment(args, "transcription_sid")?
                ),
                &[],
            )?,
            // AP: src/lib/common/index.ts — fetchAllIncomingPhoneNumbers polls
            //   `IncomingPhoneNumbers.json?PageSize=1000` to fill the `from` dropdowns
            "TWILIO_LIST_INCOMING_PHONE_NUMBERS" => self.account_get(
                args,
                "IncomingPhoneNumbers.json",
                &[("PageSize", q_u64(args, "page_size"))],
            )?,
            // AP: src/lib/trigger/new-phone-number.ts — sampleData `uri`
            //   = `/2010-04-01/Accounts/{AccountSid}/IncomingPhoneNumbers/{PhoneNumberSid}.json`
            "TWILIO_GET_INCOMING_PHONE_NUMBER" => self.account_get(
                args,
                &format!(
                    "IncomingPhoneNumbers/{}.json",
                    path_segment(args, "phone_number_sid")?
                ),
                &[],
            )?,
            // AP: src/lib/action/phone-number-lookup.ts —
            //   `https://lookups.twilio.com/v2/PhoneNumbers/${encodeURIComponent(phone_number)}?Fields=line_type_intelligence`
            "TWILIO_PHONE_NUMBER_LOOKUP" => HttpRequest::new(
                "GET",
                format!(
                    "{}/PhoneNumbers/{}{}",
                    self.lookup_base,
                    encode_path_segment(req_str(args, "phone_number")?.trim()),
                    query(&[(
                        "Fields",
                        Some("line_type_intelligence".to_string()),
                    )])
                ),
            ),
            // AP: src/lib/action/send-sms.ts — callTwilioApi(POST, 'Messages.json', {From, Body, To})
            "TWILIO_SEND_SMS" => {
                let pairs: Vec<(&str, &str)> = vec![
                    ("From", req_str(args, "from")?.trim()),
                    ("To", req_str(args, "to")?.trim()),
                    ("Body", req_str(args, "body")?),
                ];
                HttpRequest::new("POST", self.account_url(args, "Messages.json")?).form(&pairs)
            }
            // AP: src/lib/action/make-call.ts — callTwilioApi(POST, 'Calls.json',
            //   {From, To, Twiml, SendDigits?, Timeout?}) with the TwiML `<Response><Say …>`
            "TWILIO_MAKE_CALL" => {
                let mut twiml = String::from("<Response><Say");
                if let Some(v) = pick(args, "voice", &VOICES)? {
                    twiml.push_str(&format!(" voice=\"{v}\""));
                }
                if let Some(l) = pick(args, "language", &LANGUAGES)? {
                    twiml.push_str(&format!(" language=\"{l}\""));
                }
                twiml.push('>');
                twiml.push_str(&twiml_escape(req_str(args, "message")?));
                twiml.push_str("</Say></Response>");

                let mut pairs: Vec<(&str, &str)> = vec![
                    ("From", req_str(args, "from")?.trim()),
                    ("To", req_str(args, "to")?.trim()),
                    ("Twiml", twiml.as_str()),
                ];
                let send_digits = opt_str(args, "send_digits");
                if let Some(d) = send_digits {
                    pairs.push(("SendDigits", d));
                }
                let timeout = args
                    .get("timeout")
                    .and_then(Value::as_u64)
                    .map(|t| t.clamp(5, 600).to_string());
                let timeout = timeout.as_deref();
                if let Some(t) = timeout {
                    pairs.push(("Timeout", t));
                }
                HttpRequest::new("POST", self.account_url(args, "Calls.json")?).form(&pairs)
            }
            other => bail!("unknown Twilio tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        if tool == "TWILIO_DOWNLOAD_RECORDING_MEDIA" {
            // The action returns the audio file; the hub has no file store, so
            // report the payload size and the format detected from the bytes.
            return Ok(json!({
                "media_bytes": response.body.len(),
                "detected_format": detect_audio_format(&response.body),
                "status": response.status,
            }));
        }
        let v = response.json()?;
        // Twilio list envelopes: `{ "<resource>": [...], "next_page_uri": … }`.
        let key = match tool {
            "TWILIO_LIST_MESSAGES" => Some("messages"),
            "TWILIO_LIST_MESSAGE_MEDIA" => Some("media"),
            "TWILIO_LIST_CALLS" => Some("calls"),
            "TWILIO_LIST_RECORDINGS" => Some("recordings"),
            "TWILIO_LIST_TRANSCRIPTIONS" => Some("transcriptions"),
            "TWILIO_LIST_INCOMING_PHONE_NUMBERS" => Some("incoming_phone_numbers"),
            _ => None,
        };
        Ok(match key {
            Some(k) => {
                let items = v.get(k).cloned().unwrap_or_else(|| Value::Array(vec![]));
                json!({
                    "items": items.clone(),
                    "count": items.as_array().map_or(0, Vec::len),
                    "next_page_uri": v["next_page_uri"],
                    "page_size": v["page_size"],
                    "page": v["page"],
                })
            }
            // `return response.body` in the actions — pass the resource through.
            None => v,
        })
    }
}

/// MP3 (ID3 tag / MPEG frame sync) or WAV (`RIFF….WAVE`) from the payload.
fn detect_audio_format(body: &[u8]) -> &'static str {
    if body.starts_with(b"ID3")
        || (body.len() > 1 && body[0] == 0xFF && body[1] & 0xE0 == 0xE0)
    {
        "mp3"
    } else if body.len() > 12 && &body[0..4] == b"RIFF" && &body[8..12] == b"WAVE" {
        "wav"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> Twilio {
        Twilio::with_bases("https://api.twilio.test/2010-04-01", "https://lookups.twilio.test/v2")
    }

    #[test]
    fn requests_follow_the_piece() {
        let t = adapter();
        let r = t
            .build_request("TWILIO_GET_MESSAGE", &json!({"account_sid": "AC1", "message_sid": "SM1"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://api.twilio.test/2010-04-01/Accounts/AC1/Messages/SM1.json"
        );

        let r = t
            .build_request(
                "TWILIO_LIST_MESSAGES",
                &json!({"account_sid": "AC1", "page_size": 20, "to": "+15558675310"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.twilio.test/2010-04-01/Accounts/AC1/Messages.json?PageSize=20&To=%2B15558675310"
        );

        let r = t
            .build_request(
                "TWILIO_PHONE_NUMBER_LOOKUP",
                &json!({"phone_number": "+15558675310"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://lookups.twilio.test/v2/PhoneNumbers/%2B15558675310?Fields=line_type_intelligence"
        );

        let r = t
            .build_request(
                "TWILIO_DOWNLOAD_RECORDING_MEDIA",
                &json!({"account_sid": "AC1", "recording_sid": "RE1", "format": "wav", "channels": 2}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.twilio.test/2010-04-01/Accounts/AC1/Recordings/RE1.wav?RequestedChannels=2"
        );

        // mp3 is the piece's default when `format` is absent.
        let r = t
            .build_request(
                "TWILIO_DOWNLOAD_RECORDING_MEDIA",
                &json!({"account_sid": "AC1", "recording_sid": "RE1"}),
            )
            .unwrap();
        assert!(r.url.ends_with("Recordings/RE1.mp3"), "{}", r.url);

        let r = t
            .build_request(
                "TWILIO_SEND_SMS",
                &json!({"account_sid": "AC1", "from": "+15017122661", "to": "+15558675310", "body": "Hi"}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(
            r.url,
            "https://api.twilio.test/2010-04-01/Accounts/AC1/Messages.json"
        );
        assert_eq!(
            r.headers.get("content-type").map(String::as_str),
            Some("application/x-www-form-urlencoded")
        );
        let body = String::from_utf8(r.body.unwrap()).unwrap();
        assert_eq!(body, "From=%2B15017122661&To=%2B15558675310&Body=Hi");
    }

    #[test]
    fn twiml_is_escaped_like_the_piece() {
        let t = adapter();
        let r = t
            .build_request(
                "TWILIO_MAKE_CALL",
                &json!({"account_sid": "AC1", "from": "+15017122661", "to": "+15558675310",
                        "message": "5 < 6 & \"call\" me", "voice": "alice", "language": "de-DE",
                        "send_digits": "W1234#", "timeout": 30}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.twilio.test/2010-04-01/Accounts/AC1/Calls.json"
        );
        let body = String::from_utf8(r.body.unwrap()).unwrap();
        assert!(body.contains("Twiml=%3CResponse%3E%3CSay+voice%3D%22alice%22+language%3D%22de-DE%22%3E"), "{body}");
        assert!(body.contains("5+%26lt%3B+6+%26amp%3B+%26quot%3Bcall%26quot%3B+me%3C%2FSay%3E%3C%2FResponse%3E"), "{body}");
        assert!(body.contains("SendDigits=W1234%23"), "{body}");
        assert!(body.contains("Timeout=30"), "{body}");
    }

    #[test]
    fn validation_rejects_unsafe_input() {
        let t = adapter();
        assert!(t.build_request("TWILIO_GET_MESSAGE", &json!({"account_sid": "AC/../x", "message_sid": "SM1"})).is_err());
        assert!(t.build_request("TWILIO_GET_MESSAGE", &json!({"message_sid": "SM1"})).is_err());
        assert!(t
            .build_request(
                "TWILIO_DOWNLOAD_RECORDING_MEDIA",
                &json!({"account_sid": "AC1", "recording_sid": "RE1", "format": "flac"})
            )
            .is_err());
        assert!(t
            .build_request(
                "TWILIO_PHONE_NUMBER_LOOKUP",
                &json!({"phone_number": "  "})
            )
            .is_err());
        assert!(t.build_request("TWILIO_NOPE", &json!({})).is_err());
    }

    #[test]
    fn responses_are_flattened() {
        let t = adapter();
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"messages": [{"sid": "SM1"}], "next_page_uri": null,
                                              "page_size": 20, "page": 0})).unwrap(),
            session_id: None,
        };
        let out = t.parse_response("TWILIO_LIST_MESSAGES", &resp).unwrap();
        assert_eq!(out["count"], 1);
        assert_eq!(out["items"][0]["sid"], "SM1");

        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"sid": "SM1", "status": "delivered"})).unwrap(),
            session_id: None,
        };
        assert_eq!(
            t.parse_response("TWILIO_GET_MESSAGE", &resp).unwrap()["status"],
            "delivered"
        );

        let resp = HttpResponse {
            status: 200,
            body: b"ID3\x04\x00\x00\x00audio".to_vec(),
            session_id: None,
        };
        let out = t.parse_response("TWILIO_DOWNLOAD_RECORDING_MEDIA", &resp).unwrap();
        assert_eq!(out["detected_format"], "mp3");
        assert_eq!(out["media_bytes"], 12);
        assert_eq!(detect_audio_format(b"RIFF\x00\x00\x00\x00WAVEfmt "), "wav");
        assert_eq!(detect_audio_format(b"nope"), "unknown");
    }
}