//! C++ SocketCore TLS (OpenSSL): rustls ClientConfig from `--ca-certificate` /
//! `--check-certificate` / `--min-tls-version`.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use std::sync::Arc;

pub fn client_config(opts: &OptionSet) -> Result<Arc<rustls::ClientConfig>> {
    let provider = rustls::crypto::ring::default_provider();
    let versions: &[&'static rustls::SupportedProtocolVersion] =
        match opts.get("min-tls-version").unwrap_or("TLSv1.2") {
            "TLSv1.3" => &[&rustls::version::TLS13],
            _ => &[&rustls::version::TLS12, &rustls::version::TLS13],
        };
    let mut cfg = if !opts.bool("check-certificate", true) {
        rustls::ClientConfig::builder_with_provider(provider.into())
            .with_protocol_versions(versions)
            .map_err(|e| Error::Http(format!("tls: {e}")))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerify))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        if let Some(path) = opts.get("ca-certificate").filter(|s| !s.is_empty()) {
            let pem = std::fs::read(path).map_err(|e| Error::Http(format!("ca-certificate: {e}")))?;
            for cert in pem_certs(&pem)? {
                let _ = roots.add(cert);
            }
        } else {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        rustls::ClientConfig::builder_with_provider(provider.into())
            .with_protocol_versions(versions)
            .map_err(|e| Error::Http(format!("tls: {e}")))?
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(cfg))
}

fn pem_certs(pem: &[u8]) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let text = std::str::from_utf8(pem).map_err(|_| Error::Http("ca-certificate not utf-8".into()))?;
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("-----BEGIN CERTIFICATE-----") {
        rest = &rest[i + "-----BEGIN CERTIFICATE-----".len()..];
        let Some(end) = rest.find("-----END CERTIFICATE-----") else {
            return Err(Error::Http("truncated ca-certificate".into()));
        };
        let b64: String = rest[..end].chars().filter(|c| !c.is_whitespace()).collect();
        out.push(rustls::pki_types::CertificateDer::from(decode_b64(&b64)?));
        rest = &rest[end + 1..];
    }
    if out.is_empty() {
        return Err(Error::Http("no certificates in ca-certificate".into()));
    }
    Ok(out)
}

fn decode_b64(s: &str) -> Result<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = [0u8; 4];
    let mut bi = 0;
    for &c in bytes {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else {
            continue;
        };
        buf[bi] = v;
        bi += 1;
        if bi == 4 {
            out.push((buf[0] << 2) | (buf[1] >> 4));
            out.push((buf[1] << 4) | (buf[2] >> 2));
            out.push((buf[2] << 6) | buf[3]);
            bi = 0;
        }
    }
    if bi >= 2 {
        out.push((buf[0] << 2) | (buf[1] >> 4));
    }
    if bi >= 3 {
        out.push((buf[1] << 4) | (buf[2] >> 2));
    }
    Ok(out)
}

#[derive(Debug)]
struct SkipVerify;

impl rustls::client::danger::ServerCertVerifier for SkipVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
