//! C++ `--log` / `--log-level` file logger.
#![forbid(unsafe_code)]

use crate::options::OptionSet;
use std::io::Write;

pub fn write(opts: &OptionSet, level: &str, msg: &str) {
    let Some(path) = opts.get("log").filter(|s| !s.is_empty() && *s != "-") else {
        return;
    };
    let min = opts.get("log-level").unwrap_or("debug");
    if rank(level) < rank(min) {
        return;
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let _ = writeln!(f, "[{level}] {msg}");
}

/// C++ console logger (`--quiet`, `--console-log-level`, `--stderr`). Independent of `--log`.
pub fn write_console(
    opts: &OptionSet,
    stdout: &std::sync::Mutex<String>,
    stderr: &std::sync::Mutex<String>,
    level: &str,
    msg: &str,
) {
    if opts.bool("quiet", false) {
        return;
    }
    let min = opts.get("console-log-level").unwrap_or("notice");
    if rank(level) < rank(min) {
        return;
    }
    let body = format!("[{level}] {msg}");
    let body = colorize(opts, level, &body);
    let line = format!("{body}\n");
    let sink = if opts.bool("stderr", false) {
        stderr
    } else {
        stdout
    };
    if let Ok(mut s) = sink.lock() {
        s.push_str(&line);
    }
}

/// C++ `--enable-color` (default true): ANSI SGR around console log lines.
pub fn colorize(opts: &OptionSet, level: &str, body: &str) -> String {
    if !opts.bool("enable-color", true) {
        return body.to_string();
    }
    let code = match level.to_ascii_uppercase().as_str() {
        "DEBUG" => "1;37",
        "INFO" => "1;32",
        "NOTICE" => "1;36",
        "WARN" | "WARNING" => "1;33",
        "ERROR" => "1;31",
        _ => return body.to_string(),
    };
    format!("\x1b[{code}m{body}\x1b[0m")
}

/// C++ `util::abbrevSize` (`--human-readable`). Units B/Ki/Mi/Gi/Ti.
pub fn abbrev_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "Ki", "Mi", "Gi", "Ti"];
    let mut v = n as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n}B")
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

/// C++ console SIZE: abbrev when `--human-readable` (default true), else raw bytes.
pub fn format_length(opts: &OptionSet, n: u64) -> String {
    if opts.bool("human-readable", true) {
        abbrev_size(n)
    } else {
        n.to_string()
    }
}

/// C++ `--truncate-console-readout` (default true): clip readout to 80-col fallback width.
pub const READOUT_WIDTH: usize = 80;

pub fn truncate_readout(opts: &OptionSet, msg: &str) -> String {
    if !opts.bool("truncate-console-readout", true) {
        return msg.to_string();
    }
    let mut s = msg.to_string();
    if s.len() > READOUT_WIDTH {
        s.truncate(READOUT_WIDTH);
    }
    s
}

fn rank(level: &str) -> u8 {
    match level.to_ascii_lowercase().as_str() {
        "debug" => 0,
        "info" => 1,
        "notice" => 2,
        "warn" | "warning" => 3,
        "error" => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_order() {
        assert!(rank("debug") < rank("info"));
        assert!(rank("info") < rank("error"));
    }

    #[test]
    fn abbrev_size_units() {
        assert_eq!(abbrev_size(20), "20B");
        assert_eq!(abbrev_size(2048), "2.0Ki");
        assert_eq!(abbrev_size(1_048_576), "1.0Mi");
    }

    #[test]
    fn truncate_readout_clips_at_80() {
        let mut on = crate::options::OptionSet::new();
        on.set("truncate-console-readout", "true");
        let long = "S".repeat(100);
        let clipped = truncate_readout(&on, &long);
        assert_eq!(clipped.len(), READOUT_WIDTH);
        let mut off = crate::options::OptionSet::new();
        off.set("truncate-console-readout", "false");
        assert_eq!(truncate_readout(&off, &long).len(), 100);
    }

    #[test]
    fn enable_color_wraps_notice() {
        let mut on = crate::options::OptionSet::new();
        on.set("enable-color", "true");
        let s = colorize(&on, "NOTICE", "[NOTICE] hi");
        assert!(s.starts_with("\x1b[1;36m"), "{s:?}");
        assert!(s.ends_with("\x1b[0m"), "{s:?}");
        assert!(s.contains("[NOTICE] hi"), "{s:?}");
        let mut off = crate::options::OptionSet::new();
        off.set("enable-color", "false");
        assert_eq!(colorize(&off, "NOTICE", "[NOTICE] hi"), "[NOTICE] hi");
    }
}
