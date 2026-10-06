// ref: internal/logging/global_logger.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::log_dir_cleaner::{LogDirCleaner, LogFilesystem};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const FIELD_ORDER: &[&str] = &[
    "provider",
    "model",
    "plugin_id",
    "plugin_name",
    "source_id",
    "version",
    "active_version",
    "retired_version",
    "overwritten",
    "mode",
    "budget",
    "level",
    "baseline_level",
    "original_mode",
    "original_value",
    "min",
    "max",
    "clamped_to",
    "error",
    "credential",
    "connection",
    "proxy_scheme",
    "remote_transport",
    "media_session_id",
    "call_id",
    "peer",
    "state",
    "reason",
];
const QUOTED_FIELDS: &[&str] = &[
    "credential",
    "connection",
    "proxy_scheme",
    "remote_transport",
    "media_session_id",
    "call_id",
    "peer",
    "state",
    "reason",
];
const PLUGIN_PATH_FIELDS: &[&str] = &["path", "active_path", "retired_path"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub source: Option<(PathBuf, u32)>,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: impl Into<String>, timestamp: SystemTime) -> Self {
        Self {
            timestamp,
            level,
            message: message.into(),
            fields: BTreeMap::new(),
            source: None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LogFormatter;

impl LogFormatter {
    pub fn format(&self, entry: &LogEntry) -> String {
        // The timestamp is caller-provided; UTC rendering avoids process-local
        // timezone authority and chrono's optional clock feature.
        let timestamp: DateTime<Utc> = DateTime::<Utc>::from(entry.timestamp);
        let request_id = entry
            .fields
            .get("request_id")
            .filter(|value| !value.is_empty())
            .map(String::as_str)
            .unwrap_or("--------");
        let level = format!("{:<5}", entry.level);
        let message = entry.message.trim_end_matches(['\r', '\n']);
        let mut fields = Vec::new();
        for key in FIELD_ORDER {
            if let Some(value) = entry.fields.get(*key) {
                fields.push(format!("{key}={}", format_field_value(key, value)));
            }
        }
        if entry
            .fields
            .get("plugin_id")
            .is_some_and(|value| !value.trim().is_empty())
        {
            for key in PLUGIN_PATH_FIELDS {
                if let Some(value) = entry.fields.get(*key) {
                    fields.push(format!("{key}={value}"));
                }
            }
        }
        let suffix = if fields.is_empty() {
            String::new()
        } else {
            format!(" {}", fields.join(" "))
        };
        let stamp = timestamp.format("%Y-%m-%d %H:%M:%S");
        match &entry.source {
            Some((file, line)) => format!(
                "[{stamp}] [{request_id}] [{level}] [{}:{line}] {message}{suffix}\n",
                file.file_name().unwrap_or_default().to_string_lossy()
            ),
            None => format!("[{stamp}] [{request_id}] [{level}] {message}{suffix}\n"),
        }
    }
}

fn format_field_value(key: &str, value: &str) -> String {
    if QUOTED_FIELDS.contains(&key) {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
    } else {
        value.to_owned()
    }
}

pub trait LogSink: Send + Sync {
    fn write(&self, bytes: &[u8]) -> io::Result<()>;
    fn close(&self) -> io::Result<()> {
        Ok(())
    }
}

pub trait RotationFilesystem: Send + Sync {
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn Write + Send>>;
    fn len(&self, path: &Path) -> io::Result<u64>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct NativeRotationFilesystem;

impl RotationFilesystem for NativeRotationFilesystem {
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Box::new(
            OpenOptions::new().create(true).append(true).open(path)?,
        ))
    }

    fn len(&self, path: &Path) -> io::Result<u64> {
        match fs::metadata(path) {
            Ok(metadata) => Ok(metadata.len()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(error),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

struct RotatingState {
    writer: Option<Box<dyn Write + Send>>,
    size: u64,
}

/// Size-based file sink with bounded numbered backups. All filesystem authority
/// is supplied by the owner, making rollover deterministic in tests and hosts.
pub struct RotatingFileSink {
    path: PathBuf,
    max_bytes: u64,
    max_backups: usize,
    filesystem: Arc<dyn RotationFilesystem>,
    state: Mutex<RotatingState>,
}

impl RotatingFileSink {
    pub fn new(
        path: PathBuf,
        max_bytes: u64,
        max_backups: usize,
        filesystem: Arc<dyn RotationFilesystem>,
    ) -> io::Result<Self> {
        let size = filesystem.len(&path)?;
        let writer = filesystem.open_append(&path)?;
        Ok(Self {
            path,
            max_bytes,
            max_backups,
            filesystem,
            state: Mutex::new(RotatingState {
                writer: Some(writer),
                size,
            }),
        })
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        PathBuf::from(format!("{}.{}", self.path.display(), index))
    }

    fn rotate(&self, state: &mut RotatingState) -> io::Result<()> {
        state.writer.take();
        if self.max_backups == 0 {
            self.filesystem.remove_file(&self.path)?;
        } else {
            self.filesystem
                .remove_file(&self.backup_path(self.max_backups))?;
            for index in (1..self.max_backups).rev() {
                let from = self.backup_path(index);
                if self.filesystem.len(&from)? > 0 {
                    self.filesystem
                        .rename(&from, &self.backup_path(index + 1))?;
                }
            }
            if self.filesystem.len(&self.path)? > 0 {
                self.filesystem.rename(&self.path, &self.backup_path(1))?;
            }
        }
        state.writer = Some(self.filesystem.open_append(&self.path)?);
        state.size = 0;
        Ok(())
    }
}

impl LogSink for RotatingFileSink {
    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if self.max_bytes > 0
            && state.size > 0
            && state.size.saturating_add(bytes.len() as u64) > self.max_bytes
        {
            self.rotate(&mut state)?;
        }
        let writer = state
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("rotating log writer is closed"))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        state.size = state.size.saturating_add(bytes.len() as u64);
        Ok(())
    }

    fn close(&self) -> io::Result<()> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .writer
            .take();
        Ok(())
    }
}

pub struct WriterLogSink<W: Write + Send> {
    writer: Mutex<W>,
}

impl<W: Write + Send> WriterLogSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }
}

impl<W: Write + Send> LogSink for WriterLogSink<W> {
    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        writer.write_all(bytes)?;
        writer.flush()
    }
}

pub struct LogOutputController {
    formatter: LogFormatter,
    sink: Arc<dyn LogSink>,
    cleaner: Option<LogDirCleaner>,
}

impl LogOutputController {
    pub fn new(sink: Arc<dyn LogSink>) -> Self {
        Self {
            formatter: LogFormatter,
            sink,
            cleaner: None,
        }
    }

    pub fn with_cleaner(
        mut self,
        filesystem: Arc<dyn LogFilesystem>,
        log_dir: PathBuf,
        max_bytes: u64,
        protected_path: Option<PathBuf>,
    ) -> Self {
        self.cleaner = LogDirCleaner::start(
            filesystem,
            log_dir,
            max_bytes,
            protected_path,
            Duration::from_secs(60),
        );
        self
    }

    pub fn log(&self, entry: &LogEntry) -> io::Result<()> {
        self.sink.write(self.formatter.format(entry).as_bytes())
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(mut cleaner) = self.cleaner.take() {
            cleaner.stop();
        }
        self.sink.close()
    }
}

impl Drop for LogOutputController {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn resolve_log_directory(
    configured_writable_base: Option<&Path>,
    auth_directory: Option<&Path>,
    cwd_logs_writable: bool,
) -> PathBuf {
    if let Some(base) = configured_writable_base {
        return base.join("logs");
    }
    if cwd_logs_writable {
        return PathBuf::from("logs");
    }
    auth_directory
        .map(|path| path.join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}
