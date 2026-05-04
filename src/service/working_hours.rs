//! Working-hours window snapshot + admission gate.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use chrono::Timelike;
use serde::{Deserialize, Serialize};

use crate::inference::runtime_env;

pub const ENABLED_KEY: &str = "CTOX_WORK_HOURS_ENABLED";
pub const START_KEY: &str = "CTOX_WORK_HOURS_START";
pub const END_KEY: &str = "CTOX_WORK_HOURS_END";
pub const DEFAULT_START: &str = "08:00";
pub const DEFAULT_END: &str = "18:00";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkHoursConfig {
    pub enabled: bool,
    pub start: String,
    pub end: String,
}

impl Default for WorkHoursConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            start: DEFAULT_START.to_string(),
            end: DEFAULT_END.to_string(),
        }
    }
}

/// Snapshot of the working-hours window for diagnostic surfaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkHoursSnapshot {
    pub enabled: bool,
    pub inside_window: bool,
    pub start: String,
    pub end: String,
}

/// Capture the current working-hours window state for the given service root.
pub fn snapshot(root: &Path) -> WorkHoursSnapshot {
    let config = runtime_env::effective_operator_env_map(root)
        .map(|env_map| config_from_map(&env_map))
        .unwrap_or_default();
    WorkHoursSnapshot {
        enabled: config.enabled,
        inside_window: !config.enabled || is_inside_window(&config),
        start: config.start,
        end: config.end,
    }
}

pub fn config_from_map(env_map: &BTreeMap<String, String>) -> WorkHoursConfig {
    WorkHoursConfig {
        enabled: env_map
            .get(ENABLED_KEY)
            .map(|value| parse_enabled(value))
            .unwrap_or(false),
        start: env_map
            .get(START_KEY)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| DEFAULT_START.to_string()),
        end: env_map
            .get(END_KEY)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| DEFAULT_END.to_string()),
    }
}

pub fn validate_config(config: &WorkHoursConfig) -> Result<()> {
    let start = parse_time_minutes(&config.start)?;
    let end = parse_time_minutes(&config.end)?;
    if start == end {
        bail!("work-hours start and end must differ");
    }
    Ok(())
}

/// Whether the work-hours dispatcher should currently lease new jobs.
pub fn accepts_work(root: &Path) -> bool {
    hold_reason(root).is_none()
}

/// If the working-hours policy is currently holding work, return a
/// human-readable reason; otherwise `None`.
pub fn hold_reason(root: &Path) -> Option<String> {
    let snapshot = snapshot(root);
    if snapshot.enabled && !snapshot.inside_window {
        Some(format!(
            "outside configured work hours {}-{}",
            snapshot.start, snapshot.end
        ))
    } else {
        None
    }
}

pub fn handle_work_hours_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("set") => {
            let start = args
                .get(1)
                .ok_or_else(|| anyhow!("usage: ctox work-hours set HH:MM HH:MM"))?
                .trim()
                .to_string();
            let end = args
                .get(2)
                .ok_or_else(|| anyhow!("usage: ctox work-hours set HH:MM HH:MM"))?
                .trim()
                .to_string();
            if args.len() != 3 {
                bail!("usage: ctox work-hours set HH:MM HH:MM");
            }
            let config = WorkHoursConfig {
                enabled: true,
                start,
                end,
            };
            validate_config(&config)?;
            let mut env_map = runtime_env::effective_operator_env_map(root)?;
            env_map.insert(ENABLED_KEY.to_string(), "on".to_string());
            env_map.insert(START_KEY.to_string(), config.start.clone());
            env_map.insert(END_KEY.to_string(), config.end.clone());
            runtime_env::save_runtime_env_map(root, &env_map)?;
            println!("work-hours enabled {}-{}", config.start, config.end);
            Ok(())
        }
        Some("off") => {
            if args.len() != 1 {
                bail!("usage: ctox work-hours off");
            }
            let mut env_map = runtime_env::effective_operator_env_map(root)?;
            env_map.insert(ENABLED_KEY.to_string(), "off".to_string());
            runtime_env::save_runtime_env_map(root, &env_map)?;
            println!("work-hours disabled");
            Ok(())
        }
        Some("status") | None => {
            println!("{}", serde_json::to_string_pretty(&snapshot(root))?);
            Ok(())
        }
        _ => bail!("usage: ctox work-hours set HH:MM HH:MM | off | status"),
    }
}

fn parse_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on" | "enabled"
    )
}

fn is_inside_window(config: &WorkHoursConfig) -> bool {
    match (
        parse_time_minutes(&config.start),
        parse_time_minutes(&config.end),
    ) {
        (Ok(start), Ok(end)) if start != end => {
            let now = chrono::Local::now();
            let current = now.hour() * 60 + now.minute();
            if start < end {
                current >= start && current < end
            } else {
                current >= start || current < end
            }
        }
        _ => true,
    }
}

fn parse_time_minutes(value: &str) -> Result<u32> {
    let (hour, minute) = value
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow!("work-hours time must use HH:MM"))?;
    let hour: u32 = hour
        .parse()
        .map_err(|_| anyhow!("work-hours hour must be numeric"))?;
    let minute: u32 = minute
        .parse()
        .map_err(|_| anyhow!("work-hours minute must be numeric"))?;
    if hour > 23 || minute > 59 {
        bail!("work-hours time must be between 00:00 and 23:59");
    }
    Ok(hour * 60 + minute)
}
