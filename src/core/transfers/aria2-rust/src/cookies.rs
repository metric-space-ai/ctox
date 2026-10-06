//! Netscape cookie file (`--load-cookies` / `--save-cookies`).
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use std::path::Path;
use url::Url;

#[derive(Clone, Debug)]
pub struct Cookie {
    pub domain: String,
    pub include_sub: bool,
    pub path: String,
    pub secure: bool,
    pub expires: i64,
    pub name: String,
    pub value: String,
}

pub fn load_netscape(path: &Path) -> Result<Vec<Cookie>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Http(format!("load-cookies: {e}")))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(c) = parse_netscape_line(line) {
            out.push(c);
        }
    }
    Ok(out)
}

fn parse_netscape_line(line: &str) -> Option<Cookie> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 7 {
        return None;
    }
    let domain = cols[0].trim();
    if domain.is_empty() {
        return None;
    }
    let include_sub = domain.starts_with('.') || cols[1].eq_ignore_ascii_case("TRUE");
    let path = if cols[2].is_empty() { "/" } else { cols[2] };
    let secure = cols[3].eq_ignore_ascii_case("TRUE");
    let expires: i64 = cols[4].parse().unwrap_or(0);
    let name = cols[5].trim();
    let value = cols[6].trim();
    if name.is_empty() {
        return None;
    }
    Some(Cookie {
        domain: domain.trim_start_matches('.').to_string(),
        include_sub,
        path: path.to_string(),
        secure,
        expires,
        name: name.to_string(),
        value: value.to_string(),
    })
}

pub fn header_for(cookies: &[Cookie], uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let path = if url.path().is_empty() { "/" } else { url.path() };
    let https = url.scheme() == "https";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut parts = Vec::new();
    for c in cookies {
        if c.expires > 0 && c.expires < now {
            continue;
        }
        if c.secure && !https {
            continue;
        }
        if !domain_matches(&c.domain, c.include_sub, &host) {
            continue;
        }
        if !path.starts_with(&c.path) {
            continue;
        }
        parts.push(format!("{}={}", c.name, c.value));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn domain_matches(cookie_domain: &str, include_sub: bool, host: &str) -> bool {
    let d = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    if host == d {
        return true;
    }
    include_sub && host.ends_with(&format!(".{d}"))
}

pub fn parse_set_cookie(raw: &str, default_host: &str, default_path: &str) -> Option<Cookie> {
    let mut segs = raw.split(';');
    let nv = segs.next()?.trim();
    let (name, value) = nv.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut domain = default_host.trim_start_matches('.').to_string();
    let mut path = default_path.to_string();
    if path.is_empty() {
        path = "/".into();
    }
    let mut secure = false;
    let mut expires = 0i64;
    let mut include_sub = false;
    for s in segs {
        let s = s.trim();
        if s.eq_ignore_ascii_case("secure") {
            secure = true;
            continue;
        }
        if let Some((k, v)) = s.split_once('=') {
            match k.trim().to_ascii_lowercase().as_str() {
                "domain" => {
                    domain = v.trim().trim_start_matches('.').to_string();
                    include_sub = true;
                }
                "path" => path = v.trim().to_string(),
                "max-age" => {
                    if let Ok(n) = v.trim().parse::<i64>() {
                        expires = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0)
                            + n;
                    }
                }
                _ => {}
            }
        }
    }
    Some(Cookie {
        domain,
        include_sub,
        path,
        secure,
        expires,
        name: name.to_string(),
        value: value.trim().to_string(),
    })
}

pub fn save_netscape(path: &Path, cookies: &[Cookie]) -> Result<()> {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        let flag = if c.include_sub { "TRUE" } else { "FALSE" };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            c.domain, flag, c.path, secure, c.expires, c.name, c.value
        ));
    }
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    std::fs::write(path, out).map_err(|e| Error::Http(format!("save-cookies: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netscape_roundtrip_header() {
        let line = "127.0.0.1\tFALSE\t/\tFALSE\t0\tsession\ts3cret";
        let c = parse_netscape_line(line).unwrap();
        let h = header_for(&[c.clone()], "http://127.0.0.1/x").unwrap();
        assert_eq!(h, "session=s3cret");
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.txt");
        save_netscape(&p, &[c]).unwrap();
        let loaded = load_netscape(&p).unwrap();
        assert_eq!(loaded[0].name, "session");
        assert_eq!(loaded[0].value, "s3cret");
    }
}
