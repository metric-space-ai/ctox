// ref: internal/auth/vertex/keyutil.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use aws_lc_rs::rsa::KeyPair as RsaKeyPair;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use pkcs8::der::Decode;
use pkcs8::PrivateKeyInfo;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAccountNormalizeError {
    InvalidJson,
    EmptyPayload,
    MissingPrivateKey,
    MissingPemMarkers,
    EmptyPemPayload,
    InvalidBase64,
    InvalidRsa,
    UnsupportedKeyFormat,
    Encode,
}

impl fmt::Display for ServiceAccountNormalizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidJson => "service account JSON is invalid",
            Self::EmptyPayload => "service account payload is empty",
            Self::MissingPrivateKey => "service account is missing private_key",
            Self::MissingPemMarkers => "private_key is missing PEM markers",
            Self::EmptyPemPayload => "private_key base64 payload is empty",
            Self::InvalidBase64 => "private_key base64 payload is invalid",
            Self::InvalidRsa => "private_key contains an invalid RSA key",
            Self::UnsupportedKeyFormat => "private_key uses an unsupported key format",
            Self::Encode => "private_key could not be encoded",
        })
    }
}

impl std::error::Error for ServiceAccountNormalizeError {}

pub struct ServiceAccountNormalizeFailure {
    kind: ServiceAccountNormalizeError,
    original: Zeroizing<Vec<u8>>,
}

impl ServiceAccountNormalizeFailure {
    pub fn kind(&self) -> ServiceAccountNormalizeError {
        self.kind
    }

    pub fn original(&self) -> &[u8] {
        &self.original
    }

    pub fn into_original(mut self) -> Vec<u8> {
        std::mem::take(&mut *self.original)
    }
}

impl fmt::Debug for ServiceAccountNormalizeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceAccountNormalizeFailure")
            .field("kind", &self.kind)
            .field("original", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ServiceAccountNormalizeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(formatter)
    }
}

impl std::error::Error for ServiceAccountNormalizeFailure {}

/// Mirrors upstream's fail-with-original contract. Empty input is a successful
/// no-op; every failure returns an error without modifying the caller's bytes.
pub fn normalize_service_account_json(
    raw: &[u8],
) -> Result<Vec<u8>, ServiceAccountNormalizeFailure> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let normalize = || -> Result<Vec<u8>, ServiceAccountNormalizeError> {
        let payload: Map<String, Value> =
            serde_json::from_slice(raw).map_err(|_| ServiceAccountNormalizeError::InvalidJson)?;
        let normalized = normalize_service_account_map(&payload)?;
        serde_json::to_vec(&normalized).map_err(|_| ServiceAccountNormalizeError::InvalidJson)
    };
    normalize().map_err(|kind| ServiceAccountNormalizeFailure {
        kind,
        original: Zeroizing::new(raw.to_vec()),
    })
}

/// Returns a shallow clone with only `private_key` replaced, matching the Go
/// map-copy behavior and leaving all unknown service-account fields intact.
pub fn normalize_service_account_map(
    service_account: &Map<String, Value>,
) -> Result<Map<String, Value>, ServiceAccountNormalizeError> {
    if service_account.is_empty() {
        return Err(ServiceAccountNormalizeError::EmptyPayload);
    }
    let private_key = service_account
        .get("private_key")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ServiceAccountNormalizeError::MissingPrivateKey)?;
    let normalized = sanitize_private_key(private_key)?;
    let mut cloned = service_account.clone();
    cloned.insert("private_key".to_owned(), Value::String(normalized));
    Ok(cloned)
}

/// Normalizes PKCS#1 and RSA PKCS#8 input to an LF-terminated
/// `RSA PRIVATE KEY` PEM, just like upstream's `ensureRSAPrivateKey`.
pub fn sanitize_private_key(raw: &str) -> Result<String, ServiceAccountNormalizeError> {
    let line_normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned = Zeroizing::new(strip_ansi_escape(&line_normalized).trim().to_owned());
    let reconstructed;
    let pem = if pem_parts(&cleaned).is_some() {
        cleaned.as_str()
    } else {
        reconstructed = Zeroizing::new(rebuild_pem(&cleaned)?);
        reconstructed.as_str()
    };

    let (label, der) = pem_parts(pem).ok_or(ServiceAccountNormalizeError::MissingPemMarkers)?;
    let pkcs1_der = match label {
        "RSA PRIVATE KEY" => {
            RsaKeyPair::from_der(&der).map_err(|_| ServiceAccountNormalizeError::InvalidRsa)?;
            der.to_vec()
        }
        "PRIVATE KEY" => {
            RsaKeyPair::from_pkcs8(&der)
                .map_err(|_| ServiceAccountNormalizeError::UnsupportedKeyFormat)?;
            PrivateKeyInfo::from_der(&der)
                .map_err(|_| ServiceAccountNormalizeError::UnsupportedKeyFormat)?
                .private_key
                .to_vec()
        }
        _ => {
            if RsaKeyPair::from_der(&der).is_ok() {
                der.to_vec()
            } else {
                RsaKeyPair::from_pkcs8(&der)
                    .map_err(|_| ServiceAccountNormalizeError::UnsupportedKeyFormat)?;
                PrivateKeyInfo::from_der(&der)
                    .map_err(|_| ServiceAccountNormalizeError::UnsupportedKeyFormat)?
                    .private_key
                    .to_vec()
            }
        }
    };
    Ok(encode_pem("RSA PRIVATE KEY", &pkcs1_der))
}

fn rebuild_pem(raw: &str) -> Result<String, ServiceAccountNormalizeError> {
    let kind = if raw.contains("RSA PRIVATE KEY") {
        "RSA PRIVATE KEY"
    } else {
        "PRIVATE KEY"
    };
    let header = format!("-----BEGIN {kind}-----");
    let footer = format!("-----END {kind}-----");
    let start = raw
        .find(&header)
        .ok_or(ServiceAccountNormalizeError::MissingPemMarkers)?;
    let body_start = start + header.len();
    let end = raw[body_start..]
        .find(&footer)
        .map(|offset| body_start + offset)
        .ok_or(ServiceAccountNormalizeError::MissingPemMarkers)?;
    if end <= start {
        return Err(ServiceAccountNormalizeError::MissingPemMarkers);
    }
    let payload: String = raw[body_start..end]
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
        })
        .collect();
    if payload.is_empty() {
        return Err(ServiceAccountNormalizeError::EmptyPemPayload);
    }
    let der = STANDARD
        .decode(payload)
        .map_err(|_| ServiceAccountNormalizeError::InvalidBase64)?;
    Ok(encode_pem(kind, &der))
}

fn pem_parts(value: &str) -> Option<(&str, Zeroizing<Vec<u8>>)> {
    let begin = value.find("-----BEGIN ")? + "-----BEGIN ".len();
    let label_end = value[begin..].find("-----").map(|offset| begin + offset)?;
    let label = &value[begin..label_end];
    let body_start = label_end + "-----".len();
    let footer = format!("-----END {label}-----");
    let body_end = value[body_start..]
        .find(&footer)
        .map(|offset| body_start + offset)?;
    let payload: String = value[body_start..body_end]
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
        })
        .collect();
    let der = STANDARD.decode(payload).ok()?;
    Some((label, Zeroizing::new(der)))
}

fn encode_pem(label: &str, der: &[u8]) -> String {
    let encoded = STANDARD.encode(der);
    let mut output = format!("-----BEGIN {label}-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        // Base64 is ASCII by construction.
        output.push_str(std::str::from_utf8(chunk).expect("base64 is valid ASCII"));
        output.push('\n');
    }
    output.push_str(&format!("-----END {label}-----\n"));
    output
}

fn strip_ansi_escape(value: &str) -> String {
    let characters: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '\u{1b}' {
            output.push(characters[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= characters.len() {
            continue;
        }
        match characters[index] {
            ']' => {
                index += 1;
                while index < characters.len() {
                    if characters[index] == '\u{7}' {
                        index += 1;
                        break;
                    }
                    if characters[index] == '\u{1b}' && characters.get(index + 1) == Some(&'\\') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            '[' => {
                index += 1;
                while index < characters.len() {
                    let character = characters[index];
                    index += 1;
                    if character.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            _ => index += 1,
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
    use aws_lc_rs::rsa::KeySize;

    use super::*;

    fn key() -> RsaKeyPair {
        RsaKeyPair::generate(KeySize::Rsa2048).unwrap()
    }

    fn pkcs8_pem(key: &RsaKeyPair) -> String {
        let der = AsDer::<Pkcs8V1Der>::as_der(key).unwrap();
        encode_pem("PRIVATE KEY", der.as_ref())
    }

    fn pkcs1_pem(key: &RsaKeyPair) -> String {
        let der = AsDer::<Pkcs8V1Der>::as_der(key).unwrap();
        let info = PrivateKeyInfo::from_der(der.as_ref()).unwrap();
        encode_pem("RSA PRIVATE KEY", info.private_key)
    }

    fn assert_valid_pkcs1(value: &str) {
        let (label, der) = pem_parts(value).unwrap();
        assert_eq!(label, "RSA PRIVATE KEY");
        RsaKeyPair::from_der(&der).unwrap();
    }

}
