//! The authorization token: a JWT that identifies a job to the artifact
//! service.
//!
//! `ACTIONS_RUNTIME_TOKEN` is handed to a job so that `docker/build-push-action`
//! and `docker/buildx` can push to the artifact backend with the `gha` cache
//! exporter. Those clients read two claims out of it: `scp`, the Actions
//! Results scope, and `ac`, a JSON list of cache scopes. The token is minted
//! and parsed entirely inside act — nothing verifies it — so the shape matters
//! for the clients, not for security.
//!
//! # The signing key is empty
//!
//! `token.SignedString([]byte{})` signs with a **zero-length** HMAC key. That
//! is not a placeholder: the token exists so the runner can recognise its own
//! jobs, and an empty key makes it a fixed, reproducible string for a given
//! set of claims. Changing the key would change the token, and `buildx` does
//! not care what is inside as long as the claims parse — so the empty key is
//! preserved rather than "fixed".
//!
//! # base64url without padding
//!
//! RFC 7515 uses `base64.RawURLEncoding`. This is a *different* encoding from
//! the padded one act's V4 artifact signature uses, and mixing them up either
//! corrupts the signature input or makes the token unparseable. Both live in
//! [`crate::base64url`], named for which they are.
//!
//! # Claim names
//!
//! Two claims are tagged in Go (`scp`, `ac`) and three are not (`TaskID`,
//! `RunID`, `JobID`), so the last three keep Go's capitalisation on the wire.
//! `buildx` looks for `TaskID` by that exact spelling.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::base64url;

/// How long a minted token stays valid.
pub const TOKEN_LIFETIME_SECONDS: i64 = 24 * 60 * 60;

/// A cache permission bit, as `actionsCachePermission`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", from = "u8")]
pub enum CachePermission {
    /// Read an existing cache entry.
    Read = 1,
    /// Create or overwrite one.
    Write = 2,
}

impl From<CachePermission> for u8 {
    fn from(permission: CachePermission) -> u8 {
        permission as u8
    }
}

impl From<u8> for CachePermission {
    fn from(value: u8) -> Self {
        match value {
            1 => CachePermission::Read,
            // Upstream defines only two values; anything else is not
            // representable and is reported as a write, which is the more
            // permissive of the two.
            _ => CachePermission::Write,
        }
    }
}

/// One entry of the `ac` claim: what the cache backend may be used for.
///
/// Field names are Go's defaults — no struct tags upstream — so they keep
/// their capitalisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheScope {
    /// The scope's prefix. act mints the empty string.
    #[serde(rename = "Scope")]
    pub scope: String,
    /// The permission bits.
    #[serde(rename = "Permission")]
    pub permission: CachePermission,
}

/// The claims act mints.
///
/// The field order is the wire order, because Go marshals a struct in
/// declaration order and `exp`/`nbf` come from the embedded registered claims.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Expiry, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    /// Not-before, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// The Actions Results scope, `Actions.Results:<run>:<job>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scp: String,
    /// The task — the runner's identifier for a job.
    #[serde(rename = "TaskID", default)]
    pub task_id: i64,
    /// The workflow run.
    #[serde(rename = "RunID", default)]
    pub run_id: i64,
    /// The job.
    #[serde(rename = "JobID", default)]
    pub job_id: i64,
    /// The cache scopes, as a JSON **string** rather than an array. `buildx`
    /// parses it a second time, and the string-in-a-string is not a mistake.
    #[serde(rename = "ac", default, skip_serializing_if = "String::is_empty")]
    pub ac: String,
}

impl Claims {
    /// The `ac` claim, parsed. `buildx` reads it this way.
    pub fn cache_scopes(&self) -> Result<Vec<CacheScope>> {
        if self.ac.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&self.ac).map_err(|err| anyhow!("invalid ac claim: {err}"))
    }
}

/// `CreateAuthorizationToken`.
///
/// `task_id` is the caller's identifier for the job; `run_id` and `job_id`
/// build the `scp` scope. Mints exactly one cache scope, write-only, which is
/// what the exporter needs.
pub fn create_authorization_token(task_id: i64, run_id: i64, job_id: i64) -> Result<String> {
    let ac = serde_json::to_string(&vec![CacheScope {
        scope: String::new(),
        permission: CachePermission::Write,
    }])?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let claims = Claims {
        exp: Some(now + TOKEN_LIFETIME_SECONDS),
        nbf: Some(now),
        scp: format!("Actions.Results:{run_id}:{job_id}"),
        task_id,
        run_id,
        job_id,
        ac,
    };

    sign(&claims)
}

/// The three JWT parts, base64url-encoded without padding.
pub fn sign(claims: &Claims) -> Result<String> {
    // `jwt.NewWithClaims` marshals this exact header.
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    let encoded_header = base64url::encode_raw(header.as_bytes());
    let encoded_claims = base64url::encode_raw(&serde_json::to_vec(claims)?);
    let signing_input = format!("{encoded_header}.{encoded_claims}");

    let signature = hmac_sha256(&[], signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        base64url::encode_raw(&signature)
    ))
}

/// `ParseAuthorizationToken`, from a header value.
///
/// A missing `Authorization` header is **not** an error: it yields task id 0,
/// which is what an unauthenticated request gets. A header that is not
/// `Bearer <token>` is an error, and so is a token that does not verify.
pub fn parse_authorization_token(header: Option<&str>) -> Result<i64> {
    let Some(header) = header else {
        return Ok(0);
    };
    let (scheme, token) = header
        .split_once(' ')
        .ok_or_else(|| anyhow!("split token failed: {header}"))?;
    if scheme.is_empty() {
        return Err(anyhow!("split token failed: {header}"));
    }
    let claims = verify(token)?;
    Ok(claims.task_id)
}

/// Verifies the signature and the time claims, returning the payload.
///
/// The signing method is checked to be HMAC before the key is used, which is
/// the check that stops an `alg: none` token from being accepted. With an
/// empty key the signature proves very little, but it is what upstream does
/// and a client that mints its own token has to be able to.
pub fn verify(token: &str) -> Result<Claims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("token contains an invalid number of segments"));
    }
    let (encoded_header, encoded_claims, encoded_signature) = (parts[0], parts[1], parts[2]);

    let header: serde_json::Value = serde_json::from_slice(&base64url::decode(encoded_header))
        .map_err(|err| anyhow!("invalid token header: {err}"))?;
    let algorithm = header
        .get("alg")
        .and_then(|alg| alg.as_str())
        .unwrap_or_default();
    if algorithm != "HS256" {
        // This is the check that stops an `alg: none` token being accepted.
        return Err(anyhow!("unexpected signing method: {algorithm}"));
    }

    let signing_input = format!("{encoded_header}.{encoded_claims}");
    let expected = hmac_sha256(&[], signing_input.as_bytes());
    let presented = base64url::decode(encoded_signature);
    if !constant_time_eq(&presented, &expected) {
        return Err(anyhow!("signature is invalid"));
    }

    let claims: Claims = serde_json::from_slice(&base64url::decode(encoded_claims))
        .map_err(|err| anyhow!("invalid claims: {err}"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Go's `VerifyNotBefore` is inclusive and `VerifyExpiresAt` is strict.
    if let Some(not_before) = claims.nbf {
        if now < not_before {
            return Err(anyhow!("token is not valid yet"));
        }
    }
    if let Some(expires) = claims.exp {
        if now >= expires {
            return Err(anyhow!("token is expired"));
        }
    }
    Ok(claims)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    // An empty key is a valid HMAC key, and act relies on that.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("hmac accepts any key");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // auth_test.go: TestCreateAuthorizationToken
    #[test]
    fn a_minted_token_carries_the_claims_buildx_reads() {
        let token = create_authorization_token(23, 1, 2).expect("a token");
        assert!(!token.is_empty());

        let claims = verify(&token).expect("a fresh token verifies");
        assert!(claims.scp.contains("Actions.Results:1:2"));
        assert_eq!(claims.task_id, 23);
        assert_eq!(claims.run_id, 1);
        assert_eq!(claims.job_id, 2);
        // `ac` is a string holding JSON, not a JSON array — `buildx`
        // parses it a second time.
        assert!(claims.ac.starts_with("["), "{}", claims.ac);
        assert!(!claims.cache_scopes().expect("scopes").is_empty());
    }

    /// The wire field names are load-bearing: `buildx` looks for `TaskID` and
    /// a stringified `ac`, so a tidy-up to snake_case breaks it.
    #[test]
    fn the_claim_names_match_what_buildx_expects() {
        let token = create_authorization_token(23, 1, 2).expect("a token");
        let claims_json =
            String::from_utf8(base64url::decode(token.split('.').nth(1).expect("payload")))
                .expect("utf-8");
        assert!(claims_json.contains("\"TaskID\":23"), "{claims_json}");
        assert!(claims_json.contains("\"RunID\":1"), "{claims_json}");
        assert!(claims_json.contains("\"JobID\":2"), "{claims_json}");
        assert!(claims_json.contains("\"scp\":"), "{claims_json}");
        // `ac` is a string containing a JSON array, not an array.
        assert!(
            claims_json.contains(r#""ac":"[{\"Scope\":\"\",\"Permission\":2}]"#),
            "{claims_json}",
        );
        // The registered claims come first, as the embedded struct puts them.
        assert!(claims_json.starts_with(r#"{"exp":"#), "{claims_json}");
    }

    /// RFC 7515 is base64url **without** padding. A `=` in any of the three
    /// parts means the token is not the one act mints.
    #[test]
    fn the_token_is_base64url_without_padding() {
        let token = create_authorization_token(23, 1, 2).expect("a token");
        for part in token.split('.') {
            assert!(!part.contains('='), "padded segment in {token}");
        }
        assert_eq!(token.split('.').count(), 3);
    }

    // auth_test.go: TestParseAuthorizationToken
    #[test]
    fn a_bearer_header_yields_the_task_id() {
        let token = create_authorization_token(23, 1, 2).expect("a token");
        assert_eq!(
            parse_authorization_token(Some(&format!("Bearer {token}"))).expect("parsed"),
            23,
        );
    }

    // auth_test.go: TestParseAuthorizationTokenNoAuthHeader
    #[test]
    fn a_missing_header_is_not_an_error() {
        assert_eq!(parse_authorization_token(None).expect("parsed"), 0);
    }

    #[test]
    fn a_malformed_header_is_an_error() {
        // `SplitN(h, " ", 2)` needs the space, or act reports the split
        // failure.
        assert!(parse_authorization_token(Some("not-a-bearer-header")).is_err());
    }

    #[test]
    fn a_tampered_token_does_not_verify() {
        let token = create_authorization_token(23, 1, 2).expect("a token");
        let parts: Vec<&str> = token.split('.').collect();

        // A different signature.
        let forged = format!(
            "{}.{}.{}",
            parts[0],
            parts[1],
            base64url::encode_raw(b"nope")
        );
        assert!(verify(&forged).is_err(), "a forged signature");

        // A different payload, original signature.
        let claims = Claims {
            task_id: 9999,
            ..verify(&token).expect("a fresh token verifies")
        };
        let swapped = format!(
            "{}.{}.{}",
            parts[0],
            base64url::encode_raw(&serde_json::to_vec(&claims).expect("claims")),
            parts[2]
        );
        assert!(verify(&swapped).is_err(), "a swapped payload");

        // `alg: none`.
        let none_header = base64url::encode_raw(br#"{"alg":"none","typ":"JWT"}"#);
        let alg_none = format!("{none_header}.{}.{}", parts[1], parts[2]);
        assert!(verify(&alg_none).is_err(), "alg none");

        // Not three parts.
        assert!(verify("only.two").is_err());
    }

    #[test]
    fn an_expired_token_is_rejected() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs() as i64;
        let claims = Claims {
            exp: Some(now - 1),
            nbf: Some(now - 100),
            scp: "Actions.Results:1:2".to_string(),
            task_id: 7,
            run_id: 1,
            job_id: 2,
            ac: String::new(),
        };
        assert!(verify(&sign(&claims).expect("a token")).is_err());

        // And one that is not valid yet.
        let future = Claims {
            exp: Some(now + 100),
            nbf: Some(now + 50),
            ..claims
        };
        assert!(verify(&sign(&future).expect("a token")).is_err());
    }

    #[test]
    fn the_cache_scopes_round_trip() {
        let claims = Claims {
            ac: r#"[{"Scope":"","Permission":1}]"#.to_string(),
            ..Claims::default()
        };
        let scopes = claims.cache_scopes().expect("scopes");
        assert_eq!(scopes[0].permission, CachePermission::Read);
        assert_eq!(scopes[0].scope, "");
        // An absent claim is an empty list, not an error.
        assert!(Claims::default().cache_scopes().expect("empty").is_empty());
        // Garbage is an error rather than an empty list.
        assert!(Claims {
            ac: "not json".into(),
            ..Claims::default()
        }
        .cache_scopes()
        .is_err());
    }
}
