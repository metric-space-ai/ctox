// Origin: CTOX
// License: AGPL-3.0-only

//! Whether an inbound mail really comes from its From domain, judged only by
//! `Authentication-Results` headers (RFC 8601) that the operator's own
//! receiving server stamped.
//!
//! A sender can write any header into a mail, including a forged
//! `Authentication-Results: <our server>; dmarc=pass`. The receiving server
//! stamps its own result as well, so a spoofed mail carries the server's
//! genuine `fail` next to the forgery. A mail therefore passes only when it
//! carries at least one result from a trusted server and every result from a
//! trusted server shows an aligned pass.

/// Runtime setting: comma-separated authserv-ids of the receiving servers
/// whose `Authentication-Results` this instance trusts. Unset means no mail
/// counts as authenticated.
pub(crate) const TRUSTED_AUTHSERV_IDS_KEY: &str = "CTO_EMAIL_TRUSTED_AUTHSERV_IDS";

pub(crate) fn trusted_authserv_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect()
}

/// True when the From domain of `sender_address` is authenticated by every
/// `Authentication-Results` header of a trusted server, and at least one
/// such header exists. Aligned pass: `dmarc=pass` with `header.from` equal
/// to the From domain, or `dkim=pass` whose `header.d` is the From domain or
/// one of its parent domains.
pub(crate) fn sender_domain_authenticated(
    authentication_results: &[String],
    trusted_authserv_ids: &[String],
    sender_address: &str,
) -> bool {
    let Some(from_domain) = sender_address
        .trim()
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
    else {
        return false;
    };
    if trusted_authserv_ids.is_empty() {
        return false;
    }
    let mut trusted_seen = false;
    for header in authentication_results {
        let parsed = parse(header);
        if !trusted_authserv_ids
            .iter()
            .any(|trusted| trusted == &parsed.authserv_id)
        {
            continue;
        }
        trusted_seen = true;
        if !parsed
            .results
            .iter()
            .any(|result| result.aligned_pass(&from_domain))
        {
            return false;
        }
    }
    trusted_seen
}

struct ParsedHeader {
    authserv_id: String,
    results: Vec<MethodResult>,
}

struct MethodResult {
    method: String,
    result: String,
    properties: Vec<(String, String)>,
}

impl MethodResult {
    fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn aligned_pass(&self, from_domain: &str) -> bool {
        if self.result != "pass" {
            return false;
        }
        match self.method.as_str() {
            "dmarc" => self.property("header.from") == Some(from_domain),
            "dkim" => self.property("header.d").is_some_and(|signing| {
                signing.contains('.')
                    && (from_domain == signing || from_domain.ends_with(&format!(".{signing}")))
            }),
            _ => false,
        }
    }
}

fn parse(header: &str) -> ParsedHeader {
    let cleaned = strip_comments(header);
    let mut segments = cleaned.split(';');
    let authserv_id = segments
        .next()
        .and_then(|first| first.split_whitespace().next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let results = segments
        .filter_map(|segment| {
            let mut tokens = segment.split_whitespace();
            let (method, result) = tokens.next()?.split_once('=')?;
            let properties = tokens
                .filter_map(|token| token.split_once('='))
                .map(|(key, value)| {
                    (
                        key.to_ascii_lowercase(),
                        value
                            .trim_matches('"')
                            .trim_end_matches('.')
                            .to_ascii_lowercase(),
                    )
                })
                .collect();
            Some(MethodResult {
                method: method.to_ascii_lowercase(),
                result: result.to_ascii_lowercase(),
                properties,
            })
        })
        .collect();
    ParsedHeader {
        authserv_id,
        results,
    }
}

/// RFC 5322 comments (`(sender IP is 1.2.3.4)`) carry no result; nested
/// parentheses are dropped with their content.
fn strip_comments(value: &str) -> String {
    let mut depth = 0usize;
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trusted() -> Vec<String> {
        trusted_authserv_ids(" MX.Thesen.example , ")
    }

    fn headers(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_dmarc_pass_from_the_trusted_server_authenticates_the_from_domain() {
        let results = headers(&["mx.thesen.example; spf=pass (sender IP is 192.0.2.1) smtp.mailfrom=metric-space.ai; dkim=pass (signature was verified) header.d=metric-space.ai; dmarc=pass action=none header.from=metric-space.ai"]);
        assert!(sender_domain_authenticated(
            &results,
            &trusted(),
            "Michael.Welsch@Metric-Space.ai"
        ));
        // An aligned DKIM pass from a parent signing domain also counts.
        let dkim_only =
            headers(&["mx.thesen.example 1; dkim=pass header.d=metric-space.ai header.s=sel"]);
        assert!(sender_domain_authenticated(
            &dkim_only,
            &trusted(),
            "a@mail.metric-space.ai"
        ));
    }

    #[test]
    fn spoofed_mail_fails_even_with_a_forged_pass_header() {
        // The sender forged a pass under our server's name; our server
        // stamped the genuine fail.
        let spoofed = headers(&[
            "mx.thesen.example; dmarc=fail action=reject header.from=metric-space.ai",
            "mx.thesen.example; dmarc=pass header.from=metric-space.ai",
        ]);
        assert!(!sender_domain_authenticated(
            &spoofed,
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
    }

    #[test]
    fn untrusted_servers_missing_results_and_misaligned_passes_do_not_count() {
        let foreign = headers(&["attacker.example; dmarc=pass header.from=metric-space.ai"]);
        assert!(!sender_domain_authenticated(
            &foreign,
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
        assert!(!sender_domain_authenticated(
            &[],
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
        let other_domain = headers(&[
            "mx.thesen.example; dmarc=pass header.from=attacker.example; dkim=pass header.d=attacker.example",
        ]);
        assert!(!sender_domain_authenticated(
            &other_domain,
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
        // A suffix that is not a parent domain is not aligned.
        let lookalike = headers(&["mx.thesen.example; dkim=pass header.d=space.ai"]);
        assert!(!sender_domain_authenticated(
            &lookalike,
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
        // A bare top-level domain never aligns.
        let tld = headers(&["mx.thesen.example; dkim=pass header.d=ai"]);
        assert!(!sender_domain_authenticated(
            &tld,
            &trusted(),
            "michael.welsch@metric-space.ai"
        ));
        // Without configured servers nothing is authenticated.
        let pass = headers(&["mx.thesen.example; dmarc=pass header.from=metric-space.ai"]);
        assert!(!sender_domain_authenticated(
            &pass,
            &[],
            "michael.welsch@metric-space.ai"
        ));
    }
}
