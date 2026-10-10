//! The signed-URL machinery: HMAC-SHA256 over four fields, and Go's
//! `2006-01-02 15:04:05.999999999 -0700 MST` timestamp layout.
//!
//! Both halves are wire format. `upload-artifact@v4` takes the URL act hands
//! it and echoes the query back verbatim, and act signs the query it is about
//! to be sent, so the layout has to round-trip through its own verifier as well
//! as through a real client.
//!
//! Two Go behaviours are reproduced rather than replaced:
//!
//! * **`.999999999` trims trailing zeros, and omits the point entirely at
//!   zero.** Go's `9`s are the "print the fractional part, dropping trailing
//!   zeros" verb, not "print nine digits".
//! * **The numeric offset wins over the zone name.** In `time.Parse`, when a
//!   layout carries both, the `-0700` field sets the instant and the
//!   abbreviation only picks a `Location`. act compares the parsed instant
//!   against `time.Now()`, so the abbreviation cannot change the outcome.

use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// The signing key, verbatim from act: four bytes, not a passphrase.
const SIGNING_KEY: [u8; 4] = [0xba, 0xdb, 0xee, 0xf0];

/// How long a signed URL stays valid.
pub const SIGNED_URL_LIFETIME_SECONDS: i64 = 60 * 60;

/// `buildSignature`: HMAC-SHA256 over `endp || expires || artifactName || taskID`
/// with no separators between them.
///
/// `fmt.Sprint` on the `int64` is plain decimal, with a leading `-` when
/// negative.
pub fn build_signature(endp: &str, expires: &str, artifact_name: &str, task_id: i64) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&SIGNING_KEY).expect("hmac accepts any key");
    mac.update(endp.as_bytes());
    mac.update(expires.as_bytes());
    mac.update(artifact_name.as_bytes());
    mac.update(task_id.to_string().as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// `hmac.Equal`: constant-time, and length-checked. The length check is why a
/// missing or truncated `sig` is a 401 rather than a 500.
pub fn signatures_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// `time.Now().Add(time.Hour).Format("2006-01-02 15:04:05.999999999 -0700 MST")`.
pub fn format_signed_expiry(at: SystemTime) -> String {
    let (secs, nanos) = unix_parts(at);
    let local = chrono::DateTime::from_timestamp(secs, nanos)
        .expect("timestamp in range")
        .with_timezone(&chrono::Local);
    format_layout(&local.naive_local(), &local.format("%z").to_string(), &local.format("%Z").to_string())
}

/// The layout itself, for an instant already in the target zone.
///
/// Split out from [`format_signed_expiry`] so the fraction rule can be pinned
/// against Go's output without the test having to control `TZ`.
pub fn format_layout(
    local: &chrono::NaiveDateTime,
    numeric_offset: &str,
    zone: &str,
) -> String {
    let mut out = local.format("%Y-%m-%d %H:%M:%S").to_string();
    let nanos = local.and_utc().timestamp_subsec_nanos();
    if nanos != 0 {
        // Go's `.999999999` prints up to nine digits and drops trailing
        // zeros, and prints no point at all when the fraction is zero.
        let fraction = format!("{nanos:09}");
        out.push('.');
        out.push_str(fraction.trim_end_matches('0'));
    }
    out.push(' ');
    out.push_str(numeric_offset);
    out.push(' ');
    // Go falls back to the numeric offset when the zone has no abbreviation.
    if zone.is_empty() {
        out.push_str(numeric_offset);
    } else {
        out.push_str(zone);
    }
    out
}

/// `time.Parse("2006-01-02 15:04:05.999999999 -0700 MST", value)` reduced to the
/// one thing act uses it for: the instant, as seconds since the epoch.
///
/// The zone name is accepted and ignored — Go prefers the numeric offset for
/// the instant, and act only ever compares against `time.Now()`.
pub fn parse_signed_expiry(value: &str) -> Option<i64> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let zone = parts[parts.len() - 1];
    let offset = parts[parts.len() - 2];
    let stamp = parts[..parts.len() - 2].join(" ");

    // Go accepts any three-letter (or longer) abbreviation here.
    if zone.len() < 3 {
        return None;
    }
    let offset_seconds = parse_numeric_offset(offset)?;

    let (head, fraction) = match stamp.split_once('.') {
        Some((head, fraction)) => (head, Some(fraction)),
        None => (stamp.as_str(), None),
    };
    let date_time = chrono::NaiveDateTime::parse_from_str(head, "%Y-%m-%d %H:%M:%S").ok()?;
    let nanos = match fraction {
        // Go reads at most nine digits, *scales* a shorter fraction up to
        // nanoseconds — `.9` is 0.9 seconds, not 9 nanoseconds — and ignores
        // anything beyond nine.
        Some(digits) if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) => {
            let taken = digits.chars().take(9).count();
            let mut value: u32 = 0;
            for c in digits.chars().take(9) {
                value = value * 10 + c.to_digit(10)?;
            }
            for _ in taken..9 {
                value *= 10;
            }
            value
        }
        _ => 0,
    };
    let naive = date_time.and_utc() + chrono::Duration::nanoseconds(nanos as i64);
    Some(naive.timestamp() - offset_seconds)
}

/// A signed offset such as `+0100`, in seconds.
fn parse_numeric_offset(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 5 || (bytes[0] != b'+' && bytes[0] != b'-') {
        return None;
    }
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i64 = value[1..3].parse().ok()?;
    let minutes: i64 = value[3..5].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// Seconds and nanoseconds since the epoch, negative times included.
fn unix_parts(at: SystemTime) -> (i64, u32) {
    match at.duration_since(UNIX_EPOCH) {
        Ok(delta) => (delta.as_secs() as i64, delta.subsec_nanos()),
        Err(err) => {
            let delta = err.duration();
            // `SystemTime` counts a pre-epoch instant with a positive
            // magnitude, so a nanosecond fraction has to borrow.
            if delta.subsec_nanos() == 0 {
                (-(delta.as_secs() as i64), 0)
            } else {
                (
                    -(delta.as_secs() as i64) - 1,
                    1_000_000_000 - delta.subsec_nanos(),
                )
            }
        }
    }
}

/// `timestamppb` rendered by protojson: RFC 3339, with the fraction at 0, 3, 6
/// or 9 digits and `Z` at UTC.
pub fn proto_timestamp(at: SystemTime) -> String {
    let (secs, nanos) = unix_parts(at);
    let naive = chrono::DateTime::from_timestamp(secs, nanos).expect("timestamp in range");
    let mut out = naive.format("%Y-%m-%dT%H:%M:%S").to_string();
    match nanos {
        0 => {}
        n if n % 1_000_000 == 0 => out.push_str(&format!(".{:03}", n / 1_000_000)),
        n if n % 1_000 == 0 => out.push_str(&format!(".{:06}", n / 1_000)),
        n => out.push_str(&format!(".{n:09}")),
    }
    out.push('Z');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{artifact_name_to_id, base64_url_decode, base64_url_encode};
    use chrono::TimeZone;

    /// Every expectation below was produced by running act's own
    /// `buildSignature` and `artifactNameToID` under Go 1.26, not by reading
    /// the source. The key is four literal bytes, so a typo here would be
    /// invisible to a self-consistent test.
    #[test]
    fn signatures_match_go() {
        let table: &[(&str, &str, &str, i64, &str)] = &[
            (
                "UploadArtifact",
                "2024-01-23 21:48:37.20833956 +0100 CET",
                "test",
                75,
                "HFJ6KMOPhyNim68cIgzFGBMa3kjLGKXC4PVMPxkIDOc=",
            ),
            (
                "DownloadArtifact",
                "2024-01-23 21:51:56.872846295 +0100 CET",
                "test",
                76,
                "WdrjwHI6xM_PFSqRNTplohSOiBXyfo45uucTlgVKlB8=",
            ),
            (
                "UploadArtifact",
                "",
                "",
                0,
                "lu92xiBB5ddySwHI2eUhMCmb7kPlrS2xyi6k22cMD88=",
            ),
            (
                "UploadArtifact",
                "2024-01-02 03:04:05 +0000 UTC",
                "a b/c",
                -5,
                "iFLII6KekY-3y21pTpjpixCz5hI6UqtmGXSa8j6iTRc=",
            ),
        ];
        for (endp, expires, name, task_id, want) in table {
            let raw = build_signature(endp, expires, name, *task_id);
            assert_eq!(&base64_url_encode(&raw), want, "signature of {name:?}");
            // Go's encoder is padded, and `verifySignature` decodes it back, so
            // the round trip has to be exact.
            assert_eq!(base64_url_decode(want), raw);
        }
    }

    #[test]
    fn a_tampered_query_fails_the_signature() {
        let signature = build_signature("UploadArtifact", "2024-01-02 03:04:05 +0000 UTC", "test", 75);
        // Every one of the four signed fields is load-bearing.
        assert!(!signatures_equal(
            &build_signature("DownloadArtifact", "2024-01-02 03:04:05 +0000 UTC", "test", 75),
            &signature,
        ));
        assert!(!signatures_equal(
            &build_signature("UploadArtifact", "2024-01-02 03:04:06 +0000 UTC", "test", 75),
            &signature,
        ));
        assert!(!signatures_equal(
            &build_signature("UploadArtifact", "2024-01-02 03:04:05 +0000 UTC", "tesT", 75),
            &signature,
        ));
        assert!(!signatures_equal(
            &build_signature("UploadArtifact", "2024-01-02 03:04:05 +0000 UTC", "test", 76),
            &signature,
        ));
        assert!(signatures_equal(&signature, &signature));
    }

    #[test]
    fn a_missing_signature_is_rejected_by_length_not_by_content() {
        let signature = build_signature("UploadArtifact", "", "", 0);
        // `hmac.Equal` returns false for a length mismatch, which is what
        // keeps a missing `sig` a 401 instead of a panic.
        assert!(!signatures_equal(&[], &signature));
        assert!(!signatures_equal(&base64_url_decode("not base64!!"), &signature));
    }

    #[test]
    fn artifact_ids_match_go() {
        let table: &[(&str, i64)] = &[
            ("test", 2949673445),
            ("", 2166136261),
            ("a", 3826002220),
            ("some.zip", 2232015820),
            ("artifact-with-a-longer-name-0123456789", 2745454702),
        ];
        for (name, want) in table {
            assert_eq!(artifact_name_to_id(name), *want, "id of {name:?}");
        }
    }

    /// Go's `.999999999` trims trailing zeros and drops the point at zero.
    /// The formatted instants here are Go's, with the zone fixed to CET so the
    /// table does not depend on the machine's `TZ`.
    #[test]
    fn the_expiry_layout_matches_go() {
        let table: &[(u32, &str)] = &[
            (0, "2024-01-02 03:04:05 +0100 CET"),
            (1, "2024-01-02 03:04:05.000000001 +0100 CET"),
            (1_000, "2024-01-02 03:04:05.000001 +0100 CET"),
            (100_000, "2024-01-02 03:04:05.0001 +0100 CET"),
            (1_000_000, "2024-01-02 03:04:05.001 +0100 CET"),
            (208_339_560, "2024-01-02 03:04:05.20833956 +0100 CET"),
            (872_846_295, "2024-01-02 03:04:05.872846295 +0100 CET"),
            (999_999_999, "2024-01-02 03:04:05.999999999 +0100 CET"),
            (123_456_789, "2024-01-02 03:04:05.123456789 +0100 CET"),
            (120_000_000, "2024-01-02 03:04:05.12 +0100 CET"),
        ];
        let cet = chrono::FixedOffset::east_opt(3600).expect("valid offset");
        for (nanos, want) in table {
            let at = chrono::Utc
                .with_ymd_and_hms(2024, 1, 2, 2, 4, 5)
                .unwrap()
                .with_timezone(&cet)
                + chrono::Duration::nanoseconds(*nanos as i64);
            let got = format_layout(&at.naive_local(), "+0100", "CET");
            assert_eq!(&got, want, "nanos = {nanos}");
        }
    }

    /// The formatter and the parser have to compose: act signs the string it
    /// formats and then verifies the string a client echoed back, so a value
    /// that cannot survive the round trip would make every upload 401.
    ///
    /// Checked at every fractional-second shape, because the two sides of the
    /// round trip trim and scale differently.
    #[test]
    fn a_formatted_expiry_parses_back_to_the_same_instant() {
        for nanos in [0u32, 1, 1_000, 208_339_560, 872_846_295, 999_999_999] {
            for offset in [0i64, 3_600, -5 * 3_600] {
                let instant = 1_704_164_645_i64;
                let at = chrono::Utc.timestamp_opt(instant, nanos).single().expect("valid");
                let sign = if offset < 0 { '-' } else { '+' };
                let absolute = offset.abs();
                let numeric = format!("{sign}{:02}{:02}", absolute / 3_600, (absolute % 3_600) / 60);
                let formatted = format_layout(&at.naive_utc(), &numeric, "TEST");
                // The same wall clock labelled with a different offset names a
                // different instant, and the numeric offset is what the
                // verifier goes by.
                assert_eq!(
                    parse_signed_expiry(&formatted),
                    Some(instant - offset + nanos as i64 / 1_000_000_000),
                    "{formatted:?} at offset {offset}",
                );
            }
        }
    }

    #[test]
    fn parsing_the_expiry_matches_go() {
        let table: &[(&str, i64)] = &[
            ("2024-01-02 03:04:05 +0000 UTC", 1704164645),
            ("2024-01-02 03:04:05.9 +0000 UTC", 1704164645),
            ("2024-01-02 03:04:05.20833956 +0100 CET", 1704161045),
            ("2024-01-02 03:04:05.872846295 +0100 CEST", 1704161045),
            // An abbreviation Go has never heard of still parses, and the
            // numeric offset decides the instant regardless.
            ("2024-01-02 03:04:05 +0000 XYZ", 1704164645),
        ];
        for (value, want) in table {
            assert_eq!(parse_signed_expiry(value), Some(*want), "parse {value:?}");
        }
        // Both of these are a parse error in Go, and both are a 401 here.
        assert_eq!(parse_signed_expiry("garbage"), None);
        assert_eq!(parse_signed_expiry("2024-01-02 03:04:05 +0100"), None);
    }

    /// protojson's Timestamp rule is **not** Go's `.999999999` layout verb: it
    /// picks 0, 3, 6 or 9 fractional digits and pads, so `.208339560` keeps
    /// its trailing zero where the signed-URL layout trims to `.20833956`.
    /// Every expectation here is a `protojson.Marshal` of a real
    /// `timestamppb`.
    #[test]
    fn protojson_timestamps_pad_to_three_six_or_nine_digits() {
        let table: &[(u32, &str)] = &[
            (0, "2024-01-02T03:04:05Z"),
            (1, "2024-01-02T03:04:05.000000001Z"),
            (1_000, "2024-01-02T03:04:05.000001Z"),
            (100_000, "2024-01-02T03:04:05.000100Z"),
            (1_000_000, "2024-01-02T03:04:05.001Z"),
            (2_000_000, "2024-01-02T03:04:05.002Z"),
            (208_339_560, "2024-01-02T03:04:05.208339560Z"),
            (872_846_295, "2024-01-02T03:04:05.872846295Z"),
            (999_999_999, "2024-01-02T03:04:05.999999999Z"),
            (123_456_789, "2024-01-02T03:04:05.123456789Z"),
            (120_000_000, "2024-01-02T03:04:05.120Z"),
            (999_000_000, "2024-01-02T03:04:05.999Z"),
            (12_000_000, "2024-01-02T03:04:05.012Z"),
        ];
        for (nanos, want) in table {
            let at = chrono::Utc
                .with_ymd_and_hms(2024, 1, 2, 3, 4, 5)
                .unwrap()
                + chrono::Duration::nanoseconds(*nanos as i64);
            assert_eq!(
                proto_timestamp(SystemTime::from(at)),
                *want,
                "nanos = {nanos}",
            );
        }
    }

    /// A modification time before the epoch still has to render.
    #[test]
    fn a_pre_epoch_timestamp_renders() {
        let at = chrono::Utc
            .with_ymd_and_hms(1969, 12, 31, 23, 59, 59)
            .unwrap()
            + chrono::Duration::milliseconds(500);
        assert_eq!(
            proto_timestamp(SystemTime::from(at)),
            "1969-12-31T23:59:59.500Z",
        );
    }
}
