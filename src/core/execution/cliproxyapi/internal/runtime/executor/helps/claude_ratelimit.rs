// ref: internal/runtime/executor/helps/claude_ratelimit.go:24-119,129-287,299-330
// Upstream: 80809679914f376605e2ae6aa2c5d68f140f98d3 (CLIProxyAPI v8.0.22)
// License: MIT (upstream); modifications AGPL-3.0-only
//
// Port the subscription/overage reset selection into the existing HTTP retry
// path. Keep CTOX's existing generic Retry-After/Ms parsing and conductor
// backoff; this slice does not change account selection or introduce jitter.
use std::time::{Duration, SystemTime};

use crate::sdk::cliproxy::executor::Headers;

const MAX_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600 + 3600);

fn header<'a>(headers: &'a Headers, name: &str) -> &'a str {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map_or("", |value| value.trim())
}

fn status(headers: &Headers, window: &str) -> String {
    header(headers, &format!("Anthropic-Ratelimit-Unified-{window}")).to_ascii_lowercase()
}

fn allowed(value: &str) -> bool {
    matches!(value, "allowed" | "allowed_warning")
}

fn healthy(value: &str) -> bool {
    value
        .parse::<f64>()
        .is_ok_and(|value| value.is_finite() && (0.0..1.0).contains(&value))
}

fn overage_only(headers: &Headers, unified: &str, five: &str, seven: &str, oi: &str) -> bool {
    if five == "rejected" || seven == "rejected" {
        return false;
    }
    let claim = status(headers, "Representative-Claim");
    let overage = oi == "rejected"
        || status(headers, "Overage-Status") == "rejected"
        || !header(
            headers,
            "Anthropic-Ratelimit-Unified-Overage-Disabled-Reason",
        )
        .is_empty()
        || claim.contains("overage");
    if !overage {
        return false;
    }
    if allowed(five) && allowed(seven) {
        return true;
    }
    let five_util = header(headers, "Anthropic-Ratelimit-Unified-5h-Utilization");
    let seven_util = header(headers, "Anthropic-Ratelimit-Unified-7d-Utilization");
    if allowed(seven) && five.is_empty() && healthy(five_util) {
        return true;
    }
    if allowed(five) && seven.is_empty() && healthy(seven_util) {
        return true;
    }
    // Absence alone is accepted only for the explicit rejected overage claim.
    unified == "rejected"
        && five.is_empty()
        && seven.is_empty()
        && claim.contains("overage")
        && (five_util.is_empty() || healthy(five_util))
        && (seven_util.is_empty() || healthy(seven_util))
}

fn timestamp(raw: &str) -> Option<SystemTime> {
    if let Ok(seconds) = raw.parse::<f64>() {
        if !seconds.is_finite() || seconds <= 0.0 {
            return None;
        }
        return SystemTime::UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(seconds).ok()?);
    }
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(raw) {
        let seconds = u64::try_from(value.timestamp()).ok()?;
        return SystemTime::UNIX_EPOCH
            .checked_add(Duration::new(seconds, value.timestamp_subsec_nanos()));
    }
    httpdate::parse_http_date(raw).ok()
}

pub(crate) fn claude_retry_delay(
    headers: &Headers,
    now: SystemTime,
    standard: Option<Duration>,
) -> Option<Duration> {
    if !headers.keys().any(|name| {
        name.to_ascii_lowercase()
            .starts_with("anthropic-ratelimit-unified-")
    }) {
        return standard;
    }
    let unified = status(headers, "Status");
    let five = status(headers, "5h-Status");
    let seven = status(headers, "7d-Status");
    let oi = status(headers, "7d_oi-Status");
    if overage_only(headers, &unified, &five, &seven, &oi) {
        return None;
    }

    let mut candidates = standard.into_iter().collect::<Vec<_>>();
    for (window, state) in [("5h", &five), ("7d", &seven), ("7d_oi", &oi)] {
        if state == "rejected" {
            if let Some(delay) = timestamp(header(
                headers,
                &format!("Anthropic-Ratelimit-Unified-{window}-Reset"),
            ))
            .and_then(|when| when.duration_since(now).ok())
            {
                candidates.push(delay);
            }
        }
    }
    let rejected = unified == "rejected"
        || five == "rejected"
        || seven == "rejected"
        || oi == "rejected"
        || (unified.is_empty() && !allowed(&five) && !allowed(&seven));
    if rejected {
        let reset = timestamp(header(headers, "Anthropic-Ratelimit-Unified-Reset"));
        let overage_reset = timestamp(header(headers, "Anthropic-Ratelimit-Unified-Overage-Reset"));
        let billing_boundary = status(headers, "Representative-Claim").contains("overage")
            && reset.is_some()
            && reset == overage_reset;
        if !billing_boundary {
            if let Some(delay) = reset.and_then(|when| when.duration_since(now).ok()) {
                candidates.push(delay);
            }
        }
    }
    candidates
        .into_iter()
        .filter(|delay| !delay.is_zero() && *delay <= MAX_WINDOW)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Headers {
        pairs
            .iter()
            .map(|(key, value)| {
                (
                    format!("Anthropic-Ratelimit-Unified-{key}"),
                    vec![value.to_string()],
                )
            })
            .collect()
    }
    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_406_000)
    }

    #[test]
    fn monthly_overage_without_shared_windows_never_becomes_subscription_cooldown() {
        let h = headers(&[
            ("Status", "rejected"),
            ("Representative-Claim", "overage"),
            ("Reset", "1793491200"),
            ("Overage-Reset", "1793491200"),
        ]);
        assert_eq!(
            claude_retry_delay(&h, now(), Some(Duration::from_secs(10800))),
            None
        );
    }

    #[test]
    fn genuine_shared_window_wins_over_monthly_unified_reset() {
        let reset = (now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 18000)
            .to_string();
        let h = headers(&[
            ("Status", "rejected"),
            ("5h-Status", "rejected"),
            ("5h-Reset", &reset),
            ("Reset", "1793491200"),
        ]);
        assert_eq!(
            claude_retry_delay(&h, now(), None),
            Some(Duration::from_secs(18000))
        );
    }

    #[test]
    fn equal_overage_reset_within_seven_days_is_still_billing() {
        let reset = (now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 172800)
            .to_string();
        let h = headers(&[
            ("Status", "rejected"),
            ("Representative-Claim", "overage"),
            ("7d-Status", "allowed"),
            ("Reset", &reset),
            ("Overage-Reset", &reset),
        ]);
        assert_eq!(
            claude_retry_delay(&h, now(), Some(Duration::from_secs(60))),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn unhealthy_or_invalid_shared_utilization_retains_subscription_retry() {
        for util in ["1.0", "-0.1", "NaN", "inf", "invalid"] {
            let h = headers(&[
                ("Status", "rejected"),
                ("Representative-Claim", "overage"),
                ("5h-Utilization", util),
            ]);
            assert_eq!(
                claude_retry_delay(&h, now(), Some(Duration::from_secs(10800))),
                Some(Duration::from_secs(10800))
            );
        }
    }

    #[test]
    fn allowed_or_missing_unified_claim_keeps_short_retry() {
        for unified in ["allowed", ""] {
            let h = headers(&[
                ("Status", unified),
                ("Representative-Claim", "overage"),
                ("Overage-Status", "allowed"),
            ]);
            assert_eq!(
                claude_retry_delay(&h, now(), Some(Duration::from_secs(30))),
                Some(Duration::from_secs(30))
            );
        }
    }

    #[test]
    fn model_only_oi_rejection_does_not_bench_healthy_shared_windows() {
        let h = headers(&[
            ("Status", "rejected"),
            ("5h-Status", "allowed_warning"),
            ("7d-Status", "allowed"),
            ("7d_oi-Status", "rejected"),
            ("7d_oi-Reset", "1793491200"),
        ]);
        assert_eq!(
            claude_retry_delay(&h, now(), Some(Duration::from_secs(10800))),
            None
        );
    }

    #[test]
    fn case_insensitive_rfc3339_and_latest_applicable_reset() {
        let unix = now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let five = chrono::DateTime::from_timestamp((unix + 60) as i64, 0)
            .unwrap()
            .to_rfc3339();
        let seven = (unix + 120).to_string();
        let h: Headers = [
            ("anthropic-ratelimit-unified-5h-status", "rejected"),
            ("Anthropic-Ratelimit-Unified-5h-Reset", &five),
            ("Anthropic-Ratelimit-Unified-7d-Status", "rejected"),
            ("Anthropic-Ratelimit-Unified-7d-Reset", &seven),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), vec![v.to_string()]))
        .collect();
        assert_eq!(
            claude_retry_delay(&h, now(), Some(Duration::from_secs(30))),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn out_of_window_or_past_reset_falls_back_without_changing_generic_headers() {
        let h = headers(&[
            ("Status", "rejected"),
            ("5h-Status", "rejected"),
            ("5h-Reset", "1"),
        ]);
        assert_eq!(
            claude_retry_delay(&h, now(), Some(MAX_WINDOW + Duration::from_secs(1))),
            None
        );
        assert_eq!(
            claude_retry_delay(&Headers::new(), now(), Some(Duration::from_millis(2500))),
            Some(Duration::from_millis(2500))
        );
    }
}
