// ref: stalwart/src/calcard/vcard/mod.rs:1-100
// ref: ctox-mailserver streamlined vCard parser

use crate::util::errors::StalwartResult;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VCard {
    pub uid: String,
    pub fn_name: String,
    pub emails: Vec<String>,
    pub tels: Vec<String>,
    pub properties: HashMap<String, Vec<String>>,
}

impl VCard {
    pub fn parse(data: &str) -> StalwartResult<Self> {
        let unfolded = unfold_lines(data);
        let mut uid = String::new();
        let mut fn_name = String::new();
        let mut emails = Vec::new();
        let mut tels = Vec::new();
        let mut properties: HashMap<String, Vec<String>> = HashMap::new();

        let mut in_vcard = false;

        for line in unfolded.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.eq_ignore_ascii_case("BEGIN:VCARD") {
                in_vcard = true;
                continue;
            }

            if line.eq_ignore_ascii_case("END:VCARD") {
                break;
            }

            if in_vcard {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key_part = parts[0];
                    let val = parts[1].to_string();

                    // Strip parameters from key, e.g. "EMAIL;TYPE=internet" -> "EMAIL"
                    let key = key_part.split(';').next().unwrap_or(key_part).to_uppercase();

                    match key.as_str() {
                        "UID" => uid = val.clone(),
                        "FN" => fn_name = val.clone(),
                        "EMAIL" => emails.push(val.clone()),
                        "TEL" => tels.push(val.clone()),
                        _ => {}
                    }
                    properties.entry(key).or_default().push(val);
                }
            }
        }

        if uid.is_empty() {
            // Fallback: if no UID in vcard, we can generate a stable one or return error
            uid = crate::util::generate_unique_id();
        }

        Ok(Self {
            uid,
            fn_name,
            emails,
            tels,
            properties,
        })
    }
}

fn unfold_lines(data: &str) -> String {
    let mut unfolded = String::new();
    for line in data.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            unfolded.push_str(&line[1..]);
        } else {
            if !unfolded.is_empty() {
                unfolded.push('\n');
            }
            unfolded.push_str(line);
        }
    }
    unfolded
}
