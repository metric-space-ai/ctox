//! Published Swiss commercial-register notices, not a current-register snapshot.
//! API contract: https://www.amtsblattportal.ch/docs/api/
use super::{
    Confidence, Country, FieldEvidence, FieldKey, ShapedQuery, SourceCtx, SourceError, SourceHit,
    SourceModule, Tier,
};
use anyhow::anyhow;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::{Duration, Instant};

const ORIGIN: &str = "https://amtsblattportal.ch";
const MAX_BYTES: usize = 1_048_576;
const MAX_NOTICES: usize = 20;
const PREFIX: &str = "shab_notice:";
static MODULE: Shab = Shab;
struct Shab;
pub fn module() -> &'static dyn SourceModule {
    &MODULE
}

impl SourceModule for Shab {
    fn id(&self) -> &'static str {
        "shab.ch"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["shab", "schweizerisches-handelsamtsblatt"]
    }
    fn host_suffixes(&self) -> &'static [&'static str] {
        &["amtsblattportal.ch"]
    }
    fn scrape_target_key(&self) -> Option<&'static str> {
        Some("shab-ch")
    }
    fn tier(&self) -> Tier {
        Tier::P
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::Ch]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[
            FieldKey::FirmaName,
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::FirmaLand,
            FieldKey::FirmaGeschaeftstaetigkeit,
        ]
    }
    fn shape_query(&self, _: &str, _: &SourceCtx<'_>) -> Option<ShapedQuery> {
        None
    }
    fn fetch_direct(
        &self,
        ctx: &SourceCtx<'_>,
        company: &str,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        if matches!(ctx.country, Some(c) if c != Country::Ch) {
            return None;
        }
        Some(search(company))
    }
    fn extract_from_hits(
        &self,
        _: &SourceCtx<'_>,
        company: &str,
        hits: &[SourceHit],
    ) -> Vec<(FieldKey, FieldEvidence)> {
        hits.iter()
            .flat_map(|hit| {
                if !valid_detail_url(&hit.url) || hit.snippet.len() > 16_384 {
                    return Vec::new();
                }
                let Some(raw) = hit.snippet.strip_prefix(PREFIX) else {
                    return Vec::new();
                };
                let Ok(notice) = serde_json::from_str::<Notice>(raw) else {
                    return Vec::new();
                };
                if normalized(&notice.name) != normalized(company)
                    || !valid_date(&notice.date)
                    || !valid_uid(&notice.uid)
                {
                    return Vec::new();
                }
                notice.fields(&hit.url)
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Notice {
    name: String,
    uid: String,
    date: String,
    street: String,
    number: String,
    zip: String,
    town: String,
    purpose: String,
}
impl Notice {
    fn fields(&self, url: &str) -> Vec<(FieldKey, FieldEvidence)> {
        let address = format!("{} {}", self.street, self.number).trim().to_owned();
        [(FieldKey::FirmaName, self.name.clone()), (FieldKey::FirmaAnschrift, address),
         (FieldKey::FirmaPlz, self.zip.clone()), (FieldKey::FirmaOrt, self.town.clone()),
         (FieldKey::FirmaLand, "CH".into()), (FieldKey::FirmaGeschaeftstaetigkeit, self.purpose.clone())]
            .into_iter().filter(|(_, value)| !value.trim().is_empty() && value.len() <= 4096)
            .map(|(key, value)| (key, FieldEvidence { value, confidence: Confidence::Medium,
                source_url: url.into(), note: Some(format!("SHAB publication {}; UID {}; historical notice, not confirmation of current register state", self.date, self.uid)) })).collect()
    }
}

fn invalid(detail: &str) -> SourceError {
    SourceError::ParseFailed {
        detail: detail.into(),
    }
}
fn normalized(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn valid_uid(value: &str) -> bool {
    let Some(digits) = value.strip_prefix("CHE-") else {
        return false;
    };
    digits.len() == 11
        && digits.bytes().enumerate().all(|(i, b)| {
            if i == 3 || i == 7 {
                b == b'.'
            } else {
                b.is_ascii_digit()
            }
        })
}
fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}
fn valid_detail_url(value: &str) -> bool {
    let Some(id) = value
        .strip_prefix("https://amtsblattportal.ch/api/v1/publications/")
        .and_then(|v| v.strip_suffix("/xml"))
    else {
        return false;
    };
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn child<'a, 'b>(node: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}
fn value(node: Node<'_, '_>, name: &str) -> String {
    child(node, name)
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_owned()
}
fn document(xml: &str) -> Result<Document<'_>, SourceError> {
    if xml.len() > MAX_BYTES || xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return Err(invalid("XML size or DTD restriction"));
    }
    Document::parse(xml).map_err(|_| invalid("invalid SHAB XML"))
}

fn list_refs(xml: &str) -> Result<Vec<String>, SourceError> {
    let doc = document(xml)?;
    let root = doc.root_element();
    if root.tag_name().name() != "bulk-export" {
        return Err(invalid("missing bulk-export"));
    }
    let total = value(root, "total")
        .parse::<usize>()
        .map_err(|_| invalid("missing publication total"))?;
    if total > MAX_NOTICES {
        return Err(SourceError::Other(anyhow!(
            "shab partial_output: search exceeds bounded notice budget; refine company identity"
        )));
    }
    let mut refs = Vec::new();
    for publication in root.children().filter(|n| n.has_tag_name("publication")) {
        let url = publication
            .attribute("ref")
            .ok_or_else(|| invalid("missing publication reference"))?;
        if !valid_detail_url(url) {
            return Err(invalid("invalid publication origin or id"));
        }
        refs.push(url.to_owned());
    }
    if refs.len() != total {
        return Err(invalid("incomplete publication page"));
    }
    Ok(refs)
}

fn parse_notice(xml: &str, company: &str) -> Result<Option<Notice>, SourceError> {
    let doc = document(xml)?;
    let root = doc.root_element();
    let meta = child(root, "meta").ok_or_else(|| invalid("missing publication metadata"))?;
    if value(meta, "publicationState") != "PUBLISHED" {
        return Ok(None);
    }
    if value(meta, "rubric") != "HR" || value(meta, "primaryTenantCode") != "shab" {
        return Err(invalid("not a SHAB commercial-register notice"));
    }
    let date = value(meta, "publicationDate");
    if !valid_date(&date) {
        return Err(invalid("missing publication date"));
    }
    let content = child(root, "content").ok_or_else(|| invalid("missing publication content"))?;
    // Never merge old/current blocks or revisionCompany (the auditor) into the subject.
    let block = child(content, "commonsNew")
        .or_else(|| child(content, "commonsActual"))
        .ok_or_else(|| invalid("unsupported commercial-register notice layout"))?;
    let subject = child(block, "company").ok_or_else(|| invalid("missing subject company"))?;
    let name = value(subject, "name");
    if normalized(&name) != normalized(company) {
        return Ok(None);
    }
    let uid = value(subject, "uid");
    if !valid_uid(&uid) {
        return Err(invalid("invalid company UID"));
    }
    let address = child(subject, "address");
    let address_value = |key| address.map(|a| value(a, key)).unwrap_or_default();
    Ok(Some(Notice {
        name,
        uid,
        date,
        street: address_value("street"),
        number: address_value("houseNumber"),
        zip: address_value("swissZipCode"),
        town: address_value("town"),
        purpose: value(block, "purpose"),
    }))
}

fn get_xml(agent: &ureq::Agent, url: &str, deadline: Instant) -> Result<String, SourceError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| SourceError::Network(anyhow!("SHAB request budget exhausted")))?;
    let response = agent
        .get(url)
        .timeout(remaining.min(Duration::from_secs(10)))
        .set("accept", "application/xml")
        .call();
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(429, _)) => {
            return Err(SourceError::RateLimited {
                retry_after_ms: None,
            })
        }
        Err(ureq::Error::Status(code @ (401 | 403), _)) => {
            return Err(SourceError::Blocked {
                reason: format!("SHAB HTTP {code}"),
            })
        }
        Err(ureq::Error::Status(code, _)) => {
            return Err(SourceError::Other(anyhow!("SHAB HTTP {code}")))
        }
        Err(_) => return Err(SourceError::Network(anyhow!("SHAB transport failed"))),
    };
    if response.status() != 200 {
        return Err(SourceError::Other(anyhow!("SHAB unexpected HTTP status")));
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| SourceError::Network(anyhow!("SHAB response read failed")))?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("SHAB response exceeds byte budget"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("SHAB response is not UTF-8"))
}
fn search(company: &str) -> Result<Vec<SourceHit>, SourceError> {
    let company = company.trim();
    if company.is_empty() || company.len() > 256 {
        return Err(invalid("company name required, maximum 256 bytes"));
    }
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(10))
        .user_agent("CTOX research adapter")
        .resolver(crate::egress::SsrfResolver::new(Vec::new()))
        .build();
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut url = url::Url::parse(&format!("{ORIGIN}/api/v1/publications/xml"))
        .map_err(|_| invalid("invalid API origin"))?;
    url.query_pairs_mut().extend_pairs([
        ("publicationStates", "PUBLISHED"),
        ("tenant", "shab"),
        ("rubrics", "HR"),
        ("title", company),
        ("pageRequest.page", "0"),
        ("pageRequest.size", "20"),
        (
            "pageRequest.sortOrders",
            "column:PUBLICATION_DATE|direction:DESC",
        ),
    ]);
    let refs = list_refs(&get_xml(&agent, url.as_str(), deadline)?)?;
    let mut notices = Vec::new();
    for url in refs {
        if let Some(notice) = parse_notice(&get_xml(&agent, &url, deadline)?, company)? {
            notices.push((url, notice));
        }
    }
    if notices.is_empty() {
        return Err(SourceError::NoMatch);
    }
    let uid = &notices[0].1.uid;
    if notices.iter().any(|(_, n)| &n.uid != uid) {
        return Err(invalid("ambiguous company identity: multiple UIDs"));
    }
    notices.sort_by(|a, b| b.1.date.cmp(&a.1.date));
    // Multiple notices on one date can conflict; preserve each dated source rather than guessing order.
    let newest = notices[0].1.date.clone();
    notices
        .into_iter()
        .filter(|(_, n)| n.date == newest)
        .map(|(url, n)| {
            let snippet = format!(
                "{PREFIX}{}",
                serde_json::to_string(&n).map_err(|_| invalid("notice serialization failed"))?
            );
            if snippet.len() > 16_384 {
                return Err(invalid("notice fields exceed budget"));
            }
            Ok(SourceHit {
                title: n.name,
                url,
                snippet,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const XML: &str = r#"<publication><meta><rubric>HR</rubric><primaryTenantCode>shab</primaryTenantCode><publicationState>PUBLISHED</publicationState><publicationDate>2026-09-02</publicationDate></meta><content><commonsNew><company><name>Example AG</name><uid>CHE-123.456.789</uid><address><street>Teststrasse</street><houseNumber>24</houseNumber><swissZipCode>8000</swissZipCode><town>Zürich</town></address></company><purpose>Manufacturing</purpose><revisionCompany><name>Auditor AG</name></revisionCompany></commonsNew><commonsActual><company><name>Old Example AG</name></company></commonsActual></content></publication>"#;
    #[test]
    fn exact_subject_not_parent_old_name_or_auditor() {
        let notice = parse_notice(XML, "Example AG").unwrap().unwrap();
        assert_eq!(notice.town, "Zürich");
        assert_eq!(notice.number, "24");
        for name in [
            "Example GmbH",
            "Old Example AG",
            "Auditor AG",
            "Example",
            "Example Pharma AG",
        ] {
            assert!(parse_notice(XML, name).unwrap().is_none());
        }
        assert!(notice
            .fields("https://amtsblattportal.ch/")
            .iter()
            .all(|(_, f)| f.note.as_ref().unwrap().contains("2026-09-02")));
    }
    #[test]
    fn cancelled_malformed_and_dtd_do_not_produce_fields() {
        assert!(
            parse_notice(&XML.replace("PUBLISHED", "CANCELLED"), "Example AG")
                .unwrap()
                .is_none()
        );
        assert!(parse_notice(&XML.replace("CHE-123.456.789", "bad"), "Example AG").is_err());
        assert!(document("<!DOCTYPE foo><foo/>").is_err());
        assert!(document(&"x".repeat(MAX_BYTES + 1)).is_err());
        assert!(document("<a><b></a>").is_err());
    }
    #[test]
    fn list_bounds_and_reference_origin_fail_closed() {
        assert!(list_refs("<bulk-export><total>21</total></bulk-export>").is_err());
        assert!(list_refs("<bulk-export><total>1</total></bulk-export>").is_err());
        assert!(list_refs("<bulk-export><total>0</total></bulk-export>")
            .unwrap()
            .is_empty());
        assert!(!valid_detail_url(
            "https://amtsblattportal.ch.evil.test/api/v1/publications/a/xml"
        ));
        assert!(!valid_detail_url(
            "http://127.0.0.1/api/v1/publications/a/xml"
        ));
        assert!(valid_detail_url("https://amtsblattportal.ch/api/v1/publications/365d4bec-183a-4f4a-a7ad-411c95bc2430/xml"));
    }
    #[test]
    #[ignore = "real public API; operator admission required"]
    fn live_smoke() {
        assert!(!search("Novartis Pharma Schweiz AG").unwrap().is_empty());
    }
}
