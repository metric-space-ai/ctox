//! Working-hours window snapshot + admission gate.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use chrono::Timelike;
use serde::{Deserialize, Serialize};

use crate::inference::runtime_env;

pub const ENABLED_KEY: &str = "CTOX_WORK_HOURS_ENABLED";
pub const START_KEY: &str = "CTOX_WORK_HOURS_START";
pub const END_KEY: &str = "CTOX_WORK_HOURS_END";
pub const DEFAULT_START: &str = "08:00";
pub const DEFAULT_END: &str = "18:00";
const CONFIG_CACHE_TTL_SECS: u64 = 60;

type RuntimeConfigStamp = (u64, u128);

#[derive(Debug, Clone)]
struct CachedConfig {
    generated_at: Instant,
    stamp: RuntimeConfigStamp,
    config: WorkHoursConfig,
}

fn config_cache() -> &'static Mutex<HashMap<PathBuf, CachedConfig>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn root_key_cache() -> &'static Mutex<HashMap<PathBuf, PathBuf>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, PathBuf>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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
    let config = load_config_cached(root);
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

fn load_config_cached(root: &Path) -> WorkHoursConfig {
    let key = work_hours_cache_key(root);
    let db_path = runtime_env::runtime_config_path(root);
    let stamp = runtime_config_stamp(&db_path);
    let now = Instant::now();
    {
        let cache = config_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.get(&key).filter(|entry| {
            entry.stamp == stamp
                && now.duration_since(entry.generated_at)
                    < Duration::from_secs(CONFIG_CACHE_TTL_SECS)
        }) {
            return entry.config.clone();
        }
    }

    let config = runtime_env::load_persisted_runtime_env_map_cached(root)
        .map(|env_map| config_from_map(&env_map))
        .unwrap_or_default();
    config_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            key,
            CachedConfig {
                generated_at: now,
                stamp,
                config: config.clone(),
            },
        );
    config
}

fn invalidate_config_cache(root: &Path) {
    config_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&work_hours_cache_key(root));
}

fn work_hours_cache_key(root: &Path) -> PathBuf {
    if !root.is_absolute() {
        return fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    }
    let raw = root.to_path_buf();
    {
        let cache = root_key_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.get(&raw) {
            return cached.clone();
        }
    }
    let key = fs::canonicalize(root).unwrap_or_else(|_| raw.clone());
    root_key_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(raw, key.clone());
    key
}

fn runtime_config_stamp(path: &Path) -> RuntimeConfigStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return (0, 0);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (metadata.len(), modified_at)
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
            let mut env_map = runtime_env::load_persisted_runtime_env_map(root)?;
            env_map.insert(ENABLED_KEY.to_string(), "on".to_string());
            env_map.insert(START_KEY.to_string(), config.start.clone());
            env_map.insert(END_KEY.to_string(), config.end.clone());
            runtime_env::save_runtime_env_map(root, &env_map)?;
            invalidate_config_cache(root);
            println!("work-hours enabled {}-{}", config.start, config.end);
            Ok(())
        }
        Some("off") => {
            if args.len() != 1 {
                bail!("usage: ctox work-hours off");
            }
            let mut env_map = runtime_env::load_persisted_runtime_env_map(root)?;
            env_map.insert(ENABLED_KEY.to_string(), "off".to_string());
            runtime_env::save_runtime_env_map(root, &env_map)?;
            invalidate_config_cache(root);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_does_not_touch_secret_store() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut env_map = BTreeMap::new();
        env_map.insert(ENABLED_KEY.to_string(), "on".to_string());
        env_map.insert(START_KEY.to_string(), "00:00".to_string());
        env_map.insert(END_KEY.to_string(), "23:59".to_string());
        runtime_env::save_runtime_env_map(temp.path(), &env_map)?;

        let secret_store_path = crate::secrets::secret_store_path(temp.path());
        assert!(!secret_store_path.exists());

        let snapshot = snapshot(temp.path());

        assert!(snapshot.enabled);
        assert_eq!(snapshot.start, "00:00");
        assert_eq!(snapshot.end, "23:59");
        assert!(
            !secret_store_path.exists(),
            "working-hours snapshot must not load or initialize the secret store"
        );
        Ok(())
    }

    #[test]
    fn work_hours_cache_key_reuses_absolute_root_resolution() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().to_path_buf();
        assert!(root.is_absolute());

        let first = work_hours_cache_key(&root);
        let second = work_hours_cache_key(&root);

        assert_eq!(first, second);
        assert_eq!(first, fs::canonicalize(&root)?);
        Ok(())
    }
}
