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
    #[serde(rename = "project_session")]
    ProjectSession,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) external_id: Option<String>,
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
        if let Some(value) = &self.project_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.project_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEvent.project_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.session_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.session_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEvent.session_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.account_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.account_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEvent.account_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.external_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEvent.external_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEvent.external_id violates max_chars".into());
            }
        }
        if self.end_ms <= self.start_ms {
            return Err("CalendarEvent.end_ms must follow start_ms".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarEventUpdate {
    pub(crate) expected_revision: u64,
    pub(crate) event: CalendarEvent,
}
impl WireValidate for CalendarEventUpdate {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.event;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarEventDelete {
    pub(crate) id: String,
    pub(crate) expected_revision: u64,
}
impl WireValidate for CalendarEventDelete {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEventDelete.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarEventDelete.id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarAccountsReadRequest {
    pub(crate) request_id: String,
}
impl WireValidate for CalendarAccountsReadRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.request_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarAccountsReadRequest.request_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarAccountsReadRequest.request_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarEventsReadRequest {
    pub(crate) request_id: String,
    pub(crate) account_id: String,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
}
impl WireValidate for CalendarEventsReadRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.request_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEventsReadRequest.request_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarEventsReadRequest.request_id violates max_chars".into());
            }
        }
        {
            let value = &self.account_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarEventsReadRequest.account_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarEventsReadRequest.account_id violates max_chars".into());
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
        if self.end_ms <= self.start_ms {
            return Err("CalendarEventsReadRequest.end_ms must follow start_ms".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarAccount {
    pub(crate) id: String,
    pub(crate) calendar_id: String,
    pub(crate) label: String,
    pub(crate) supported: bool,
}
impl WireValidate for CalendarAccount {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarAccount.id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarAccount.id violates max_chars".into());
            }
        }
        {
            let value = &self.calendar_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarAccount.calendar_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("CalendarAccount.calendar_id violates max_chars".into());
            }
        }
        {
            let value = &self.label;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("CalendarAccount.label violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("CalendarAccount.label violates max_chars".into());
            }
        }
        {
            let value = &self.supported;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarAccountsPage {
    pub(crate) ok: bool,
    pub(crate) truncated: bool,
    pub(crate) accounts: Vec<CalendarAccount>,
}
impl WireValidate for CalendarAccountsPage {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.ok;
            value.validate()?;
        }
        {
            let value = &self.truncated;
            value.validate()?;
        }
        {
            let value = &self.accounts;
            value.validate()?;
            if value.len() > 100 {
                return Err("CalendarAccountsPage.accounts violates max_items".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarEventsPage {
    pub(crate) ok: bool,
    pub(crate) truncated: bool,
    pub(crate) synced_at_ms: u64,
    pub(crate) events: Vec<CalendarEvent>,
}
impl WireValidate for CalendarEventsPage {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.ok;
            value.validate()?;
        }
        {
            let value = &self.truncated;
            value.validate()?;
        }
        {
            let value = &self.synced_at_ms;
            value.validate()?;
        }
        {
            let value = &self.events;
            value.validate()?;
            if value.len() > 100 {
                return Err("CalendarEventsPage.events violates max_items".into());
            }
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
        "CalendarEventUpdate" => serde_json::from_value::<CalendarEventUpdate>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarEventDelete" => serde_json::from_value::<CalendarEventDelete>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarAccountsReadRequest" => {
            serde_json::from_value::<CalendarAccountsReadRequest>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "CalendarEventsReadRequest" => serde_json::from_value::<CalendarEventsReadRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarAccount" => serde_json::from_value::<CalendarAccount>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarAccountsPage" => serde_json::from_value::<CalendarAccountsPage>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "CalendarEventsPage" => serde_json::from_value::<CalendarEventsPage>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}
