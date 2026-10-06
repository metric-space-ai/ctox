#![forbid(unsafe_code)]

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct OptionSet {
    pub map: HashMap<String, String>,
}

impl OptionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        let mut s = Self::new();
        s.set("split", "5");
        s.set("min-split-size", "20971520");
        s.set("piece-length", "1048576");
        s.set("file-allocation", "trunc");
        s.set("timeout", "60");
        s.set("connect-timeout", "60");
        s.set("max-connection-per-server", "1");
        s.set("http-no-cache", "true");
        s.set("max-concurrent-downloads", "5");
        s.set("enable-http-keep-alive", "true");
        s.set("check-certificate", "true");
        s.set("continue", "false");
        s.set("min-tls-version", "TLSv1.2");
        s.set("ftp-pasv", "true");
        s.set("ftp-type", "binary");
        s.set("ftp-user", "anonymous");
        s.set("ftp-passwd", "ARIA2USER@");
        s.set("user-agent", crate::USER_AGENT);
        s.set("enable-peer-exchange", "true");
        s.set("enable-dht", "true");
        s.set("dht-listen-port", "6881");
        s.set("dht-message-timeout", "10");
        s.set("bt-enable-lpd", "false");
        s.set("seed-ratio", "1.0");
        s.set("follow-torrent", "true");
        s.set("enable-bittorrent", "true");
        s.set("enable-metalink", "true");
        s.set("disk-cache", "16M");
        s.set("realtime-chunk-checksum", "true");
        s.set("enable-color", "true");
        s.set("max-outstanding-request", "16");
        s.set("uri-selector", "feedback");
        s
    }

    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        self.map.insert(k.to_string(), v.into());
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(|s| s.as_str())
    }

    pub fn bool(&self, k: &str, default: bool) -> bool {
        match self.get(k) {
            Some("true" | "1" | "yes") => true,
            Some("false" | "0" | "no") => false,
            _ => default,
        }
    }

    pub fn u64(&self, k: &str, default: u64) -> u64 {
        self.get(k).and_then(|s| s.parse().ok()).unwrap_or(default)
    }

    pub fn usize(&self, k: &str, default: usize) -> usize {
        self.get(k).and_then(|s| s.parse().ok()).unwrap_or(default)
    }

    pub fn split(&self) -> usize {
        self.usize("split", 5).max(1)
    }

    pub fn min_split_size(&self) -> u64 {
        self.u64("min-split-size", 20 * 1024 * 1024)
    }

    pub fn piece_length(&self) -> u32 {
        self.u64("piece-length", 1024 * 1024).max(1) as u32
    }

    pub fn dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.get("dir").unwrap_or("."))
    }

    pub fn out(&self) -> Option<&str> {
        self.get("out").filter(|s| !s.is_empty())
    }

    pub fn user_agent(&self) -> &str {
        self.get("user-agent").unwrap_or(crate::USER_AGENT)
    }

    pub fn merge(&mut self, other: &OptionSet) {
        for (k, v) in &other.map {
            self.map.insert(k.clone(), v.clone());
        }
    }

    /// C++ aria2.conf: `key=value` lines, `#` comments, optional `--` prefix.
    pub fn apply_conf_text(&mut self, text: &str) {
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("--").unwrap_or(line);
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                if !k.is_empty() {
                    self.set(k, v.trim());
                }
            }
        }
    }

    pub fn apply_conf_file(&mut self, path: &std::path::Path) -> crate::Result<()> {
        let text = std::fs::read_to_string(path)?;
        self.apply_conf_text(&text);
        Ok(())
    }
}

pub fn default_conf_path() -> Option<std::path::PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            let p = std::path::PathBuf::from(xdg).join("aria2").join("aria2.conf");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::PathBuf::from(home).join(".aria2").join("aria2.conf");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// C++ `--no-conf` skips load. `--conf-path` is required if set; else default aria2.conf if present.
pub fn load_conf(opts: &mut OptionSet, conf_path: Option<&str>, no_conf: bool) -> crate::Result<()> {
    if no_conf {
        return Ok(());
    }
    if let Some(p) = conf_path.filter(|s| !s.is_empty()) {
        return opts.apply_conf_file(std::path::Path::new(p));
    }
    if let Some(p) = default_conf_path() {
        opts.apply_conf_file(&p)?;
    }
    Ok(())
}

/// C++ `--optimize-concurrent-downloads[=true|false|A:B]`. `None` = disabled.
/// Default coefficients when true: A=5, B=25. `N = A + B * log10(speed Mbps)`.
pub fn parse_optimize_concurrent(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    match s {
        "" | "false" | "0" | "no" => None,
        "true" | "1" | "yes" => Some((5.0, 25.0)),
        _ => {
            let (a, b) = s.split_once(':')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        }
    }
}

/// C++ RequestGroupMan: clamp N to `[1, max-concurrent-downloads]`.
pub fn optimize_concurrent_n(a: f64, b: f64, speed_bps: u64, cap: usize) -> usize {
    let cap = cap.max(1);
    let n = if speed_bps == 0 {
        a
    } else {
        let mbps = speed_bps as f64 * 8.0 / 1_000_000.0;
        if mbps <= 0.0 {
            a
        } else {
            (a + b * mbps.log10()).ceil()
        }
    };
    (n as i64).clamp(1, cap as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_conf_comments_and_dashes() {
        let mut o = OptionSet::new();
        o.apply_conf_text("# hi\n dir = /tmp/x \n--out=from.conf\n\n");
        assert_eq!(o.get("dir"), Some("/tmp/x"));
        assert_eq!(o.get("out"), Some("from.conf"));
    }

    #[test]
    fn optimize_concurrent_true_and_ab() {
        assert_eq!(parse_optimize_concurrent("false"), None);
        assert_eq!(parse_optimize_concurrent("true"), Some((5.0, 25.0)));
        assert_eq!(parse_optimize_concurrent("1:0"), Some((1.0, 0.0)));
        assert_eq!(optimize_concurrent_n(1.0, 0.0, 0, 5), 1);
        assert_eq!(optimize_concurrent_n(1.0, 0.0, 1_000_000, 5), 1);
        assert_eq!(optimize_concurrent_n(5.0, 25.0, 0, 5), 5);
        assert_eq!(optimize_concurrent_n(5.0, 25.0, 125_000, 50), 5); // 1 Mbps
        assert_eq!(optimize_concurrent_n(5.0, 25.0, 12_500_000, 50), 50); // 100 Mbps clamp
    }
}
