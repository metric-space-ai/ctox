//! C++ `--netrc-path` / `--no-netrc` (prefs.h PREF_NETRC_PATH, PREF_NO_NETRC).
#![forbid(unsafe_code)]

use crate::options::OptionSet;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetrcEntry {
    pub machine: Option<String>,
    pub login: String,
    pub password: String,
}

pub fn parse_netrc(text: &str) -> Vec<NetrcEntry> {
    let mut tokens = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        tokens.extend(line.split_whitespace().map(|s| s.to_string()));
    }
    let mut out = Vec::new();
    let mut i = 0;
    let mut machine: Option<String> = None;
    let mut login = String::new();
    let mut password = String::new();
    let mut in_entry = false;
    let flush = |machine: &mut Option<String>,
                 login: &mut String,
                 password: &mut String,
                 in_entry: &mut bool,
                 out: &mut Vec<NetrcEntry>| {
        if *in_entry && !login.is_empty() {
            out.push(NetrcEntry {
                machine: machine.take(),
                login: std::mem::take(login),
                password: std::mem::take(password),
            });
        }
        *in_entry = false;
    };
    while i < tokens.len() {
        match tokens[i].as_str() {
            "machine" if i + 1 < tokens.len() => {
                flush(&mut machine, &mut login, &mut password, &mut in_entry, &mut out);
                machine = Some(tokens[i + 1].clone());
                in_entry = true;
                i += 2;
            }
            "default" => {
                flush(&mut machine, &mut login, &mut password, &mut in_entry, &mut out);
                machine = None;
                in_entry = true;
                i += 1;
            }
            "login" if i + 1 < tokens.len() => {
                login = tokens[i + 1].clone();
                in_entry = true;
                i += 2;
            }
            "password" if i + 1 < tokens.len() => {
                password = tokens[i + 1].clone();
                in_entry = true;
                i += 2;
            }
            "macdef" => {
                i += 1;
                while i < tokens.len() && !tokens[i].is_empty() {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    flush(&mut machine, &mut login, &mut password, &mut in_entry, &mut out);
    out
}

pub fn lookup<'a>(entries: &'a [NetrcEntry], host: &str) -> Option<&'a NetrcEntry> {
    entries
        .iter()
        .find(|e| e.machine.as_deref() == Some(host))
        .or_else(|| entries.iter().find(|e| e.machine.is_none()))
}

pub fn load(opts: &OptionSet) -> Vec<NetrcEntry> {
    if opts.bool("no-netrc", false) {
        return Vec::new();
    }
    let path = opts
        .get("netrc-path")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".netrc"))
        });
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_netrc(&text)
}

pub fn credentials_for(opts: &OptionSet, uri: &str) -> Option<(String, String)> {
    if opts.bool("no-netrc", false) {
        return None;
    }
    if opts.get("http-user").filter(|s| !s.is_empty()).is_some() {
        return None;
    }
    let host = url::Url::parse(uri).ok()?.host_str()?.to_string();
    let entries = load(opts);
    let e = lookup(&entries, &host)?;
    Some((e.login.clone(), e.password.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_machine_login_password() {
        let e = parse_netrc("machine 127.0.0.1 login alice password secret\n");
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].machine.as_deref(), Some("127.0.0.1"));
        assert_eq!(e[0].login, "alice");
        assert_eq!(e[0].password, "secret");
        assert_eq!(lookup(&e, "127.0.0.1").unwrap().login, "alice");
        assert!(lookup(&e, "other").is_none());
    }
}
