// Generated from src/core/rxdb/tests/fixtures/workjet-calendar-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.calendar.v1";

pub(crate) trait WireValidate {
    fn validate(&self) -> Result<(), String>;
}
impl WireValidate for String {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for bool {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for u64 {
    fn validate(&self) -> Result<(), String> {
        if *self > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for i64 {
    fn validate(&self) -> Result<(), String> {
        if self.unsigned_abs() > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for f64 {
    fn validate(&self) -> Result<(), String> {
        if !self.is_finite() {
            return Err("non-finite number".into());
        }
        Ok(())
    }
}
impl<T: WireValidate> WireValidate for Vec<T> {
    fn validate(&self) -> Result<(), String> {
        for item in self {
            item.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum CalendarKind {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "project_meeting")]
    ProjectMeeting,
    #[serde(rename = "synced")]
    Synced,
}
impl WireValidate for CalendarKind {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarEvent {
    pub(crate) id: String,
    pub(crate) calendar_id: String,
    pub(crate) kind: CalendarKind,
    pub(crate) title: String,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    pub(crate) all_day: bool,
    pub(crate) timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<String>,
    pub(crate) revision: u64,
}
impl WireValidate for CalendarEvent {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarEvent.id violates max_chars".into());
            }
        }
        {
            let value = &self.calendar_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.calendar_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarEvent.calendar_id violates max_chars".into());
            }
        }
        {
            let value = &self.kind;
            value.validate()?;
        }
        {
            let value = &self.title;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.title violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEvent.title violates max_chars".into());
            }
        }
        {
            let value = &self.start_ms;
            value.validate()?;
        }
        {
            let value = &self.end_ms;
            value.validate()?;
        }
        {
            let value = &self.all_day;
            value.validate()?;
        }
        {
            let value = &self.timezone;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.timezone violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarEvent.timezone violates max_chars".into());
            }
        }
        if let Some(value) = &self.location {
            value.validate()?;
            if value.chars().count() > 512 {
                return Err("CalendarEvent.location violates max_chars".into());
            }
        }
        if let Some(value) = &self.notes {
            value.validate()?;
            if value.chars().count() > 4096 {
                return Err("CalendarEvent.notes violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "CalendarKind" => serde_json::from_value::<CalendarKind>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarEvent" => serde_json::from_value::<CalendarEvent>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
