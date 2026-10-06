//! C++ `--server-stat-if` / `--server-stat-of` / `--uri-selector` / `--server-stat-timeout`.
#![forbid(unsafe_code)]

use crate::options::OptionSet;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerStat {
    pub host: String,
    pub protocol: String,
    pub dl_speed: u64,
    pub last_updated: u64,
    pub status: StatStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatStatus {
    Ok,
    Error,
}

impl StatStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Error => "ERROR",
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn parse_server_stat(text: &str) -> Vec<ServerStat> {
    let mut out = Vec::new();
    let now = unix_now();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut host = String::new();
        let mut protocol = String::from("http");
        let mut dl_speed = 0u64;
        let mut last_updated = now;
        let mut saw_updated = false;
        let mut status = StatStatus::Ok;
        for part in line.split(',') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k.trim() {
                "host" => host = v.trim().to_string(),
                "protocol" => protocol = v.trim().to_string(),
                "dl_speed" => dl_speed = v.trim().parse().unwrap_or(0),
                "last_updated" => {
                    last_updated = v.trim().parse().unwrap_or(now);
                    saw_updated = true;
                }
                "status" => {
                    status = if v.trim().eq_ignore_ascii_case("ERROR") {
                        StatStatus::Error
                    } else {
                        StatStatus::Ok
                    };
                }
                _ => {}
            }
        }
        if !saw_updated {
            last_updated = now;
        }
        if !host.is_empty() {
            out.push(ServerStat {
                host,
                protocol,
                dl_speed,
                last_updated,
                status,
            });
        }
    }
    out
}

pub fn format_server_stat(stats: &[ServerStat]) -> String {
    let mut s = String::new();
    for st in stats {
        s.push_str(&format!(
            "host={}, protocol={}, dl_speed={}, ul_speed=0, last_updated={}, counter=0, status={}\n",
            st.host,
            st.protocol,
            st.dl_speed,
            st.last_updated,
            st.status.as_str()
        ));
    }
    s
}

pub fn load(path: &Path) -> Vec<ServerStat> {
    std::fs::read_to_string(path)
        .ok()
        .map(|t| parse_server_stat(&t))
        .unwrap_or_default()
}

pub fn save(path: &Path, stats: &[ServerStat]) -> std::io::Result<()> {
    std::fs::write(path, format_server_stat(stats))
}

pub fn load_from_opts(opts: &OptionSet) -> Vec<ServerStat> {
    opts.get("server-stat-if")
        .filter(|s| !s.is_empty())
        .map(|p| load(Path::new(p)))
        .unwrap_or_default()
}

pub fn persist(opts: &OptionSet, stats: &[ServerStat]) {
    if let Some(p) = opts.get("server-stat-of").filter(|s| !s.is_empty()) {
        let _ = save(Path::new(p), stats);
    }
}

pub fn timeout_secs(opts: &OptionSet) -> u64 {
    opts.u64("server-stat-timeout", 86400)
}

static HOST_USE: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();

fn host_use_map() -> &'static Mutex<HashMap<String, u32>> {
    HOST_USE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn host_uses(host: &str) -> u32 {
    host_use_map()
        .lock()
        .ok()
        .and_then(|m| m.get(host).copied())
        .unwrap_or(0)
}

pub struct HostUseGuard {
    host: String,
}

impl Drop for HostUseGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = host_use_map().lock() {
            if let Some(n) = m.get_mut(&self.host) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    m.remove(&self.host);
                }
            }
        }
    }
}

pub fn acquire_host(host: &str) -> HostUseGuard {
    if let Ok(mut m) = host_use_map().lock() {
        *m.entry(host.to_string()).or_insert(0) += 1;
    }
    HostUseGuard {
        host: host.to_string(),
    }
}

pub fn host_proto(uri: &str) -> (String, String) {
    match url::Url::parse(uri) {
        Ok(u) => (
            u.host_str().unwrap_or("").to_string(),
            u.scheme().to_string(),
        ),
        Err(_) => (String::new(), String::from("http")),
    }
}

fn fresh<'a>(st: Option<&'a ServerStat>, now: u64, timeout: u64) -> Option<&'a ServerStat> {
    let s = st?;
    if timeout > 0 && s.last_updated > 0 && now.saturating_sub(s.last_updated) > timeout {
        return None;
    }
    Some(s)
}

fn score(st: Option<&ServerStat>) -> i64 {
    match st {
        Some(s) if s.status == StatStatus::Error => -1,
        Some(s) => s.dl_speed as i64,
        None => 0,
    }
}

/// C++ AdaptiveURISelector: probe untested first, then highest OK speed, ERROR last.
fn adaptive_tier(st: Option<&ServerStat>) -> u8 {
    match st {
        None => 0,
        Some(s) if s.status == StatStatus::Error => 2,
        Some(_) => 1,
    }
}

pub fn rank_uris(
    uris: &[String],
    selector: &str,
    stats: &[ServerStat],
    timeout: u64,
    least_used: bool,
) -> Vec<String> {
    let now = unix_now();
    let mut indexed: Vec<(usize, String)> =
        uris.iter().cloned().enumerate().collect();
    indexed.sort_by(|a, b| {
        let (ha, pa) = host_proto(&a.1);
        let (hb, pb) = host_proto(&b.1);
        let host_ord = if least_used {
            host_uses(&ha).cmp(&host_uses(&hb))
        } else {
            std::cmp::Ordering::Equal
        };
        let sel = match selector {
            "feedback" => {
                let sa = score(fresh(lookup(stats, &ha, &pa), now, timeout));
                let sb = score(fresh(lookup(stats, &hb, &pb), now, timeout));
                sb.cmp(&sa)
            }
            "adaptive" => {
                let ta = adaptive_tier(fresh(lookup(stats, &ha, &pa), now, timeout));
                let tb = adaptive_tier(fresh(lookup(stats, &hb, &pb), now, timeout));
                ta.cmp(&tb).then_with(|| {
                    if ta == 1 && tb == 1 {
                        let sa = score(fresh(lookup(stats, &ha, &pa), now, timeout));
                        let sb = score(fresh(lookup(stats, &hb, &pb), now, timeout));
                        sb.cmp(&sa)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
            }
            _ => std::cmp::Ordering::Equal,
        };
        host_ord.then(sel).then(a.0.cmp(&b.0))
    });
    indexed.into_iter().map(|(_, u)| u).collect()
}

pub fn lookup<'a>(stats: &'a [ServerStat], host: &str, proto: &str) -> Option<&'a ServerStat> {
    stats
        .iter()
        .find(|s| s.host == host && s.protocol == proto)
        .or_else(|| stats.iter().find(|s| s.host == host))
}

pub fn upsert(stats: &mut Vec<ServerStat>, host: &str, proto: &str, status: StatStatus, dl_speed: u64) {
    let now = unix_now();
    if let Some(s) = stats
        .iter_mut()
        .find(|s| s.host == host && s.protocol == proto)
    {
        s.status = status;
        s.last_updated = now;
        if dl_speed > 0 {
            s.dl_speed = dl_speed;
        }
        return;
    }
    stats.push(ServerStat {
        host: host.to_string(),
        protocol: proto.to_string(),
        dl_speed,
        last_updated: now,
        status,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_ranks_ok_host_first() {
        let stats = parse_server_stat(
            "host=bad, protocol=http, dl_speed=0, status=ERROR\nhost=good, protocol=http, dl_speed=9000, status=OK\n",
        );
        let uris = vec![
            "http://bad:1/a.bin".into(),
            "http://good:1/b.bin".into(),
        ];
        let ranked = rank_uris(&uris, "feedback", &stats, 86400, false);
        assert_eq!(ranked[0], "http://good:1/b.bin");
        assert_eq!(ranked[1], "http://bad:1/a.bin");
    }

    #[test]
    fn timeout_ignores_stale_error() {
        let now = unix_now();
        let stats = parse_server_stat(&format!(
            "host=stale, protocol=http, dl_speed=0, last_updated=1, status=ERROR\nhost=fresh, protocol=http, dl_speed=0, last_updated={now}, status=ERROR\n"
        ));
        let uris = vec![
            "http://fresh:1/a.bin".into(),
            "http://stale:1/b.bin".into(),
        ];
        let ranked = rank_uris(&uris, "feedback", &stats, 60, false);
        assert_eq!(ranked[0], "http://stale:1/b.bin");
        assert_eq!(ranked[1], "http://fresh:1/a.bin");
    }

    #[test]
    fn adaptive_probes_untested_before_known_ok() {
        let stats = parse_server_stat(
            "host=known, protocol=http, dl_speed=9000, status=OK\nhost=bad, protocol=http, dl_speed=0, status=ERROR\n",
        );
        let uris = vec![
            "http://known:1/a.bin".into(),
            "http://fresh:1/b.bin".into(),
            "http://bad:1/c.bin".into(),
        ];
        let ranked = rank_uris(&uris, "adaptive", &stats, 86400, false);
        assert_eq!(ranked[0], "http://fresh:1/b.bin");
        assert_eq!(ranked[1], "http://known:1/a.bin");
        assert_eq!(ranked[2], "http://bad:1/c.bin");
        let fb = rank_uris(&uris, "feedback", &stats, 86400, false);
        assert_eq!(fb[0], "http://known:1/a.bin");
    }

    #[test]
    fn least_used_host_first() {
        let _g = acquire_host("busy");
        let uris = vec![
            "http://busy:1/a.bin".into(),
            "http://free:1/b.bin".into(),
        ];
        let ranked = rank_uris(&uris, "inorder", &[], 86400, true);
        assert_eq!(ranked[0], "http://free:1/b.bin");
        assert_eq!(ranked[1], "http://busy:1/a.bin");
    }
}
