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
}
