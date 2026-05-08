//! Open-access full-text fetcher.
//!
//! When a resolver-resolved source carries a `url_full_text` and a
//! permissive (or no-restriction) license, this module downloads the
//! payload, extracts text via the existing `tools/pdf-parse` crate (for
//! PDFs) or a basic HTML→text strip (for HTML), and returns markdown the
//! caller can persist into `report_evidence_register.full_text_md`.
//!
//! No closed-access fetching, no auth, no scraping bypasses. The caller
//! is expected to have already vetted the license — this module just
//! does the fetch + extract once that decision is made.

use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// Outcome of a single full-text fetch attempt.
#[derive(Debug, Clone)]
pub struct FullTextFetch {
    /// Markdown-ish plain text extracted from the source.
    pub markdown: String,
    /// Provenance label that goes into `full_text_source` so the
    /// operator can later see how the text was obtained. One of
    /// `"open_access_pdf"`, `"open_access_html"`, `"web_read"`.
    pub source_label: String,
}

/// Decide whether the supplied license string permits an automated
/// full-text download. Conservative — anything we can't recognise as a
/// CC / public-domain / open-access license returns `false`.
pub fn license_permits_open_access(license: Option<&str>) -> bool {
    let Some(value) = license else {
        return false;
    };
    let lower = value.to_ascii_lowercase();
    let permits = [
        "creativecommons.org/licenses/by",
        "creativecommons.org/publicdomain",
        "cc-by",
        "cc0",
        "publicdomain",
        "open-access",
        "openaccess",
    ];
    permits.iter().any(|needle| lower.contains(needle))
}

fn build_agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(timeout)
        .timeout_write(Duration::from_secs(15))
        .user_agent("CTOX deep-research full-text fetcher (+https://github.com/metric-space-ai/ctox)")
        .build()
}

/// Fetch + extract the full text from `url`. The URL must already be
/// vetted as open-access by the caller. PDF detection is by content type
/// (`application/pdf`) with a `.pdf` URL fallback. HTML pages are
/// stripped to plain text by removing tags and collapsing whitespace.
pub fn fetch_full_text(url: &str) -> Result<FullTextFetch> {
    let agent = build_agent(Duration::from_secs(60));
    let response = agent
        .get(url)
        .call()
        .with_context(|| format!("HTTP GET failed for full-text URL {url}"))?;

    let content_type = response
        .header("content-type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let url_lower = url.to_ascii_lowercase();
    let looks_pdf =
        content_type.contains("application/pdf") || url_lower.ends_with(".pdf");

    if looks_pdf {
        let mut bytes: Vec<u8> = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .with_context(|| format!("read PDF body from {url}"))?;
        if bytes.len() < 1024 {
            return Err(anyhow!(
                "PDF response from {url} is suspiciously small ({} bytes); aborting",
                bytes.len()
            ));
        }
        let parsed = ctox_pdf_parse::parse_pdf_bytes(
            &bytes,
            ctox_pdf_parse::LiteParseConfigOverrides::default(),
        )
        .map_err(|err| anyhow!("PDF parse error for {url}: {err}"))?;
        let text = parsed.text.trim().to_string();
        if text.is_empty() {
            return Err(anyhow!(
                "PDF parser returned empty text for {url}; the PDF may be a scanned image"
            ));
        }
        Ok(FullTextFetch {
            markdown: text,
            source_label: "open_access_pdf".to_string(),
        })
    } else {
        let body = response
            .into_string()
            .with_context(|| format!("read HTML body from {url}"))?;
        let stripped = strip_html_to_text(&body);
        if stripped.chars().count() < 500 {
            return Err(anyhow!(
                "HTML response from {url} produced too little text after stripping ({} chars)",
                stripped.chars().count()
            ));
        }
        Ok(FullTextFetch {
            markdown: stripped,
            source_label: "open_access_html".to_string(),
        })
    }
}

/// Coarse HTML-to-text: strip `<script>` / `<style>` blocks, drop tags,
/// decode the few common entities, collapse whitespace. Good enough as
/// a fallback when no PDF is available; the LLM only needs paragraphs
/// of body text, not formatting.
fn strip_html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut work = String::with_capacity(html.len());
    let mut i = 0usize;
    let bytes = html.as_bytes();
    let lower_bytes = lower.as_bytes();
    while i < bytes.len() {
        if i + 8 < bytes.len() && &lower_bytes[i..i + 7] == b"<script" {
            if let Some(end) = lower[i..].find("</script>") {
                i += end + "</script>".len();
                continue;
            }
        }
        if i + 7 < bytes.len() && &lower_bytes[i..i + 6] == b"<style" {
            if let Some(end) = lower[i..].find("</style>") {
                i += end + "</style>".len();
                continue;
            }
        }
        work.push(bytes[i] as char);
        i += 1;
    }
    // Tag strip.
    let mut out = String::with_capacity(work.len());
    let mut in_tag = false;
    for ch in work.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    // Collapse whitespace.
    let mut collapsed = String::with_capacity(decoded.len());
    let mut last_was_space = false;
    for ch in decoded.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                collapsed.push(' ');
            }
            last_was_space = true;
        } else {
            collapsed.push(ch);
            last_was_space = false;
        }
    }
    collapsed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_recognises_cc_by() {
        assert!(license_permits_open_access(Some(
            "https://creativecommons.org/licenses/by/4.0"
        )));
        assert!(license_permits_open_access(Some("CC-BY 4.0")));
        assert!(license_permits_open_access(Some(
            "https://creativecommons.org/publicdomain/zero/1.0"
        )));
        assert!(license_permits_open_access(Some("openaccess")));
    }

    #[test]
    fn license_rejects_unknown_or_restrictive() {
        assert!(!license_permits_open_access(None));
        assert!(!license_permits_open_access(Some("All rights reserved")));
        assert!(!license_permits_open_access(Some("proprietary")));
        assert!(!license_permits_open_access(Some("")));
    }

    #[test]
    fn html_strip_removes_tags_scripts_styles() {
        let html = "<html><head><style>body{}</style></head><body>\
            <script>alert(1)</script>\
            <h1>Title</h1><p>Hello &amp; goodbye.</p>\
            </body></html>";
        let text = strip_html_to_text(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello & goodbye"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("body{}"));
        assert!(!text.contains("<"));
    }
}
