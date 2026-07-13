// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub const EDITOR_PROTOCOL: &str = "ctox-euro-office-editor-bootstrap-v1";
pub const EDITOR_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeKind {
    Document,
    Spreadsheet,
}

impl OfficeKind {
    pub fn canonical_extension(self) -> &'static str {
        match self {
            Self::Document => "docx",
            Self::Spreadsheet => "xlsx",
        }
    }

    pub fn canonical_mime(self) -> &'static str {
        match self {
            Self::Document => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Self::Spreadsheet => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrepareOptions {
    #[serde(default)]
    pub implemented_features: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyChangesOptions {
    #[serde(default)]
    pub expected_base_sha256: String,
    #[serde(default)]
    pub implemented_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedEditorPayload {
    pub kind: OfficeKind,
    pub protocol: String,
    pub protocol_version: u32,
    pub source_sha256: String,
    pub editor_sha256: String,
    pub editor_payload: Vec<u8>,
    pub manifest: SemanticManifest,
    pub implemented_features: Vec<String>,
    pub diagnostics: Vec<OfficeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficePackage {
    pub kind: OfficeKind,
    pub mime_type: String,
    pub extension: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub manifest: SemanticManifest,
    pub diagnostics: Vec<OfficeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticManifest {
    pub schema_version: String,
    pub kind: OfficeKind,
    pub package_sha256: String,
    pub parts: Vec<OfficePartManifest>,
    pub relationship_parts: usize,
    pub content_types_present: bool,
    pub primary_part: String,
    pub primary_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficePartManifest {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub xml: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeDiagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
}

pub fn prepare(
    kind: OfficeKind,
    source_bytes: &[u8],
    options: PrepareOptions,
) -> anyhow::Result<PreparedEditorPayload> {
    let manifest = inspect(kind, source_bytes)?;
    let source_sha256 = sha256_hex(source_bytes);
    Ok(PreparedEditorPayload {
        kind,
        protocol: EDITOR_PROTOCOL.to_string(),
        protocol_version: EDITOR_PROTOCOL_VERSION,
        source_sha256: source_sha256.clone(),
        editor_sha256: source_sha256,
        editor_payload: source_bytes.to_vec(),
        manifest,
        implemented_features: options.implemented_features,
        diagnostics: vec![OfficeDiagnostic {
            level: "info".to_string(),
            code: "office.bootstrap.identity-payload".to_string(),
            message: "The initial protocol payload preserves the complete OOXML package while the Euro-Office binary protocol is ported feature by feature.".to_string(),
        }],
    })
}

pub fn apply_changes(
    kind: OfficeKind,
    base_payload: &[u8],
    changed_payload: &[u8],
    options: ApplyChangesOptions,
) -> anyhow::Result<PreparedEditorPayload> {
    if !options.expected_base_sha256.is_empty() {
        ensure!(
            sha256_hex(base_payload) == options.expected_base_sha256,
            "version_conflict: base editor payload hash does not match"
        );
    }
    let mut prepared = prepare(
        kind,
        changed_payload,
        PrepareOptions {
            implemented_features: options.implemented_features,
        },
    )?;
    prepared.diagnostics.push(OfficeDiagnostic {
        level: "info".to_string(),
        code: "office.bootstrap.complete-package-change".to_string(),
        message: "The editor supplied a complete OOXML package; unmodified package parts remain byte-preserved by the editor export.".to_string(),
    });
    Ok(prepared)
}

pub fn export(
    kind: OfficeKind,
    editor_payload: &[u8],
    original_package: Option<&[u8]>,
) -> anyhow::Result<OfficePackage> {
    inspect(kind, editor_payload)?;
    let bytes = match original_package {
        Some(original) => merge_understood_parts(kind, editor_payload, original)?,
        None => editor_payload.to_vec(),
    };
    let manifest = inspect(kind, &bytes)?;
    Ok(OfficePackage {
        kind,
        mime_type: kind.canonical_mime().to_string(),
        extension: kind.canonical_extension().to_string(),
        sha256: sha256_hex(&bytes),
        bytes,
        manifest,
        diagnostics: vec![OfficeDiagnostic {
            level: "info".to_string(),
            code: "office.escrow.understood-parts-export".to_string(),
            message: "The exporter replaced only understood OOXML parts and retained all other parts from the original escrow package.".to_string(),
        }],
    })
}

fn merge_understood_parts(
    kind: OfficeKind,
    editor_payload: &[u8],
    original_package: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if editor_payload == original_package {
        return Ok(original_package.to_vec());
    }
    let mut editor_archive =
        ZipArchive::new(Cursor::new(editor_payload)).context("open editor OOXML package")?;
    let mut replacements = std::collections::BTreeMap::new();
    for index in 0..editor_archive.len() {
        let mut entry = editor_archive
            .by_index(index)
            .context("read editor OOXML entry")?;
        if entry.is_dir() {
            continue;
        }
        let path = entry.name().replace('\\', "/");
        if !is_understood_part(kind, &path) {
            continue;
        }
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or_default());
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("read editor OOXML part {path}"))?;
        validate_xml(&path, &bytes)?;
        replacements.insert(path, bytes);
    }
    ensure!(
        replacements.contains_key(primary_part_for(kind)),
        "editor package is missing understood primary part {}",
        primary_part_for(kind)
    );

    let mut original_archive = ZipArchive::new(Cursor::new(original_package))
        .context("open original escrow OOXML package")?;
    let output = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(output);
    for index in 0..original_archive.len() {
        let mut entry = original_archive
            .by_index(index)
            .context("read original escrow OOXML entry")?;
        let path = entry.name().replace('\\', "/");
        ensure!(
            !path.starts_with('/') && !path.split('/').any(|segment| segment == ".."),
            "unsafe OOXML package path: {path}"
        );
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer
                .add_directory(&path, options)
                .with_context(|| format!("copy OOXML directory {path}"))?;
            continue;
        }
        if is_understood_part(kind, &path) {
            let Some(replacement) = replacements.remove(path.as_str()) else {
                // An understood header/footer part omitted by the editor was
                // intentionally detached and must not survive the escrow merge.
                continue;
            };
            writer
                .start_file(&path, options)
                .with_context(|| format!("start changed OOXML part {path}"))?;
            writer
                .write_all(&replacement)
                .with_context(|| format!("write changed OOXML part {path}"))?;
        } else {
            writer
                .start_file(&path, options)
                .with_context(|| format!("start OOXML part {path}"))?;
            std::io::copy(&mut entry, &mut writer)
                .with_context(|| format!("preserve OOXML part {path}"))?;
        }
    }
    for (path, bytes) in replacements {
        writer
            .start_file(&path, SimpleFileOptions::default())
            .with_context(|| format!("start added understood OOXML part {path}"))?;
        writer
            .write_all(&bytes)
            .with_context(|| format!("write added understood OOXML part {path}"))?;
    }
    Ok(writer.finish()?.into_inner())
}

fn primary_part_for(kind: OfficeKind) -> &'static str {
    match kind {
        OfficeKind::Document => "word/document.xml",
        OfficeKind::Spreadsheet => "xl/workbook.xml",
    }
}

fn is_understood_part(kind: OfficeKind, path: &str) -> bool {
    match kind {
        OfficeKind::Document => is_understood_document_part(path),
        OfficeKind::Spreadsheet => is_understood_spreadsheet_part(path),
    }
}

fn is_understood_document_part(path: &str) -> bool {
    matches!(
        path,
        "[Content_Types].xml"
            | "word/document.xml"
            | "word/numbering.xml"
            | "word/settings.xml"
            | "word/_rels/document.xml.rels"
    ) || is_header_footer_part(path)
        || is_comment_review_part(path)
        || is_chart_drawing_part(path)
}

fn is_header_footer_part(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or_default();
    let is_xml_part = (path.starts_with("word/header") || path.starts_with("word/footer"))
        && file_name.ends_with(".xml");
    let is_rels_part = path.starts_with("word/_rels/")
        && (file_name.starts_with("header") || file_name.starts_with("footer"))
        && file_name.ends_with(".xml.rels");
    is_xml_part || is_rels_part
}

fn is_comment_review_part(path: &str) -> bool {
    matches!(
        path,
        "word/comments.xml"
            | "word/commentsExtended.xml"
            | "word/commentsExtensible.xml"
            | "word/commentsIds.xml"
            | "word/people.xml"
            | "word/_rels/comments.xml.rels"
    )
}

fn is_chart_drawing_part(path: &str) -> bool {
    (path.starts_with("word/charts/") && path.ends_with(".xml"))
        || (path.starts_with("word/charts/_rels/") && path.ends_with(".rels"))
}

fn is_understood_spreadsheet_part(path: &str) -> bool {
    matches!(
        path,
        "[Content_Types].xml"
            | "xl/workbook.xml"
            | "xl/_rels/workbook.xml.rels"
            | "xl/sharedStrings.xml"
            | "xl/styles.xml"
    ) || (path.starts_with("xl/worksheets/") && path.ends_with(".xml"))
        || (path.starts_with("xl/worksheets/_rels/") && path.ends_with(".rels"))
        || (path.starts_with("xl/tables/") && path.ends_with(".xml"))
}

pub fn inspect(kind: OfficeKind, office_bytes: &[u8]) -> anyhow::Result<SemanticManifest> {
    ensure!(!office_bytes.is_empty(), "Office package is empty");
    let mut archive =
        ZipArchive::new(Cursor::new(office_bytes)).context("open OOXML ZIP package")?;
    let primary_part = primary_part_for(kind);
    let mut seen = BTreeSet::new();
    let mut parts = Vec::with_capacity(archive.len());
    let mut content_types_present = false;
    let mut relationship_parts = 0usize;
    let mut primary_text = String::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read OOXML ZIP entry")?;
        if !entry.is_file() {
            continue;
        }
        let path = entry.name().replace('\\', "/");
        ensure!(
            !path.starts_with('/') && !path.split('/').any(|segment| segment == ".."),
            "unsafe OOXML package path: {path}"
        );
        ensure!(
            seen.insert(path.clone()),
            "duplicate OOXML package part: {path}"
        );
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or_default());
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("read OOXML part {path}"))?;
        let xml = path.ends_with(".xml") || path.ends_with(".rels");
        if path == "[Content_Types].xml" {
            content_types_present = true;
        }
        if path.ends_with(".rels") {
            relationship_parts += 1;
            validate_xml(&path, &bytes)?;
        } else if xml {
            validate_xml(&path, &bytes)?;
        }
        if path == primary_part {
            primary_text = primary_text_for(kind, &bytes)?;
        }
        parts.push(OfficePartManifest {
            path,
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(&bytes),
            xml,
        });
    }

    ensure!(
        content_types_present,
        "OOXML package is missing [Content_Types].xml"
    );
    ensure!(
        seen.contains(primary_part),
        "OOXML package is missing {primary_part}"
    );
    parts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(SemanticManifest {
        schema_version: "ctox-office-semantic-manifest-v1".to_string(),
        kind,
        package_sha256: sha256_hex(office_bytes),
        parts,
        relationship_parts,
        content_types_present,
        primary_part: primary_part.to_string(),
        primary_text,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn validate_xml(path: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let text = std::str::from_utf8(bytes)
        .with_context(|| format!("OOXML XML part is not UTF-8: {path}"))?;
    ensure!(
        !contains_doctype_declaration(text),
        "OOXML XML part contains a forbidden DOCTYPE declaration: {path}"
    );
    roxmltree::Document::parse(text).with_context(|| format!("parse OOXML XML part {path}"))?;
    Ok(())
}

fn contains_doctype_declaration(text: &str) -> bool {
    text.as_bytes()
        .windows(b"<!DOCTYPE".len())
        .any(|window| window.eq_ignore_ascii_case(b"<!DOCTYPE"))
}

fn primary_text_for(kind: OfficeKind, bytes: &[u8]) -> anyhow::Result<String> {
    let xml = std::str::from_utf8(bytes).context("primary OOXML part is not UTF-8")?;
    let document = roxmltree::Document::parse(xml).context("parse primary OOXML part")?;
    let mut values = Vec::new();
    match kind {
        OfficeKind::Document => {
            for node in document.descendants().filter(|node| node.is_element()) {
                if node.tag_name().name() == "t" {
                    if let Some(text) = node.text().filter(|value| !value.is_empty()) {
                        values.push(text);
                    }
                }
            }
        }
        OfficeKind::Spreadsheet => {
            for node in document.descendants().filter(|node| node.is_element()) {
                if node.tag_name().name() == "sheet" {
                    if let Some(name) = node.attribute("name") {
                        values.push(name);
                    }
                }
            }
        }
    }
    Ok(values.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::{fs, path::Path};
    use zip::result::ZipError;
    use zip::write::SimpleFileOptions;

    fn docx(extra: Option<(&str, &[u8])>) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
        writer.start_file("_rels/.rels", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).unwrap();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello CTOX</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        if let Some((path, bytes)) = extra {
            writer.start_file(path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn docx_parts(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
        writer.start_file("_rels/.rels", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).unwrap();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello CTOX</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        for (path, bytes) in parts {
            writer.start_file(path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn prepare_and_export_preserve_complete_docx() {
        let source = docx(Some((
            "customXml/item1.xml",
            br#"<?xml version="1.0"?><root value="keep"/>"#,
        )));
        let prepared = prepare(OfficeKind::Document, &source, PrepareOptions::default()).unwrap();
        assert_eq!(prepared.editor_payload, source);
        assert_eq!(prepared.manifest.primary_text, "Hello CTOX");
        assert!(prepared
            .manifest
            .parts
            .iter()
            .any(|part| part.path == "customXml/item1.xml"));
        let exported = export(
            OfficeKind::Document,
            &prepared.editor_payload,
            Some(&source),
        )
        .unwrap();
        assert_eq!(exported.bytes, source);
        assert_eq!(exported.sha256, sha256_hex(&source));
    }

    #[test]
    fn inspect_rejects_external_entity_declarations() {
        let payload = br#"<?xml version="1.0"?>
<!DOCTYPE w:document [<!ENTITY xxe SYSTEM "file:///tmp/ctox-xxe-sentinel">]>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>&xxe;</w:t></w:r></w:p></w:body>
</w:document>"#;
        let package = docx(Some(("customXml/item1.xml", payload)));

        let error = inspect(OfficeKind::Document, &package).unwrap_err();

        assert!(error
            .to_string()
            .contains("forbidden DOCTYPE declaration: customXml/item1.xml"));
    }

    #[test]
    fn apply_changes_enforces_base_hash() {
        let source = docx(None);
        let error = apply_changes(
            OfficeKind::Document,
            &source,
            &source,
            ApplyChangesOptions {
                expected_base_sha256: "bad".to_string(),
                implemented_features: Vec::new(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("version_conflict"));
    }

    #[test]
    fn export_replaces_only_understood_document_part() {
        let original = docx(Some((
            "customXml/item1.xml",
            br#"<?xml version="1.0"?><root value="preserve-byte-for-byte"/>"#,
        )));
        let changed = {
            let cursor = Cursor::new(Vec::new());
            let mut writer = zip::ZipWriter::new(cursor);
            let options = SimpleFileOptions::default();
            writer.start_file("[Content_Types].xml", options).unwrap();
            writer.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
            writer.start_file("word/document.xml", options).unwrap();
            writer.write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Changed CTOX</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
            writer.finish().unwrap().into_inner()
        };
        let exported = export(OfficeKind::Document, &changed, Some(&original)).unwrap();
        assert_eq!(exported.manifest.primary_text, "Changed CTOX");

        let mut original_archive = ZipArchive::new(Cursor::new(&original)).unwrap();
        let mut exported_archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut original_custom = Vec::new();
        original_archive
            .by_name("customXml/item1.xml")
            .unwrap()
            .read_to_end(&mut original_custom)
            .unwrap();
        let mut exported_custom = Vec::new();
        exported_archive
            .by_name("customXml/item1.xml")
            .unwrap()
            .read_to_end(&mut exported_custom)
            .unwrap();
        assert_eq!(exported_custom, original_custom);
        assert!(!exported
            .manifest
            .parts
            .iter()
            .any(|part| part.path == "word/footnotes.xml"));
    }

    #[test]
    fn export_replaces_understood_numbering_part() {
        let original = docx(Some((
            "word/numbering.xml",
            br#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="8"><w:lvl w:ilvl="0"/></w:abstractNum></w:numbering>"#,
        )));
        let changed_numbering = br#"<?xml version="1.0"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="8"><w:lvl w:ilvl="0"/><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/><w:lvlText w:val=""/></w:lvl></w:abstractNum></w:numbering>"#;
        let changed = docx(Some(("word/numbering.xml", changed_numbering)));
        let exported = export(OfficeKind::Document, &changed, Some(&original)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut numbering = Vec::new();
        archive
            .by_name("word/numbering.xml")
            .unwrap()
            .read_to_end(&mut numbering)
            .unwrap();
        assert_eq!(numbering, changed_numbering);
    }

    #[test]
    fn export_merges_header_footer_part_family_and_preserves_escrow() {
        let original = docx_parts(&[
            ("word/header1.xml", br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Old header</w:t></w:r></w:p></w:hdr>"#),
            ("word/footer1.xml", br#"<?xml version="1.0"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p/></w:ftr>"#),
            ("customXml/preserve.xml", br#"<?xml version="1.0"?><keep>escrow</keep>"#),
        ]);
        let changed_header = br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Changed header</w:t></w:r></w:p></w:hdr>"#;
        let added_header = br#"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Added header</w:t></w:r></w:p></w:hdr>"#;
        let editor = docx_parts(&[
            ("word/header1.xml", changed_header),
            ("word/header2.xml", added_header),
        ]);
        let exported = export(OfficeKind::Document, &editor, Some(&original)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("word/header1.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, changed_header);
        bytes.clear();
        archive
            .by_name("word/header2.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, added_header);
        bytes.clear();
        archive
            .by_name("customXml/preserve.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, br#"<?xml version="1.0"?><keep>escrow</keep>"#);
        assert!(matches!(
            archive.by_name("word/footer1.xml"),
            Err(ZipError::FileNotFound)
        ));
    }

    #[test]
    fn export_merges_document_hyperlinks_and_preserves_escrow() {
        let original_relationships = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://ctox.dev/preserve-link" TargetMode="External"/></Relationships>"#;
        let changed_relationships = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://ctox.dev/preserve-link" TargetMode="External"/><Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://ctox.dev/office-oracle" TargetMode="External"/></Relationships>"#;
        let original = docx_parts(&[
            ("word/_rels/document.xml.rels", original_relationships),
            (
                "customXml/ctox-links-preserve.xml",
                br#"<?xml version="1.0"?><keep>link-escrow</keep>"#,
            ),
        ]);
        let editor = docx_parts(&[("word/_rels/document.xml.rels", changed_relationships)]);
        let exported = export(OfficeKind::Document, &editor, Some(&original)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("word/_rels/document.xml.rels")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, changed_relationships);
        bytes.clear();
        archive
            .by_name("customXml/ctox-links-preserve.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, br#"<?xml version="1.0"?><keep>link-escrow</keep>"#);
    }

    #[test]
    fn export_merges_comment_review_part_family_and_preserves_escrow() {
        let original_comments = br#"<?xml version="1.0"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="7"><w:p><w:r><w:t>Existing</w:t></w:r></w:p></w:comment></w:comments>"#;
        let changed_comments = br#"<?xml version="1.0"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="0"><w:p><w:r><w:t>New root</w:t></w:r></w:p></w:comment><w:comment w:id="1"><w:p><w:r><w:t>Reply</w:t></w:r></w:p></w:comment><w:comment w:id="7"><w:p><w:r><w:t>Existing</w:t></w:r></w:p></w:comment></w:comments>"#;
        let comments_extended = br#"<?xml version="1.0"?><w15:commentsEx xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"><w15:commentEx w15:paraId="1" w15:done="1"/></w15:commentsEx>"#;
        let people = br#"<?xml version="1.0"?><w15:people xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"><w15:person w15:author="CTOX"/></w15:people>"#;
        let original = docx_parts(&[
            ("word/comments.xml", original_comments),
            (
                "customXml/ctox-comments-preserve.xml",
                br#"<?xml version="1.0"?><keep>comments-escrow</keep>"#,
            ),
        ]);
        let editor = docx_parts(&[
            ("word/comments.xml", changed_comments),
            ("word/commentsExtended.xml", comments_extended),
            ("word/people.xml", people),
        ]);
        let exported = export(OfficeKind::Document, &editor, Some(&original)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("word/comments.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, changed_comments);
        bytes.clear();
        archive
            .by_name("word/commentsExtended.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, comments_extended);
        bytes.clear();
        archive
            .by_name("word/people.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, people);
        bytes.clear();
        archive
            .by_name("customXml/ctox-comments-preserve.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(
            bytes,
            br#"<?xml version="1.0"?><keep>comments-escrow</keep>"#
        );
    }

    #[test]
    fn export_merges_chart_part_family_and_preserves_embedding_escrow() {
        let original_chart = br#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart/></c:chartSpace>"#;
        let changed_chart = br#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:style val="2"/><c:chart/></c:chartSpace>"#;
        let chart_style = br#"<?xml version="1.0"?><cs:chartStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" id="102"/>"#;
        let chart_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/data.xlsx"/></Relationships>"#;
        let embedding = b"PK\x03\x04opaque-workbook";
        let original = docx_parts(&[
            ("word/charts/chart1.xml", original_chart),
            ("word/charts/_rels/chart1.xml.rels", chart_rels),
            ("word/embeddings/data.xlsx", embedding),
            (
                "customXml/ctox-drawings-preserve.xml",
                br#"<?xml version="1.0"?><keep>drawing-escrow</keep>"#,
            ),
        ]);
        let editor = docx_parts(&[
            ("word/charts/chart1.xml", changed_chart),
            ("word/charts/style1.xml", chart_style),
            ("word/charts/_rels/chart1.xml.rels", chart_rels),
        ]);
        let exported = export(OfficeKind::Document, &editor, Some(&original)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("word/charts/chart1.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, changed_chart);
        bytes.clear();
        archive
            .by_name("word/charts/style1.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, chart_style);
        bytes.clear();
        archive
            .by_name("word/embeddings/data.xlsx")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, embedding);
        bytes.clear();
        archive
            .by_name("customXml/ctox-drawings-preserve.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(
            bytes,
            br#"<?xml version="1.0"?><keep>drawing-escrow</keep>"#
        );
    }

    #[test]
    fn document_roundtrip_corpus_matches_manifest_and_preserves_identity() {
        let corpus_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/office/document");
        let manifest_bytes = fs::read(corpus_dir.join("corpus.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let entries = manifest["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 11);
        for entry in entries {
            let file_name = entry["file"].as_str().unwrap();
            let feature_id = entry["feature_id"].as_str().unwrap();
            let source = fs::read(corpus_dir.join(file_name)).unwrap();
            assert_eq!(source.len() as u64, entry["bytes"].as_u64().unwrap());
            assert_eq!(sha256_hex(&source), entry["sha256"].as_str().unwrap());
            let manifest = inspect(OfficeKind::Document, &source).unwrap();
            assert_eq!(
                manifest.parts.len() as u64,
                entry["parts"].as_u64().unwrap()
            );
            let prepared = prepare(
                OfficeKind::Document,
                &source,
                PrepareOptions {
                    implemented_features: vec![feature_id.to_string()],
                },
            )
            .unwrap();
            let exported = export(
                OfficeKind::Document,
                &prepared.editor_payload,
                Some(&source),
            )
            .unwrap();
            assert_eq!(
                exported.bytes, source,
                "identity roundtrip changed {file_name}"
            );
            let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
            for path in entry["must_preserve"].as_array().unwrap() {
                let path = path.as_str().unwrap();
                let mut preserved = Vec::new();
                archive
                    .by_name(path)
                    .unwrap_or_else(|_| panic!("missing preservation part {path} in {file_name}"))
                    .read_to_end(&mut preserved)
                    .unwrap();
                assert!(
                    !preserved.is_empty(),
                    "empty preservation part {path} in {file_name}"
                );
            }
        }
    }

    #[test]
    fn spreadsheet_open_render_fixture_roundtrips_with_sheet_manifest() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/office/spreadsheet/open-render-sheets.xlsx");
        let source = fs::read(fixture).unwrap();
        assert_eq!(source.len(), 4_951);
        assert_eq!(
            sha256_hex(&source),
            "e0553218296a0224945569b84bddfad70e9fdee60605333982c171cfe9891043"
        );
        let prepared = prepare(
            OfficeKind::Spreadsheet,
            &source,
            PrepareOptions {
                implemented_features: vec!["spreadsheet.open-render-sheets".to_string()],
            },
        )
        .unwrap();
        assert_eq!(prepared.manifest.parts.len(), 12);
        assert_eq!(prepared.manifest.primary_text, "Overview\nDetails\nArchive");
        let exported = export(
            OfficeKind::Spreadsheet,
            &prepared.editor_payload,
            Some(&source),
        )
        .unwrap();
        assert_eq!(exported.bytes, source);
        assert_eq!(exported.extension, "xlsx");
        let mut archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let mut escrow = Vec::new();
        archive
            .by_name("customXml/ctox-spreadsheet-preserve.xml")
            .unwrap()
            .read_to_end(&mut escrow)
            .unwrap();
        assert!(String::from_utf8(escrow)
            .unwrap()
            .contains("SPREADSHEET_CUSTOM_PART_5E91"));
    }

    #[test]
    fn spreadsheet_export_merges_shared_strings_and_preserves_escrow() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/office/spreadsheet/edit-save.xlsx");
        let source = fs::read(fixture).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let output = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(output);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let path = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if path == "xl/sharedStrings.xml" {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replace("CTOX_EDIT_CELL_ALPHA", "CTOX_EDIT_CELL_BRAVO_42")
                    .into_bytes();
            }
            writer
                .start_file(
                    path,
                    SimpleFileOptions::default().compression_method(entry.compression()),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        let changed = writer.finish().unwrap().into_inner();
        let package = export(OfficeKind::Spreadsheet, &changed, Some(&source)).unwrap();
        let mut merged = ZipArchive::new(Cursor::new(&package.bytes)).unwrap();
        let mut shared = String::new();
        merged
            .by_name("xl/sharedStrings.xml")
            .unwrap()
            .read_to_string(&mut shared)
            .unwrap();
        assert!(shared.contains("CTOX_EDIT_CELL_BRAVO_42"));
        let mut escrow = String::new();
        merged
            .by_name("customXml/ctox-spreadsheet-preserve.xml")
            .unwrap()
            .read_to_string(&mut escrow)
            .unwrap();
        assert!(escrow.contains("SPREADSHEET_EDIT_ESCROW_81F4"));
    }

    #[test]
    fn inspect_rejects_unsafe_or_incomplete_packages() {
        assert!(inspect(OfficeKind::Document, b"not a zip").is_err());
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file("word/document.xml", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(br#"<?xml version="1.0"?><document/>"#)
            .unwrap();
        let incomplete = writer.finish().unwrap().into_inner();
        assert!(inspect(OfficeKind::Document, &incomplete)
            .unwrap_err()
            .to_string()
            .contains("Content_Types"));
    }
}
