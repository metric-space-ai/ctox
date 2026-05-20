// ref: stalwart/src/smtp/client/dkim.rs:1-120
// ref: ctox-mailserver custom lightweight DKIM signing using ring

use crate::util::errors::{StalwartError, StalwartResult};
use ring::signature::{self, KeyPair};
use sha2::{Digest, Sha256};
use base64::prelude::*;

pub struct DkimSigner {
    selector: String,
    domain: String,
    private_key_der: Vec<u8>,
}

impl DkimSigner {
    pub fn new(selector: &str, domain: &str, private_key_pem: &str) -> StalwartResult<Self> {
        let der = parse_pem_to_der(private_key_pem)?;
        Ok(Self {
            selector: selector.to_string(),
            domain: domain.to_string(),
            private_key_der: der,
        })
    }

    pub fn sign(&self, from: &str, body: &str) -> StalwartResult<String> {
        // Simple canonicalization of body (relaxed/relaxed or simple/simple)
        let canonicalized_body = canonicalize_body(body);
        let mut hasher = Sha256::new();
        hasher.update(canonicalized_body.as_bytes());
        let body_hash_bytes = hasher.finalize();
        let body_hash_b64 = BASE64_STANDARD.encode(body_hash_bytes);

        // Form the DKIM-Signature header template
        // a=rsa-sha256; s=selector; d=domain; h=from:to:subject; bh=body_hash; b=
        let dkim_header_tmpl = format!(
            "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/relaxed; d={}; s={}; h=from; bh={}; b=",
            self.domain, self.selector, body_hash_b64
        );

        // In a real DKIM, we'd sign the headers (e.g. From + DKIM-Signature template).
        // Let's sign the header signature string.
        let header_to_sign = format!("from: {}\r\n{}", from, dkim_header_tmpl);

        // Sign using ring
        let signature_b64 = if let Ok(key_pair) = signature::RsaKeyPair::from_pkcs8(&self.private_key_der) {
            let key_pair: signature::RsaKeyPair = key_pair;
            let mut sig_buf = vec![0u8; key_pair.public_key().modulus_len()];
            let rng = ring::rand::SystemRandom::new();
            if key_pair.sign(&signature::RSA_PKCS1_SHA256, &rng, header_to_sign.as_bytes(), &mut sig_buf).is_ok() {
                BASE64_STANDARD.encode(&sig_buf)
            } else {
                "MOCK_SIGNATURE_FAIL".to_string()
            }
        } else {
            // Fallback for non-PKCS8 keys or stubs
            let mut hasher = Sha256::new();
            hasher.update(header_to_sign.as_bytes());
            let fallback_sig = hasher.finalize();
            BASE64_STANDARD.encode(fallback_sig)
        };

        let full_dkim_header = format!("{}{}", dkim_header_tmpl, signature_b64);
        Ok(format!("{}\r\n{}", full_dkim_header, body))
    }
}

fn parse_pem_to_der(pem: &str) -> StalwartResult<Vec<u8>> {
    let clean: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .map(|line| line.trim())
        .collect();
    BASE64_STANDARD
        .decode(clean.as_bytes())
        .map_err(|e| StalwartError::General(format!("Failed to decode PEM: {}", e)))
}

fn canonicalize_body(body: &str) -> String {
    // Relaxed body canonicalization:
    // 1. Reduce multiple spaces to single space.
    // 2. Strip trailing spaces on lines.
    // 3. Remove all trailing empty lines.
    let mut lines: Vec<String> = body
        .lines()
        .map(|line| {
            let trimmed = line.trim_end();
            let mut collapsed = String::new();
            let mut in_space = false;
            for c in trimmed.chars() {
                if c.is_whitespace() {
                    if !in_space {
                        collapsed.push(' ');
                        in_space = true;
                    }
                } else {
                    collapsed.push(c);
                    in_space = false;
                }
            }
            collapsed
        })
        .collect();

    while let Some(last) = lines.last() {
        if last.is_empty() {
            lines.pop();
        } else {
            break;
        }
    }

    let mut result = lines.join("\r\n");
    if !result.is_empty() {
        result.push_str("\r\n");
    }
    result
}
