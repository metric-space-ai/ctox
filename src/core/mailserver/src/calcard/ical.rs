// ref: stalwart/src/calcard/ical/mod.rs:1-100
// ref: ctox-mailserver streamlined iCalendar parser

use crate::util::errors::StalwartResult;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ICalendarEvent {
    pub uid: String,
    pub summary: String,
    pub dtstart: String,
    pub dtend: String,
    pub rrule: Option<String>,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ICalendar {
    pub events: Vec<ICalendarEvent>,
}

impl ICalendar {
    pub fn parse(data: &str) -> StalwartResult<Self> {
        let unfolded = unfold_lines(data);
        let mut events = Vec::new();
        let mut current_event: Option<ICalendarEvent> = None;

        for line in unfolded.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
                current_event = Some(ICalendarEvent {
                    uid: String::new(),
                    summary: String::new(),
                    dtstart: String::new(),
                    dtend: String::new(),
                    rrule: None,
                    properties: HashMap::new(),
                });
                continue;
            }

            if line.eq_ignore_ascii_case("END:VEVENT") {
                if let Some(event) = current_event.take() {
                    if !event.uid.is_empty() {
                        events.push(event);
                    }
                }
                continue;
            }

            if let Some(ref mut event) = current_event {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key_part = parts[0];
                    let val = parts[1].to_string();

                    // Strip parameters from key, e.g. "DTSTART;TZID=America/New_York" -> "DTSTART"
                    let key = key_part
                        .split(';')
                        .next()
                        .unwrap_or(key_part)
                        .to_uppercase();

                    match key.as_str() {
                        "UID" => event.uid = val.clone(),
                        "SUMMARY" => event.summary = val.clone(),
                        "DTSTART" => event.dtstart = val.clone(),
                        "DTEND" => event.dtend = val.clone(),
                        "RRULE" => event.rrule = Some(val.clone()),
                        _ => {}
                    }
                    event.properties.insert(key, val);
                }
            }
        }

        Ok(Self { events })
    }
}

fn unfold_lines(data: &str) -> String {
    let mut unfolded = String::new();
    for line in data.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line
            unfolded.push_str(&line[1..]);
        } else {
            if !unfolded.is_empty() {
                unfolded.push('\n');
            }
            unfolded.push_str(line);
        }
    }
    unfolded
}
