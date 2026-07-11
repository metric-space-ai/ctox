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
// tracked-changes reject
// ---------------------------------------------------------------------------

/// Reject all content revisions: insertions and move targets disappear,
/// deletions and move sources are restored to plain content. Formatting
/// revisions (`*PrChange`) would require re-applying the stored previous
/// properties; that is not implemented yet, so their presence is a refusal,
/// not a silent wrong result.
pub fn reject_tracked_changes(package: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in &story_names {
        let xml = parts.get(name).expect("story part present");
        let text = std::str::from_utf8(xml).with_context(|| format!("{name} is not UTF-8"))?;
        if text.contains("PrChange") {
            bail!(
                "{name} contains formatting revisions (*PrChange); rejecting those is not \
                 supported yet — resolve them in the editor first"
            );
        }
    }
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let transformed = reject_tracked_changes_xml(&xml)
            .with_context(|| format!("failed to reject tracked changes in {name}"))?;
        parts.insert(name, transformed);
    }
    write_parts(&parts)
}

const REJECT_DROP: &[&str] = &[
    "w:ins",
    "w:moveTo",
    "w:moveToRangeStart",
    "w:moveToRangeEnd",
    "w:cellIns",
    "w:customXmlInsRangeStart",
    "w:customXmlInsRangeEnd",
];

const REJECT_UNWRAP: &[&str] = &[
    "w:del",
    "w:moveFrom",
    "w:moveFromRangeStart",
    "w:moveFromRangeEnd",
    "w:cellDel",
];

fn reject_tracked_changes_xml(xml: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut skip_depth = 0usize;
    let mut del_depth = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 || REJECT_DROP.contains(&name.as_str()) {
                    skip_depth += 1;
                    continue;
                }
                if REJECT_UNWRAP.contains(&name.as_str()) {
                    if name == "w:del" {
                        del_depth += 1;
                    }
                    continue;
                }
                if del_depth > 0 && name == "w:delText" {
                    // Restored deletion text becomes regular text again.
                    let mut renamed = BytesStart::new("w:t");
                    for attr in start.attributes() {
                        let attr = attr.context("bad attribute")?;
                        renamed.push_attribute((
                            String::from_utf8_lossy(attr.key.as_ref()).as_ref(),
                            attr.unescape_value()?.as_ref(),
                        ));
                    }
                    writer.write_event(Event::Start(renamed))?;
                    continue;
                }
                if del_depth > 0 && name == "w:delInstrText" {
                    writer.write_event(Event::Start(BytesStart::new("w:instrText")))?;
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
                if REJECT_UNWRAP.contains(&name.as_str()) {
                    if name == "w:del" {
                        del_depth = del_depth.saturating_sub(1);
                    }
                    continue;
                }
                if del_depth > 0 && name == "w:delText" {
                    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:t")))?;
                    continue;
                }
                if del_depth > 0 && name == "w:delInstrText" {
                    writer
                        .write_event(Event::End(quick_xml::events::BytesEnd::new("w:instrText")))?;
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if skip_depth > 0
                    || REJECT_DROP.contains(&name.as_str())
                    || REJECT_UNWRAP.contains(&name.as_str())
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
// redact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RedactReport {
    pub replacements: usize,
    pub parts_touched: Vec<String>,
    /// Matching happens within single text nodes; terms split across runs
    /// are not found. Compare `terms_not_found` against expectations.
    pub terms_not_found: Vec<String>,
}

/// Replace matched text with a same-length block-character mask so the
/// layout stays stable. Matches literal terms plus optional e-mail and
/// phone-number patterns, inside story parts and comments.
pub fn redact(
    package: &[u8],
    terms: &[String],
    emails: bool,
    phones: bool,
) -> anyhow::Result<(Vec<u8>, RedactReport)> {
    if terms.is_empty() && !emails && !phones {
        bail!("nothing to redact: pass terms and/or --emails/--phones");
    }
    let mut patterns: Vec<regex::Regex> = Vec::new();
    for term in terms {
        patterns.push(
            regex::Regex::new(&regex::escape(term)).with_context(|| format!("bad term: {term}"))?,
        );
    }
    if emails {
        patterns.push(
            regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
                .expect("static regex"),
        );
    }
    if phones {
        patterns.push(regex::Regex::new(r"\+?\d[\d\s\-()/]{6,}\d").expect("static regex"));
    }

    let mut parts = read_parts(package)?;
    let mut report = RedactReport {
        replacements: 0,
        parts_touched: Vec::new(),
        terms_not_found: Vec::new(),
    };
    let mut found_term = vec![false; terms.len()];

    let target_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name) || name.as_str() == "word/comments.xml")
        .cloned()
        .collect();
    for name in target_names {
        let xml = parts.get(&name).expect("part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let (redacted, count, term_hits) = redact_xml(&xml, &patterns, terms.len())?;
        if count > 0 {
            report.replacements += count;
            report.parts_touched.push(name.clone());
            parts.insert(name, redacted);
        }
        for (index, hit) in term_hits.iter().enumerate() {
            if *hit {
                found_term[index] = true;
            }
        }
    }
    for (index, term) in terms.iter().enumerate() {
        if !found_term[index] {
            report.terms_not_found.push(term.clone());
        }
    }
    Ok((write_parts(&parts)?, report))
}

fn redact_xml(
    xml: &[u8],
    patterns: &[regex::Regex],
    term_count: usize,
) -> anyhow::Result<(Vec<u8>, usize, Vec<bool>)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut in_text_depth = 0usize;
    let mut replacements = 0usize;
    let mut term_hits = vec![false; term_count];
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if name == "w:t" || name == "w:delText" || name == "w:instrText" {
                    in_text_depth += 1;
                }
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if name == "w:t" || name == "w:delText" || name == "w:instrText" {
                    in_text_depth = in_text_depth.saturating_sub(1);
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Text(text) => {
                if in_text_depth > 0 {
                    let value = text.unescape().context("bad text node")?.to_string();
                    let mut masked = value.clone();
                    for (index, pattern) in patterns.iter().enumerate() {
                        let mut result = String::with_capacity(masked.len());
                        let mut last = 0usize;
                        for found in pattern.find_iter(&masked.clone()) {
                            result.push_str(&masked[last..found.start()]);
                            result.extend(masked[found.range()].chars().map(|c| {
                                if c.is_whitespace() {
                                    c
                                } else {
                                    '\u{2588}'
                                }
                            }));
                            last = found.end();
                            replacements += 1;
                            if index < term_count {
                                term_hits[index] = true;
                            }
                        }
                        result.push_str(&masked[last..]);
                        masked = result;
                    }
                    writer.write_event(Event::Text(
                        quick_xml::events::BytesText::new(&masked).into_owned(),
                    ))?;
                    continue;
                }
                writer.write_event(Event::Text(text.into_owned()))?;
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    Ok((writer.into_inner(), replacements, term_hits))
}

// ---------------------------------------------------------------------------
// comments strip / resolve / add
// ---------------------------------------------------------------------------

const COMMENT_PARTS: &[&str] = &[
    "word/comments.xml",
    "word/commentsExtended.xml",
    "word/commentsIds.xml",
    "word/commentsExtensible.xml",
];

const COMMENT_ANCHORS: &[&str] = &[
    "w:commentRangeStart",
    "w:commentRangeEnd",
    "w:commentReference",
];

/// Remove every comment: the comment parts, their content-type overrides and
/// relationships, and the range/reference anchors in the stories.
pub fn strip_comments(package: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let mut removed_any = false;
    for name in COMMENT_PARTS {
        if parts.remove(*name).is_some() {
            removed_any = true;
            let part_name = format!("/{name}");
            if let Some(types) = parts.get("[Content_Types].xml").cloned() {
                parts.insert(
                    "[Content_Types].xml".to_string(),
                    drop_elements_with_attr(&types, "Override", "PartName", &part_name)?,
                );
            }
            let target = name.trim_start_matches("word/");
            if let Some(rels) = parts.get("word/_rels/document.xml.rels").cloned() {
                parts.insert(
                    "word/_rels/document.xml.rels".to_string(),
                    drop_elements_with_attr(&rels, "Relationship", "Target", target)?,
                );
            }
        }
    }
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let mut transformed = xml;
        for anchor in COMMENT_ANCHORS {
            transformed = drop_elements_with_attr_any(&transformed, anchor)?;
        }
        parts.insert(name, transformed);
    }
    if !removed_any {
        // Nothing to strip is fine; the result is simply unchanged content.
    }
    write_parts(&parts)
}

/// Mark a comment (or all comments) as resolved in commentsExtended.xml.
pub fn resolve_comments(package: &[u8], comment_id: Option<&str>) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let comments_xml = parts
        .get("word/comments.xml")
        .context("package has no comments to resolve")?;
    let comments_text =
        std::str::from_utf8(comments_xml).context("word/comments.xml is not valid UTF-8")?;
    let doc =
        roxmltree::Document::parse(comments_text).context("failed to parse word/comments.xml")?;

    // Which paraIds should flip to done: the last paragraph of each targeted
    // comment.
    let mut target_para_ids: Vec<String> = Vec::new();
    for comment in doc.descendants().filter(|node| {
        node.tag_name().name() == "comment" && node.tag_name().namespace() == Some(W_NS)
    }) {
        let id = comment
            .attributes()
            .find(|a| a.name() == "id")
            .map(|a| a.value().to_string())
            .unwrap_or_default();
        if let Some(wanted) = comment_id {
            if id != wanted {
                continue;
            }
        }
        if let Some(para_id) = comment
            .descendants()
            .filter(|node| node.tag_name().name() == "p")
            .filter_map(|node| {
                node.attributes()
                    .find(|a| a.name() == "paraId")
                    .map(|a| a.value().to_string())
            })
            .last()
        {
            target_para_ids.push(para_id);
        }
    }
    if target_para_ids.is_empty() {
        bail!(
            "no matching comment{} found",
            comment_id
                .map(|id| format!(" with id {id}"))
                .unwrap_or_default()
        );
    }

    let extended = parts
        .get("word/commentsExtended.xml")
        .context("package has no word/commentsExtended.xml; cannot store resolution state")?
        .clone();
    let mut reader = Reader::from_reader(extended.as_slice());
    let mut writer = Writer::new(Vec::new());
    let mut flipped = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Empty(start) if qname_string(&start).ends_with("commentEx") => {
                let mut para_id = None;
                for attr in start.attributes() {
                    let attr = attr.context("bad attribute")?;
                    if String::from_utf8_lossy(attr.key.as_ref()).ends_with("paraId") {
                        para_id = Some(attr.unescape_value()?.to_string());
                    }
                }
                let should_flip = para_id
                    .as_deref()
                    .map(|id| target_para_ids.iter().any(|t| t == id))
                    .unwrap_or(false);
                if should_flip {
                    let name = qname_string(&start);
                    let mut rebuilt = BytesStart::new(name);
                    for attr in start.attributes() {
                        let attr = attr.context("bad attribute")?;
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        if key.ends_with("done") {
                            continue;
                        }
                        rebuilt.push_attribute((key.as_str(), attr.unescape_value()?.as_ref()));
                    }
                    rebuilt.push_attribute(("w15:done", "1"));
                    flipped += 1;
                    writer.write_event(Event::Empty(rebuilt))?;
                    continue;
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    if flipped == 0 {
        bail!("comment found but no matching commentEx entry in commentsExtended.xml");
    }
    parts.insert("word/commentsExtended.xml".to_string(), writer.into_inner());
    write_parts(&parts)
}

/// Anchor a new comment on the first paragraph whose text contains
/// `anchor_substring`. Creates word/comments.xml (plus content-type override
/// and relationship) when the package has no comments yet.
pub fn add_comment(
    package: &[u8],
    anchor_substring: &str,
    author: &str,
    text: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?
        .clone();
    ensure_conventional_prefix(&document, "word/document.xml")?;

    // Locate the target paragraph by document order.
    let doc_text = std::str::from_utf8(&document).context("word/document.xml is not UTF-8")?;
    let doc = roxmltree::Document::parse(doc_text).context("failed to parse document.xml")?;
    let mut target_index: Option<usize> = None;
    for (index, paragraph) in doc
        .descendants()
        .filter(|node| node.tag_name().name() == "p" && node.tag_name().namespace() == Some(W_NS))
        .enumerate()
    {
        let text: String = paragraph
            .descendants()
            .filter(|node| node.tag_name().name() == "t")
            .filter_map(|node| node.text())
            .collect();
        if text.contains(anchor_substring) {
            target_index = Some(index);
            break;
        }
    }
    let target_index =
        target_index.with_context(|| format!("no paragraph contains: {anchor_substring}"))?;

    // Next free comment id.
    let next_id = parts
        .get("word/comments.xml")
        .and_then(|xml| std::str::from_utf8(xml).ok().map(str::to_string))
        .and_then(|text| {
            roxmltree::Document::parse(&text).ok().map(|doc| {
                doc.descendants()
                    .filter(|node| node.tag_name().name() == "comment")
                    .filter_map(|node| {
                        node.attributes()
                            .find(|a| a.name() == "id")
                            .and_then(|a| a.value().parse::<u64>().ok())
                    })
                    .max()
                    .map(|max| max + 1)
                    .unwrap_or(0)
            })
        })
        .unwrap_or(0);
    let id = next_id.to_string();

    // Inject the range anchors into the document.
    let transformed = inject_comment_anchors(&document, target_index, &id)?;
    parts.insert("word/document.xml".to_string(), transformed);

    // Append the comment content (creating the part when needed).
    let comment_body = format!(
        "<w:comment w:id=\"{id}\" w:author=\"{}\" w:initials=\"\"><w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:comment>",
        xml_escape(author),
        xml_escape(text)
    );
    match parts.get("word/comments.xml").cloned() {
        Some(existing) => {
            let appended = append_before_root_end(&existing, "w:comments", &comment_body)?;
            parts.insert("word/comments.xml".to_string(), appended);
        }
        None => {
            let part = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:comments xmlns:w=\"{W_NS}\">{comment_body}</w:comments>"
            );
            parts.insert("word/comments.xml".to_string(), part.into_bytes());
            let types = parts
                .get("[Content_Types].xml")
                .context("package has no [Content_Types].xml")?
                .clone();
            parts.insert(
                "[Content_Types].xml".to_string(),
                append_before_root_end(
                    &types,
                    "Types",
                    "<Override PartName=\"/word/comments.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml\"/>",
                )?,
            );
            let rels = parts
                .get("word/_rels/document.xml.rels")
                .context("package has no word/_rels/document.xml.rels")?
                .clone();
            let next_rid = next_relationship_id(&rels)?;
            parts.insert(
                "word/_rels/document.xml.rels".to_string(),
                append_before_root_end(
                    &rels,
                    "Relationships",
                    &format!(
                        "<Relationship Id=\"{next_rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"comments.xml\"/>"
                    ),
                )?,
            );
        }
    }
    write_parts(&parts)
}

fn inject_comment_anchors(xml: &[u8], target_index: usize, id: &str) -> anyhow::Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut para_index: isize = -1;
    let mut para_depth = 0usize;
    let mut in_target = false;
    let mut start_pending = false;
    let mut awaiting_ppr_end = false;
    loop {
        let event = reader.read_event().context("XML parse error")?;
        match &event {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(start);
                if name == "w:p" {
                    para_depth += 1;
                    if para_depth == 1 {
                        para_index += 1;
                        if para_index as usize == target_index {
                            in_target = true;
                            start_pending = true;
                            writer.write_event(event.borrow())?;
                            continue;
                        }
                    }
                } else if in_target && start_pending {
                    if name == "w:pPr" {
                        awaiting_ppr_end = true;
                    } else {
                        write_range_start(&mut writer, id)?;
                        start_pending = false;
                    }
                }
                writer.write_event(event.borrow())?;
            }
            Event::Empty(start) => {
                let name = qname_string(start);
                if in_target && start_pending && name != "w:pPr" {
                    write_range_start(&mut writer, id)?;
                    start_pending = false;
                }
                writer.write_event(event.borrow())?;
                if in_target && start_pending && name == "w:pPr" {
                    write_range_start(&mut writer, id)?;
                    start_pending = false;
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if in_target && awaiting_ppr_end && name == "w:pPr" {
                    writer.write_event(event.borrow())?;
                    write_range_start(&mut writer, id)?;
                    awaiting_ppr_end = false;
                    start_pending = false;
                    continue;
                }
                if name == "w:p" {
                    if in_target && para_depth == 1 {
                        if start_pending {
                            // Empty paragraph: open the range before closing.
                            write_range_start(&mut writer, id)?;
                            start_pending = false;
                        }
                        write_range_end(&mut writer, id)?;
                        in_target = false;
                    }
                    para_depth = para_depth.saturating_sub(1);
                }
                writer.write_event(event.borrow())?;
            }
            _ => writer.write_event(event.borrow())?,
        }
    }
    Ok(writer.into_inner())
}

fn write_range_start(writer: &mut Writer<Vec<u8>>, id: &str) -> anyhow::Result<()> {
    let mut elem = BytesStart::new("w:commentRangeStart");
    elem.push_attribute(("w:id", id));
    writer.write_event(Event::Empty(elem))?;
    Ok(())
}

fn write_range_end(writer: &mut Writer<Vec<u8>>, id: &str) -> anyhow::Result<()> {
    let mut elem = BytesStart::new("w:commentRangeEnd");
    elem.push_attribute(("w:id", id));
    writer.write_event(Event::Empty(elem))?;
    let run = BytesStart::new("w:r");
    writer.write_event(Event::Start(run))?;
    let mut reference = BytesStart::new("w:commentReference");
    reference.push_attribute(("w:id", id));
    writer.write_event(Event::Empty(reference))?;
    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:r")))?;
    Ok(())
}

fn append_before_root_end(xml: &[u8], root: &str, fragment: &str) -> anyhow::Result<Vec<u8>> {
    let text = std::str::from_utf8(xml).context("part is not valid UTF-8")?;
    let close_tag = format!("</{root}>");
    let Some(position) = text.rfind(&close_tag) else {
        bail!("part has no closing </{root}> tag");
    };
    let mut result = String::with_capacity(text.len() + fragment.len());
    result.push_str(&text[..position]);
    result.push_str(fragment);
    result.push_str(&text[position..]);
    Ok(result.into_bytes())
}

fn next_relationship_id(rels_xml: &[u8]) -> anyhow::Result<String> {
    let text = std::str::from_utf8(rels_xml).context("rels part is not valid UTF-8")?;
    let doc = roxmltree::Document::parse(text).context("failed to parse rels part")?;
    let max = doc
        .descendants()
        .filter(|node| node.tag_name().name() == "Relationship")
        .filter_map(|node| {
            node.attributes().find(|a| a.name() == "Id").and_then(|a| {
                a.value()
                    .strip_prefix("rId")
                    .and_then(|n| n.parse::<u64>().ok())
            })
        })
        .max()
        .unwrap_or(0);
    Ok(format!("rId{}", max + 1))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// table export
// ---------------------------------------------------------------------------

/// Extract the nth table of the document as CSV.
pub fn export_table_csv(package: &[u8], table_index: usize) -> anyhow::Result<String> {
    let parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?;
    let text = std::str::from_utf8(document).context("word/document.xml is not valid UTF-8")?;
    let doc = roxmltree::Document::parse(text).context("failed to parse word/document.xml")?;
    let table = doc
        .descendants()
        .filter(|node| node.tag_name().name() == "tbl" && node.tag_name().namespace() == Some(W_NS))
        .nth(table_index)
        .with_context(|| format!("document has no table with index {table_index}"))?;
    let mut lines = Vec::new();
    for row in table
        .children()
        .filter(|node| node.tag_name().name() == "tr")
    {
        let mut cells = Vec::new();
        for cell in row.children().filter(|node| node.tag_name().name() == "tc") {
            let value: String = cell
                .descendants()
                .filter(|node| node.tag_name().name() == "t")
                .filter_map(|node| node.text())
                .collect();
            cells.push(csv_escape(&value));
        }
        lines.push(cells.join(","));
    }
    Ok(lines.join("\n"))
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// fields report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FieldReport {
    pub instruction: String,
    pub cached_result: String,
}

/// List every field with its instruction and the cached display result —
/// the basis for deciding whether cached results are stale before a
/// deterministic render.
pub fn fields_report(package: &[u8]) -> anyhow::Result<Vec<FieldReport>> {
    let parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?;
    let text = std::str::from_utf8(document).context("word/document.xml is not valid UTF-8")?;
    let doc = roxmltree::Document::parse(text).context("failed to parse word/document.xml")?;
    let mut fields = Vec::new();

    // Simple fields carry the instruction as an attribute.
    for field in doc
        .descendants()
        .filter(|node| node.tag_name().name() == "fldSimple")
    {
        let instruction = field
            .attributes()
            .find(|a| a.name() == "instr")
            .map(|a| a.value().trim().to_string())
            .unwrap_or_default();
        let cached: String = field
            .descendants()
            .filter(|node| node.tag_name().name() == "t")
            .filter_map(|node| node.text())
            .collect();
        fields.push(FieldReport {
            instruction,
            cached_result: cached,
        });
    }

    // Complex fields: begin -> instruction runs -> separate -> result runs -> end.
    let mut state = 0u8; // 0 idle, 1 instruction, 2 result
    let mut instruction = String::new();
    let mut result = String::new();
    for node in doc.descendants() {
        match node.tag_name().name() {
            "fldChar" => {
                let fld_type = node
                    .attributes()
                    .find(|a| a.name() == "fldCharType")
                    .map(|a| a.value())
                    .unwrap_or("");
                match fld_type {
                    "begin" => {
                        state = 1;
                        instruction.clear();
                        result.clear();
                    }
                    "separate" => {
                        if state == 1 {
                            state = 2;
                        }
                    }
                    "end" => {
                        if state != 0 {
                            fields.push(FieldReport {
                                instruction: instruction.trim().to_string(),
                                cached_result: result.clone(),
                            });
                        }
                        state = 0;
                    }
                    _ => {}
                }
            }
            "instrText" => {
                if state == 1 {
                    if let Some(text) = node.text() {
                        instruction.push_str(text);
                    }
                }
            }
            "t" => {
                if state == 2 {
                    if let Some(text) = node.text() {
                        result.push_str(text);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(fields)
}

// ---------------------------------------------------------------------------
// style lint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LintFinding {
    pub code: String,
    pub message: String,
}

/// Report structural style problems that break navigation, numbering, and
/// downstream tooling.
pub fn style_lint(package: &[u8]) -> anyhow::Result<Vec<LintFinding>> {
    let parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?;
    let text = std::str::from_utf8(document).context("word/document.xml is not valid UTF-8")?;
    let doc = roxmltree::Document::parse(text).context("failed to parse word/document.xml")?;
    let mut findings = Vec::new();

    for paragraph in doc
        .descendants()
        .filter(|node| node.tag_name().name() == "p" && node.tag_name().namespace() == Some(W_NS))
    {
        let para_text: String = paragraph
            .descendants()
            .filter(|node| node.tag_name().name() == "t")
            .filter_map(|node| node.text())
            .collect();
        if para_text.trim().is_empty() {
            continue;
        }
        let style = paragraph
            .descendants()
            .find(|node| node.tag_name().name() == "pStyle")
            .and_then(|node| {
                node.attributes()
                    .find(|a| a.name() == "val")
                    .map(|a| a.value().to_string())
            });
        let has_num_pr = paragraph
            .descendants()
            .any(|node| node.tag_name().name() == "numPr");

        // Fake bullets: typed markers instead of a numbering definition.
        let trimmed = para_text.trim_start();
        if !has_num_pr
            && (trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("\u{2022}"))
        {
            findings.push(LintFinding {
                code: "fake-bullet".to_string(),
                message: format!(
                    "paragraph starts with a typed list marker instead of real numbering: {}",
                    snippet(&para_text)
                ),
            });
        }

        // Fake headings: short, fully bold paragraphs without a heading style.
        let is_heading_style = style
            .as_deref()
            .map(|s| s.starts_with("Heading") || s == "Title" || s == "Subtitle")
            .unwrap_or(false);
        if !is_heading_style && para_text.trim().len() <= 80 {
            let runs: Vec<_> = paragraph
                .descendants()
                .filter(|node| {
                    node.tag_name().name() == "r"
                        && node
                            .descendants()
                            .any(|child| child.tag_name().name() == "t")
                })
                .collect();
            let all_bold = !runs.is_empty()
                && runs
                    .iter()
                    .all(|run| run.descendants().any(|node| tag_is_bold(node)));
            if all_bold {
                findings.push(LintFinding {
                    code: "fake-heading".to_string(),
                    message: format!(
                        "short all-bold paragraph without a heading style: {}",
                        snippet(&para_text)
                    ),
                });
            }
        }
    }
    Ok(findings)
}

fn tag_is_bold(node: roxmltree::Node<'_, '_>) -> bool {
    if node.tag_name().name() != "b" {
        return false;
    }
    node.attributes()
        .find(|a| a.name() == "val")
        .map(|a| a.value() != "0" && a.value() != "false" && a.value() != "none")
        .unwrap_or(true)
}

fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= 48 {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(48).collect();
        format!("{cut}...")
    }
}

// ---------------------------------------------------------------------------
// fields materialize
// ---------------------------------------------------------------------------

/// Instruction prefixes that are safe to flatten: their cached results are
/// position-independent. PAGE/NUMPAGES stay live — materializing them would
/// freeze one page's number into every page.
const MATERIALIZE_DEFAULT: &[&str] = &["REF", "PAGEREF", "SEQ"];

#[derive(Debug, Clone, Serialize)]
pub struct MaterializeReport {
    pub materialized: usize,
    pub kept_live: usize,
}

/// Replace matching fields with their cached display text so headless
/// renders are deterministic. Only fields whose instruction starts with one
/// of `prefixes` (default REF/PAGEREF/SEQ) are flattened.
pub fn fields_materialize(
    package: &[u8],
    prefixes: &[String],
) -> anyhow::Result<(Vec<u8>, MaterializeReport)> {
    let prefixes: Vec<String> = if prefixes.is_empty() {
        MATERIALIZE_DEFAULT.iter().map(|p| p.to_string()).collect()
    } else {
        prefixes.to_vec()
    };
    let mut parts = read_parts(package)?;
    let mut report = MaterializeReport {
        materialized: 0,
        kept_live: 0,
    };
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;

        // Decide per field (in document order) whether it is flattened.
        let text = std::str::from_utf8(&xml).with_context(|| format!("{name} is not UTF-8"))?;
        let doc =
            roxmltree::Document::parse(text).with_context(|| format!("failed to parse {name}"))?;
        let mut complex_decisions = Vec::new();
        let mut simple_decisions = Vec::new();
        let mut current_instr = String::new();
        let mut in_field = false;
        for node in doc.descendants() {
            match node.tag_name().name() {
                "fldChar" => {
                    match node
                        .attributes()
                        .find(|a| a.name() == "fldCharType")
                        .map(|a| a.value())
                        .unwrap_or("")
                    {
                        "begin" => {
                            in_field = true;
                            current_instr.clear();
                        }
                        "end" => {
                            if in_field {
                                complex_decisions
                                    .push(instruction_matches(&current_instr, &prefixes));
                                in_field = false;
                            }
                        }
                        _ => {}
                    }
                }
                "instrText" => {
                    if in_field {
                        if let Some(text) = node.text() {
                            current_instr.push_str(text);
                        }
                    }
                }
                "fldSimple" => {
                    let instr = node
                        .attributes()
                        .find(|a| a.name() == "instr")
                        .map(|a| a.value())
                        .unwrap_or("");
                    simple_decisions.push(instruction_matches(instr, &prefixes));
                }
                _ => {}
            }
        }

        let (transformed, materialized, kept) =
            materialize_xml(&xml, &complex_decisions, &simple_decisions)
                .with_context(|| format!("failed to materialize fields in {name}"))?;
        report.materialized += materialized;
        report.kept_live += kept;
        parts.insert(name, transformed);
    }
    Ok((write_parts(&parts)?, report))
}

fn instruction_matches(instruction: &str, prefixes: &[String]) -> bool {
    let trimmed = instruction.trim_start();
    prefixes.iter().any(|prefix| {
        trimmed
            .strip_prefix(prefix.as_str())
            .map(|rest| rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\\'))
            .unwrap_or(false)
    })
}

fn materialize_xml(
    xml: &[u8],
    complex_decisions: &[bool],
    simple_decisions: &[bool],
) -> anyhow::Result<(Vec<u8>, usize, usize)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut complex_index = 0usize;
    let mut simple_index = 0usize;
    let mut flatten_active = false; // current complex field is being flattened
    let mut in_instr_phase = false; // between begin and separate
    let mut instr_skip_depth = 0usize; // instrText subtree skip
    let mut simple_unwrap_depth = 0usize;
    let mut materialized = 0usize;
    let mut kept = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if instr_skip_depth > 0 {
                    instr_skip_depth += 1;
                    continue;
                }
                match name.as_str() {
                    "w:fldSimple" => {
                        let flatten = simple_decisions.get(simple_index).copied().unwrap_or(false);
                        simple_index += 1;
                        if flatten {
                            materialized += 1;
                            simple_unwrap_depth += 1;
                            continue;
                        }
                        kept += 1;
                        writer.write_event(Event::Start(start.into_owned()))?;
                    }
                    "w:instrText" | "w:delInstrText" if flatten_active && in_instr_phase => {
                        instr_skip_depth = 1;
                    }
                    _ => writer.write_event(Event::Start(start.into_owned()))?,
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if instr_skip_depth > 0 {
                    instr_skip_depth -= 1;
                    continue;
                }
                if name == "w:fldSimple" && simple_unwrap_depth > 0 {
                    simple_unwrap_depth -= 1;
                    continue;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if instr_skip_depth > 0 {
                    continue;
                }
                if name == "w:fldChar" {
                    let fld_type = attr_value(&start, "w:fldCharType")?.unwrap_or_default();
                    match fld_type.as_str() {
                        "begin" => {
                            let flatten = complex_decisions
                                .get(complex_index)
                                .copied()
                                .unwrap_or(false);
                            complex_index += 1;
                            flatten_active = flatten;
                            in_instr_phase = true;
                            if flatten {
                                materialized += 1;
                                continue; // drop the begin marker
                            }
                            kept += 1;
                        }
                        "separate" => {
                            in_instr_phase = false;
                            if flatten_active {
                                continue; // drop the separator
                            }
                        }
                        "end" => {
                            let was_flattening = flatten_active;
                            flatten_active = false;
                            in_instr_phase = false;
                            if was_flattening {
                                continue; // drop the end marker
                            }
                        }
                        _ => {}
                    }
                    writer.write_event(Event::Empty(start.into_owned()))?;
                    continue;
                }
                if name == "w:fldSimple" {
                    // Cached-result-free simple field: nothing to keep either way.
                    let flatten = simple_decisions.get(simple_index).copied().unwrap_or(false);
                    simple_index += 1;
                    if flatten {
                        materialized += 1;
                        continue;
                    }
                    kept += 1;
                    writer.write_event(Event::Empty(start.into_owned()))?;
                    continue;
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            Event::Text(text) => {
                if instr_skip_depth > 0 {
                    continue;
                }
                writer.write_event(Event::Text(text.into_owned()))?;
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    Ok((writer.into_inner(), materialized, kept))
}

fn attr_value(start: &BytesStart<'_>, key: &str) -> anyhow::Result<Option<String>> {
    for attr in start.attributes() {
        let attr = attr.context("bad attribute")?;
        if String::from_utf8_lossy(attr.key.as_ref()) == key {
            return Ok(Some(attr.unescape_value()?.to_string()));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// watermark audit / remove
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct WatermarkFinding {
    pub part: String,
    pub shape_id: String,
    pub is_watermark: bool,
}

fn header_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    parts
        .keys()
        .filter(|name| {
            (name.starts_with("word/header") || name.starts_with("word/footer"))
                && name.ends_with(".xml")
        })
        .cloned()
        .collect()
}

/// List VML picture objects in headers/footers; watermark heuristics match
/// the shape id Word assigns to watermark objects.
pub fn watermark_audit(package: &[u8]) -> anyhow::Result<Vec<WatermarkFinding>> {
    let parts = read_parts(package)?;
    let mut findings = Vec::new();
    for name in header_parts(&parts) {
        let xml = parts.get(&name).expect("header part present");
        let text = std::str::from_utf8(xml).with_context(|| format!("{name} is not UTF-8"))?;
        let doc =
            roxmltree::Document::parse(text).with_context(|| format!("failed to parse {name}"))?;
        for pict in doc
            .descendants()
            .filter(|node| node.tag_name().name() == "pict")
        {
            for shape in pict
                .descendants()
                .filter(|node| node.tag_name().name() == "shape")
            {
                let id = shape
                    .attributes()
                    .find(|a| a.name() == "id")
                    .map(|a| a.value().to_string())
                    .unwrap_or_default();
                findings.push(WatermarkFinding {
                    part: name.clone(),
                    is_watermark: id.contains("WaterMark") || id.contains("Watermark"),
                    shape_id: id,
                });
            }
        }
    }
    Ok(findings)
}

/// Remove watermark picture objects from headers/footers. With `all`, every
/// VML pict in headers/footers is removed, not just recognized watermarks.
pub fn watermark_remove(package: &[u8], all: bool) -> anyhow::Result<(Vec<u8>, usize)> {
    let audit = watermark_audit(package)?;
    let mut parts = read_parts(package)?;
    let mut removed = 0usize;
    for name in header_parts(&parts) {
        let has_target = audit
            .iter()
            .any(|f| f.part == name && (all || f.is_watermark));
        if !has_target {
            continue;
        }
        let xml = parts.get(&name).expect("header part present").clone();
        let (transformed, count) = remove_picts_xml(&xml, all)?;
        removed += count;
        parts.insert(name, transformed);
    }
    Ok((write_parts(&parts)?, removed))
}

fn remove_picts_xml(xml: &[u8], all: bool) -> anyhow::Result<(Vec<u8>, usize)> {
    // Two-pass per pict: cheap approach — stream and drop w:pict subtrees
    // whose raw slice contains a watermark id (or all of them).
    let text = std::str::from_utf8(xml).context("header part is not UTF-8")?;
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut skip_depth = 0usize;
    let mut removed = 0usize;
    loop {
        let position = reader.buffer_position() as usize;
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if qname_string(&start) == "w:pict" {
                    // Look ahead in the raw text for the matching close tag to
                    // decide whether this pict is a watermark.
                    let rest = &text[position..];
                    let end = rest.find("</w:pict>").unwrap_or(rest.len());
                    let slice = &rest[..end];
                    if all || slice.contains("WaterMark") || slice.contains("Watermark") {
                        skip_depth = 1;
                        removed += 1;
                        continue;
                    }
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
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            other => {
                if skip_depth == 0 {
                    writer.write_event(other.into_owned())?;
                }
            }
        }
    }
    Ok((writer.into_inner(), removed))
}

// ---------------------------------------------------------------------------
// a11y fix
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct A11yFixReport {
    pub image_alts_set: usize,
    pub table_headers_marked: usize,
}

/// Apply safe accessibility fixes: derive missing image alt text from the
/// drawing name, and mark each table's first row as a repeating header row.
pub fn a11y_fix(
    package: &[u8],
    image_alt_from_name: bool,
    table_headers: bool,
) -> anyhow::Result<(Vec<u8>, A11yFixReport)> {
    if !image_alt_from_name && !table_headers {
        bail!("nothing to fix: pass --image-alt-from-name and/or --table-headers");
    }
    let mut parts = read_parts(package)?;
    let mut report = A11yFixReport {
        image_alts_set: 0,
        table_headers_marked: 0,
    };
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let (transformed, alts, headers) = a11y_fix_xml(&xml, image_alt_from_name, table_headers)?;
        report.image_alts_set += alts;
        report.table_headers_marked += headers;
        parts.insert(name, transformed);
    }
    Ok((write_parts(&parts)?, report))
}

fn a11y_fix_xml(
    xml: &[u8],
    image_alt_from_name: bool,
    table_headers: bool,
) -> anyhow::Result<(Vec<u8>, usize, usize)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut alts = 0usize;
    let mut headers = 0usize;
    // Stack of per-table state: has the first row been seen yet?
    let mut table_stack: Vec<bool> = Vec::new();
    // When inside the first row and waiting to see whether it has a trPr.
    let mut pending_header_row = false;
    let mut row_depth = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                match name.as_str() {
                    "w:tbl" => {
                        table_stack.push(false);
                        writer.write_event(Event::Start(start.into_owned()))?;
                    }
                    "w:tr" => {
                        row_depth += 1;
                        let is_first = table_stack.last().map(|seen| !seen).unwrap_or(false);
                        if let Some(seen) = table_stack.last_mut() {
                            *seen = true;
                        }
                        writer.write_event(Event::Start(start.into_owned()))?;
                        if table_headers && is_first && row_depth == table_stack.len() {
                            pending_header_row = true;
                        }
                    }
                    "w:trPr" if pending_header_row => {
                        // Row property block exists: append the header mark
                        // inside it.
                        writer.write_event(Event::Start(start.into_owned()))?;
                        writer.write_event(Event::Empty(BytesStart::new("w:tblHeader")))?;
                        headers += 1;
                        pending_header_row = false;
                    }
                    _ => {
                        if pending_header_row {
                            // First child is not a trPr: create one.
                            writer.write_event(Event::Start(BytesStart::new("w:trPr")))?;
                            writer.write_event(Event::Empty(BytesStart::new("w:tblHeader")))?;
                            writer.write_event(Event::End(quick_xml::events::BytesEnd::new(
                                "w:trPr",
                            )))?;
                            headers += 1;
                            pending_header_row = false;
                        }
                        writer.write_event(Event::Start(start.into_owned()))?;
                    }
                }
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if pending_header_row && name != "w:trPr" {
                    writer.write_event(Event::Start(BytesStart::new("w:trPr")))?;
                    writer.write_event(Event::Empty(BytesStart::new("w:tblHeader")))?;
                    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:trPr")))?;
                    headers += 1;
                    pending_header_row = false;
                }
                if image_alt_from_name && name == "wp:docPr" {
                    let descr = attr_value(&start, "descr")?.unwrap_or_default();
                    if descr.trim().is_empty() {
                        let alt = attr_value(&start, "name")?
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or_else(|| "Figure".to_string());
                        let mut rebuilt = BytesStart::new("wp:docPr");
                        for attr in start.attributes() {
                            let attr = attr.context("bad attribute")?;
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "descr" {
                                continue;
                            }
                            rebuilt.push_attribute((key.as_str(), attr.unescape_value()?.as_ref()));
                        }
                        rebuilt.push_attribute(("descr", alt.as_str()));
                        alts += 1;
                        writer.write_event(Event::Empty(rebuilt))?;
                        continue;
                    }
                }
                writer.write_event(Event::Empty(start.into_owned()))?;
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                match name.as_str() {
                    "w:tbl" => {
                        table_stack.pop();
                    }
                    "w:tr" => {
                        row_depth = row_depth.saturating_sub(1);
                        if pending_header_row {
                            // Empty first row: still mark it.
                            writer.write_event(Event::Start(BytesStart::new("w:trPr")))?;
                            writer.write_event(Event::Empty(BytesStart::new("w:tblHeader")))?;
                            writer.write_event(Event::End(quick_xml::events::BytesEnd::new(
                                "w:trPr",
                            )))?;
                            headers += 1;
                            pending_header_row = false;
                        }
                    }
                    _ => {}
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            other => writer.write_event(other.into_owned())?,
        }
    }
    Ok((writer.into_inner(), alts, headers))
}

// ---------------------------------------------------------------------------
// tracked-changes replace
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TrackedReplaceReport {
    pub replacements: usize,
    /// Runs containing the term whose structure was too complex to split
    /// safely (multiple text nodes, nested content, or already revised).
    pub skipped_complex_runs: usize,
}

/// Replace literal text with a tracked deletion + insertion pair, authored
/// as a proper revision. Only simple runs (one text node) are restructured;
/// complex matches are reported, never guessed at.
pub fn tracked_replace(
    package: &[u8],
    find: &str,
    replace: &str,
    author: &str,
) -> anyhow::Result<(Vec<u8>, TrackedReplaceReport)> {
    if find.is_empty() {
        bail!("find text must not be empty");
    }
    let mut parts = read_parts(package)?;
    let mut report = TrackedReplaceReport {
        replacements: 0,
        skipped_complex_runs: 0,
    };
    // Revision ids must be unique per document.
    let mut next_id = {
        let document = parts
            .get("word/document.xml")
            .context("package has no word/document.xml")?;
        let text = std::str::from_utf8(document).context("document.xml is not UTF-8")?;
        let doc = roxmltree::Document::parse(text).context("failed to parse document.xml")?;
        doc.descendants()
            .filter_map(|node| {
                node.attributes()
                    .find(|a| a.name() == "id")
                    .and_then(|a| a.value().parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1
    };
    let date = revision_date();
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;
        let (transformed, made, skipped, used_ids) =
            tracked_replace_xml(&xml, find, replace, author, &date, next_id)?;
        next_id += used_ids;
        report.replacements += made;
        report.skipped_complex_runs += skipped;
        parts.insert(name, transformed);
    }
    Ok((write_parts(&parts)?, report))
}

/// ISO-8601 UTC timestamp for revision attributes, derived from the system
/// clock (civil-from-days conversion, no external time dependency).
fn revision_date() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let rem = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

struct SimpleRun {
    rpr: Vec<Event<'static>>,
    text: String,
    space_preserve: bool,
}

fn tracked_replace_xml(
    xml: &[u8],
    find: &str,
    replace: &str,
    author: &str,
    date: &str,
    first_id: u64,
) -> anyhow::Result<(Vec<u8>, usize, usize, u64)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut replacements = 0usize;
    let mut skipped = 0usize;
    let mut next_id = first_id;
    let mut revision_depth = 0usize; // inside w:ins/w:del: pass through
    let mut buffer: Option<Vec<Event<'static>>> = None;
    loop {
        let event = reader.read_event().context("XML parse error")?.into_owned();
        match &event {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(start);
                if name == "w:ins" || name == "w:del" {
                    revision_depth += 1;
                }
                if name == "w:r" && revision_depth == 0 && buffer.is_none() {
                    buffer = Some(vec![event.clone()]);
                    continue;
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                if name == "w:ins" || name == "w:del" {
                    revision_depth = revision_depth.saturating_sub(1);
                }
                if name == "w:r" {
                    if let Some(mut events) = buffer.take() {
                        events.push(event.clone());
                        match analyze_simple_run(&events) {
                            Some(run) if run.text.contains(find) => {
                                emit_tracked_replacement(
                                    &mut writer,
                                    &run,
                                    find,
                                    replace,
                                    author,
                                    date,
                                    &mut next_id,
                                )?;
                                replacements += run.text.matches(find).count();
                            }
                            Some(_) => {
                                for buffered in events {
                                    writer.write_event(buffered)?;
                                }
                            }
                            None => {
                                let contains = events.iter().any(|buffered| {
                                    matches!(buffered, Event::Text(text)
                                        if String::from_utf8_lossy(text.as_ref()).contains(find))
                                });
                                if contains {
                                    skipped += 1;
                                }
                                for buffered in events {
                                    writer.write_event(buffered)?;
                                }
                            }
                        }
                        continue;
                    }
                }
            }
            _ => {}
        }
        if let Some(events) = buffer.as_mut() {
            events.push(event);
            continue;
        }
        writer.write_event(event)?;
    }
    Ok((
        writer.into_inner(),
        replacements,
        skipped,
        next_id - first_id,
    ))
}

/// A run qualifies for splitting when it is exactly: w:r, optional w:rPr
/// subtree, one w:t with one text node, end.
fn analyze_simple_run(events: &[Event<'static>]) -> Option<SimpleRun> {
    let mut rpr: Vec<Event<'static>> = Vec::new();
    let mut text: Option<String> = None;
    let mut space_preserve = false;
    let mut index = 1; // skip Start(w:r)
                       // Optional rPr subtree.
    if let Some(Event::Start(start)) = events.get(index) {
        if qname_string(start) == "w:rPr" {
            let mut depth = 0usize;
            loop {
                let event = events.get(index)?;
                match event {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => {
                        depth -= 1;
                        rpr.push(event.clone());
                        index += 1;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    _ => {}
                }
                rpr.push(event.clone());
                index += 1;
            }
        }
    }
    if let Some(Event::Empty(start)) = events.get(index) {
        if qname_string(start) == "w:rPr" {
            rpr.push(events[index].clone());
            index += 1;
        }
    }
    // Exactly one w:t with one text node.
    match events.get(index) {
        Some(Event::Start(start)) if qname_string(start) == "w:t" => {
            space_preserve = start
                .attributes()
                .flatten()
                .any(|attr| attr.key.as_ref() == b"xml:space");
            index += 1;
        }
        _ => return None,
    }
    match events.get(index) {
        Some(Event::Text(value)) => {
            text = Some(String::from_utf8_lossy(value.as_ref()).to_string());
            index += 1;
        }
        _ => return None,
    }
    match events.get(index) {
        Some(Event::End(end)) if end.name().as_ref() == b"w:t" => index += 1,
        _ => return None,
    }
    match events.get(index) {
        Some(Event::End(end)) if end.name().as_ref() == b"w:r" => index += 1,
        _ => return None,
    }
    if index != events.len() {
        return None;
    }
    text.map(|text| SimpleRun {
        rpr,
        text,
        space_preserve,
    })
}

#[allow(clippy::too_many_arguments)]
fn emit_tracked_replacement(
    writer: &mut Writer<Vec<u8>>,
    run: &SimpleRun,
    find: &str,
    replace: &str,
    author: &str,
    date: &str,
    next_id: &mut u64,
) -> anyhow::Result<()> {
    let emit_run = |writer: &mut Writer<Vec<u8>>, tag: &str, content: &str| -> anyhow::Result<()> {
        writer.write_event(Event::Start(BytesStart::new("w:r")))?;
        for event in &run.rpr {
            writer.write_event(event.clone())?;
        }
        let mut text_elem = BytesStart::new(tag);
        text_elem.push_attribute(("xml:space", "preserve"));
        writer.write_event(Event::Start(text_elem))?;
        writer.write_event(Event::Text(quick_xml::events::BytesText::new(content)))?;
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new(tag)))?;
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:r")))?;
        Ok(())
    };
    let mut remaining = run.text.as_str();
    let _ = run.space_preserve;
    while let Some(position) = remaining.find(find) {
        let before = &remaining[..position];
        if !before.is_empty() {
            emit_run(writer, "w:t", before)?;
        }
        let del_id = *next_id;
        let ins_id = *next_id + 1;
        *next_id += 2;
        let mut del = BytesStart::new("w:del");
        del.push_attribute(("w:id", del_id.to_string().as_str()));
        del.push_attribute(("w:author", author));
        del.push_attribute(("w:date", date));
        writer.write_event(Event::Start(del))?;
        emit_run(writer, "w:delText", find)?;
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:del")))?;
        let mut ins = BytesStart::new("w:ins");
        ins.push_attribute(("w:id", ins_id.to_string().as_str()));
        ins.push_attribute(("w:author", author));
        ins.push_attribute(("w:date", date));
        writer.write_event(Event::Start(ins))?;
        emit_run(writer, "w:t", replace)?;
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:ins")))?;
        remaining = &remaining[position + find.len()..];
    }
    if !remaining.is_empty() {
        emit_run(writer, "w:t", remaining)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// merge append
// ---------------------------------------------------------------------------

/// Relationship types the appended document may use; anything else (media,
/// hyperlinks, headers, comments, embedded parts) would need id rebasing and
/// is refused rather than silently dropped.
const MERGE_SAFE_REL_SUFFIXES: &[&str] = &[
    "/styles",
    "/settings",
    "/webSettings",
    "/fontTable",
    "/theme",
    "/numbering",
];

/// Append the body content of `appendix` to `base`, separated by a page
/// break. The base document's styles win; appendix content referencing
/// styles or numbering the base lacks degrades to defaults.
pub fn merge_append(base: &[u8], appendix: &[u8]) -> anyhow::Result<Vec<u8>> {
    let appendix_parts = read_parts(appendix)?;
    if let Some(rels) = appendix_parts.get("word/_rels/document.xml.rels") {
        let text = std::str::from_utf8(rels).context("appendix rels part is not UTF-8")?;
        let doc = roxmltree::Document::parse(text).context("failed to parse appendix rels")?;
        for rel in doc
            .descendants()
            .filter(|node| node.tag_name().name() == "Relationship")
        {
            let rel_type = rel
                .attributes()
                .find(|a| a.name() == "Type")
                .map(|a| a.value())
                .unwrap_or("");
            if !MERGE_SAFE_REL_SUFFIXES
                .iter()
                .any(|suffix| rel_type.ends_with(suffix))
            {
                bail!(
                    "appendix document uses relationship type {rel_type}; merging documents \
                     with media, hyperlinks, headers, or comments is not supported yet"
                );
            }
        }
    }
    let appendix_doc = appendix_parts
        .get("word/document.xml")
        .context("appendix has no word/document.xml")?;
    ensure_conventional_prefix(appendix_doc, "appendix word/document.xml")?;
    let appendix_text =
        std::str::from_utf8(appendix_doc).context("appendix document.xml is not UTF-8")?;
    let body_inner = extract_body_content(appendix_text)?;

    let mut parts = read_parts(base)?;
    let base_doc = parts
        .get("word/document.xml")
        .context("base has no word/document.xml")?
        .clone();
    ensure_conventional_prefix(&base_doc, "word/document.xml")?;
    let base_text = std::str::from_utf8(&base_doc).context("base document.xml is not UTF-8")?;

    let page_break = "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";
    let insertion = format!("{page_break}{body_inner}");
    // Insert before the base body's trailing section properties, or before
    // </w:body> when the base has no sectPr.
    let merged = if let Some(position) = base_text.rfind("<w:sectPr") {
        let mut result = String::with_capacity(base_text.len() + insertion.len());
        result.push_str(&base_text[..position]);
        result.push_str(&insertion);
        result.push_str(&base_text[position..]);
        result
    } else {
        String::from_utf8(append_before_root_end(
            base_text.as_bytes(),
            "w:body",
            &insertion,
        )?)
        .context("merged document is not UTF-8")?
    };
    parts.insert("word/document.xml".to_string(), merged.into_bytes());
    write_parts(&parts)
}

fn extract_body_content(document_xml: &str) -> anyhow::Result<String> {
    let start = document_xml
        .find("<w:body>")
        .or_else(|| document_xml.find("<w:body "))
        .context("appendix document has no w:body")?;
    let open_end = document_xml[start..]
        .find('>')
        .map(|offset| start + offset + 1)
        .context("malformed w:body start tag")?;
    let close = document_xml
        .rfind("</w:body>")
        .context("appendix document has no closing w:body")?;
    let mut inner = document_xml[open_end..close].to_string();
    // Drop the appendix's trailing section properties; the base's page
    // geometry governs the merged document.
    if let Some(position) = inner.rfind("<w:sectPr") {
        let after = inner[position..]
            .find("</w:sectPr>")
            .map(|offset| position + offset + "</w:sectPr>".len());
        if let Some(end) = after {
            inner.replace_range(position..end, "");
        } else if inner[position..].contains("/>") {
            // Self-closing sectPr.
            let end = inner[position..]
                .find("/>")
                .map(|offset| position + offset + 2)
                .unwrap_or(inner.len());
            inner.replace_range(position..end, "");
        }
    }
    Ok(inner)
}

// ---------------------------------------------------------------------------
// style normalize
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct NormalizeReport {
    pub overrides_cleared: usize,
    pub empty_property_blocks_removed: usize,
}

/// Direct-formatting properties that a named heading/title style should
/// govern; run-level copies of these fight the style sheet.
const NORMALIZE_OVERRIDES: &[&str] = &["w:rFonts", "w:sz", "w:szCs", "w:color"];

/// Conservative style cleanup: inside paragraphs styled as
/// Heading*/Title/Subtitle, remove run-level font/size/color overrides so
/// the style governs; afterwards drop property blocks that became empty.
/// Body text and deliberate emphasis (bold/italic) are never touched.
pub fn style_normalize(package: &[u8]) -> anyhow::Result<(Vec<u8>, NormalizeReport)> {
    let mut parts = read_parts(package)?;
    let mut report = NormalizeReport {
        overrides_cleared: 0,
        empty_property_blocks_removed: 0,
    };
    let story_names: Vec<String> = parts
        .keys()
        .filter(|name| is_story_part(name))
        .cloned()
        .collect();
    for name in story_names {
        let xml = parts.get(&name).expect("story part present").clone();
        ensure_conventional_prefix(&xml, &name)?;

        // Pass 1: which paragraphs (document order) carry a governed style?
        let text = std::str::from_utf8(&xml).with_context(|| format!("{name} is not UTF-8"))?;
        let doc =
            roxmltree::Document::parse(text).with_context(|| format!("failed to parse {name}"))?;
        let governed: Vec<bool> = doc
            .descendants()
            .filter(|node| {
                node.tag_name().name() == "p" && node.tag_name().namespace() == Some(W_NS)
            })
            .map(|paragraph| {
                paragraph
                    .descendants()
                    .find(|node| node.tag_name().name() == "pStyle")
                    .and_then(|node| {
                        node.attributes()
                            .find(|a| a.name() == "val")
                            .map(|a| a.value().to_string())
                    })
                    .map(|style| {
                        style.starts_with("Heading") || style == "Title" || style == "Subtitle"
                    })
                    .unwrap_or(false)
            })
            .collect();

        let (transformed, cleared) = normalize_xml(&xml, &governed)
            .with_context(|| format!("failed to normalize styles in {name}"))?;
        report.overrides_cleared += cleared;

        // Pass 2: drop property blocks that are now (or were already) empty.
        let mut cleaned = String::from_utf8(transformed).context("normalized part not UTF-8")?;
        for empty in ["<w:rPr></w:rPr>", "<w:rPr/>", "<w:pPr></w:pPr>", "<w:pPr/>"] {
            let count = cleaned.matches(empty).count();
            if count > 0 {
                cleaned = cleaned.replace(empty, "");
                report.empty_property_blocks_removed += count;
            }
        }
        parts.insert(name, cleaned.into_bytes());
    }
    Ok((write_parts(&parts)?, report))
}

fn normalize_xml(xml: &[u8], governed: &[bool]) -> anyhow::Result<(Vec<u8>, usize)> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::new());
    let mut para_index: isize = -1;
    let mut para_depth = 0usize;
    let mut in_governed = false;
    let mut in_rpr = false;
    let mut skip_depth = 0usize;
    let mut cleared = 0usize;
    loop {
        match reader.read_event().context("XML parse error")? {
            Event::Eof => break,
            Event::Start(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if name == "w:p" {
                    para_depth += 1;
                    if para_depth == 1 {
                        para_index += 1;
                        in_governed = governed.get(para_index as usize).copied().unwrap_or(false);
                    }
                }
                if name == "w:rPr" {
                    in_rpr = true;
                }
                if in_governed && in_rpr && NORMALIZE_OVERRIDES.contains(&name.as_str()) {
                    skip_depth = 1;
                    cleared += 1;
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
                if name == "w:p" {
                    if para_depth == 1 {
                        in_governed = false;
                    }
                    para_depth = para_depth.saturating_sub(1);
                }
                if name == "w:rPr" {
                    in_rpr = false;
                }
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(start) => {
                let name = qname_string(&start);
                if skip_depth > 0 {
                    continue;
                }
                if in_governed && in_rpr && NORMALIZE_OVERRIDES.contains(&name.as_str()) {
                    cleared += 1;
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
    Ok((writer.into_inner(), cleared))
}

// ---------------------------------------------------------------------------
// watermark add
// ---------------------------------------------------------------------------

/// Insert a diagonal text watermark into every header part. The VML fragment
/// declares its namespaces inline and includes the text-on-path shapetype so
/// it renders without relying on document-level declarations.
pub fn watermark_add(package: &[u8], text: &str) -> anyhow::Result<(Vec<u8>, usize)> {
    if text.trim().is_empty() {
        bail!("watermark text must not be empty");
    }
    let mut parts = read_parts(package)?;
    let headers: Vec<String> = parts
        .keys()
        .filter(|name| name.starts_with("word/header") && name.ends_with(".xml"))
        .cloned()
        .collect();
    if headers.is_empty() {
        bail!(
            "package has no header parts; creating headers (sectPr references, rels, \
             content types) is an editor-flow concern"
        );
    }
    let mut added = 0usize;
    for (index, name) in headers.iter().enumerate() {
        let xml = parts.get(name).expect("header part present").clone();
        ensure_conventional_prefix(&xml, name)?;
        let escaped = xml_escape(text);
        let shape_id = format!("PowerPlusWaterMarkObject{}", 357922000 + index);
        let fragment = format!(
            r##"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:formulas><v:f eqn="sum #0 0 10800"/><v:f eqn="prod #0 2 1"/><v:f eqn="sum 21600 0 @1"/><v:f eqn="sum 0 0 @2"/><v:f eqn="sum 21600 0 @3"/><v:f eqn="if @0 @3 0"/><v:f eqn="if @0 21600 @1"/><v:f eqn="if @0 0 @2"/><v:f eqn="if @0 @4 21600"/><v:f eqn="mid @5 @6"/><v:f eqn="mid @8 @5"/><v:f eqn="mid @7 @8"/><v:f eqn="mid @6 @7"/><v:f eqn="sum @6 0 @5"/></v:formulas><v:path textpathok="t" o:connecttype="custom" o:connectlocs="@9,0;@10,10800;@11,21600;@12,10800" o:connectangles="270,180,90,0"/><v:textpath on="t" fitshape="t"/><v:handles><v:h position="#0,bottomRight" xrange="6629,14971"/></v:handles><o:lock v:ext="edit" text="t" shapetype="t"/></v:shapetype><v:shape id="{shape_id}" type="#_x0000_t136" style="position:absolute;margin-left:0;margin-top:0;width:412.4pt;height:137.45pt;rotation:315;z-index:-251656192;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin" o:allowincell="f" fillcolor="silver" stroked="f"><v:fill opacity=".5"/><v:textpath style="font-family:&quot;Calibri&quot;;font-size:1pt" string="{escaped}"/></v:shape></w:pict></w:r></w:p>"##
        );
        let transformed = append_before_root_end(&xml, "w:hdr", &fragment)
            .with_context(|| format!("failed to add watermark to {name}"))?;
        parts.insert(name.clone(), transformed);
        added += 1;
    }
    Ok((write_parts(&parts)?, added))
}

// ---------------------------------------------------------------------------
// table import
// ---------------------------------------------------------------------------

/// Usable page width in twentieths of a point for US Letter portrait with
/// one-inch margins — the explicit-geometry baseline the doc skill mandates.
const TABLE_TOTAL_WIDTH_DXA: usize = 9360;

/// Append a table built from CSV data to the end of the document body, with
/// explicit column geometry and the first row marked as a repeating header.
pub fn table_import(package: &[u8], csv: &str, header_row: bool) -> anyhow::Result<Vec<u8>> {
    let rows = parse_csv(csv)?;
    if rows.is_empty() {
        bail!("CSV contains no rows");
    }
    let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    if columns == 0 {
        bail!("CSV contains no columns");
    }
    let col_width = TABLE_TOTAL_WIDTH_DXA / columns;

    let mut table = String::new();
    table.push_str("<w:tbl><w:tblPr>");
    table.push_str(&format!(
        "<w:tblW w:w=\"{TABLE_TOTAL_WIDTH_DXA}\" w:type=\"dxa\"/>"
    ));
    table.push_str(
        "<w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/>\
         <w:left w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/>\
         <w:right w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/>\
         <w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/></w:tblBorders>",
    );
    table.push_str("</w:tblPr><w:tblGrid>");
    for _ in 0..columns {
        table.push_str(&format!("<w:gridCol w:w=\"{col_width}\"/>"));
    }
    table.push_str("</w:tblGrid>");
    for (row_index, row) in rows.iter().enumerate() {
        table.push_str("<w:tr>");
        if header_row && row_index == 0 {
            table.push_str("<w:trPr><w:tblHeader/></w:trPr>");
        }
        for column in 0..columns {
            let value = row.get(column).map(String::as_str).unwrap_or("");
            table.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{col_width}\" w:type=\"dxa\"/></w:tcPr>\
                 <w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                xml_escape(value)
            ));
        }
        table.push_str("</w:tr>");
    }
    table.push_str("</w:tbl><w:p/>");

    let mut parts = read_parts(package)?;
    let document = parts
        .get("word/document.xml")
        .context("package has no word/document.xml")?
        .clone();
    ensure_conventional_prefix(&document, "word/document.xml")?;
    let text = std::str::from_utf8(&document).context("document.xml is not UTF-8")?;
    let merged = if let Some(position) = text.rfind("<w:sectPr") {
        let mut result = String::with_capacity(text.len() + table.len());
        result.push_str(&text[..position]);
        result.push_str(&table);
        result.push_str(&text[position..]);
        result
    } else {
        String::from_utf8(append_before_root_end(text.as_bytes(), "w:body", &table)?)
            .context("merged document is not UTF-8")?
    };
    parts.insert("word/document.xml".to_string(), merged.into_bytes());
    write_parts(&parts)
}

/// Minimal CSV parser: comma separators, double-quoted fields with `""`
/// escapes, newlines allowed inside quotes.
fn parse_csv(input: &str) -> anyhow::Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' => {
                if field.is_empty() {
                    in_quotes = true;
                } else {
                    field.push('"');
                }
            }
            ',' => {
                row.push(std::mem::take(&mut field));
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            other => field.push(other),
        }
    }
    if in_quotes {
        bail!("unterminated quoted field in CSV");
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
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

    fn build_slice2_docx() -> Vec<u8> {
        let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Normal"/></w:pPr>
      <w:r><w:t>Contact alice@example.com or +49 30 1234567 for details.</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>Fake Heading Candidate</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>- fake bullet item</w:t></w:r>
    </w:p>
    <w:p>
      <w:ins w:id="1" w:author="R"><w:r><w:t>added</w:t></w:r></w:ins>
      <w:del w:id="2" w:author="R"><w:r><w:delText>removed</w:delText></w:r></w:del>
    </w:p>
    <w:p>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText xml:space="preserve"> REF section1 </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>Section One</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p>
    <w:p>
      <w:commentRangeStart w:id="1"/>
      <w:r><w:t>secret project name</w:t></w:r>
      <w:commentRangeEnd w:id="1"/>
      <w:r><w:commentReference w:id="1"/></w:r>
    </w:p>
    <w:tbl>
      <w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Value, with comma</w:t></w:r></w:p></w:tc></w:tr>
      <w:tr><w:tc><w:p><w:r><w:t>alpha</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc></w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;
        let comments = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">
  <w:comment w:id="1" w:author="Alice">
    <w:p w14:paraId="AAAA0001"><w:r><w:t>Check the secret name.</w:t></w:r></w:p>
  </w:comment>
</w:comments>"#;
        let comments_extended = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w15:commentsEx xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml">
  <w15:commentEx w15:paraId="AAAA0001" w15:done="0"/>
</w15:commentsEx>"#;
        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>
</Types>"#;
        let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
        let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>
</Relationships>"#;

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, content) in [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/document.xml", document),
            ("word/comments.xml", comments),
            ("word/commentsExtended.xml", comments_extended),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn redact_masks_terms_emails_and_phones_preserving_length() {
        let package = build_slice2_docx();
        let (redacted, report) = redact(
            &package,
            &["secret".to_string(), "unfindable".to_string()],
            true,
            true,
        )
        .unwrap();
        let document = part_text(&redacted, "word/document.xml");
        assert!(!document.contains("alice@example.com"));
        assert!(!document.contains("secret"));
        assert!(document.contains('\u{2588}'));
        assert!(report.replacements >= 3);
        assert_eq!(report.terms_not_found, vec!["unfindable".to_string()]);
        // Comments are also redacted.
        let comments = part_text(&redacted, "word/comments.xml");
        assert!(!comments.contains("secret"));
    }

    #[test]
    fn strip_comments_removes_parts_rels_and_anchors() {
        let package = build_slice2_docx();
        let stripped = strip_comments(&package).unwrap();
        let parts = read_parts(&stripped).unwrap();
        assert!(parts.get("word/comments.xml").is_none());
        assert!(parts.get("word/commentsExtended.xml").is_none());
        let document = part_text(&stripped, "word/document.xml");
        assert!(!document.contains("commentRangeStart"));
        assert!(!document.contains("commentReference"));
        assert!(document.contains("secret project name"));
        let types = part_text(&stripped, "[Content_Types].xml");
        assert!(!types.contains("comments"));
        let rels = part_text(&stripped, "word/_rels/document.xml.rels");
        assert!(!rels.contains("comments.xml"));
    }

    #[test]
    fn resolve_comments_flips_done_flag() {
        let package = build_slice2_docx();
        let resolved = resolve_comments(&package, Some("1")).unwrap();
        let extended = part_text(&resolved, "word/commentsExtended.xml");
        assert!(extended.contains("done=\"1\""));
        let all = resolve_comments(&package, None).unwrap();
        let extended = part_text(&all, "word/commentsExtended.xml");
        assert!(extended.contains("done=\"1\""));
        assert!(resolve_comments(&package, Some("99")).is_err());
    }

    #[test]
    fn add_comment_anchors_and_appends() {
        let package = build_slice2_docx();
        let commented = add_comment(&package, "fake bullet", "Bob", "Use real numbering.").unwrap();
        let document = part_text(&commented, "word/document.xml");
        assert!(document.contains("w:commentRangeStart w:id=\"2\""));
        assert!(document.contains("w:commentRangeEnd w:id=\"2\""));
        assert!(document.contains("w:commentReference w:id=\"2\""));
        let comments = part_text(&commented, "word/comments.xml");
        assert!(comments.contains("Use real numbering."));
        assert!(comments.contains("w:author=\"Bob\""));
        let extracted = extract_comments(&commented).unwrap();
        assert_eq!(extracted.len(), 2);
        // Round trip through the anchor stripper still works.
        strip_comments(&commented).unwrap();
    }

    #[test]
    fn add_comment_creates_comments_part_when_missing() {
        let package = build_slice2_docx();
        let stripped = strip_comments(&package).unwrap();
        let commented = add_comment(&stripped, "fake bullet", "Bob", "First comment.").unwrap();
        let parts = read_parts(&commented).unwrap();
        assert!(parts.get("word/comments.xml").is_some());
        let types = part_text(&commented, "[Content_Types].xml");
        assert!(types.contains("/word/comments.xml"));
        let rels = part_text(&commented, "word/_rels/document.xml.rels");
        assert!(rels.contains("comments.xml"));
        let extracted = extract_comments(&commented).unwrap();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].text, "First comment.");
    }

    #[test]
    fn reject_tracked_changes_restores_deletions_drops_insertions() {
        let package = build_slice2_docx();
        let rejected = reject_tracked_changes(&package).unwrap();
        let document = part_text(&rejected, "word/document.xml");
        assert!(!document.contains("added"));
        assert!(document.contains("removed"));
        assert!(!document.contains("w:delText"));
        assert!(!document.contains("<w:ins "));
        assert!(!document.contains("<w:del "));
    }

    #[test]
    fn reject_refuses_formatting_revisions() {
        let document = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:rPr><w:rPrChange w:id="1" w:author="R"><w:rPr/></w:rPrChange></w:rPr><w:t>x</w:t></w:r></w:p>
</w:body></w:document>"#;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(document.as_bytes()).unwrap();
        let package = writer.finish().unwrap().into_inner();
        assert!(reject_tracked_changes(&package).is_err());
    }

    #[test]
    fn export_table_csv_extracts_rows_with_escaping() {
        let package = build_slice2_docx();
        let csv = export_table_csv(&package, 0).unwrap();
        assert_eq!(csv, "Name,\"Value, with comma\"\nalpha,1");
        assert!(export_table_csv(&package, 1).is_err());
    }

    #[test]
    fn fields_report_lists_complex_fields() {
        let package = build_slice2_docx();
        let fields = fields_report(&package).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].instruction, "REF section1");
        assert_eq!(fields[0].cached_result, "Section One");
    }

    #[test]
    fn style_lint_finds_fake_bullets_and_headings() {
        let package = build_slice2_docx();
        let findings = style_lint(&package).unwrap();
        let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"fake-bullet"));
        assert!(codes.contains(&"fake-heading"));
    }

    fn build_slice3_docx() -> Vec<u8> {
        let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>Replace the target word here.</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText xml:space="preserve"> REF anchor1 </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>Cached Ref</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>7</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing><wp:inline><wp:docPr id="3" name="Chart 3"/></wp:inline></w:drawing>
      </w:r>
    </w:p>
    <w:tbl>
      <w:tr><w:tc><w:p><w:r><w:t>h1</w:t></w:r></w:p></w:tc></w:tr>
      <w:tr><w:tc><w:p><w:r><w:t>v1</w:t></w:r></w:p></w:tc></w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;
        let header = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office">
  <w:p><w:r><w:pict><v:shape id="PowerPlusWaterMarkObject1" o:spid="_x0000_s2049"><v:textpath string="DRAFT"/></v:shape></w:pict></w:r></w:p>
</w:hdr>"#;
        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
</Types>"#;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, content) in [
            ("[Content_Types].xml", content_types),
            ("word/document.xml", document),
            ("word/header1.xml", header),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn fields_materialize_flattens_ref_keeps_page_live() {
        let package = build_slice3_docx();
        let (materialized, report) = fields_materialize(&package, &[]).unwrap();
        assert_eq!(report.materialized, 1);
        assert_eq!(report.kept_live, 1);
        let document = part_text(&materialized, "word/document.xml");
        assert!(document.contains("Cached Ref"));
        assert!(!document.contains("REF anchor1"));
        // The PAGE field survives with instruction and markers.
        assert!(document.contains("PAGE"));
        assert!(document.contains("fldChar"));
    }

    #[test]
    fn watermark_audit_and_remove() {
        let package = build_slice3_docx();
        let findings = watermark_audit(&package).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_watermark);
        let (removed, count) = watermark_remove(&package, false).unwrap();
        assert_eq!(count, 1);
        let header = part_text(&removed, "word/header1.xml");
        assert!(!header.contains("WaterMark"));
        assert!(!header.contains("w:pict"));
        assert!(watermark_audit(&removed).unwrap().is_empty());
    }

    #[test]
    fn a11y_fix_sets_alt_text_and_table_headers() {
        let package = build_slice3_docx();
        let (fixed, report) = a11y_fix(&package, true, true).unwrap();
        assert_eq!(report.image_alts_set, 1);
        assert_eq!(report.table_headers_marked, 1);
        let document = part_text(&fixed, "word/document.xml");
        assert!(document.contains("descr=\"Chart 3\""));
        assert!(document.contains("w:tblHeader"));
        // Second row stays unmarked.
        assert_eq!(document.matches("w:tblHeader").count(), 1);
        // Findings that the audit reported are now gone.
        let findings = a11y_audit(&fixed).unwrap();
        let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(!codes.contains(&"image-missing-alt"));
        assert!(!codes.contains(&"table-missing-header-row"));
    }

    #[test]
    fn tracked_replace_creates_revision_pair() {
        let package = build_slice3_docx();
        let (replaced, report) = tracked_replace(&package, "target", "chosen", "Bob").unwrap();
        assert_eq!(report.replacements, 1);
        assert_eq!(report.skipped_complex_runs, 0);
        let document = part_text(&replaced, "word/document.xml");
        assert!(document.contains("<w:del "));
        assert!(document.contains("<w:ins "));
        assert!(document.contains("w:author=\"Bob\""));
        assert!(document.contains(">target</w:delText>"));
        assert!(document.contains(">chosen</w:t>"));
        // Formatting of the split run is preserved.
        assert!(document.matches("<w:b/>").count() >= 3);
        // Accepting the tracked replacement yields the new text (split
        // across runs, so check the segments and the absence of the old).
        let accepted = accept_tracked_changes(&replaced).unwrap();
        let document = part_text(&accepted, "word/document.xml");
        assert!(document.contains("Replace the "));
        assert!(document.contains("chosen"));
        assert!(document.contains(" word here."));
        assert!(!document.contains("target"));
        // Rejecting restores the original.
        let rejected = reject_tracked_changes(&replaced).unwrap();
        let document = part_text(&rejected, "word/document.xml");
        assert!(document.contains("target"));
        assert!(!document.contains("chosen"));
    }

    #[test]
    fn style_normalize_clears_heading_overrides_only() {
        let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:rPr><w:rFonts w:ascii="Comic Sans MS"/><w:sz w:val="48"/><w:b/></w:rPr><w:t>Heading</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>Body keeps its size.</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(document.as_bytes()).unwrap();
        let package = writer.finish().unwrap().into_inner();
        let (normalized, report) = style_normalize(&package).unwrap();
        assert_eq!(report.overrides_cleared, 2);
        let text = part_text(&normalized, "word/document.xml");
        assert!(!text.contains("Comic Sans"));
        assert!(!text.contains("w:val=\"48\""));
        // Bold on the heading run survives (deliberate emphasis).
        assert!(text.contains("<w:b/>"));
        // Body-run override survives.
        assert!(text.contains("w:val=\"20\""));
    }

    #[test]
    fn watermark_add_then_audit_and_remove_round_trip() {
        let package = build_slice3_docx();
        // Remove the fixture's existing watermark first for a clean base.
        let (clean, _) = watermark_remove(&package, true).unwrap();
        let (marked, added) = watermark_add(&clean, "DRAFT & CONFIDENTIAL").unwrap();
        assert_eq!(added, 1);
        let findings = watermark_audit(&marked).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_watermark);
        let header = part_text(&marked, "word/header1.xml");
        assert!(header.contains("DRAFT &amp; CONFIDENTIAL"));
        let (removed, count) = watermark_remove(&marked, false).unwrap();
        assert_eq!(count, 1);
        assert!(watermark_audit(&removed).unwrap().is_empty());
    }

    #[test]
    fn watermark_add_requires_header_parts() {
        let package = build_slice2_docx();
        assert!(watermark_add(&package, "DRAFT").is_err());
    }

    #[test]
    fn table_import_appends_table_with_geometry_and_header() {
        let package = build_slice3_docx();
        let csv = "Name,Value\n\"quoted, comma\",\"line\nbreak\"\nplain,42";
        let imported = table_import(&package, csv, true).unwrap();
        let document = part_text(&imported, "word/document.xml");
        assert!(document
            .contains("<w:tblGrid><w:gridCol w:w=\"4680\"/><w:gridCol w:w=\"4680\"/></w:tblGrid>"));
        assert!(document.contains("quoted, comma"));
        assert!(document.contains(">42</w:t>"));
        // Round trip: the imported table is extractable again (it is the
        // second table in the fixture document).
        let csv_out = export_table_csv(&imported, 1).unwrap();
        assert!(csv_out.starts_with("Name,Value"));
        assert!(csv_out.contains("\"quoted, comma\""));
        assert!(csv_out.contains("plain,42"));
        // Header row of the imported table is marked.
        let a11y = a11y_audit(&imported).unwrap();
        let header_findings = a11y
            .iter()
            .filter(|f| f.code == "table-missing-header-row")
            .count();
        assert_eq!(header_findings, 1); // only the fixture's own table
    }

    #[test]
    fn merge_append_joins_bodies_with_page_break() {
        let base = build_slice3_docx();
        let appendix = build_slice2_docx();
        // slice2 has a comments relationship -> must refuse.
        assert!(merge_append(&base, &appendix).is_err());
        // A plain appendix merges.
        let plain = build_slice3_docx();
        let merged = merge_append(&base, &plain).unwrap();
        let document = part_text(&merged, "word/document.xml");
        assert!(document.contains("w:br w:type=\"page\""));
        assert_eq!(document.matches("Replace the target word here.").count(), 2);
    }
}
