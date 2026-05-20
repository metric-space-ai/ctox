// ref: stalwart/src/smtp/server/dsn.rs:1-120
// ref: ctox-mailserver RFC 3463 Delivery Status Notification parser for bounce categorizing

#[derive(Debug, Clone)]
pub struct DsnReport {
    pub recipient: String,
    pub status_code: String,
    pub is_hard_bounce: bool,
}

pub fn parse_dsn_report(body: &str) -> Option<DsnReport> {
    let mut recipient = String::new();
    let mut status_code = String::new();

    // Iterate through lines looking for standard DSN status reports
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_lowercase();
            let val = parts[1].trim().to_string();

            match key.as_str() {
                "final-recipient" | "original-recipient" => {
                    // e.g. "rfc822; user@domain.com"
                    if let Some(email) = val.split(';').last() {
                        recipient = email.trim().to_string();
                    }
                }
                "status" => {
                    // e.g. "5.1.1" or "4.2.2"
                    status_code = val.clone();
                }
                _ => {}
            }
        }
    }

    if recipient.is_empty() {
        // Fallback: search the email body text using simple regex or split pattern to find recipient
        for word in body.split_whitespace() {
            if word.contains('@') && (word.ends_with('.') || word.len() > 5) {
                recipient = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '.' && c != '-' && c != '_').to_string();
                break;
            }
        }
    }

    if !status_code.is_empty() && !recipient.is_empty() {
        let is_hard_bounce = status_code.starts_with('5');
        Some(DsnReport {
            recipient,
            status_code,
            is_hard_bounce,
        })
    } else {
        None
    }
}
