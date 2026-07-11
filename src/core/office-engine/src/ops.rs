// Origin: CTOX
// License: AGPL-3.0-only
//
// Deterministic OOXML batch operations ("Ebene B" in
// docs/ctox-office-skills-adaptation-plan.md). These transform the package
// directly without the editor: review-lifecycle finalization, privacy
// scrubbing, protection, and structural audits. Layout-affecting work stays
// on the editor-flow surface; nothing here re-renders or reflows content.
//
// All operations preserve untouched package parts byte-identically: the zip
// is re-emitted entry by entry and only explicitly transformed parts change.

use anyhow::{bail, Context};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Story parts that carry tracked changes and comment anchors.
fn is_story_part(name: &str) -> bool {
    name == "word/document.xml"
        || name == "word/footnotes.xml"
        || name == "word/endnotes.xml"
        || (name.starts_with("word/header") && name.ends_with(".xml"))
        || (name.starts_with("word/footer") && name.ends_with(".xml"))
}

fn read_parts(package: &[u8]) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    let mut archive =
        ZipArchive::new(Cursor::new(package)).context("failed to open OOXML package as zip")?;
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read zip entry {index}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read package part {name}"))?;
        parts.insert(name, bytes);
    }
    Ok(parts)
}

fn write_parts(parts: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, bytes) in parts {
        writer
            .start_file(name.as_str(), options)
            .with_context(|| format!("failed to start zip entry {name}"))?;
        writer
            .write_all(bytes)
            .with_context(|| format!("failed to write zip entry {name}"))?;
    }
    Ok(writer
        .finish()
        .context("failed to finish zip")?
        .into_inner())
}

/// The transforms below match elements by their conventional `w:` prefix.
/// Word and the CTOX engine always emit that mapping; refuse anything else
/// instead of producing a silently wrong document.
fn ensure_conventional_prefix(xml: &[u8], part: &str) -> anyhow::Result<()> {
    let text = std::str::from_utf8(xml).with_context(|| format!("{part} is not valid UTF-8"))?;
    let doc =
        roxmltree::Document::parse(text).with_context(|| format!("failed to parse {part}"))?;
    let root = doc.root_element();
    if root.lookup_namespace_uri(Some("w")) == Some(W_NS) {
        return Ok(());
    }
    // Parts without any wordprocessingml content (e.g. docProps) are fine.
    if !text.contains(W_NS) {
        return Ok(());
    }
    bail!("{part} does not map the conventional w: prefix; refusing to transform");
}

// ---------------------------------------------------------------------------
// tracked-changes accept
// ---------------------------------------------------------------------------

/// Elements whose entire subtree is removed when accepting revisions:
/// deletions disappear, move sources disappear, and property-change history
/// is discarded in favor of the current formatting.
const ACCEPT_DROP: &[&str] = &[
    "w:del",
    "w:moveFrom",
    "w:moveFromRangeStart",
    "w:moveFromRangeEnd",
    "w:rPrChange",
    "w:pPrChange",
    "w:sectPrChange",
    "w:tblPrChange",
    "w:tblGridChange",
    "w:trPrChange",
    "w:tcPrChange",
    "w:numberingChange",
    "w:cellDel",
    "w:customXmlDelRangeStart",
    "w:customXmlDelRangeEnd",
];

/// Elements that are unwrapped (children kept) when accepting revisions:
/// insertions and move targets become plain content.
const ACCEPT_UNWRAP: &[&str] = &[
    "w:ins",
    "w:moveTo",
    "w:moveToRangeStart",
    "w:moveToRangeEnd",
    "w:cellIns",
];

pub fn accept_tracked_changes(package: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let transformed = accept_tracked_changes_xml(&xml)
            .with_context(|| format!("failed to accept tracked changes in {name}"))?;
        parts.insert(name, transformed);
    }
    write_parts(&parts)
}

fn accept_tracked_changes_xml(xml: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().expand_empty_elements = false;
    let mut writer = Writer::new(Vec::new());
    let mut skip_depth: usize = 0;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 || ACCEPT_DROP.contains(&name.as_str()) {
                    skip_depth += 1;
                    continue;
                }
                if ACCEPT_UNWRAP.contains(&name.as_str()) {
                    // Children are kept; the wrapper itself is not emitted.
                    // Track unwrapped depth via a sentinel on the stack-free
                    // model: nothing to do because End events are matched by
                    // name below.
                    continue;
                }
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if ACCEPT_UNWRAP.contains(&name.as_str()) {
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if skip_depth > 0
                    || ACCEPT_DROP.contains(&name.as_str())
                    || ACCEPT_UNWRAP.contains(&name.as_str())
                {
                    continue;
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            other => {
                if skip_depth == 0 {
                    writer.write_event(other.into_owned())?;
                }
            }
        }
    }
    Ok(writer.into_inner())
}

// ---------------------------------------------------------------------------
// privacy-scrub
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PrivacyScrubReport {
    pub scrubbed_core_fields: Vec<String>,
    pub rsid_attributes_removed: usize,
    pub rsid_tables_removed: usize,
    pub custom_properties_removed: bool,
}

/// Remove personal metadata: author fields in docProps/core.xml, every
/// `w:rsid*` revision-save attribute, the `w:rsids` table in settings.xml,
/// and docProps/custom.xml entirely.
pub fn privacy_scrub(package: &[u8]) -> anyhow::Result<(Vec<u8>, PrivacyScrubReport)> {
    let mut parts = read_parts(package)?;
    let mut report = PrivacyScrubReport {
        scrubbed_core_fields: Vec::new(),
        rsid_attributes_removed: 0,
        rsid_tables_removed: 0,
        custom_properties_removed: false,
    };

    if let Some(core) = parts.get("docProps/core.xml").cloned() {
        let (scrubbed, fields) = scrub_core_props(&core)?;
        parts.insert("docProps/core.xml".to_string(), scrubbed);
        report.scrubbed_core_fields = fields;
    }

    if parts.remove("docProps/custom.xml").is_some() {
        report.custom_properties_removed = true;
        // Drop the content-type override and relationship for the removed part.
        if let Some(types) = parts.get("[Content_Types].xml").cloned() {
            parts.insert(
                "[Content_Types].xml".to_string(),
                drop_elements_with_attr(&types, "Override", "PartName", "/docProps/custom.xml")?,
            );
        }
        if let Some(rels) = parts.get("_rels/.rels").cloned() {
            parts.insert(
                "_rels/.rels".to_string(),
                drop_elements_with_attr(&rels, "Relationship", "Target", "docProps/custom.xml")?,
            );
        }
    }

    let word_xml_names: Vec<String> = parts
        .keys()
        .filter(|name| name.starts_with("word/") && name.ends_with(".xml"))
        .cloned()
        .collect();
    for name in word_xml_names {
        let xml = parts.get(&name).expect("word part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let (scrubbed, removed_attrs, removed_tables) =
            scrub_rsids_xml(&xml).with_context(|| format!("failed to scrub rsids in {name}"))?;
        report.rsid_attributes_removed += removed_attrs;
        report.rsid_tables_removed += removed_tables;
        parts.insert(name, scrubbed);
    }

    Ok((write_parts(&parts)?, report))
}

fn scrub_core_props(xml: &[u8]) -> anyhow::Result<(Vec<u8>, Vec<String>)> {
    const FIELDS: &[&str] = &["dc:creator", "cp:lastModifiedBy"];
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut inside: Option<String> = None;
    let mut scrubbed = Vec::new();
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if FIELDS.contains(&name.as_str()) {
                    inside = Some(name.clone());
                }
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if inside.as_deref() == Some(name.as_str()) {
                    inside = None;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Text(text) => {
                if let Some(field) = &inside {
                    if !text.as_ref().is_empty() && !scrubbed.contains(field) {
                        scrubbed.push(field.clone());
                    }
                    continue;
                }
                writer.write_event(Event::Text(text.into_owned()))?;
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    Ok((writer.into_inner(), scrubbed))
}

fn scrub_rsids_xml(xml: &[u8]) -> anyhow::Result<(Vec<u8>, usize, usize)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut removed_attrs = 0usize;
    let mut removed_tables = 0usize;
    let mut skip_depth = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 || name == "w:rsids" {
                    if skip_depth == 0 {
                        removed_tables += 1;
                    }
                    skip_depth += 1;
                    continue;
                }
                let (rebuilt, removed) = strip_rsid_attrs(&start)?;
                removed_attrs += removed;
                writer.write_event(Event::Start(rebuilt))?;
            }
            Event::End(end) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 || name == "w:rsids" {
                    if skip_depth == 0 {
                        removed_tables += 1;
                    }
                    continue;
                }
                if name.starts_with("w:rsid") {
                    // Standalone <w:rsid w:val="..."/> entries (settings table
                    // remnants) are dropped entirely.
                    removed_attrs += 1;
                    continue;
                }
                let (rebuilt, removed) = strip_rsid_attrs(&start)?;
                removed_attrs += removed;
                writer.write_event(Event::Empty(rebuilt))?;
            }
            other => {
                if skip_depth == 0 {
                    writer.write_event(other.into_owned())?;
                }
            }
        }
    }
    Ok((writer.into_inner(), removed_attrs, removed_tables))
}

fn strip_rsid_attrs(start: &BytesStart<'_>) -> anyhow::Result<(BytesStart<'static>, usize)> {
    let name = qname_string(start);
    let mut rebuilt = BytesStart::new(name);
    let mut removed = 0usize;
    for attr in start.attributes() {
        let attr = attr.context("bad attribute")?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if key.starts_with("w:rsid") {
            removed += 1;
            continue;
        }
        rebuilt.push_attribute((key.as_str(), attr.unescape_value()?.as_ref()));
    }
    Ok((rebuilt, removed))
}

fn drop_elements_with_attr(
    xml: &[u8],
    element: &str,
    attr_name: &str,
    attr_value: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut skip_depth = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if qname_string(&start) == element && attr_equals(&start, attr_name, attr_value)? {
                    skip_depth += 1;
                    continue;
                }
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                if skip_depth > 0 {
                    continue;
                }
                if qname_string(&start) == element && attr_equals(&start, attr_name, attr_value)? {
                    continue;
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            other => {
                if skip_depth == 0 {
                    writer.write_event(other.into_owned())?;
                }
            }
        }
    }
    Ok(writer.into_inner())
}

fn attr_equals(start: &BytesStart<'_>, attr_name: &str, value: &str) -> anyhow::Result<bool> {
    for attr in start.attributes() {
        let attr = attr.context("bad attribute")?;
        if String::from_utf8_lossy(attr.key.as_ref()) == attr_name {
            return Ok(attr.unescape_value()?.as_ref().contains(value));
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// protection set
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionMode {
    ReadOnly,
    Comments,
    Forms,
    None,
}

impl ProtectionMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "readonly" => Ok(Self::ReadOnly),
            "comments" => Ok(Self::Comments),
            "forms" => Ok(Self::Forms),
            "none" => Ok(Self::None),
            other => bail!("unsupported protection mode: {other} (readonly|comments|forms|none)"),
        }
    }

    fn edit_value(self) -> Option<&'static str> {
        match self {
            Self::ReadOnly => Some("readOnly"),
            Self::Comments => Some("comments"),
            Self::Forms => Some("forms"),
            Self::None => None,
        }
    }
}

/// Set or clear `w:documentProtection` in word/settings.xml. No password hash
/// is written; this is the cooperative protection level Word offers without a
/// password, and it is honest about that.
pub fn set_protection(package: &[u8], mode: ProtectionMode) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let settings = parts
        .get("word/settings.xml")
        .context("package has no word/settings.xml")?
        .clone();
    ensure_conventional_prefix(&settings, "word/settings.xml")?;
    let transformed = set_protection_xml(&settings, mode)?;
    parts.insert("word/settings.xml".to_string(), transformed);
    write_parts(&parts)
}

fn set_protection_xml(xml: &[u8], mode: ProtectionMode) -> anyhow::Result<Vec<u8>> {
    // First drop any existing protection element, then inject the new one
    // directly after the settings root opens.
    let without = drop_elements_with_attr_any(xml, "w:documentProtection")?;
    let Some(edit) = mode.edit_value() else {
        return Ok(without);
    };
    let mut reader = Reader::from_reader(without.as_slice());
    let mut writer = Writer::new(Vec::new());
    let mut injected = false;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let is_root = qname_string(&start) == "w:settings";
                writer.write_event(Event::Start(start.into_owned()))?;
                if is_root && !injected {
                    let mut elem = BytesStart::new("w:documentProtection");
                    elem.push_attribute(("w:edit", edit));
                    elem.push_attribute(("w:enforcement", "1"));
                    writer.write_event(Event::Empty(elem))?;
                    injected = true;
                }
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    if !injected {
        bail!("word/settings.xml has no w:settings root element");
    }
    Ok(writer.into_inner())
}

fn drop_elements_with_attr_any(xml: &[u8], element: &str) -> anyhow::Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut skip_depth = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                if skip_depth > 0 || qname_string(&start) == element {
                    skip_depth += 1;
                    continue;
                }
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                if skip_depth > 0 || qname_string(&start) == element {
                    continue;
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            other => {
                if skip_depth == 0 {
                    writer.write_event(other.into_owned())?;
                }
            }
        }
    }
    Ok(writer.into_inner())
}

fn qname_string(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.name().as_ref()).to_string()
}

// ---------------------------------------------------------------------------
// comments extract
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedComment {
    pub id: String,
    pub author: String,
    pub initials: Option<String>,
    pub date: Option<String>,
    pub text: String,
    pub resolved: Option<bool>,
}

pub fn extract_comments(package: &[u8]) -> anyhow::Result<Vec<ExtractedComment>> {
    let parts = read_parts(package)?;
    let Some(comments_xml) = parts.get("word/comments.xml") else {
        return Ok(Vec::new());
    };
    let comments_text =
        std::str::from_utf8(comments_xml).context("word/comments.xml is not valid UTF-8")?;
    let doc =
        roxmltree::Document::parse(comments_text).context("failed to parse word/comments.xml")?;

    // Resolution state lives in commentsExtended.xml, keyed by the paraId of
    // the comment's last paragraph.
    let mut done_by_para: BTreeMap<String, bool> = BTreeMap::new();
    if let Some(extended_xml) = parts.get("word/commentsExtended.xml") {
        if let Ok(text) = std::str::from_utf8(extended_xml) {
            if let Ok(extended) = roxmltree::Document::parse(text) {
                for node in extended
                    .descendants()
                    .filter(|node| node.tag_name().name() == "commentEx")
                {
                    let para_id = node
                        .attributes()
                        .find(|attr| attr.name() == "paraId")
                        .map(|attr| attr.value().to_string());
                    let done = node
                        .attributes()
                        .find(|attr| attr.name() == "done")
                        .map(|attr| attr.value() == "1")
                        .unwrap_or(false);
                    if let Some(para_id) = para_id {
                        done_by_para.insert(para_id, done);
                    }
                }
            }
        }
    }

    let mut extracted = Vec::new();
    for comment in doc.descendants().filter(|node| {
        node.tag_name().name() == "comment" && node.tag_name().namespace() == Some(W_NS)
    }) {
        let attr = |name: &str| {
            comment
                .attributes()
                .find(|a| a.name() == name)
                .map(|a| a.value().to_string())
        };
        let text: String = comment
            .descendants()
            .filter(|node| node.tag_name().name() == "t")
            .filter_map(|node| node.text())
            .collect::<Vec<_>>()
            .join("");
        let last_para_id = comment
            .descendants()
            .filter(|node| node.tag_name().name() == "p")
            .filter_map(|node| {
                node.attributes()
                    .find(|a| a.name() == "paraId")
                    .map(|a| a.value().to_string())
            })
            .last();
        let resolved = last_para_id.and_then(|id| done_by_para.get(&id).copied());
        extracted.push(ExtractedComment {
            id: attr("id").unwrap_or_default(),
            author: attr("author").unwrap_or_default(),
            initials: attr("initials"),
            date: attr("date"),
            text,
            resolved,
        });
    }
    Ok(extracted)
}

// ---------------------------------------------------------------------------
// a11y audit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct A11yFinding {
    pub code: String,
    pub message: String,
}

pub fn a11y_audit(package: &[u8]) -> anyhow::Result<Vec<A11yFinding>> {
    let parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?;
    let text = std::str::from_utf8(document).context("word/document.xml is not valid UTF-8")?;
    let doc = roxmltree::Document::parse(text).context("failed to parse word/document.xml")?;
    let mut findings = Vec::new();

    // Images without alternative text: wp:docPr carries the description.
    for doc_pr in doc
        .descendants()
        .filter(|node| node.tag_name().name() == "docPr")
    {
        let descr = doc_pr
            .attributes()
            .find(|attr| attr.name() == "descr")
            .map(|attr| attr.value().trim().to_string())
            .unwrap_or_default();
        if descr.is_empty() {
            let name = doc_pr
                .attributes()
                .find(|attr| attr.name() == "name")
                .map(|attr| attr.value().to_string())
                .unwrap_or_else(|| "unnamed drawing".to_string());
            findings.push(A11yFinding {
                code: "image-missing-alt".to_string(),
                message: format!("drawing '{name}' has no alternative text (docPr descr)"),
            });
        }
    }

    // Heading-ladder skips: outline levels must not jump (H1 -> H3).
    let mut last_level: Option<u32> = None;
    for style in doc.descendants().filter(|node| {
        node.tag_name().name() == "pStyle" && node.tag_name().namespace() == Some(W_NS)
    }) {
        let Some(value) = style.attributes().find(|attr| attr.name() == "val") else {
            continue;
        };
        let Some(level) = value
            .value()
            .strip_prefix("Heading")
            .and_then(|rest| rest.parse::<u32>().ok())
        else {
            continue;
        };
        if let Some(previous) = last_level {
            if level > previous + 1 {
                findings.push(A11yFinding {
                    code: "heading-skip".to_string(),
                    message: format!("heading level jumps from {previous} to {level}"),
                });
            }
        }
        last_level = Some(level);
    }

    // Tables whose first row is not marked as a repeating header row.
    for table in doc
        .descendants()
        .filter(|node| node.tag_name().name() == "tbl" && node.tag_name().namespace() == Some(W_NS))
    {
        let Some(first_row) = table.children().find(|node| node.tag_name().name() == "tr") else {
            continue;
        };
        let has_header = first_row
            .descendants()
            .any(|node| node.tag_name().name() == "tblHeader");
        if !has_header {
            findings.push(A11yFinding {
                code: "table-missing-header-row".to_string(),
                message: "table's first row is not marked as a repeating header row (w:tblHeader)"
                    .to_string(),
            });
        }
    }

    Ok(findings)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_docx() -> Vec<u8> {
        let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
  <w:body>
    <w:p w:rsidR="00AB12CD" w:rsidRDefault="00AB12CD">
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>Title</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading3"/></w:pPr>
      <w:r><w:t>Skipped level</w:t></w:r>
    </w:p>
    <w:p>
      <w:ins w:id="1" w:author="Reviewer"><w:r><w:t>inserted text</w:t></w:r></w:ins>
      <w:del w:id="2" w:author="Reviewer"><w:r><w:delText>deleted text</w:delText></w:r></w:del>
      <w:r><w:t>kept text</w:t></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing><wp:inline><wp:docPr id="7" name="Diagram 7"/></wp:inline></w:drawing>
      </w:r>
    </w:p>
    <w:tbl>
      <w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;
        let settings = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:zoom w:percent="100"/>
  <w:rsids>
    <w:rsidRoot w:val="00AB12CD"/>
    <w:rsid w:val="00AB12CD"/>
  </w:rsids>
</w:settings>"#;
        let comments = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">
  <w:comment w:id="1" w:author="Alice" w:initials="AL" w:date="2026-07-11T08:00:00Z">
    <w:p w14:paraId="11112222"><w:r><w:t>Please tighten this paragraph.</w:t></w:r></w:p>
  </w:comment>
</w:comments>"#;
        let comments_extended = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w15:commentsEx xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml">
  <w15:commentEx w15:paraId="11112222" w15:done="1"/>
</w15:commentsEx>"#;
        let core = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:creator>Alice Example</dc:creator>
  <cp:lastModifiedBy>Bob Example</cp:lastModifiedBy>
</cp:coreProperties>"#;
        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>
</Types>"#;
        let custom = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"/>"#;
        let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/>
</Relationships>"#;

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, content) in [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("docProps/core.xml", core),
            ("docProps/custom.xml", custom),
            ("word/document.xml", document),
            ("word/settings.xml", settings),
            ("word/comments.xml", comments),
            ("word/commentsExtended.xml", comments_extended),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn part_text(package: &[u8], name: &str) -> String {
        let parts = read_parts(package).unwrap();
        String::from_utf8(parts.get(name).unwrap().clone()).unwrap()
    }

    #[test]
    fn accept_tracked_changes_keeps_insertions_drops_deletions() {
        let package = build_test_docx();
        let accepted = accept_tracked_changes(&package).unwrap();
        let document = part_text(&accepted, "word/document.xml");
        assert!(document.contains("inserted text"));
        assert!(document.contains("kept text"));
        assert!(!document.contains("deleted text"));
        assert!(!document.contains("<w:ins"));
        assert!(!document.contains("<w:del"));
    }

    #[test]
    fn privacy_scrub_removes_authors_rsids_and_custom_props() {
        let package = build_test_docx();
        let (scrubbed, report) = privacy_scrub(&package).unwrap();
        let core = part_text(&scrubbed, "docProps/core.xml");
        assert!(!core.contains("Alice Example"));
        assert!(!core.contains("Bob Example"));
        let document = part_text(&scrubbed, "word/document.xml");
        assert!(!document.contains("w:rsid"));
        let settings = part_text(&scrubbed, "word/settings.xml");
        assert!(!settings.contains("w:rsids"));
        assert!(read_parts(&scrubbed)
            .unwrap()
            .get("docProps/custom.xml")
            .is_none());
        let content_types = part_text(&scrubbed, "[Content_Types].xml");
        assert!(!content_types.contains("custom.xml"));
        let rels = part_text(&scrubbed, "_rels/.rels");
        assert!(!rels.contains("custom.xml"));
        assert!(report.custom_properties_removed);
        assert!(report.rsid_attributes_removed >= 2);
        assert_eq!(report.rsid_tables_removed, 1);
        assert_eq!(
            report.scrubbed_core_fields,
            vec!["dc:creator".to_string(), "cp:lastModifiedBy".to_string()]
        );
        // Untouched content survives.
        assert!(document.contains("kept text"));
    }

    #[test]
    fn set_protection_injects_and_clears_document_protection() {
        let package = build_test_docx();
        let protected = set_protection(&package, ProtectionMode::ReadOnly).unwrap();
        let settings = part_text(&protected, "word/settings.xml");
        assert!(settings.contains("w:documentProtection"));
        assert!(settings.contains("readOnly"));
        let cleared = set_protection(&protected, ProtectionMode::None).unwrap();
        let settings = part_text(&cleared, "word/settings.xml");
        assert!(!settings.contains("w:documentProtection"));
    }

    #[test]
    fn extract_comments_reads_text_author_and_resolution() {
        let package = build_test_docx();
        let comments = extract_comments(&package).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, "1");
        assert_eq!(comments[0].author, "Alice");
        assert_eq!(comments[0].text, "Please tighten this paragraph.");
        assert_eq!(comments[0].resolved, Some(true));
    }

    #[test]
    fn a11y_audit_finds_missing_alt_heading_skip_and_table_header() {
        let package = build_test_docx();
        let findings = a11y_audit(&package).unwrap();
        let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"image-missing-alt"));
        assert!(codes.contains(&"heading-skip"));
        assert!(codes.contains(&"table-missing-header-row"));
    }

    #[test]
    fn untouched_parts_round_trip_byte_identically() {
        let package = build_test_docx();
        let accepted = accept_tracked_changes(&package).unwrap();
        let before = read_parts(&package).unwrap();
        let after = read_parts(&accepted).unwrap();
        assert_eq!(
            before.get("word/comments.xml"),
            after.get("word/comments.xml")
        );
        assert_eq!(
            before.get("docProps/core.xml"),
            after.get("docProps/core.xml")
        );
    }
}
