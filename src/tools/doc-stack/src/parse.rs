use anyhow::{Context, Result};
use ctox_pdf_parse::{
    parse_pdf_bytes as parse_pdf_bytes_internal, LiteParseConfigOverrides, OutputFormat,
};
use mailparse::{parse_mail, DispositionType, MailHeaderMap, ParsedMail};
use regex::Regex;
use roxmltree::{Document as XmlDocument, Node as XmlNode};
use scraper::Html;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;
use zip::ZipArchive;

use crate::formats::{parser_kind_for_path, supports_extension};

const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
const TARGET_CHUNK_CHARS: usize = 1_400;
const MIN_CHUNK_CHARS: usize = 280;

#[derive(Debug, Clone, Serialize)]
pub struct ParsedChunk {
    pub ordinal: usize,
    pub text: String,
    pub page_number: Option<usize>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub section_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedDocument {
    pub path: String,
    pub title: String,
    pub parser_kind: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub content_hash: String,
    pub is_pdf: bool,
    pub page_count: Option<usize>,
    pub chunks: Vec<ParsedChunk>,
}

#[derive(Debug, Clone)]
pub struct FileFingerprint {
    pub size_bytes: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone)]
struct ContentBlock {
    text: String,
    page_number: Option<usize>,
    section_title: Option<String>,
}

pub fn supported_document_file(path: &Path) -> bool {
    path.is_file()
        && !is_ignored_metadata_file(path)
        && (supports_extension(path) || looks_like_text(path).unwrap_or(false))
}

fn is_ignored_metadata_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("._"))
}

pub fn file_fingerprint(path: &Path) -> Result<FileFingerprint> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
        anyhow::bail!(
            "file {} exceeds the current size limit of {} bytes",
            path.display(),
            MAX_FILE_BYTES
        );
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);
    Ok(FileFingerprint {
        size_bytes: metadata.len(),
        modified_at,
    })
}

pub fn parse_document(path: &Path) -> Result<ParsedDocument> {
    let fingerprint = file_fingerprint(path)?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let title = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .to_string();
    let parser_kind = parser_kind_for_path(path).to_string();
    let content_hash = sha256_hex(&bytes);

    let (chunks, page_count, is_pdf) = match parser_kind.as_str() {
        "pdf" => {
            let (chunks, page_count) = parse_pdf_chunks(&bytes)?;
            (chunks, Some(page_count), true)
        }
        "docx" => (parse_docx_chunks(&bytes)?, None, false),
        "pptx" => {
            let (chunks, slide_count) = parse_pptx_chunks(&bytes)?;
            (chunks, Some(slide_count), false)
        }
        "xlsx" => (parse_xlsx_chunks(&bytes)?, None, false),
        "odt" => (parse_odt_chunks(&bytes)?, None, false),
        "ods" => (parse_ods_chunks(&bytes)?, None, false),
        "odp" => {
            let (chunks, slide_count) = parse_odp_chunks(&bytes)?;
            (chunks, Some(slide_count), false)
        }
        "email" => (parse_email_chunks(&bytes)?, None, false),
        _ => {
            let text = extract_text_for_kind(&parser_kind, &bytes)?;
            let chunks = if parser_kind == "table" {
                chunk_table_document(&text)
            } else {
                chunk_text_document(&text, &parser_kind)
            };
            (chunks, None, false)
        }
    };

    Ok(ParsedDocument {
        path: path.display().to_string(),
        title,
        parser_kind,
        size_bytes: fingerprint.size_bytes,
        modified_at: fingerprint.modified_at,
        content_hash,
        is_pdf,
        page_count,
        chunks,
    })
}

fn looks_like_text(path: &Path) -> Result<bool> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let sample = &bytes[..bytes.len().min(2_048)];
    if sample.is_empty() {
        return Ok(false);
    }
    let printable = sample
        .iter()
        .filter(|byte| matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e | 0x80..=0xff))
        .count();
    Ok((printable as f64 / sample.len() as f64) > 0.95)
}

fn extract_text_for_kind(kind: &str, bytes: &[u8]) -> Result<String> {
    let raw = String::from_utf8_lossy(bytes).to_string();
    match kind {
        "html" => Ok(extract_html_text(&raw)),
        "xml" => Ok(extract_xml_text(&raw)?),
        "rtf" => Ok(strip_rtf_text(&raw)),
        _ => Ok(clean_text(&raw)),
    }
}

fn parse_pdf_chunks(bytes: &[u8]) -> Result<(Vec<ParsedChunk>, usize)> {
    let parsed = parse_pdf_bytes_internal(
        bytes,
        LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            output_format: Some(OutputFormat::Text),
            ..Default::default()
        },
    )
    .context("failed to parse PDF bytes")?;
    let mut chunks = Vec::new();
    let mut ordinal = 0usize;
    for page in parsed.pages {
        let page_text = clean_pdf_text(&page.text);
        if page_text.trim().is_empty() {
            continue;
        }
        for paragraph in split_into_paragraphs(&page_text) {
            for text in fold_paragraphs_into_chunks(&paragraph, TARGET_CHUNK_CHARS) {
                chunks.push(ParsedChunk {
                    ordinal,
                    text,
                    page_number: Some(page.page_num),
                    start_line: None,
                    end_line: None,
                    section_title: None,
                });
                ordinal += 1;
            }
        }
    }
    Ok((chunks, parsed.total_pages))
}

fn parse_docx_chunks(bytes: &[u8]) -> Result<Vec<ParsedChunk>> {
    let mut archive = open_zip_archive(bytes)?;
    let document_xml = read_zip_entry_string(&mut archive, "word/document.xml")
        .context("DOCX missing word/document.xml")?;
    let xml = XmlDocument::parse(&document_xml).context("failed to parse DOCX document.xml")?;
    let mut blocks = Vec::new();
    let mut current_section = None;

    for paragraph in xml.descendants().filter(|node| has_tag(*node, "p")) {
        let text = clean_text(&extract_docx_paragraph_text(paragraph));
        if text.is_empty() {
            continue;
        }
        let style = paragraph
            .descendants()
            .find(|node| has_tag(*node, "pStyle"))
            .and_then(|node| attribute_local(node, "val"));
        let is_heading = style.as_deref().map(is_heading_style).unwrap_or(false);
        if is_heading {
            current_section = Some(text.clone());
            blocks.push(ContentBlock {
                text,
                page_number: None,
                section_title: current_section.clone(),
            });
        } else {
            blocks.push(ContentBlock {
                text,
                page_number: None,
                section_title: current_section.clone(),
            });
        }
    }

    Ok(chunk_blocks(blocks))
}

fn parse_pptx_chunks(bytes: &[u8]) -> Result<(Vec<ParsedChunk>, usize)> {
    let mut archive = open_zip_archive(bytes)?;
    let mut slide_paths = zip_entry_names(&mut archive, |name| {
        name.starts_with("ppt/slides/slide") && name.ends_with(".xml")
    });
    slide_paths.sort_by_key(|name| trailing_number(name, "ppt/slides/slide", ".xml"));

    let mut blocks = Vec::new();
    for (index, slide_path) in slide_paths.iter().enumerate() {
        let slide_xml = read_zip_entry_string(&mut archive, slide_path)?;
        let slide_text = extract_presentation_slide_text(&slide_xml)
            .with_context(|| format!("failed to extract PPTX slide {}", index + 1))?;
        if slide_text.trim().is_empty() {
            continue;
        }
        let title = first_meaningful_line(&slide_text);
        blocks.push(ContentBlock {
            text: slide_text,
            page_number: Some(index + 1),
            section_title: title,
        });
    }

    Ok((chunk_blocks(blocks), slide_paths.len()))
}

fn parse_xlsx_chunks(bytes: &[u8]) -> Result<Vec<ParsedChunk>> {
    let mut archive = open_zip_archive(bytes)?;
    let shared_strings = read_optional_zip_entry_string(&mut archive, "xl/sharedStrings.xml")?
        .map(|xml| parse_shared_strings(&xml))
        .transpose()?
        .unwrap_or_default();
    let sheet_names = read_optional_zip_entry_string(&mut archive, "xl/workbook.xml")?
        .map(|xml| parse_workbook_sheet_names(&xml))
        .transpose()?
        .unwrap_or_default();
    let mut worksheet_paths = zip_entry_names(&mut archive, |name| {
        name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml")
    });
    worksheet_paths.sort_by_key(|name| trailing_number(name, "xl/worksheets/sheet", ".xml"));

    let mut blocks = Vec::new();
    for (index, worksheet_path) in worksheet_paths.iter().enumerate() {
        let worksheet_xml = read_zip_entry_string(&mut archive, worksheet_path)?;
        let sheet_name = sheet_names
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("Sheet {}", index + 1));
        blocks.extend(parse_xlsx_sheet_blocks(
            &worksheet_xml,
            &shared_strings,
            &sheet_name,
        )?);
    }

    Ok(chunk_blocks(blocks))
}

fn parse_odt_chunks(bytes: &[u8]) -> Result<Vec<ParsedChunk>> {
    let mut archive = open_zip_archive(bytes)?;
    let content_xml =
        read_zip_entry_string(&mut archive, "content.xml").context("ODT missing content.xml")?;
    let xml = XmlDocument::parse(&content_xml).context("failed to parse ODT content.xml")?;
    let mut blocks = Vec::new();
    let mut current_section = None;

    for node in xml
        .descendants()
        .filter(|node| has_tag(*node, "h") || has_tag(*node, "p"))
    {
        let text = clean_text(&xml_node_text(node));
        if text.is_empty() {
            continue;
        }
        if has_tag(node, "h") {
            current_section = Some(text.clone());
        }
        blocks.push(ContentBlock {
            text,
            page_number: None,
            section_title: current_section.clone(),
        });
    }

    Ok(chunk_blocks(blocks))
}

fn parse_ods_chunks(bytes: &[u8]) -> Result<Vec<ParsedChunk>> {
    let mut archive = open_zip_archive(bytes)?;
    let content_xml =
        read_zip_entry_string(&mut archive, "content.xml").context("ODS missing content.xml")?;
    let xml = XmlDocument::parse(&content_xml).context("failed to parse ODS content.xml")?;
    let mut blocks = Vec::new();

    for table in xml.descendants().filter(|node| has_tag(*node, "table")) {
        let sheet_name = attribute_local(table, "name")
            .or_else(|| first_meaningful_line(&xml_node_text(table)))
            .unwrap_or_else(|| "Sheet".to_string());
        for row in table
            .descendants()
            .filter(|node| has_tag(*node, "table-row"))
        {
            let mut cells = Vec::new();
            for cell in row.children().filter(|node| has_tag(*node, "table-cell")) {
                let text = clean_text(&xml_node_text(cell));
                if !text.is_empty() {
                    cells.push(text);
                }
            }
            if cells.is_empty() {
                continue;
            }
            blocks.push(ContentBlock {
                text: format!("{} | {}", sheet_name, cells.join(" | ")),
                page_number: None,
                section_title: Some(sheet_name.clone()),
            });
        }
    }

    Ok(chunk_blocks(blocks))
}

fn parse_odp_chunks(bytes: &[u8]) -> Result<(Vec<ParsedChunk>, usize)> {
    let mut archive = open_zip_archive(bytes)?;
    let content_xml =
        read_zip_entry_string(&mut archive, "content.xml").context("ODP missing content.xml")?;
    let xml = XmlDocument::parse(&content_xml).context("failed to parse ODP content.xml")?;
    let mut blocks = Vec::new();
    let mut slide_count = 0usize;

    for page in xml.descendants().filter(|node| has_tag(*node, "page")) {
        slide_count += 1;
        let mut lines = Vec::new();
        for paragraph in page
            .descendants()
            .filter(|node| has_tag(*node, "p") || has_tag(*node, "h"))
        {
            let text = clean_text(&xml_node_text(paragraph));
            if !text.is_empty() {
                lines.push(text);
            }
        }
        if lines.is_empty() {
            continue;
        }
        let slide_text = lines.join("\n\n");
        let title = attribute_local(page, "name").or_else(|| first_meaningful_line(&slide_text));
        blocks.push(ContentBlock {
            text: slide_text,
            page_number: Some(slide_count),
            section_title: title,
        });
    }

    Ok((chunk_blocks(blocks), slide_count))
}

fn parse_email_chunks(bytes: &[u8]) -> Result<Vec<ParsedChunk>> {
    let parsed = parse_mail(bytes).context("failed to parse email message")?;
    let subject = header_value(&parsed, "Subject");
    let from = header_value(&parsed, "From");
    let to = header_value(&parsed, "To");
    let date = header_value(&parsed, "Date");

    let mut blocks = Vec::new();
    let mut header_lines = Vec::new();
    if let Some(subject) = &subject {
        header_lines.push(format!("Subject: {subject}"));
    }
    if let Some(from) = from {
        header_lines.push(format!("From: {from}"));
    }
    if let Some(to) = to {
        header_lines.push(format!("To: {to}"));
    }
    if let Some(date) = date {
        header_lines.push(format!("Date: {date}"));
    }
    if !header_lines.is_empty() {
        blocks.push(ContentBlock {
            text: header_lines.join("\n"),
            page_number: None,
            section_title: subject.clone(),
        });
    }

    let body = best_mail_body(&parsed).unwrap_or_default();
    for paragraph in split_into_paragraphs(&body) {
        blocks.push(ContentBlock {
            text: paragraph,
            page_number: None,
            section_title: subject.clone(),
        });
    }

    Ok(chunk_blocks(blocks))
}

fn chunk_text_document(text: &str, parser_kind: &str) -> Vec<ParsedChunk> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut current_start_line = 1usize;
    let mut current_chars = 0usize;
    let mut section_title: Option<String> = None;

    let flush = |chunks: &mut Vec<ParsedChunk>,
                 current: &mut Vec<String>,
                 current_start_line: &mut usize,
                 current_chars: &mut usize,
                 section_title: &Option<String>| {
        let text = current.join("\n");
        let text = clean_text(&text);
        if text.is_empty() {
            current.clear();
            *current_chars = 0;
            return;
        }
        let end_line = current_start_line.saturating_add(current.len().saturating_sub(1));
        for piece in fold_paragraphs_into_chunks(&text, TARGET_CHUNK_CHARS) {
            let ordinal = chunks.len();
            chunks.push(ParsedChunk {
                ordinal,
                text: piece,
                page_number: None,
                start_line: Some(*current_start_line),
                end_line: Some(end_line),
                section_title: section_title.clone(),
            });
        }
        current.clear();
        *current_chars = 0;
    };

    for (index, raw_line) in lines.iter().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim_end();
        let heading = parser_kind == "markdown" && trimmed.starts_with('#');

        if current.is_empty() {
            current_start_line = line_number;
        }

        if heading {
            flush(
                &mut chunks,
                &mut current,
                &mut current_start_line,
                &mut current_chars,
                &section_title,
            );
            section_title = Some(trimmed.trim_start_matches('#').trim().to_string());
            current_start_line = line_number;
        }

        current.push(trimmed.to_string());
        current_chars = current_chars.saturating_add(trimmed.len() + 1);
        let paragraph_break = trimmed.is_empty();
        if current_chars >= TARGET_CHUNK_CHARS
            || (paragraph_break && current_chars >= MIN_CHUNK_CHARS)
        {
            flush(
                &mut chunks,
                &mut current,
                &mut current_start_line,
                &mut current_chars,
                &section_title,
            );
        }
    }

    flush(
        &mut chunks,
        &mut current,
        &mut current_start_line,
        &mut current_chars,
        &section_title,
    );

    chunks
}

fn chunk_table_document(text: &str) -> Vec<ParsedChunk> {
    let lines = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some((index + 1, trimmed.to_string()))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let header = lines.first().map(|(_, line)| line.clone());
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut current_start_line = lines.first().map(|(line_no, _)| *line_no).unwrap_or(1);
    let mut current_chars = 0usize;

    let flush = |chunks: &mut Vec<ParsedChunk>,
                 current_lines: &mut Vec<String>,
                 current_start_line: usize,
                 current_chars: &mut usize,
                 header: &Option<String>| {
        if current_lines.is_empty() {
            *current_chars = 0;
            return;
        }
        let mut text_lines = Vec::new();
        if let Some(header) = header {
            text_lines.push(header.clone());
        }
        text_lines.extend(current_lines.clone());
        let text = clean_text(&text_lines.join("\n"));
        let ordinal = chunks.len();
        chunks.push(ParsedChunk {
            ordinal,
            text,
            page_number: None,
            start_line: Some(current_start_line),
            end_line: Some(current_start_line + current_lines.len().saturating_sub(1)),
            section_title: header.clone(),
        });
        current_lines.clear();
        *current_chars = 0;
    };

    for (index, (line_no, line)) in lines.into_iter().enumerate() {
        if index == 0 {
            current_start_line = line_no;
        }
        if current_lines.is_empty() {
            current_start_line = line_no;
        }
        current_chars = current_chars.saturating_add(line.len() + 1);
        current_lines.push(line);
        if current_chars >= TARGET_CHUNK_CHARS || current_lines.len() >= 24 {
            flush(
                &mut chunks,
                &mut current_lines,
                current_start_line,
                &mut current_chars,
                &header,
            );
        }
    }

    flush(
        &mut chunks,
        &mut current_lines,
        current_start_line,
        &mut current_chars,
        &header,
    );

    chunks
}

fn chunk_blocks(blocks: Vec<ContentBlock>) -> Vec<ParsedChunk> {
    let mut chunks = Vec::new();
    let mut current_text = String::new();
    let mut current_page = None;
    let mut current_section = None;

    let flush = |chunks: &mut Vec<ParsedChunk>,
                 current_text: &mut String,
                 current_page: &mut Option<usize>,
                 current_section: &mut Option<String>| {
        let text = clean_text(current_text);
        if text.is_empty() {
            current_text.clear();
            return;
        }
        for piece in fold_paragraphs_into_chunks(&text, TARGET_CHUNK_CHARS) {
            let ordinal = chunks.len();
            chunks.push(ParsedChunk {
                ordinal,
                text: piece,
                page_number: *current_page,
                start_line: None,
                end_line: None,
                section_title: current_section.clone(),
            });
        }
        current_text.clear();
    };

    for block in blocks {
        let text = clean_text(&block.text);
        if text.is_empty() {
            continue;
        }
        let metadata_changed = !current_text.is_empty()
            && (current_page != block.page_number || current_section != block.section_title);
        let would_overflow =
            !current_text.is_empty() && current_text.len() + 2 + text.len() > TARGET_CHUNK_CHARS;
        if metadata_changed || would_overflow {
            flush(
                &mut chunks,
                &mut current_text,
                &mut current_page,
                &mut current_section,
            );
        }
        if current_text.is_empty() {
            current_page = block.page_number;
            current_section = block.section_title.clone();
        } else {
            current_text.push_str("\n\n");
        }
        current_text.push_str(&text);
    }

    flush(
        &mut chunks,
        &mut current_text,
        &mut current_page,
        &mut current_section,
    );

    chunks
}

fn split_into_paragraphs(text: &str) -> Vec<String> {
    clean_text(text)
        .split("\n\n")
        .map(clean_text)
        .filter(|value| !value.is_empty())
        .collect()
}

fn fold_paragraphs_into_chunks(text: &str, target_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in split_into_paragraphs(text) {
        if current.is_empty() {
            current.push_str(&paragraph);
            continue;
        }
        if current.len() + 2 + paragraph.len() > target_chars {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
            }
            current.clear();
            current.push_str(&paragraph);
        } else {
            current.push_str("\n\n");
            current.push_str(&paragraph);
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(text.trim().to_string());
    }
    chunks
}

fn clean_text(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\u{000c}", "\n")
        .replace("\u{00a0}", " ")
        .trim()
        .to_string()
}

fn clean_pdf_text(text: &str) -> String {
    clean_text(text)
        .lines()
        .filter(|line| !line.trim().chars().all(|value| value.is_numeric()))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn extract_html_text(raw: &str) -> String {
    clean_text(
        &Html::parse_document(raw)
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn extract_xml_text(raw: &str) -> Result<String> {
    let xml = XmlDocument::parse(raw).context("failed to parse XML document")?;
    let lines = xml
        .descendants()
        .filter(|node| {
            has_tag(*node, "p")
                || has_tag(*node, "h")
                || has_tag(*node, "title")
                || has_tag(*node, "item")
                || has_tag(*node, "entry")
                || has_tag(*node, "row")
        })
        .map(xml_node_text)
        .map(|text| clean_text(&text))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    if !lines.is_empty() {
        Ok(lines.join("\n\n"))
    } else {
        Ok(clean_text(
            &xml.descendants()
                .filter_map(|node| node.text())
                .collect::<Vec<_>>()
                .join(" "),
        ))
    }
}

fn strip_rtf_text(raw: &str) -> String {
    let hex_re = Regex::new(r#"\\'([0-9a-fA-F]{2})"#).expect("valid RTF hex regex");
    let control_re = Regex::new(r#"\\[a-zA-Z]+-?\d* ?"#).expect("valid RTF control regex");
    let mut text = hex_re
        .replace_all(raw, |captures: &regex::Captures<'_>| {
            u8::from_str_radix(&captures[1], 16)
                .ok()
                .map(|value| (value as char).to_string())
                .unwrap_or_default()
        })
        .to_string();
    for (pattern, replacement) in [
        ("\\par", "\n"),
        ("\\pard", "\n"),
        ("\\line", "\n"),
        ("\\tab", "\t"),
        ("\\{", "{"),
        ("\\}", "}"),
        ("\\\\", "\\"),
    ] {
        text = text.replace(pattern, replacement);
    }
    text = control_re.replace_all(&text, " ").to_string();
    text = text.replace(['{', '}'], " ");
    clean_text(&text)
}

fn open_zip_archive(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>> {
    ZipArchive::new(Cursor::new(bytes)).context("failed to parse zipped document container")
}

fn read_zip_entry_string(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("missing archive entry {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read archive entry {name}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn read_optional_zip_entry_string(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(mut file) => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .with_context(|| format!("failed to read archive entry {name}"))?;
            Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(err)).with_context(|| format!("failed to open {name}")),
    }
}

fn zip_entry_names(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    predicate: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut names = Vec::new();
    for index in 0..archive.len() {
        if let Ok(file) = archive.by_index(index) {
            let name = file.name().to_string();
            if predicate(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn trailing_number(path: &str, prefix: &str, suffix: &str) -> usize {
    path.strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

fn has_tag(node: XmlNode<'_, '_>, expected: &str) -> bool {
    node.is_element() && node.tag_name().name() == expected
}

fn attribute_local(node: XmlNode<'_, '_>, expected: &str) -> Option<String> {
    node.attributes()
        .find(|attribute| attribute.name() == expected)
        .map(|attribute| attribute.value().to_string())
}

fn xml_node_text(node: XmlNode<'_, '_>) -> String {
    clean_text(
        &node
            .descendants()
            .filter_map(|child| child.text())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn extract_docx_paragraph_text(paragraph: XmlNode<'_, '_>) -> String {
    let mut out = String::new();
    for node in paragraph.descendants().filter(|node| node.is_element()) {
        match node.tag_name().name() {
            "t" => {
                if let Some(text) = node.text() {
                    out.push_str(text);
                }
            }
            "tab" => out.push('\t'),
            "br" | "cr" => out.push('\n'),
            _ => {}
        }
    }
    clean_text(&out)
}

fn is_heading_style(style: &str) -> bool {
    let lowered = style.to_ascii_lowercase();
    lowered.starts_with("heading") || lowered == "title" || lowered == "subtitle"
}

fn extract_presentation_slide_text(xml: &str) -> Result<String> {
    let document = XmlDocument::parse(xml).context("failed to parse presentation slide XML")?;
    let lines = document
        .descendants()
        .filter(|node| has_tag(*node, "t"))
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Ok(clean_text(&lines.join("\n")))
}

fn parse_shared_strings(xml: &str) -> Result<Vec<String>> {
    let document = XmlDocument::parse(xml).context("failed to parse XLSX sharedStrings.xml")?;
    Ok(document
        .descendants()
        .filter(|node| has_tag(*node, "si"))
        .map(xml_node_text)
        .map(|text| clean_text(&text))
        .collect())
}

fn parse_workbook_sheet_names(xml: &str) -> Result<Vec<String>> {
    let document = XmlDocument::parse(xml).context("failed to parse XLSX workbook.xml")?;
    Ok(document
        .descendants()
        .filter(|node| has_tag(*node, "sheet"))
        .filter_map(|node| attribute_local(node, "name"))
        .collect())
}

fn parse_xlsx_sheet_blocks(
    worksheet_xml: &str,
    shared_strings: &[String],
    sheet_name: &str,
) -> Result<Vec<ContentBlock>> {
    let document = XmlDocument::parse(worksheet_xml).context("failed to parse XLSX worksheet")?;
    let mut blocks = Vec::new();
    for row in document.descendants().filter(|node| has_tag(*node, "row")) {
        let row_number = attribute_local(row, "r").unwrap_or_else(|| "?".to_string());
        let mut cells = Vec::new();
        for cell in row.children().filter(|node| has_tag(*node, "c")) {
            let cell_ref = attribute_local(cell, "r").unwrap_or_default();
            let cell_type = attribute_local(cell, "t").unwrap_or_default();
            let value = if cell_type == "inlineStr" {
                clean_text(&xml_node_text(cell))
            } else {
                let raw_value = cell
                    .children()
                    .find(|node| has_tag(*node, "v"))
                    .and_then(|node| node.text())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                match cell_type.as_str() {
                    "s" => raw_value
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| shared_strings.get(index).cloned())
                        .unwrap_or(raw_value),
                    "b" => {
                        if raw_value == "1" {
                            "true".to_string()
                        } else {
                            "false".to_string()
                        }
                    }
                    _ => raw_value,
                }
            };
            if value.is_empty() {
                continue;
            }
            if cell_ref.is_empty() {
                cells.push(value);
            } else {
                cells.push(format!("{cell_ref}={value}"));
            }
        }
        if cells.is_empty() {
            continue;
        }
        blocks.push(ContentBlock {
            text: format!("Row {row_number}: {}", cells.join(" | ")),
            page_number: None,
            section_title: Some(sheet_name.to_string()),
        });
    }
    Ok(blocks)
}

fn header_value(parsed: &ParsedMail<'_>, header: &str) -> Option<String> {
    parsed
        .headers
        .get_first_value(header)
        .map(|value| clean_text(&value))
        .filter(|value| !value.is_empty())
}

fn best_mail_body(parsed: &ParsedMail<'_>) -> Option<String> {
    let mut best = None;
    collect_mail_bodies(parsed, &mut best);
    best.map(|(_, body)| clean_text(&body))
        .filter(|body| !body.is_empty())
}

fn collect_mail_bodies(parsed: &ParsedMail<'_>, best: &mut Option<(u8, String)>) {
    if !parsed.subparts.is_empty() {
        for subpart in &parsed.subparts {
            collect_mail_bodies(subpart, best);
        }
        return;
    }

    let disposition = parsed.get_content_disposition().disposition;
    if matches!(disposition, DispositionType::Attachment) {
        return;
    }

    let mime_type = parsed.ctype.mimetype.to_ascii_lowercase();
    let body = parsed.get_body().ok().unwrap_or_default();
    let (priority, normalized) = match mime_type.as_str() {
        "text/plain" => (0, clean_text(&body)),
        "text/html" => (1, extract_html_text(&body)),
        _ => (2, clean_text(&body)),
    };
    if normalized.is_empty() {
        return;
    }

    match best {
        Some((best_priority, _)) if *best_priority <= priority => {}
        _ => *best = Some((priority, normalized)),
    }
}

fn first_meaningful_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::chunk_text_document;
    use super::parse_document;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::FileOptions;
    use zip::CompressionMethod;
    use zip::ZipWriter;

    #[test]
    fn markdown_chunking_keeps_heading_and_line_numbers() {
        let text = "# Title\n\nParagraph one.\n\n## Section\nLine a\nLine b\n";
        let chunks = chunk_text_document(text, "markdown");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].section_title.as_deref(), Some("Title"));
        assert_eq!(chunks[0].start_line, Some(1));
        assert_eq!(chunks[1].section_title.as_deref(), Some("Section"));
        assert_eq!(chunks[1].start_line, Some(5));
        assert!(chunks[1].text.contains("Line a"));
    }

    #[test]
    fn docx_parser_extracts_heading_and_body() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.docx");
        let file = fs::File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
              <w:body>
                <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Quarterly Plan</w:t></w:r></w:p>
                <w:p><w:r><w:t>Ship the local retrieval pipeline.</w:t></w:r></w:p>
              </w:body>
            </w:document>"#).unwrap();
        zip.finish().unwrap();

        let parsed = parse_document(&path).unwrap();
        assert_eq!(parsed.parser_kind, "docx");
        assert_eq!(parsed.chunks.len(), 1);
        assert_eq!(
            parsed.chunks[0].section_title.as_deref(),
            Some("Quarterly Plan")
        );
        assert!(parsed.chunks[0]
            .text
            .contains("Ship the local retrieval pipeline."));
    }

    #[test]
    fn xlsx_parser_extracts_sheet_rows() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sheet.xlsx");
        let file = fs::File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("xl/workbook.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
            <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
              <sheets><sheet name="Budget" sheetId="1"/></sheets>
            </workbook>"#,
        )
        .unwrap();
        zip.start_file("xl/sharedStrings.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
            <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
              <si><t>Owner</t></si>
              <si><t>Michael</t></si>
            </sst>"#,
        )
        .unwrap();
        zip.start_file("xl/worksheets/sheet1.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
            <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
              <sheetData>
                <row r="1">
                  <c r="A1" t="s"><v>0</v></c>
                  <c r="B1"><v>42</v></c>
                </row>
                <row r="2">
                  <c r="A2" t="s"><v>1</v></c>
                  <c r="B2"><v>7</v></c>
                </row>
              </sheetData>
            </worksheet>"#,
        )
        .unwrap();
        zip.finish().unwrap();

        let parsed = parse_document(&path).unwrap();
        assert_eq!(parsed.parser_kind, "xlsx");
        assert_eq!(parsed.chunks.len(), 1);
        assert_eq!(parsed.chunks[0].section_title.as_deref(), Some("Budget"));
        assert!(parsed.chunks[0].text.contains("A1=Owner"));
        assert!(parsed.chunks[0].text.contains("A2=Michael"));
    }

    #[test]
    fn email_parser_extracts_headers_and_body() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("message.eml");
        fs::write(
            &path,
            concat!(
                "From: Alice <alice@example.com>\r\n",
                "To: Bob <bob@example.com>\r\n",
                "Subject: Planning\r\n",
                "Content-Type: text/plain; charset=utf-8\r\n",
                "\r\n",
                "The roadmap is attached.\r\n"
            ),
        )
        .unwrap();

        let parsed = parse_document(&path).unwrap();
        assert_eq!(parsed.parser_kind, "email");
        assert_eq!(parsed.chunks[0].section_title.as_deref(), Some("Planning"));
        assert!(parsed.chunks[0].text.contains("Subject: Planning"));
        assert!(parsed.chunks[0].text.contains("The roadmap is attached."));
    }
}
