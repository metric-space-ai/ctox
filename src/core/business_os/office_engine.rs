// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::{ensure, Context};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub const EDITOR_PROTOCOL: &str = "ctox-ooxml-escrow-v1";
pub const EDITOR_PROTOCOL_VERSION: u32 = 1;
pub const DOCUMENT_EDITOR_PROTOCOL: &str = "euro-office-word-binary-v10";
pub const DOCUMENT_EDITOR_PROTOCOL_VERSION: u32 = 10;
pub const SPREADSHEET_EDITOR_PROTOCOL: &str = "euro-office-cell-binary-v10";
pub const SPREADSHEET_EDITOR_PROTOCOL_VERSION: u32 = 10;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_manifest: Option<EditorPayloadManifest>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorPayloadManifest {
    pub schema_version: String,
    pub kind: OfficeKind,
    pub protocol: String,
    pub protocol_version: u32,
    pub declared_data_size: u64,
    pub payload_bytes: u64,
    pub payload_sha256: String,
    pub table_directory_bytes: u64,
    pub tables: Vec<EditorBinaryTable>,
    pub shared_strings: Vec<String>,
    #[serde(default)]
    pub defined_names: Vec<EditorDefinedNameManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbook_protection: Option<EditorWorkbookProtectionManifest>,
    pub worksheets: Vec<EditorWorksheetManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorDefinedNameManifest {
    pub name: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_sheet_id: Option<u32>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EditorWorkbookProtectionManifest {
    pub lock_structure: bool,
    pub lock_windows: bool,
    pub lock_revision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EditorSheetProtectionManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub sheet: bool,
    pub objects: bool,
    pub scenarios: bool,
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_hyperlinks: bool,
    pub insert_rows: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub select_locked_cells: bool,
    pub sort: bool,
    pub auto_filter: bool,
    pub pivot_tables: bool,
    pub select_unlocked_cells: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorSpreadsheetCommentManifest {
    pub reference: String,
    pub text: String,
    pub author: String,
    pub guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorBinaryTable {
    pub table_type: u8,
    pub name: String,
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorWorksheetManifest {
    pub name: String,
    pub sheet_id: u32,
    pub visibility: String,
    pub xlsb_offset: u64,
    #[serde(default)]
    pub default_row_height: Option<f64>,
    #[serde(default)]
    pub columns: Vec<EditorColumnManifest>,
    #[serde(default)]
    pub rows: Vec<EditorRowManifest>,
    #[serde(default)]
    pub merged_cells: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_pane: Option<EditorFrozenPaneManifest>,
    #[serde(default)]
    pub tables: Vec<EditorTableManifest>,
    #[serde(default)]
    pub data_validations: Vec<EditorDataValidationManifest>,
    #[serde(default)]
    pub conditional_formats: Vec<EditorConditionalFormatManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<EditorSheetProtectionManifest>,
    #[serde(default)]
    pub comments: Vec<EditorSpreadsheetCommentManifest>,
    pub cells: Vec<EditorCellManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorDataValidationManifest {
    pub reference: String,
    pub validation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    pub allow_blank: bool,
    pub show_error_message: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorConditionalFormatManifest {
    pub reference: String,
    pub rule_type: String,
    pub priority: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(default)]
    pub formulas: Vec<String>,
    #[serde(default)]
    pub thresholds: Vec<EditorConditionalThresholdManifest>,
    #[serde(default)]
    pub colors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub differential_style: Option<EditorDifferentialStyleManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorConditionalThresholdManifest {
    pub threshold_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EditorDifferentialStyleManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_rgb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_rgb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorFrozenPaneManifest {
    pub active_pane: String,
    pub state: String,
    pub top_left_cell: String,
    pub x_split: f64,
    pub y_split: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorTableManifest {
    pub reference: String,
    pub display_name: String,
    pub style_name: String,
    pub show_column_stripes: bool,
    pub show_row_stripes: bool,
    pub show_first_column: bool,
    pub show_last_column: bool,
    pub columns: Vec<String>,
    pub filters: Vec<EditorFilterColumnManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<EditorSortManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorFilterColumnManifest {
    pub column_id: u32,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorSortManifest {
    pub reference: String,
    pub condition_reference: String,
    pub descending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorColumnManifest {
    pub min: u32,
    pub max: u32,
    pub width: f64,
    pub custom_width: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorRowManifest {
    pub index: u32,
    pub height: Option<f64>,
    pub custom_height: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorCellManifest {
    pub reference: String,
    pub value_type: String,
    pub display: String,
    pub style_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
}

/// Parse and validate the native Euro-Office editor payload without invoking
/// DocumentServer. The table directory is the stable boundary used by the
/// incremental Rust writer port; unknown table types remain preserved.
// ref: sdkjs/cell/model/Serialize.js:13733-13870
pub fn inspect_editor_payload(
    kind: OfficeKind,
    editor_payload: &[u8],
) -> anyhow::Result<EditorPayloadManifest> {
    if kind == OfficeKind::Document {
        let (version, declared_data_size, body_offset) = parse_docy_header(editor_payload)?;
        ensure!(
            version == DOCUMENT_EDITOR_PROTOCOL_VERSION,
            "unsupported Euro-Office DOCY protocol version: v{version}"
        );
        let (table_directory_bytes, tables) = inspect_binary_table_directory(
            editor_payload,
            body_offset,
            document_table_name,
            "DOCY",
        )?;
        return Ok(EditorPayloadManifest {
            schema_version: "ctox-office-editor-payload-manifest-v1".to_string(),
            kind,
            protocol: DOCUMENT_EDITOR_PROTOCOL.to_string(),
            protocol_version: version,
            declared_data_size,
            payload_bytes: editor_payload.len() as u64,
            payload_sha256: sha256_hex(editor_payload),
            table_directory_bytes,
            tables,
            shared_strings: Vec::new(),
            defined_names: Vec::new(),
            workbook_protection: None,
            worksheets: Vec::new(),
        });
    }
    let (version, declared_data_size, body_offset) = parse_xlsy_header(editor_payload)?;
    ensure!(
        version == SPREADSHEET_EDITOR_PROTOCOL_VERSION,
        "unsupported Euro-Office XLSY protocol version: v{version}"
    );
    let body = editor_payload
        .get(body_offset..)
        .context("XLSY payload body is missing")?;
    let table_count = usize::from(*body.first().context("XLSY table directory is missing")?);
    let directory_bytes = 1usize
        .checked_add(
            table_count
                .checked_mul(5)
                .context("XLSY table directory overflow")?,
        )
        .context("XLSY table directory overflow")?;
    ensure!(
        directory_bytes <= body.len(),
        "XLSY table directory is truncated"
    );
    let mut directory = Vec::with_capacity(table_count);
    let mut seen_types = BTreeSet::new();
    let mut seen_offsets = BTreeSet::new();
    for index in 0..table_count {
        let position = 1 + index * 5;
        let table_type = body[position];
        let offset = u32::from_le_bytes(
            body[position + 1..position + 5]
                .try_into()
                .expect("validated XLSY directory entry"),
        ) as usize;
        ensure!(
            seen_types.insert(table_type),
            "duplicate XLSY table type: {table_type}"
        );
        ensure!(
            offset >= body_offset + directory_bytes && offset < editor_payload.len(),
            "XLSY table {table_type} offset is outside the payload: {offset}"
        );
        ensure!(
            seen_offsets.insert(offset),
            "duplicate XLSY table offset: {offset}"
        );
        directory.push((table_type, offset));
    }
    let tables: Vec<EditorBinaryTable> = directory
        .iter()
        .map(|(table_type, offset)| {
            let end = directory
                .iter()
                .map(|(_, candidate)| *candidate)
                .filter(|candidate| candidate > offset)
                .min()
                .unwrap_or(editor_payload.len());
            EditorBinaryTable {
                table_type: *table_type,
                name: spreadsheet_table_name(*table_type).to_string(),
                offset: *offset as u64,
                bytes: (end - *offset) as u64,
            }
        })
        .collect();
    let shared_strings = tables
        .iter()
        .find(|table| table.table_type == 1)
        .map(|table| {
            let start = table.offset as usize;
            let end = start + table.bytes as usize;
            decode_spreadsheet_shared_strings(&editor_payload[start..end])
        })
        .transpose()?
        .unwrap_or_default();
    let (defined_names, workbook_protection) = tables
        .iter()
        .find(|table| table.table_type == 3)
        .map(|table| {
            let start = table.offset as usize;
            let end = start + table.bytes as usize;
            decode_spreadsheet_workbook(&editor_payload[start..end])
        })
        .transpose()?
        .unwrap_or_default();
    let worksheets = tables
        .iter()
        .find(|table| table.table_type == 4)
        .map(|table| {
            let start = table.offset as usize;
            let end = start + table.bytes as usize;
            decode_spreadsheet_worksheets(
                &editor_payload[start..end],
                editor_payload,
                &shared_strings,
            )
        })
        .transpose()?
        .unwrap_or_default();
    Ok(EditorPayloadManifest {
        schema_version: "ctox-office-editor-payload-manifest-v1".to_string(),
        kind,
        protocol: SPREADSHEET_EDITOR_PROTOCOL.to_string(),
        protocol_version: version,
        declared_data_size,
        payload_bytes: editor_payload.len() as u64,
        payload_sha256: sha256_hex(editor_payload),
        table_directory_bytes: directory_bytes as u64,
        tables,
        shared_strings,
        defined_names,
        workbook_protection,
        worksheets,
    })
}

fn parse_docy_header(payload: &[u8]) -> anyhow::Result<(u32, u64, usize)> {
    parse_editor_header(payload, b"DOCY", "DOCY")
}

fn parse_editor_header(
    payload: &[u8],
    signature: &[u8; 4],
    label: &str,
) -> anyhow::Result<(u32, u64, usize)> {
    ensure!(payload.starts_with(signature), "invalid {label} signature");
    let first = payload
        .iter()
        .position(|byte| *byte == b';')
        .context(format!("{label} signature terminator is missing"))?;
    let second = payload[first + 1..]
        .iter()
        .position(|byte| *byte == b';')
        .map(|p| p + first + 1)
        .context(format!("{label} version terminator is missing"))?;
    let third = payload[second + 1..]
        .iter()
        .position(|byte| *byte == b';')
        .map(|p| p + second + 1)
        .context(format!("{label} size terminator is missing"))?;
    ensure!(
        &payload[first + 1..first + 2] == b"v",
        "invalid {label} version prefix"
    );
    let version = std::str::from_utf8(&payload[first + 2..second])?.parse::<u32>()?;
    let declared = std::str::from_utf8(&payload[second + 1..third])?.parse::<u64>()?;
    if declared != 0 {
        ensure!(
            declared == (payload.len() - third - 1) as u64,
            "{label} declared data size does not match payload"
        );
    }
    Ok((version, declared, third + 1))
}

fn inspect_binary_table_directory(
    payload: &[u8],
    body_offset: usize,
    table_name: fn(u8) -> &'static str,
    label: &str,
) -> anyhow::Result<(u64, Vec<EditorBinaryTable>)> {
    let body = payload
        .get(body_offset..)
        .context(format!("{label} payload body is missing"))?;
    let count = usize::from(
        *body
            .first()
            .context(format!("{label} table directory is missing"))?,
    );
    let directory_bytes = 1usize
        .checked_add(count.checked_mul(5).context("table directory overflow")?)
        .context("table directory overflow")?;
    ensure!(
        directory_bytes <= body.len(),
        "{label} table directory is truncated"
    );
    let mut directory = Vec::with_capacity(count);
    let mut seen = BTreeSet::new();
    for index in 0..count {
        let position = 1 + index * 5;
        let table_type = body[position];
        let offset = u32::from_le_bytes(
            body[position + 1..position + 5]
                .try_into()
                .expect("validated directory entry"),
        ) as usize;
        ensure!(
            seen.insert(table_type),
            "duplicate {label} table type: {table_type}"
        );
        ensure!(
            offset >= body_offset + directory_bytes && offset < payload.len(),
            "{label} table {table_type} offset is outside the payload: {offset}"
        );
        if let Some((_, previous)) = directory.last() {
            ensure!(
                offset > *previous,
                "{label} table offsets are not strictly increasing"
            );
        }
        directory.push((table_type, offset));
    }
    let tables = directory
        .iter()
        .enumerate()
        .map(|(index, (table_type, offset))| {
            let end = directory
                .get(index + 1)
                .map(|(_, next)| *next)
                .unwrap_or(payload.len());
            EditorBinaryTable {
                table_type: *table_type,
                name: table_name(*table_type).to_string(),
                offset: *offset as u64,
                bytes: (end - *offset) as u64,
            }
        })
        .collect();
    Ok((directory_bytes as u64, tables))
}

fn document_table_name(table_type: u8) -> &'static str {
    // ref: sdkjs/word/Editor/Serialize2.js:57-80
    match table_type {
        0 => "signature",
        1 => "info",
        2 => "media",
        3 => "numbering",
        4 => "header_footer",
        5 => "styles",
        6 => "document",
        7 => "other",
        8 => "comments",
        9 => "settings",
        10 => "footnotes",
        11 => "endnotes",
        12 => "background",
        13 => "vba_project",
        15 => "app",
        16 => "core",
        17 => "document_comments",
        18 => "custom_properties",
        19 => "glossary",
        20 => "customs",
        21 => "oform",
        _ => "unknown",
    }
}

// ref: sdkjs/cell/model/Serialize.js:3765-4012,4963-5026
fn decode_spreadsheet_workbook(
    table: &[u8],
) -> anyhow::Result<(
    Vec<EditorDefinedNameManifest>,
    Option<EditorWorkbookProtectionManifest>,
)> {
    let content = length_prefixed_content(table, "workbook table")?;
    let mut names = Vec::new();
    let mut protection = None;
    for (item_type, item) in length_prefixed_items(content, "workbook table")? {
        match item_type {
            3 => {
                for (name_type, name_item) in length_prefixed_items(item, "defined names")? {
                    if name_type != 4 {
                        continue;
                    }
                    let mut name = EditorDefinedNameManifest {
                        name: String::new(),
                        reference: String::new(),
                        local_sheet_id: None,
                        hidden: false,
                    };
                    let mut position = 0usize;
                    while position < name_item.len() {
                        let property_type = *name_item
                            .get(position)
                            .context("XLSY defined-name property is truncated")?;
                        position += 1;
                        match property_type {
                            0 | 1 => {
                                ensure!(
                                    name_item.len() - position >= 4,
                                    "XLSY defined-name string length is truncated"
                                );
                                let bytes = u32::from_le_bytes(
                                    name_item[position..position + 4].try_into().unwrap(),
                                ) as usize;
                                position += 4;
                                ensure!(
                                    bytes <= name_item.len() - position,
                                    "XLSY defined-name string is truncated"
                                );
                                let value = decode_utf16_le(
                                    &name_item[position..position + bytes],
                                    "defined name",
                                )?;
                                position += bytes;
                                if property_type == 0 {
                                    name.name = value;
                                } else {
                                    name.reference = value;
                                }
                            }
                            2 | 3 => {
                                ensure!(
                                    name_item.len() - position >= 4,
                                    "XLSY defined-name item length is truncated"
                                );
                                let length = u32::from_le_bytes(
                                    name_item[position..position + 4].try_into().unwrap(),
                                ) as usize;
                                position += 4;
                                ensure!(
                                    length <= name_item.len() - position,
                                    "XLSY defined-name item is truncated"
                                );
                                let value = &name_item[position..position + length];
                                position += length;
                                if property_type == 2 && value.len() == 4 {
                                    name.local_sheet_id =
                                        Some(u32::from_le_bytes(value.try_into().unwrap()));
                                } else if property_type == 3 && value.len() == 1 {
                                    name.hidden = value[0] != 0;
                                }
                            }
                            other => {
                                anyhow::bail!("unsupported XLSY defined-name property: {other}")
                            }
                        }
                    }
                    ensure!(
                        !name.name.is_empty() && !name.reference.is_empty(),
                        "XLSY defined name is incomplete"
                    );
                    names.push(name);
                }
            }
            21 => {
                let mut value = EditorWorkbookProtectionManifest::default();
                for (property_type, property) in
                    spreadsheet_binary_properties(item, "workbook protection")?
                {
                    match property_type {
                        4 if property.len() == 1 => value.lock_structure = property[0] != 0,
                        5 if property.len() == 1 => value.lock_windows = property[0] != 0,
                        6 => value.password = Some(decode_utf16_le(property, "workbook password")?),
                        11 if property.len() == 1 => value.lock_revision = property[0] != 0,
                        _ => {}
                    }
                }
                protection = Some(value);
            }
            _ => {}
        }
    }
    Ok((names, protection))
}

fn decode_spreadsheet_sheet_protection(
    value: &[u8],
) -> anyhow::Result<EditorSheetProtectionManifest> {
    let mut result = EditorSheetProtectionManifest::default();
    for (property_type, property) in spreadsheet_binary_properties(value, "sheet protection")? {
        let boolean = || property.first().copied().unwrap_or_default() != 0;
        match property_type {
            4 => result.password = Some(decode_utf16_le(property, "sheet password")?),
            5 => result.auto_filter = boolean(),
            7 => result.delete_columns = boolean(),
            8 => result.delete_rows = boolean(),
            9 => result.format_cells = boolean(),
            10 => result.format_columns = boolean(),
            11 => result.format_rows = boolean(),
            12 => result.insert_columns = boolean(),
            13 => result.insert_hyperlinks = boolean(),
            14 => result.insert_rows = boolean(),
            15 => result.objects = boolean(),
            16 => result.pivot_tables = boolean(),
            17 => result.scenarios = boolean(),
            18 => result.select_locked_cells = boolean(),
            19 => result.select_unlocked_cells = boolean(),
            20 => result.sheet = boolean(),
            21 => result.sort = boolean(),
            _ => {}
        }
    }
    Ok(result)
}

fn decode_spreadsheet_comments(
    value: &[u8],
) -> anyhow::Result<Vec<EditorSpreadsheetCommentManifest>> {
    let mut comments = Vec::new();
    for (item_type, item) in length_prefixed_items(value, "spreadsheet comments")? {
        if item_type != 20 {
            continue;
        }
        let mut row = 0u32;
        let mut column = 0u32;
        let mut text = String::new();
        let mut author = String::new();
        for (property_type, property) in spreadsheet_binary_properties(item, "spreadsheet comment")?
        {
            match property_type {
                0 if property.len() == 4 => row = u32::from_le_bytes(property.try_into().unwrap()),
                1 if property.len() == 4 => {
                    column = u32::from_le_bytes(property.try_into().unwrap())
                }
                2 => {
                    for (data_type, data_item) in
                        length_prefixed_items(property, "spreadsheet comment data")?
                    {
                        if data_type != 3 {
                            continue;
                        }
                        let mut position = 0usize;
                        while position < data_item.len() {
                            let field = data_item[position];
                            position += 1;
                            if matches!(field, 0 | 1 | 2 | 3 | 4 | 9) {
                                ensure!(
                                    data_item.len() - position >= 4,
                                    "comment string length is truncated"
                                );
                                let bytes = u32::from_le_bytes(
                                    data_item[position..position + 4].try_into().unwrap(),
                                ) as usize;
                                position += 4;
                                ensure!(
                                    bytes <= data_item.len() - position,
                                    "comment string is truncated"
                                );
                                let value = decode_utf16_le(
                                    &data_item[position..position + bytes],
                                    "comment string",
                                )?;
                                position += bytes;
                                if field == 0 {
                                    text = value;
                                } else if field == 3 {
                                    author = value;
                                }
                            } else {
                                ensure!(
                                    data_item.len() - position >= 4,
                                    "comment item length is truncated"
                                );
                                let length = u32::from_le_bytes(
                                    data_item[position..position + 4].try_into().unwrap(),
                                ) as usize;
                                position += 4 + length;
                                ensure!(position <= data_item.len(), "comment item is truncated");
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        comments.push(EditorSpreadsheetCommentManifest {
            reference: cell_reference(row, column),
            text,
            author,
            guid: String::new(),
        });
    }
    Ok(comments)
}

// ref: sdkjs/cell/model/Serialize.js:8695-8795
fn decode_spreadsheet_shared_strings(table: &[u8]) -> anyhow::Result<Vec<String>> {
    let content = length_prefixed_content(table, "shared strings table")?;
    let mut strings = Vec::new();
    for (item_type, item) in length_prefixed_items(content, "shared strings table")? {
        if item_type != 0 {
            continue;
        }
        let mut value = String::new();
        for (string_type, string_item) in length_prefixed_items(item, "shared string")? {
            match string_type {
                3 => value.push_str(&decode_utf16_le(string_item, "shared string")?),
                1 => {
                    for (run_type, run_item) in
                        length_prefixed_items(string_item, "shared string run")?
                    {
                        if run_type == 3 {
                            value.push_str(&decode_utf16_le(run_item, "shared string run text")?);
                        }
                    }
                }
                _ => {}
            }
        }
        strings.push(value);
    }
    Ok(strings)
}

// ref: sdkjs/cell/model/Serialize.js:11001-11125,11973-11994
fn decode_spreadsheet_worksheets(
    table: &[u8],
    editor_payload: &[u8],
    shared_strings: &[String],
) -> anyhow::Result<Vec<EditorWorksheetManifest>> {
    let content = length_prefixed_content(table, "worksheets table")?;
    let mut worksheets = Vec::new();
    for (item_type, item) in length_prefixed_items(content, "worksheets table")? {
        if item_type != 0 {
            continue;
        }
        let mut worksheet = EditorWorksheetManifest {
            name: String::new(),
            sheet_id: 0,
            visibility: "visible".to_string(),
            xlsb_offset: 0,
            default_row_height: None,
            columns: Vec::new(),
            rows: Vec::new(),
            merged_cells: Vec::new(),
            frozen_pane: None,
            tables: Vec::new(),
            data_validations: Vec::new(),
            conditional_formats: Vec::new(),
            protection: None,
            comments: Vec::new(),
            cells: Vec::new(),
        };
        for (worksheet_type, worksheet_item) in length_prefixed_items(item, "worksheet record")? {
            if worksheet_type == 1 {
                decode_worksheet_properties(worksheet_item, &mut worksheet)?;
            } else if worksheet_type == 2 {
                worksheet.columns = decode_spreadsheet_columns(worksheet_item)?;
            } else if worksheet_type == 11 {
                worksheet.default_row_height =
                    decode_spreadsheet_default_row_height(worksheet_item)?;
            } else if worksheet_type == 7 {
                worksheet.merged_cells = decode_spreadsheet_merged_cells(worksheet_item)?;
            } else if worksheet_type == 22 {
                worksheet.frozen_pane = decode_spreadsheet_frozen_pane(worksheet_item)?;
            } else if worksheet_type == 18 {
                worksheet.tables = decode_spreadsheet_tables(worksheet_item)?;
            } else if worksheet_type == 32 {
                worksheet.data_validations = decode_spreadsheet_data_validations(worksheet_item)?;
            } else if worksheet_type == 21 {
                worksheet
                    .conditional_formats
                    .push(decode_spreadsheet_conditional_format(worksheet_item)?);
            } else if worksheet_type == 41 {
                worksheet.protection = Some(decode_spreadsheet_sheet_protection(worksheet_item)?);
            } else if worksheet_type == 19 {
                worksheet.comments = decode_spreadsheet_comments(worksheet_item)?;
            } else if worksheet_type == 9 {
                for (sheet_data_type, sheet_data) in
                    length_prefixed_items(worksheet_item, "worksheet sheet data")?
                {
                    if sheet_data_type == 35 {
                        ensure!(sheet_data.len() >= 4, "XLSY XlsbPos is truncated");
                        worksheet.xlsb_offset = u64::from(u32::from_le_bytes(
                            sheet_data[..4].try_into().expect("four-byte XLSB offset"),
                        ));
                    }
                }
            }
        }
        ensure!(!worksheet.name.is_empty(), "XLSY worksheet has no name");
        ensure!(worksheet.sheet_id != 0, "XLSY worksheet has no sheet id");
        ensure!(
            worksheet.xlsb_offset != 0,
            "XLSY worksheet has no XLSB data offset"
        );
        let (cells, rows) = decode_xlsb_sheet_cells(
            editor_payload,
            worksheet.xlsb_offset as usize,
            shared_strings,
        )?;
        worksheet.cells = cells;
        worksheet.rows = rows;
        worksheets.push(worksheet);
    }
    Ok(worksheets)
}

fn decode_spreadsheet_tables(value: &[u8]) -> anyhow::Result<Vec<EditorTableManifest>> {
    let mut tables = Vec::new();
    for (item_type, table) in length_prefixed_items(value, "worksheet table parts")? {
        if item_type != 0 {
            continue;
        }
        let mut result = EditorTableManifest {
            reference: String::new(),
            display_name: String::new(),
            style_name: String::new(),
            show_column_stripes: false,
            show_row_stripes: false,
            show_first_column: false,
            show_last_column: false,
            columns: Vec::new(),
            filters: Vec::new(),
            sort: None,
        };
        for (property_type, payload) in length_prefixed_items(table, "worksheet table")? {
            match property_type {
                1 => result.reference = decode_utf16_le(payload, "table reference")?,
                3 => result.display_name = decode_utf16_le(payload, "table display name")?,
                4 => {
                    let (filters, sort) = decode_spreadsheet_auto_filter(payload)?;
                    result.filters = filters;
                    result.sort = sort;
                }
                5 => result.sort = decode_spreadsheet_sort_state(payload)?,
                6 => {
                    for (column_type, column) in length_prefixed_items(payload, "table columns")? {
                        if column_type != 0 {
                            continue;
                        }
                        for (name_type, name) in length_prefixed_items(column, "table column")? {
                            if name_type == 1 {
                                result
                                    .columns
                                    .push(decode_utf16_le(name, "table column name")?);
                            }
                        }
                    }
                }
                7 => {
                    for (style_type, style) in
                        spreadsheet_binary_properties(payload, "table style")?
                    {
                        match style_type {
                            0 => result.style_name = decode_utf16_le(style, "table style name")?,
                            1 if style.len() == 1 => result.show_column_stripes = style[0] != 0,
                            2 if style.len() == 1 => result.show_row_stripes = style[0] != 0,
                            3 if style.len() == 1 => result.show_first_column = style[0] != 0,
                            4 if style.len() == 1 => result.show_last_column = style[0] != 0,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        tables.push(result);
    }
    Ok(tables)
}

// ref: sdkjs/cell/model/Serialize.js:941-962,1834-1871,11621-11691
fn decode_spreadsheet_data_validations(
    value: &[u8],
) -> anyhow::Result<Vec<EditorDataValidationManifest>> {
    let mut validations = Vec::new();
    for (container_type, container) in length_prefixed_items(value, "data validations")? {
        if container_type != 0 {
            continue;
        }
        for (item_type, item) in length_prefixed_items(container, "data validation list")? {
            if item_type != 1 {
                continue;
            }
            let mut result = EditorDataValidationManifest {
                reference: String::new(),
                validation_type: "none".to_string(),
                operator: None,
                allow_blank: false,
                show_error_message: false,
                error_style: None,
                error_title: None,
                error: None,
                formula1: None,
                formula2: None,
            };
            for (property_type, property) in spreadsheet_binary_properties(item, "data validation")?
            {
                match property_type {
                    6 if property.len() == 1 => result.allow_blank = property[0] != 0,
                    5 if property.len() == 1 => {
                        result.validation_type =
                            spreadsheet_validation_type(property[0])?.to_string()
                    }
                    7 => result.error = Some(decode_utf16_le(property, "validation error")?),
                    8 => {
                        result.error_title =
                            Some(decode_utf16_le(property, "validation error title")?)
                    }
                    9 if property.len() == 1 => {
                        result.error_style =
                            Some(spreadsheet_validation_error_style(property[0])?.to_string())
                    }
                    11 if property.len() == 1 => {
                        result.operator =
                            Some(spreadsheet_validation_operator(property[0])?.to_string())
                    }
                    15 if property.len() == 1 => result.show_error_message = property[0] != 0,
                    17 => result.reference = decode_utf16_le(property, "validation reference")?,
                    18 => result.formula1 = Some(decode_utf16_le(property, "validation formula1")?),
                    19 => result.formula2 = Some(decode_utf16_le(property, "validation formula2")?),
                    _ => {}
                }
            }
            ensure!(
                !result.reference.is_empty(),
                "XLSY data validation has no reference"
            );
            validations.push(result);
        }
    }
    Ok(validations)
}

// ref: sdkjs/cell/model/Serialize.js:877-939,1690-1761,12803-13010
fn decode_spreadsheet_conditional_format(
    value: &[u8],
) -> anyhow::Result<EditorConditionalFormatManifest> {
    let mut result = EditorConditionalFormatManifest {
        reference: String::new(),
        rule_type: String::new(),
        priority: 0,
        operator: None,
        formulas: Vec::new(),
        thresholds: Vec::new(),
        colors: Vec::new(),
        differential_style: None,
    };
    for (item_type, item) in length_prefixed_items(value, "conditional formatting")? {
        match item_type {
            1 => result.reference = decode_utf16_le(item, "conditional reference")?,
            2 => decode_spreadsheet_conditional_rule(item, &mut result)?,
            _ => {}
        }
    }
    ensure!(
        !result.reference.is_empty(),
        "XLSY conditional format has no reference"
    );
    ensure!(
        !result.rule_type.is_empty(),
        "XLSY conditional format has no rule type"
    );
    Ok(result)
}

fn decode_spreadsheet_conditional_rule(
    value: &[u8],
    result: &mut EditorConditionalFormatManifest,
) -> anyhow::Result<()> {
    for (item_type, item) in length_prefixed_items(value, "conditional rule")? {
        match item_type {
            4 if item.len() == 1 => {
                result.operator = Some(spreadsheet_conditional_operator(item[0])?.to_string())
            }
            6 if item.len() == 4 => result.priority = u32::from_le_bytes(item.try_into().unwrap()),
            12 if item.len() == 1 => {
                result.rule_type = spreadsheet_conditional_type(item[0])?.to_string()
            }
            14 => decode_spreadsheet_color_scale(item, result)?,
            16 => result
                .formulas
                .push(decode_utf16_le(item, "conditional formula")?),
            18 => result.differential_style = decode_spreadsheet_differential_style(item)?,
            _ => {}
        }
    }
    Ok(())
}

fn decode_spreadsheet_color_scale(
    value: &[u8],
    result: &mut EditorConditionalFormatManifest,
) -> anyhow::Result<()> {
    for (item_type, item) in length_prefixed_items(value, "conditional color scale")? {
        if item_type == 0 {
            let mut threshold = EditorConditionalThresholdManifest {
                threshold_type: String::new(),
                value: None,
            };
            for (property_type, property) in length_prefixed_items(item, "conditional threshold")? {
                match property_type {
                    1 if property.len() == 1 => {
                        threshold.threshold_type =
                            spreadsheet_conditional_threshold_type(property[0])?.to_string()
                    }
                    2 | 3 => {
                        threshold.value =
                            Some(decode_utf16_le(property, "conditional threshold value")?)
                    }
                    _ => {}
                }
            }
            result.thresholds.push(threshold);
        } else if item_type == 1 {
            if let Some(rgb) = decode_spreadsheet_rgb(item)? {
                result.colors.push(format!("{rgb:08X}"));
            }
        }
    }
    Ok(())
}

fn decode_spreadsheet_differential_style(
    value: &[u8],
) -> anyhow::Result<Option<EditorDifferentialStyleManifest>> {
    let mut style = EditorDifferentialStyleManifest::default();
    for (item_type, item) in length_prefixed_items(value, "differential style")? {
        if item_type == 2 {
            for (fill_type, fill) in length_prefixed_items(item, "differential fill")? {
                if fill_type != 0 {
                    continue;
                }
                for (property_type, property) in
                    length_prefixed_items(fill, "differential pattern")?
                {
                    if matches!(property_type, 3 | 4) {
                        if let Some(rgb) = decode_spreadsheet_rgb(property)? {
                            style.fill_rgb = Some(format!("{rgb:08X}"));
                        }
                    }
                }
            }
        } else if item_type == 3 {
            for (property_type, property) in
                spreadsheet_binary_properties(item, "differential font")?
            {
                if property_type == 1 {
                    if let Some(rgb) = decode_spreadsheet_rgb(property)? {
                        style.font_rgb = Some(format!("{rgb:08X}"));
                    }
                }
            }
        }
    }
    Ok((style.fill_rgb.is_some() || style.font_rgb.is_some()).then_some(style))
}

fn decode_spreadsheet_rgb(value: &[u8]) -> anyhow::Result<Option<u32>> {
    for (property_type, property) in spreadsheet_binary_properties(value, "spreadsheet color")? {
        if property_type == 0 && property.len() == 4 {
            return Ok(Some(u32::from_le_bytes(property.try_into().unwrap())));
        }
    }
    Ok(None)
}

fn spreadsheet_validation_type(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        0 => "none",
        1 => "custom",
        2 => "date",
        3 => "decimal",
        4 => "list",
        5 => "textLength",
        6 => "time",
        7 => "whole",
        _ => anyhow::bail!("unsupported XLSY validation type: {value}"),
    })
}

fn spreadsheet_validation_type_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "none" => 0,
        "custom" => 1,
        "date" => 2,
        "decimal" => 3,
        "list" => 4,
        "textLength" => 5,
        "time" => 6,
        "whole" => 7,
        _ => anyhow::bail!("unsupported OOXML validation type: {value}"),
    })
}

fn spreadsheet_validation_error_style(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        0 => "stop",
        1 => "warning",
        2 => "information",
        _ => anyhow::bail!("unsupported XLSY validation error style: {value}"),
    })
}

fn spreadsheet_validation_error_style_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "stop" => 0,
        "warning" => 1,
        "information" => 2,
        _ => anyhow::bail!("unsupported OOXML validation error style: {value}"),
    })
}

fn spreadsheet_validation_operator(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        0 => "between",
        1 => "notBetween",
        2 => "equal",
        3 => "notEqual",
        4 => "lessThan",
        5 => "lessThanOrEqual",
        6 => "greaterThan",
        7 => "greaterThanOrEqual",
        _ => anyhow::bail!("unsupported XLSY validation operator: {value}"),
    })
}

fn spreadsheet_validation_operator_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "between" => 0,
        "notBetween" => 1,
        "equal" => 2,
        "notEqual" => 3,
        "lessThan" => 4,
        "lessThanOrEqual" => 5,
        "greaterThan" => 6,
        "greaterThanOrEqual" => 7,
        _ => anyhow::bail!("unsupported OOXML validation operator: {value}"),
    })
}

fn spreadsheet_conditional_type(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        2 => "cellIs",
        3 => "colorScale",
        _ => anyhow::bail!("unsupported XLSY conditional format type: {value}"),
    })
}

fn spreadsheet_conditional_type_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "cellIs" => 2,
        "colorScale" => 3,
        _ => anyhow::bail!("unsupported OOXML conditional format type: {value}"),
    })
}

fn spreadsheet_conditional_operator(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        1 => "between",
        4 => "equal",
        5 => "greaterThan",
        6 => "greaterThanOrEqual",
        7 => "lessThan",
        8 => "lessThanOrEqual",
        9 => "notBetween",
        11 => "notEqual",
        _ => anyhow::bail!("unsupported XLSY conditional operator: {value}"),
    })
}

fn spreadsheet_conditional_operator_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "between" => 1,
        "equal" => 4,
        "greaterThan" => 5,
        "greaterThanOrEqual" => 6,
        "lessThan" => 7,
        "lessThanOrEqual" => 8,
        "notBetween" => 9,
        "notEqual" => 11,
        _ => anyhow::bail!("unsupported OOXML conditional operator: {value}"),
    })
}

fn spreadsheet_conditional_threshold_type(value: u8) -> anyhow::Result<&'static str> {
    Ok(match value {
        0 => "formula",
        1 => "max",
        2 => "min",
        3 => "num",
        4 => "percent",
        5 => "percentile",
        6 => "autoMin",
        7 => "autoMax",
        _ => anyhow::bail!("unsupported XLSY conditional threshold type: {value}"),
    })
}

fn spreadsheet_conditional_threshold_type_code(value: &str) -> anyhow::Result<u8> {
    Ok(match value {
        "formula" => 0,
        "max" => 1,
        "min" => 2,
        "num" => 3,
        "percent" => 4,
        "percentile" => 5,
        "autoMin" => 6,
        "autoMax" => 7,
        _ => anyhow::bail!("unsupported OOXML conditional threshold type: {value}"),
    })
}

fn decode_spreadsheet_auto_filter(
    value: &[u8],
) -> anyhow::Result<(Vec<EditorFilterColumnManifest>, Option<EditorSortManifest>)> {
    let mut filters = Vec::new();
    let mut sort = None;
    for (item_type, payload) in length_prefixed_items(value, "table auto filter")? {
        match item_type {
            1 => {
                for (column_type, column) in length_prefixed_items(payload, "filter columns")? {
                    if column_type != 2 {
                        continue;
                    }
                    let mut result = EditorFilterColumnManifest {
                        column_id: 0,
                        values: Vec::new(),
                    };
                    for (property_type, property) in length_prefixed_items(column, "filter column")?
                    {
                        if property_type == 0 && property.len() == 4 {
                            result.column_id = u32::from_le_bytes(property.try_into().unwrap());
                        } else if property_type == 1 {
                            for (filter_type, filter) in
                                length_prefixed_items(property, "filter values")?
                            {
                                if filter_type != 2 {
                                    continue;
                                }
                                for (value_type, value) in
                                    length_prefixed_items(filter, "filter value")?
                                {
                                    if value_type == 0 {
                                        result.values.push(decode_utf16_le(value, "filter value")?);
                                    }
                                }
                            }
                        }
                    }
                    filters.push(result);
                }
            }
            3 => sort = decode_spreadsheet_sort_state(payload)?,
            _ => {}
        }
    }
    Ok((filters, sort))
}

fn decode_spreadsheet_sort_state(value: &[u8]) -> anyhow::Result<Option<EditorSortManifest>> {
    let mut reference = String::new();
    let mut condition_reference = String::new();
    let mut descending = false;
    for (item_type, payload) in length_prefixed_items(value, "sort state")? {
        if item_type == 0 {
            reference = decode_utf16_le(payload, "sort reference")?;
        } else if item_type == 2 {
            for (condition_type, condition) in length_prefixed_items(payload, "sort conditions")? {
                if condition_type != 3 {
                    continue;
                }
                for (property_type, property) in
                    spreadsheet_binary_properties(condition, "sort condition")?
                {
                    match property_type {
                        4 => {
                            condition_reference =
                                decode_utf16_le(property, "sort condition reference")?
                        }
                        6 if property.len() == 1 => descending = property[0] != 0,
                        _ => {}
                    }
                }
            }
        }
    }
    if reference.is_empty() && condition_reference.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EditorSortManifest {
            reference,
            condition_reference,
            descending,
        }))
    }
}

// ref: sdkjs/cell/model/Serialize.js:6345-6351,12214-12226
fn decode_spreadsheet_merged_cells(value: &[u8]) -> anyhow::Result<Vec<String>> {
    length_prefixed_items(value, "worksheet merge cells")?
        .into_iter()
        .filter(|(item_type, _)| *item_type == 8)
        .map(|(_, item)| decode_utf16_le(item, "merged cell reference"))
        .collect()
}

// ref: sdkjs/cell/model/Serialize.js:5898-5952,13040-13113
fn decode_spreadsheet_frozen_pane(
    value: &[u8],
) -> anyhow::Result<Option<EditorFrozenPaneManifest>> {
    for (item_type, sheet_view) in length_prefixed_items(value, "worksheet sheet views")? {
        if item_type != 23 {
            continue;
        }
        for (property_type, pane) in length_prefixed_items(sheet_view, "worksheet sheet view")? {
            if property_type != 19 {
                continue;
            }
            let mut result = EditorFrozenPaneManifest {
                active_pane: String::new(),
                state: String::new(),
                top_left_cell: String::new(),
                x_split: 0.0,
                y_split: 0.0,
            };
            for (pane_type, payload) in length_prefixed_items(pane, "worksheet pane")? {
                match pane_type {
                    0 if payload.len() == 1 => {
                        result.active_pane = match payload[0] {
                            0 => "bottomLeft",
                            1 => "bottomRight",
                            2 => "topLeft",
                            3 => "topRight",
                            value => anyhow::bail!("unsupported XLSY active pane: {value}"),
                        }
                        .to_string();
                    }
                    1 => result.state = decode_utf16_le(payload, "pane state")?,
                    2 => result.top_left_cell = decode_utf16_le(payload, "pane top-left cell")?,
                    3 if payload.len() == 8 => {
                        result.x_split = f64::from_le_bytes(payload.try_into().unwrap())
                    }
                    4 if payload.len() == 8 => {
                        result.y_split = f64::from_le_bytes(payload.try_into().unwrap())
                    }
                    _ => {}
                }
            }
            return Ok(Some(result));
        }
    }
    Ok(None)
}

// ref: sdkjs/common/SerializeCommonWordExcel.js:916-940
// ref: sdkjs/cell/model/Workbook.js:12172-12204,17850-17935
fn decode_xlsb_sheet_cells(
    payload: &[u8],
    start: usize,
    shared_strings: &[String],
) -> anyhow::Result<(Vec<EditorCellManifest>, Vec<EditorRowManifest>)> {
    ensure!(
        start < payload.len(),
        "XLSB sheet-data offset is outside the payload"
    );
    let mut position = start;
    let mut row = None;
    let mut cells = Vec::new();
    let mut rows = Vec::new();
    while position < payload.len() {
        let record_type = read_xlsb_varint(payload, &mut position, 2, "record type")?;
        let length = read_xlsb_varint(payload, &mut position, 4, "record length")? as usize;
        let end = position
            .checked_add(length)
            .context("XLSB record length overflow")?;
        ensure!(
            end <= payload.len(),
            "XLSB record exceeds the payload boundary"
        );
        let record = &payload[position..end];
        position = end;
        match record_type {
            145 => {}
            146 => break,
            0 => {
                ensure!(record.len() >= 12, "XLSB row record is truncated");
                let index =
                    u32::from_le_bytes(record[..4].try_into().expect("row index")) & 0x000f_ffff;
                row = Some(index);
                let height_twips =
                    u16::from_le_bytes(record[8..10].try_into().expect("row height"));
                let flags = record[11];
                rows.push(EditorRowManifest {
                    index,
                    height: if height_twips == 0 {
                        None
                    } else {
                        Some(f64::from(height_twips) / 20.0)
                    },
                    custom_height: flags & 0x20 != 0,
                    hidden: flags & 0x10 != 0,
                });
            }
            1..=11 => {
                let row = row.context("XLSB cell appeared before its row header")?;
                ensure!(record.len() >= 10, "XLSB cell record is truncated");
                let column =
                    u32::from_le_bytes(record[..4].try_into().expect("column index")) & 0x3fff;
                let flags = u32::from_le_bytes(record[4..8].try_into().expect("cell flags"));
                let style_id = flags & 0x000f_ffff;
                let body = &record[8..];
                let (value_type, display, formula) =
                    decode_xlsb_cell_value(record_type, body, shared_strings)?;
                cells.push(EditorCellManifest {
                    reference: cell_reference(row, column),
                    value_type: value_type.to_string(),
                    display,
                    style_id,
                    formula,
                });
            }
            _ => {}
        }
    }
    Ok((cells, rows))
}

fn read_xlsb_varint(
    payload: &[u8],
    position: &mut usize,
    max_bytes: usize,
    label: &str,
) -> anyhow::Result<u32> {
    let mut value = 0u32;
    for index in 0..max_bytes {
        let byte = *payload
            .get(*position)
            .with_context(|| format!("XLSB {label} is truncated"))?;
        *position += 1;
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("XLSB {label} exceeds {max_bytes} bytes")
}

fn decode_xlsb_cell_value(
    record_type: u32,
    value: &[u8],
    shared_strings: &[String],
) -> anyhow::Result<(&'static str, String, Option<String>)> {
    let formula_cache_bytes = match record_type {
        8 => {
            ensure!(value.len() >= 4, "XLSB formula string cache is truncated");
            4usize
                .checked_add(
                    (u32::from_le_bytes(value[..4].try_into().unwrap()) as usize)
                        .checked_mul(2)
                        .context("XLSB formula string cache length overflow")?,
                )
                .context("XLSB formula string cache length overflow")?
        }
        9 => 8,
        10 | 11 => 1,
        _ => 0,
    };
    let formula = if (8..=11).contains(&record_type) {
        let formula_start = formula_cache_bytes
            .checked_add(12)
            .context("XLSB formula boundary overflow")?;
        ensure!(
            value.len() >= formula_start + 4,
            "XLSB formula is truncated"
        );
        let bytes =
            (u32::from_le_bytes(value[formula_start..formula_start + 4].try_into().unwrap())
                as usize)
                .checked_mul(2)
                .context("XLSB formula text length overflow")?;
        let text_start = formula_start + 4;
        let text_end = text_start
            .checked_add(bytes)
            .context("XLSB formula text boundary overflow")?;
        ensure!(text_end <= value.len(), "XLSB formula text is truncated");
        Some(format!(
            "={}",
            decode_utf16_le(&value[text_start..text_end], "cell formula")?
        ))
    } else {
        None
    };
    let value = if formula_cache_bytes > 0 {
        &value[..formula_cache_bytes]
    } else {
        ensure!(value.len() >= 2, "XLSB cell flags are truncated");
        &value[..value.len() - 2]
    };
    let (value_type, display) = match record_type {
        1 => ("blank", String::new()),
        4 | 10 => {
            let value = *value.first().context("XLSB boolean cell is truncated")?;
            (
                "boolean",
                if value == 0 { "FALSE" } else { "TRUE" }.to_string(),
            )
        }
        5 | 9 => {
            ensure!(value.len() >= 8, "XLSB numeric cell is truncated");
            let number = f64::from_le_bytes(value[..8].try_into().expect("numeric cell"));
            ("number", format_spreadsheet_number(number))
        }
        7 => {
            ensure!(value.len() >= 4, "XLSB shared-string cell is truncated");
            let index =
                u32::from_le_bytes(value[..4].try_into().expect("shared string index")) as usize;
            let text = shared_strings
                .get(index)
                .with_context(|| format!("XLSB shared-string index is out of range: {index}"))?;
            ("shared_string", text.clone())
        }
        3 | 11 => (
            "error",
            spreadsheet_error_text(value.first().copied().unwrap_or_default()).to_string(),
        ),
        6 | 8 => {
            ensure!(value.len() >= 4, "XLSB string cell is truncated");
            let bytes = (u32::from_le_bytes(value[..4].try_into().expect("string length"))
                as usize)
                .checked_mul(2)
                .context("XLSB string length overflow")?;
            ensure!(
                value.len() >= 4 + bytes,
                "XLSB string cell value is truncated"
            );
            (
                "string",
                decode_utf16_le(&value[4..4 + bytes], "cell string")?,
            )
        }
        2 => anyhow::bail!("XLSB RK numeric cells are not implemented yet"),
        other => anyhow::bail!("unsupported XLSB cell record type: {other}"),
    };
    Ok((value_type, display, formula))
}

fn format_spreadsheet_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn cell_reference(row: u32, column: u32) -> String {
    let mut value = column + 1;
    let mut letters = Vec::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        letters.push((b'A' + remainder as u8) as char);
        value = (value - 1) / 26;
    }
    letters.reverse();
    format!(
        "{}{row_number}",
        letters.into_iter().collect::<String>(),
        row_number = row + 1
    )
}

fn length_prefixed_content<'a>(value: &'a [u8], label: &str) -> anyhow::Result<&'a [u8]> {
    ensure!(value.len() >= 4, "XLSY {label} length is truncated");
    let length = u32::from_le_bytes(value[..4].try_into().expect("four-byte length")) as usize;
    ensure!(
        length <= value.len() - 4,
        "XLSY {label} exceeds its table boundary"
    );
    Ok(&value[4..4 + length])
}

fn length_prefixed_items<'a>(value: &'a [u8], label: &str) -> anyhow::Result<Vec<(u8, &'a [u8])>> {
    let mut position = 0usize;
    let mut items = Vec::new();
    while position < value.len() {
        ensure!(
            value.len() - position >= 5,
            "XLSY {label} item header is truncated"
        );
        let item_type = value[position];
        let length = u32::from_le_bytes(
            value[position + 1..position + 5]
                .try_into()
                .expect("four-byte item length"),
        ) as usize;
        position += 5;
        let end = position
            .checked_add(length)
            .context("XLSY item length overflow")?;
        ensure!(end <= value.len(), "XLSY {label} item exceeds its boundary");
        items.push((item_type, &value[position..end]));
        position = end;
    }
    Ok(items)
}

fn decode_worksheet_properties(
    value: &[u8],
    worksheet: &mut EditorWorksheetManifest,
) -> anyhow::Result<()> {
    let mut position = 0usize;
    while position < value.len() {
        ensure!(
            value.len() - position >= 2,
            "XLSY worksheet property is truncated"
        );
        let property_type = value[position];
        let length_type = value[position + 1];
        position += 2;
        let length = match length_type {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 | 7 | 8 => 8,
            6 => {
                ensure!(
                    value.len() - position >= 4,
                    "XLSY variable property length is truncated"
                );
                let length = u32::from_le_bytes(
                    value[position..position + 4]
                        .try_into()
                        .expect("four-byte property length"),
                ) as usize;
                position += 4;
                length
            }
            other => anyhow::bail!("unsupported XLSY property length type: {other}"),
        };
        let end = position
            .checked_add(length)
            .context("XLSY property length overflow")?;
        ensure!(
            end <= value.len(),
            "XLSY worksheet property exceeds its boundary"
        );
        let payload = &value[position..end];
        match property_type {
            0 => worksheet.name = decode_utf16_le(payload, "worksheet name")?,
            1 if payload.len() == 4 => {
                worksheet.sheet_id = u32::from_le_bytes(payload.try_into().expect("sheet id"));
            }
            2 if payload.len() == 1 => {
                worksheet.visibility = match payload[0] {
                    0 => "hidden",
                    1 => "very_hidden",
                    2 => "visible",
                    value => anyhow::bail!("unsupported XLSY worksheet visibility: {value}"),
                }
                .to_string();
            }
            _ => {}
        }
        position = end;
    }
    Ok(())
}

fn spreadsheet_binary_properties<'a>(
    value: &'a [u8],
    label: &str,
) -> anyhow::Result<Vec<(u8, &'a [u8])>> {
    let mut position = 0usize;
    let mut properties = Vec::new();
    while position < value.len() {
        ensure!(
            value.len() - position >= 2,
            "XLSY {label} property is truncated"
        );
        let property_type = value[position];
        let length_type = value[position + 1];
        position += 2;
        let length = match length_type {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 | 7 | 8 => 8,
            6 => {
                ensure!(
                    value.len() - position >= 4,
                    "XLSY {label} variable length is truncated"
                );
                let length = u32::from_le_bytes(
                    value[position..position + 4]
                        .try_into()
                        .expect("property length"),
                ) as usize;
                position += 4;
                length
            }
            other => anyhow::bail!("unsupported XLSY {label} property length type: {other}"),
        };
        let end = position
            .checked_add(length)
            .context("XLSY property length overflow")?;
        ensure!(
            end <= value.len(),
            "XLSY {label} property exceeds its boundary"
        );
        properties.push((property_type, &value[position..end]));
        position = end;
    }
    Ok(properties)
}

fn decode_spreadsheet_columns(value: &[u8]) -> anyhow::Result<Vec<EditorColumnManifest>> {
    let mut columns = Vec::new();
    for (item_type, item) in length_prefixed_items(value, "worksheet columns")? {
        if item_type != 3 {
            continue;
        }
        let mut column = EditorColumnManifest {
            min: 0,
            max: 0,
            width: 0.0,
            custom_width: false,
        };
        for (property_type, payload) in spreadsheet_binary_properties(item, "worksheet column")? {
            match property_type {
                2 if payload.len() == 4 => {
                    column.max = u32::from_le_bytes(payload.try_into().expect("column max"))
                }
                3 if payload.len() == 4 => {
                    column.min = u32::from_le_bytes(payload.try_into().expect("column min"))
                }
                5 if payload.len() == 8 => {
                    column.width = f64::from_le_bytes(payload.try_into().expect("column width"))
                }
                6 if payload.len() == 1 => column.custom_width = payload[0] != 0,
                _ => {}
            }
        }
        columns.push(column);
    }
    Ok(columns)
}

fn decode_spreadsheet_default_row_height(value: &[u8]) -> anyhow::Result<Option<f64>> {
    for (property_type, payload) in spreadsheet_binary_properties(value, "worksheet format")? {
        if property_type == 1 && payload.len() == 8 {
            return Ok(Some(f64::from_le_bytes(
                payload.try_into().expect("default row height"),
            )));
        }
    }
    Ok(None)
}

fn decode_utf16_le(value: &[u8], label: &str) -> anyhow::Result<String> {
    ensure!(value.len() % 2 == 0, "XLSY {label} is not valid UTF-16LE");
    let words = value
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    String::from_utf16(&words.collect::<Vec<_>>()).with_context(|| format!("decode XLSY {label}"))
}

fn parse_xlsy_header(payload: &[u8]) -> anyhow::Result<(u32, u64, usize)> {
    ensure!(payload.starts_with(b"XLSY;v"), "invalid XLSY signature");
    let mut separators = payload
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b';').then_some(index));
    let signature_end = separators
        .next()
        .context("XLSY signature terminator is missing")?;
    let version_end = separators
        .next()
        .context("XLSY version terminator is missing")?;
    let size_end = separators
        .next()
        .context("XLSY size terminator is missing")?;
    ensure!(signature_end == 4, "invalid XLSY signature header");
    let version = std::str::from_utf8(&payload[signature_end + 2..version_end])
        .context("XLSY version is not ASCII")?
        .parse::<u32>()
        .context("XLSY version is not numeric")?;
    let declared_data_size = std::str::from_utf8(&payload[version_end + 1..size_end])
        .context("XLSY data size is not ASCII")?
        .parse::<u64>()
        .context("XLSY data size is not numeric")?;
    if declared_data_size != 0 {
        ensure!(
            declared_data_size == (payload.len() - size_end - 1) as u64,
            "XLSY declared data size does not match payload"
        );
    }
    Ok((version, declared_data_size, size_end + 1))
}

fn spreadsheet_table_name(table_type: u8) -> &'static str {
    // ref: sdkjs/cell/model/Serialize.js:144-157
    match table_type {
        0 => "other",
        1 => "shared_strings",
        2 => "styles",
        3 => "workbook",
        4 => "worksheets",
        5 => "calc_chain",
        6 => "app",
        7 => "core",
        8 => "person_list",
        9 => "custom_properties",
        10 => "customs",
        _ => "unknown",
    }
}

pub fn prepare(
    kind: OfficeKind,
    source_bytes: &[u8],
    options: PrepareOptions,
) -> anyhow::Result<PreparedEditorPayload> {
    if kind == OfficeKind::Spreadsheet {
        let editor_payload = transcode_spreadsheet_to_editor_payload(source_bytes)?;
        return prepare_with_editor_payload(kind, source_bytes, &editor_payload, options);
    }
    if kind == OfficeKind::Document {
        let editor_payload = transcode_document_to_editor_payload(source_bytes)?;
        return prepare_with_editor_payload(kind, source_bytes, &editor_payload, options);
    }
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
        editor_manifest: None,
        implemented_features: options.implemented_features,
        diagnostics: vec![OfficeDiagnostic {
            level: "info".to_string(),
            code: "office.document.browser-ooxml-payload".to_string(),
            message: "The Rust prepare path validated the complete DOCX package for Euro-Office's native browser OOXML importer.".to_string(),
        }],
    })
}

/// First native Word writer slice. It follows the same length-prefixed record
/// layout as `BinaryDocumentTableWriter` and deliberately emits only records
/// understood by this slice; later feature ports extend these records instead
/// of replacing the protocol boundary.
// refs: sdkjs/word/Editor/Serialize2.js:1920-2179,5239-5305,5420-5444,5729-5740,5998-6069
pub fn transcode_document_to_editor_payload(source_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    inspect(OfficeKind::Document, source_bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(source_bytes)).context("open document OOXML")?;
    let xml = read_zip_part(&mut archive, "word/document.xml")?;
    let document_relationships =
        read_optional_zip_part(&mut archive, "word/_rels/document.xml.rels")?
            .as_deref()
            .map(parse_document_relationships)
            .transpose()?
            .unwrap_or_default();
    let styles = read_optional_zip_part(&mut archive, "word/styles.xml")?;
    let numbering = read_optional_zip_part(&mut archive, "word/numbering.xml")?;
    let theme_fonts = read_optional_zip_part(&mut archive, "word/theme/theme1.xml")?
        .as_deref()
        .map(parse_document_theme_fonts)
        .transpose()?
        .unwrap_or_default();
    let style_context = styles
        .as_deref()
        .map(|styles| parse_document_style_context(styles, theme_fonts.clone()))
        .transpose()?
        .unwrap_or_else(|| DocumentStyleContext {
            theme_fonts,
            ..DocumentStyleContext::default()
        });
    let header_footers =
        parse_document_header_footer_parts(&mut archive, &style_context, &document_relationships)?;
    let comments = read_optional_zip_part(&mut archive, "word/comments.xml")?
        .as_deref()
        .map(parse_document_comments)
        .transpose()?
        .unwrap_or_default();
    let mut chart_parts = BTreeMap::new();
    for target in document_relationships
        .values()
        .filter(|target| target.starts_with("charts/") || target.starts_with("word/charts/"))
    {
        let path = normalize_document_part_target(target);
        if let Some(bytes) = read_optional_zip_part(&mut archive, &path)? {
            chart_parts.insert(path, parse_document_drawing_chart(&bytes)?);
        }
    }
    let mut source = parse_document_source(
        &xml,
        &style_context,
        &document_relationships,
        &header_footers,
    )?;
    attach_document_drawing_charts(&mut source.blocks, &chart_parts);
    let numbering = numbering
        .as_deref()
        .map(parse_document_numbering)
        .transpose()?
        .unwrap_or_default();

    let signature = vec![0, 4, 10, 0, 0, 0];
    let settings = 0u32.to_le_bytes().to_vec();
    let numbering = write_document_numbering_table(&numbering);
    let styles = write_document_styles_table(&style_context);
    let header_footers = write_document_header_footer_table(&source);
    let document = write_document_table(&source);
    let comments = write_document_comments_table(&comments);
    let mut tables = vec![(0u8, signature), (9u8, settings), (3u8, numbering)];
    if let Some(comments) = comments {
        tables.push((8u8, comments));
    }
    if let Some(header_footers) = header_footers {
        tables.push((4u8, header_footers));
    }
    tables.push((5u8, styles));
    tables.push((6u8, document));
    let header = b"DOCY;v10;0;";
    let directory_bytes = 1 + tables.len() * 5;
    let mut offset = header.len() + directory_bytes;
    let mut payload = Vec::new();
    payload.extend_from_slice(header);
    payload.push(tables.len() as u8);
    for (table_type, bytes) in &tables {
        payload.push(*table_type);
        payload.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += bytes.len();
    }
    for (_, bytes) in tables {
        payload.extend_from_slice(&bytes);
    }
    inspect_editor_payload(OfficeKind::Document, &payload)?;
    Ok(payload)
}

#[derive(Debug)]
struct DocumentSource {
    blocks: Vec<DocumentBlock>,
    section: Option<DocumentSection>,
    header_footers: DocumentHeaderFooterParts,
}

#[derive(Debug, Clone)]
struct DocumentComment {
    id: u32,
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
    text: String,
    solved: bool,
    replies: Vec<DocumentComment>,
}

#[derive(Debug, Clone, Default)]
struct DocumentStyleContext {
    default_after_twips: Option<u32>,
    default_line_twips: Option<u32>,
    default_line_rule: Option<u8>,
    default_run: DocumentRunProperties,
    paragraph: BTreeMap<String, DocumentParagraphStyle>,
    definitions: Vec<DocumentStyleDefinition>,
    theme_fonts: DocumentThemeFonts,
}

#[derive(Debug, Clone, Default)]
struct DocumentThemeFonts {
    major_latin: Option<String>,
    minor_latin: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct DocumentParagraphStyle {
    contextual_spacing: bool,
    alignment: Option<u8>,
    before_twips: Option<u32>,
    after_twips: Option<u32>,
    line_twips: Option<u32>,
    line_rule: Option<u8>,
    left_indent_twips: Option<u32>,
    right_indent_twips: Option<u32>,
    first_line_indent_twips: Option<u32>,
    bottom_border: Option<DocumentBorder>,
    num_id: Option<u32>,
    num_level: Option<u32>,
}

#[derive(Debug, Clone)]
struct DocumentStyleDefinition {
    id: String,
    name: String,
    style_type: u8,
    is_default: bool,
    based_on: Option<String>,
    next: Option<String>,
    link: Option<String>,
    q_format: Option<bool>,
    ui_priority: Option<u32>,
    hidden: Option<bool>,
    semi_hidden: Option<bool>,
    unhide_when_used: Option<bool>,
    custom_style: bool,
    paragraph: Option<DocumentParagraphStyle>,
    run: Option<DocumentRunProperties>,
}

#[derive(Debug, Clone)]
struct DocumentBorder {
    color: [u8; 3],
    space_points: u32,
    size_eighth_points: u32,
}

#[derive(Debug, Clone)]
struct DocumentSection {
    width_twips: u32,
    height_twips: u32,
    orientation: u8,
    margins_twips: [u32; 7],
    title_page: bool,
    break_type: Option<u8>,
    header_default: Option<usize>,
    header_first: Option<usize>,
    header_even: Option<usize>,
    footer_default: Option<usize>,
    footer_first: Option<usize>,
    footer_even: Option<usize>,
}

#[derive(Debug, Default)]
struct DocumentHeaderFooterParts {
    headers: Vec<DocumentHeaderFooterPart>,
    footers: Vec<DocumentHeaderFooterPart>,
}

#[derive(Debug)]
struct DocumentHeaderFooterPart {
    path: String,
    blocks: Vec<DocumentBlock>,
}

fn parse_document_style_context(
    xml: &[u8],
    theme_fonts: DocumentThemeFonts,
) -> anyhow::Result<DocumentStyleContext> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document styles XML is not UTF-8")?,
    )?;
    let mut context = DocumentStyleContext::default();
    context.theme_fonts = theme_fonts;
    let latent_styles = parse_document_latent_styles(&tree);
    context.default_run = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rPrDefault")
        .and_then(|defaults| {
            defaults
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "rPr")
        })
        .map(|properties| parse_document_run_property_node(properties, &context.theme_fonts))
        .unwrap_or_default();
    let default_spacing = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "pPrDefault")
        .and_then(|defaults| {
            defaults
                .descendants()
                .find(|node| node.is_element() && node.tag_name().name() == "spacing")
        });
    context.default_after_twips = parse_u32_attribute(default_spacing, "after");
    context.default_line_twips = parse_u32_attribute(default_spacing, "line");
    context.default_line_rule = default_spacing
        .and_then(|spacing| word_attribute(spacing, "lineRule"))
        .and_then(parse_document_line_rule);
    for style in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "style")
    {
        let Some(style_id) = word_attribute(style, "styleId") else {
            continue;
        };
        let style_type = match word_attribute(style, "type") {
            Some("character") => 1,
            Some("numbering") => 2,
            Some("paragraph") => 3,
            Some("table") => 4,
            _ => continue,
        };
        let raw_style_name = child_attribute(style, "name", "val").unwrap_or(style_id);
        let latent = latent_styles.for_style(raw_style_name);
        let run = style
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "rPr")
            .map(|properties| parse_document_run_property_node(properties, &context.theme_fonts));
        let properties = style
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "pPr");
        let paragraph_style = properties.map(parse_document_paragraph_style_node);
        let after_twips = paragraph_style
            .as_ref()
            .and_then(|paragraph| paragraph.after_twips);
        if style_type == 3 {
            context.paragraph.insert(
                style_id.to_string(),
                paragraph_style.clone().unwrap_or_default(),
            );
        }
        context.definitions.push(DocumentStyleDefinition {
            id: style_id.to_string(),
            name: normalize_document_style_name(style_id, raw_style_name),
            style_type,
            is_default: matches!(word_attribute(style, "default"), Some("1" | "true")),
            based_on: child_attribute(style, "basedOn", "val").map(str::to_string),
            next: child_attribute(style, "next", "val").map(str::to_string),
            link: child_attribute(style, "link", "val").map(str::to_string),
            q_format: style
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "qFormat")
                .map(|node| word_attribute(node, "val") != Some("0"))
                .or(latent.q_format)
                .or(latent_styles.default_q_format),
            ui_priority: child_attribute(style, "uiPriority", "val")
                .and_then(|value| value.parse().ok())
                .or(latent.ui_priority)
                .or(latent_styles.default_ui_priority),
            hidden: style_boolean_child(style, "hidden"),
            semi_hidden: style_boolean_child(style, "semiHidden")
                .or(latent.semi_hidden)
                .or(latent_styles.default_semi_hidden),
            unhide_when_used: style_boolean_child(style, "unhideWhenUsed")
                .or(latent.unhide_when_used)
                .or(latent_styles.default_unhide_when_used),
            custom_style: matches!(word_attribute(style, "customStyle"), Some("1" | "true")),
            paragraph: paragraph_style.clone(),
            run: run.clone(),
        });
        if matches!(word_attribute(style, "default"), Some("1" | "true")) {
            context.default_after_twips = after_twips.or(context.default_after_twips);
            context.default_line_twips = paragraph_style
                .as_ref()
                .and_then(|paragraph| paragraph.line_twips)
                .or(context.default_line_twips);
            context.default_line_rule = paragraph_style
                .as_ref()
                .and_then(|paragraph| paragraph.line_rule)
                .or(context.default_line_rule);
            if let Some(run_properties) = run {
                context.default_run =
                    merge_document_run_properties(&context.default_run, run_properties);
            }
        }
    }
    Ok(context)
}

#[derive(Debug, Clone, Default)]
struct DocumentLatentStyleContext {
    default_ui_priority: Option<u32>,
    default_semi_hidden: Option<bool>,
    default_unhide_when_used: Option<bool>,
    default_q_format: Option<bool>,
    exceptions: BTreeMap<String, DocumentLatentStyleException>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DocumentLatentStyleException {
    ui_priority: Option<u32>,
    semi_hidden: Option<bool>,
    unhide_when_used: Option<bool>,
    q_format: Option<bool>,
}

impl DocumentLatentStyleContext {
    fn for_style(&self, name: &str) -> DocumentLatentStyleException {
        self.exceptions
            .get(&latent_style_key(name))
            .copied()
            .unwrap_or_default()
    }
}

fn parse_document_latent_styles(tree: &roxmltree::Document<'_>) -> DocumentLatentStyleContext {
    let Some(latent_styles) = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "latentStyles")
    else {
        return DocumentLatentStyleContext::default();
    };
    let mut context = DocumentLatentStyleContext {
        default_ui_priority: word_attribute(latent_styles, "defUIPriority")
            .and_then(|value| value.parse().ok()),
        default_semi_hidden: word_attribute(latent_styles, "defSemiHidden").map(word_bool),
        default_unhide_when_used: word_attribute(latent_styles, "defUnhideWhenUsed").map(word_bool),
        default_q_format: word_attribute(latent_styles, "defQFormat").map(word_bool),
        exceptions: BTreeMap::new(),
    };
    for exception in latent_styles
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "lsdException")
    {
        let Some(name) = word_attribute(exception, "name") else {
            continue;
        };
        context.exceptions.insert(
            latent_style_key(name),
            DocumentLatentStyleException {
                ui_priority: word_attribute(exception, "uiPriority")
                    .and_then(|value| value.parse().ok()),
                semi_hidden: word_attribute(exception, "semiHidden").map(word_bool),
                unhide_when_used: word_attribute(exception, "unhideWhenUsed").map(word_bool),
                q_format: word_attribute(exception, "qFormat").map(word_bool),
            },
        );
    }
    context
}

fn latent_style_key(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn word_bool(value: &str) -> bool {
    !matches!(value, "0" | "false" | "False" | "FALSE")
}

fn parse_document_theme_fonts(xml: &[u8]) -> anyhow::Result<DocumentThemeFonts> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document theme XML is not UTF-8")?,
    )?;
    let latin_typeface = |container_name: &str| {
        tree.descendants()
            .find(|node| node.is_element() && node.tag_name().name() == container_name)
            .and_then(|container| {
                container
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "latin")
            })
            .and_then(|latin| latin.attribute("typeface"))
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Ok(DocumentThemeFonts {
        major_latin: latin_typeface("majorFont"),
        minor_latin: latin_typeface("minorFont"),
    })
}

fn parse_document_paragraph_style_node(
    properties: roxmltree::Node<'_, '_>,
) -> DocumentParagraphStyle {
    let spacing = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "spacing");
    let indent = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "ind");
    let num_pr = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "numPr");
    DocumentParagraphStyle {
        contextual_spacing: properties
            .children()
            .any(|node| node.is_element() && node.tag_name().name() == "contextualSpacing"),
        alignment: child_attribute(properties, "jc", "val").and_then(parse_document_alignment),
        before_twips: parse_u32_attribute(spacing, "before"),
        after_twips: parse_u32_attribute(spacing, "after"),
        line_twips: parse_u32_attribute(spacing, "line"),
        line_rule: spacing
            .and_then(|node| word_attribute(node, "lineRule"))
            .and_then(parse_document_line_rule),
        left_indent_twips: parse_u32_attribute(indent, "left"),
        right_indent_twips: parse_u32_attribute(indent, "right"),
        first_line_indent_twips: parse_u32_attribute(indent, "firstLine"),
        bottom_border: properties
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "pBdr")
            .and_then(|borders| {
                borders
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "bottom")
            })
            .and_then(parse_document_border),
        num_id: num_pr
            .and_then(|num_pr| child_attribute(num_pr, "numId", "val"))
            .and_then(|value| value.parse().ok()),
        num_level: num_pr
            .and_then(|num_pr| child_attribute(num_pr, "ilvl", "val"))
            .and_then(|value| value.parse().ok()),
    }
}

fn style_boolean_child(style: roxmltree::Node<'_, '_>, child_name: &str) -> Option<bool> {
    style
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == child_name)
        .map(|node| word_attribute(node, "val") != Some("0"))
}

fn normalize_document_style_name(style_id: &str, name: &str) -> String {
    if let Some(level) = style_id.strip_prefix("Heading") {
        if !level.is_empty() && level.chars().all(|character| character.is_ascii_digit()) {
            return format!("Heading {level}");
        }
    }
    name.to_string()
}

fn parse_u32_attribute(node: Option<roxmltree::Node<'_, '_>>, name: &str) -> Option<u32> {
    node.and_then(|node| word_attribute(node, name))
        .and_then(|value| value.parse().ok())
}

fn parse_document_line_rule(value: &str) -> Option<u8> {
    match value {
        "atLeast" => Some(0),
        "auto" => Some(1),
        "exact" => Some(2),
        _ => None,
    }
}

fn parse_document_border(node: roxmltree::Node<'_, '_>) -> Option<DocumentBorder> {
    if !matches!(word_attribute(node, "val"), Some("single")) {
        return None;
    }
    Some(DocumentBorder {
        color: word_attribute(node, "color")
            .and_then(parse_rgb_hex)
            .unwrap_or([0, 0, 0]),
        space_points: word_attribute(node, "space")?.parse().ok()?,
        size_eighth_points: word_attribute(node, "sz")?.parse().ok()?,
    })
}

#[derive(Debug)]
enum DocumentBlock {
    Paragraph(DocumentParagraph),
    Table(DocumentTable),
}

#[derive(Debug)]
struct DocumentTable {
    grid_twips: Vec<u32>,
    rows: Vec<Vec<DocumentCell>>,
}

#[derive(Debug)]
struct DocumentCell {
    width_twips: Option<u32>,
    fill: Option<[u8; 3]>,
    blocks: Vec<DocumentBlock>,
}

#[derive(Debug)]
struct DocumentParagraph {
    style_id: Option<String>,
    num_id: Option<u32>,
    num_level: Option<u32>,
    alignment: Option<u8>,
    spacing_before_twips: Option<u32>,
    spacing_after_twips: Option<u32>,
    spacing_line_twips: Option<u32>,
    spacing_line_rule: Option<u8>,
    contextual_spacing: bool,
    left_indent_twips: Option<u32>,
    right_indent_twips: Option<u32>,
    first_line_indent_twips: Option<u32>,
    bottom_border: Option<DocumentBorder>,
    section: Option<DocumentSection>,
    runs: Vec<DocumentRun>,
}

#[derive(Debug)]
enum DocumentRun {
    Text {
        value: String,
        properties: DocumentRunProperties,
    },
    Drawing(DocumentDrawing),
    PageBreak,
    LineBreak,
    Tab,
    Hyperlink {
        value: String,
        anchor: Option<String>,
        tooltip: Option<String>,
        runs: Vec<DocumentRun>,
    },
    Bookmark {
        id: u32,
        name: Option<String>,
        start: bool,
    },
    FieldChar(u8),
    InstructionText {
        value: String,
        properties: DocumentRunProperties,
    },
    CommentStart(u32),
    CommentEnd(u32),
    CommentReference(u32),
    Revision {
        kind: u8,
        id: u32,
        author: Option<String>,
        date: Option<String>,
        runs: Vec<DocumentRun>,
    },
}

#[derive(Debug, Clone)]
struct DocumentDrawing {
    inline: bool,
    extent_emu: Option<(u32, u32)>,
    doc_pr: Option<DocumentDrawingDocPr>,
    image: Option<DocumentDrawingImage>,
    shape: Option<DocumentDrawingShape>,
    chart_target: Option<String>,
    chart: Option<DocumentDrawingChart>,
}

#[derive(Debug, Clone, Default)]
struct DocumentDrawingShape {
    text: String,
    preset: String,
    fill: Option<[u8; 3]>,
    line: Option<[u8; 3]>,
    line_width_emu: Option<u32>,
    rotation: i32,
}

#[derive(Debug, Clone, Default)]
struct DocumentDrawingChart {
    title: String,
    categories: Vec<String>,
    series: Vec<DocumentDrawingChartSeries>,
    category_axis_id: i32,
    value_axis_id: i32,
}

#[derive(Debug, Clone, Default)]
struct DocumentDrawingChartSeries {
    index: u32,
    name_formula: String,
    name: String,
    category_formula: String,
    value_formula: String,
    values: Vec<String>,
    fill: Option<[u8; 3]>,
}

#[derive(Debug, Clone)]
struct DocumentDrawingDocPr {
    id: Option<u32>,
    name: Option<String>,
    descr: Option<String>,
}

#[derive(Debug, Clone)]
struct DocumentDrawingImage {
    raster_id: String,
    name: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct DocumentRunProperties {
    font_ascii: Option<String>,
    font_hansi: Option<String>,
    bold: bool,
    italic: bool,
    font_size_half_points: Option<u32>,
    color: Option<[u8; 3]>,
}

fn parse_document_source(
    xml: &[u8],
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
    header_footers: &DocumentHeaderFooterParts,
) -> anyhow::Result<DocumentSource> {
    let tree =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("document XML is not UTF-8")?)?;
    let body = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "body")
        .context("document body is missing")?;
    let mut blocks = Vec::new();
    for child in body.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "p" => blocks.push(DocumentBlock::Paragraph(parse_document_paragraph(
                child,
                styles,
                relationships,
                header_footers,
            ))),
            "tbl" => blocks.push(DocumentBlock::Table(parse_document_table(
                child,
                styles,
                relationships,
                header_footers,
            ))),
            _ => {}
        }
    }
    ensure!(!blocks.is_empty(), "document contains no content blocks");
    let section = body
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "sectPr")
        .and_then(|section| parse_document_section(section, relationships, header_footers));
    Ok(DocumentSource {
        blocks,
        section,
        header_footers: DocumentHeaderFooterParts {
            headers: header_footers
                .headers
                .iter()
                .map(|part| DocumentHeaderFooterPart {
                    path: part.path.clone(),
                    blocks: part.blocks.iter().map(clone_document_block).collect(),
                })
                .collect(),
            footers: header_footers
                .footers
                .iter()
                .map(|part| DocumentHeaderFooterPart {
                    path: part.path.clone(),
                    blocks: part.blocks.iter().map(clone_document_block).collect(),
                })
                .collect(),
        },
    })
}

fn parse_document_comments(xml: &[u8]) -> anyhow::Result<Vec<DocumentComment>> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document comments XML is not UTF-8")?,
    )?;
    Ok(tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "comment")
        .filter_map(|node| {
            let id = word_attribute(node, "id")?.parse().ok()?;
            let text = node
                .descendants()
                .filter(|child| child.is_element() && child.tag_name().name() == "t")
                .filter_map(|child| child.text())
                .collect::<String>();
            Some(DocumentComment {
                id,
                author: word_attribute(node, "author").map(str::to_string),
                initials: word_attribute(node, "initials").map(str::to_string),
                date: word_attribute(node, "date").map(str::to_string),
                text,
                solved: false,
                replies: Vec::new(),
            })
        })
        .collect())
}

fn parse_document_header_footer_parts<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
) -> anyhow::Result<DocumentHeaderFooterParts> {
    let mut parts = DocumentHeaderFooterParts::default();
    for target in relationships.values() {
        let normalized = normalize_document_part_target(target);
        if normalized.starts_with("word/header") && normalized.ends_with(".xml") {
            if let Some(xml) = read_optional_zip_part(archive, &normalized)? {
                parts.headers.push(DocumentHeaderFooterPart {
                    path: normalized,
                    blocks: parse_document_part_blocks(&xml, "hdr", styles, relationships, &parts)?,
                });
            }
        }
    }
    for target in relationships.values() {
        let normalized = normalize_document_part_target(target);
        if normalized.starts_with("word/footer") && normalized.ends_with(".xml") {
            if let Some(xml) = read_optional_zip_part(archive, &normalized)? {
                parts.footers.push(DocumentHeaderFooterPart {
                    path: normalized,
                    blocks: parse_document_part_blocks(&xml, "ftr", styles, relationships, &parts)?,
                });
            }
        }
    }
    Ok(parts)
}

fn parse_document_part_blocks(
    xml: &[u8],
    root_name: &str,
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
    header_footers: &DocumentHeaderFooterParts,
) -> anyhow::Result<Vec<DocumentBlock>> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml)
            .with_context(|| format!("document {root_name} XML is not UTF-8"))?,
    )?;
    let root = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == root_name)
        .with_context(|| format!("document {root_name} root is missing"))?;
    Ok(root
        .children()
        .filter(|node| node.is_element())
        .filter_map(|child| match child.tag_name().name() {
            "p" => Some(DocumentBlock::Paragraph(parse_document_paragraph(
                child,
                styles,
                relationships,
                header_footers,
            ))),
            "tbl" => Some(DocumentBlock::Table(parse_document_table(
                child,
                styles,
                relationships,
                header_footers,
            ))),
            _ => None,
        })
        .collect())
}

fn parse_document_section(
    section: roxmltree::Node<'_, '_>,
    relationships: &BTreeMap<String, String>,
    header_footers: &DocumentHeaderFooterParts,
) -> Option<DocumentSection> {
    let size = section
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "pgSz")?;
    let margins = section
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "pgMar")?;
    let parse = |node, name| word_attribute(node, name)?.parse().ok();
    let orientation = match word_attribute(size, "orient") {
        Some("landscape") => 1,
        _ => 0,
    };
    let break_type = section
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "type")
        .and_then(|node| word_attribute(node, "val"))
        .and_then(parse_document_section_break_type);
    let mut parsed = DocumentSection {
        width_twips: parse(size, "w")?,
        height_twips: parse(size, "h")?,
        orientation,
        margins_twips: [
            parse(margins, "left")?,
            parse(margins, "top")?,
            parse(margins, "right")?,
            parse(margins, "bottom")?,
            parse(margins, "header")?,
            parse(margins, "footer")?,
            parse(margins, "gutter").unwrap_or(0),
        ],
        title_page: section
            .children()
            .any(|node| node.is_element() && node.tag_name().name() == "titlePg"),
        break_type,
        header_default: None,
        header_first: None,
        header_even: None,
        footer_default: None,
        footer_first: None,
        footer_even: None,
    };
    for reference in section
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "headerReference")
    {
        let Some(index) = parse_document_header_footer_reference_index(
            reference,
            relationships,
            &header_footers.headers,
        ) else {
            continue;
        };
        match word_attribute(reference, "type").unwrap_or("default") {
            "first" => parsed.header_first = Some(index),
            "even" => parsed.header_even = Some(index),
            _ => parsed.header_default = Some(index),
        }
    }
    for reference in section
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "footerReference")
    {
        let Some(index) = parse_document_header_footer_reference_index(
            reference,
            relationships,
            &header_footers.footers,
        ) else {
            continue;
        };
        match word_attribute(reference, "type").unwrap_or("default") {
            "first" => parsed.footer_first = Some(index),
            "even" => parsed.footer_even = Some(index),
            _ => parsed.footer_default = Some(index),
        }
    }
    Some(parsed)
}

fn parse_document_header_footer_reference_index(
    reference: roxmltree::Node<'_, '_>,
    relationships: &BTreeMap<String, String>,
    parts: &[DocumentHeaderFooterPart],
) -> Option<usize> {
    let relationship_id = relationship_attribute(reference, "id")?;
    let target = relationships.get(relationship_id)?;
    let normalized = normalize_document_part_target(target);
    parts.iter().position(|part| part.path == normalized)
}

fn parse_document_section_break_type(value: &str) -> Option<u8> {
    match value {
        "continuous" => Some(0),
        "evenPage" => Some(1),
        "nextColumn" => Some(2),
        "nextPage" => Some(3),
        "oddPage" => Some(4),
        _ => None,
    }
}

fn parse_document_paragraph(
    paragraph: roxmltree::Node<'_, '_>,
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
    header_footers: &DocumentHeaderFooterParts,
) -> DocumentParagraph {
    let mut runs = Vec::new();
    let paragraph_properties = paragraph
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "pPr");
    let alignment = paragraph_properties
        .and_then(|properties| child_attribute(properties, "jc", "val"))
        .and_then(parse_document_alignment);
    let spacing = paragraph_properties.and_then(|properties| {
        properties
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "spacing")
    });
    let spacing_before_twips = spacing
        .and_then(|node| word_attribute(node, "before"))
        .and_then(|value| value.parse().ok());
    let explicit_after_twips = spacing
        .and_then(|node| word_attribute(node, "after"))
        .and_then(|value| value.parse().ok());
    let style_id =
        paragraph_properties.and_then(|properties| child_attribute(properties, "pStyle", "val"));
    let style = style_id.and_then(|id| styles.paragraph.get(id));
    let direct_num_pr = paragraph_properties.and_then(|properties| {
        properties
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "numPr")
    });
    let num_id = direct_num_pr
        .and_then(|num_pr| child_attribute(num_pr, "numId", "val"))
        .and_then(|value| value.parse().ok())
        .or_else(|| style.and_then(|style| style.num_id));
    let num_level = direct_num_pr
        .and_then(|num_pr| child_attribute(num_pr, "ilvl", "val"))
        .and_then(|value| value.parse().ok())
        .or_else(|| style.and_then(|style| style.num_level))
        .or_else(|| num_id.map(|_| 0));
    let spacing_after_twips = explicit_after_twips
        .or_else(|| style.and_then(|style| style.after_twips))
        .or(styles.default_after_twips);
    let spacing_line_twips = parse_u32_attribute(spacing, "line")
        .or_else(|| style.and_then(|style| style.line_twips))
        .or(styles.default_line_twips);
    let spacing_line_rule = spacing
        .and_then(|node| word_attribute(node, "lineRule"))
        .and_then(parse_document_line_rule)
        .or_else(|| style.and_then(|style| style.line_rule))
        .or(styles.default_line_rule);
    let indent = paragraph_properties.and_then(|properties| {
        properties
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "ind")
    });
    let section = paragraph_properties.and_then(|properties| {
        properties
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "sectPr")
            .and_then(|section| parse_document_section(section, relationships, header_footers))
    });
    for child in paragraph.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "r" => parse_document_run(child, styles, relationships, &mut runs),
            "hyperlink" => {
                let relationship_id = relationship_attribute(child, "id");
                let value = relationship_id
                    .and_then(|id| relationships.get(id))
                    .cloned()
                    .unwrap_or_default();
                let anchor = word_attribute(child, "anchor").map(str::to_string);
                let tooltip = word_attribute(child, "tooltip").map(str::to_string);
                let mut hyperlink_runs = Vec::new();
                for run in child
                    .children()
                    .filter(|node| node.is_element() && node.tag_name().name() == "r")
                {
                    parse_document_run(run, styles, relationships, &mut hyperlink_runs);
                }
                runs.push(DocumentRun::Hyperlink {
                    value,
                    anchor,
                    tooltip,
                    runs: hyperlink_runs,
                });
            }
            "bookmarkStart" => {
                if let (Some(id), Some(name)) = (
                    word_attribute(child, "id").and_then(|value| value.parse().ok()),
                    word_attribute(child, "name"),
                ) {
                    runs.push(DocumentRun::Bookmark {
                        id,
                        name: Some(name.to_string()),
                        start: true,
                    });
                }
            }
            "bookmarkEnd" => {
                if let Some(id) = word_attribute(child, "id").and_then(|value| value.parse().ok()) {
                    runs.push(DocumentRun::Bookmark {
                        id,
                        name: None,
                        start: false,
                    });
                }
            }
            "commentRangeStart" => {
                if let Some(id) = word_attribute(child, "id").and_then(|value| value.parse().ok()) {
                    runs.push(DocumentRun::CommentStart(id));
                }
            }
            "commentRangeEnd" => {
                if let Some(id) = word_attribute(child, "id").and_then(|value| value.parse().ok()) {
                    runs.push(DocumentRun::CommentEnd(id));
                }
            }
            "ins" | "del" => {
                let mut revision_runs = Vec::new();
                for run in child
                    .children()
                    .filter(|node| node.is_element() && node.tag_name().name() == "r")
                {
                    parse_document_run(run, styles, relationships, &mut revision_runs);
                }
                runs.push(DocumentRun::Revision {
                    kind: if child.tag_name().name() == "del" {
                        12
                    } else {
                        13
                    },
                    id: word_attribute(child, "id")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0),
                    author: word_attribute(child, "author").map(str::to_string),
                    date: word_attribute(child, "date").map(str::to_string),
                    runs: revision_runs,
                });
            }
            _ => {}
        }
    }
    DocumentParagraph {
        style_id: style_id.map(str::to_string),
        num_id,
        num_level,
        alignment,
        spacing_before_twips,
        spacing_after_twips,
        spacing_line_twips,
        spacing_line_rule,
        // Contextual spacing depends on paragraph style identity. This slice
        // flattens the referenced style into direct properties, so emitting
        // the flag without a matching Styles table would incorrectly suppress
        // spacing between otherwise unrelated paragraphs.
        contextual_spacing: false,
        left_indent_twips: parse_u32_attribute(indent, "left")
            .or_else(|| style.and_then(|style| style.left_indent_twips)),
        right_indent_twips: parse_u32_attribute(indent, "right")
            .or_else(|| style.and_then(|style| style.right_indent_twips)),
        first_line_indent_twips: parse_u32_attribute(indent, "firstLine")
            .or_else(|| style.and_then(|style| style.first_line_indent_twips)),
        bottom_border: style.and_then(|style| style.bottom_border.clone()),
        section,
        runs,
    }
}

fn parse_document_run(
    run: roxmltree::Node<'_, '_>,
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
    runs: &mut Vec<DocumentRun>,
) {
    let mut text = String::new();
    let properties = parse_document_run_properties(run, &styles.theme_fonts);
    for child in run.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "t" => text.push_str(child.text().unwrap_or_default()),
            "instrText" => {
                flush_document_text(runs, &mut text, &properties);
                runs.push(DocumentRun::InstructionText {
                    value: child.text().unwrap_or_default().to_string(),
                    properties: properties.clone(),
                });
            }
            "fldChar" => {
                flush_document_text(runs, &mut text, &properties);
                let kind = match word_attribute(child, "fldCharType") {
                    Some("separate") => 1,
                    Some("end") => 2,
                    _ => 0,
                };
                runs.push(DocumentRun::FieldChar(kind));
            }
            "commentReference" => {
                flush_document_text(runs, &mut text, &properties);
                if let Some(id) = word_attribute(child, "id").and_then(|value| value.parse().ok()) {
                    runs.push(DocumentRun::CommentReference(id));
                }
            }
            "delText" => text.push_str(child.text().unwrap_or_default()),
            "tab" => {
                flush_document_text(runs, &mut text, &properties);
                runs.push(DocumentRun::Tab);
            }
            "br" => {
                flush_document_text(runs, &mut text, &properties);
                if child.attribute((
                    "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                    "type",
                )) == Some("page")
                    || child.attribute("type") == Some("page")
                {
                    runs.push(DocumentRun::PageBreak);
                } else {
                    runs.push(DocumentRun::LineBreak);
                }
            }
            "drawing" => {
                flush_document_text(runs, &mut text, &properties);
                if let Some(drawing) = parse_document_drawing(child, relationships) {
                    runs.push(DocumentRun::Drawing(drawing));
                }
            }
            _ => {}
        }
    }
    flush_document_text(runs, &mut text, &properties);
}

fn parse_document_table(
    table: roxmltree::Node<'_, '_>,
    styles: &DocumentStyleContext,
    relationships: &BTreeMap<String, String>,
    header_footers: &DocumentHeaderFooterParts,
) -> DocumentTable {
    let grid_twips = table
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "tblGrid")
        .map(|grid| {
            grid.children()
                .filter(|node| node.is_element() && node.tag_name().name() == "gridCol")
                .filter_map(|node| word_attribute(node, "w")?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let rows = table
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "tr")
        .map(|row| {
            row.children()
                .filter(|node| node.is_element() && node.tag_name().name() == "tc")
                .map(|cell| {
                    let cell_properties = cell
                        .children()
                        .find(|node| node.is_element() && node.tag_name().name() == "tcPr");
                    let width_twips = cell_properties
                        .and_then(|properties| {
                            properties
                                .children()
                                .find(|node| node.is_element() && node.tag_name().name() == "tcW")
                        })
                        .and_then(|node| word_attribute(node, "w"))
                        .and_then(|value| value.parse().ok());
                    let fill = cell_properties
                        .and_then(|properties| {
                            properties
                                .children()
                                .find(|node| node.is_element() && node.tag_name().name() == "shd")
                        })
                        .and_then(|node| word_attribute(node, "fill"))
                        .and_then(parse_rgb_hex);
                    let blocks = cell
                        .children()
                        .filter(|node| node.is_element())
                        .filter_map(|child| match child.tag_name().name() {
                            "p" => Some(DocumentBlock::Paragraph(parse_document_paragraph(
                                child,
                                styles,
                                relationships,
                                header_footers,
                            ))),
                            "tbl" => Some(DocumentBlock::Table(parse_document_table(
                                child,
                                styles,
                                relationships,
                                header_footers,
                            ))),
                            _ => None,
                        })
                        .collect();
                    DocumentCell {
                        width_twips,
                        fill,
                        blocks,
                    }
                })
                .collect()
        })
        .collect();
    DocumentTable { grid_twips, rows }
}

fn flush_document_text(
    runs: &mut Vec<DocumentRun>,
    text: &mut String,
    properties: &DocumentRunProperties,
) {
    if !text.is_empty() {
        runs.push(DocumentRun::Text {
            value: std::mem::take(text),
            properties: properties.clone(),
        });
    }
}

fn parse_document_drawing(
    drawing: roxmltree::Node<'_, '_>,
    relationships: &BTreeMap<String, String>,
) -> Option<DocumentDrawing> {
    let container = drawing.descendants().find(|node| {
        node.is_element()
            && matches!(node.tag_name().name(), "inline" | "anchor")
            && node.tag_name().namespace()
                == Some("http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing")
    })?;
    let extent_emu = container
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "extent")
        .and_then(|node| {
            Some((
                parse_u32_attribute(Some(node), "cx")?,
                parse_u32_attribute(Some(node), "cy")?,
            ))
        });
    let doc_pr = container
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "docPr")
        .map(|node| DocumentDrawingDocPr {
            id: parse_u32_attribute(Some(node), "id"),
            name: node.attribute("name").map(str::to_string),
            descr: node.attribute("descr").map(str::to_string),
        });
    let image = parse_document_drawing_image(container, relationships);
    let shape = parse_document_drawing_shape(container);
    let chart_target = container
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "chart")
        .and_then(|node| relationship_attribute(node, "id"))
        .and_then(|id| relationships.get(id))
        .map(|target| normalize_document_part_target(target));
    Some(DocumentDrawing {
        inline: container.tag_name().name() == "inline",
        extent_emu,
        doc_pr,
        image,
        shape,
        chart_target,
        chart: None,
    })
}

fn parse_document_drawing_shape(
    container: roxmltree::Node<'_, '_>,
) -> Option<DocumentDrawingShape> {
    let graphic_data = container.descendants().find(|node| {
        node.is_element()
            && node.tag_name().name() == "graphicData"
            && node
                .attribute("uri")
                .is_some_and(|uri| uri.ends_with("/wordprocessingShape"))
    })?;
    let shape_properties = graphic_data
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "spPr")?;
    let xfrm = shape_properties
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "xfrm");
    let preset = shape_properties
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "prstGeom")
        .and_then(|node| node.attribute("prst"))
        .unwrap_or("rect")
        .to_string();
    let fill = shape_properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "solidFill")
        .and_then(parse_drawing_rgb_color);
    let line_node = shape_properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "ln");
    let line = line_node.and_then(parse_drawing_rgb_color);
    let line_width_emu = line_node.and_then(|node| parse_u32_attribute(Some(node), "w"));
    let text = graphic_data
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "t")
        .filter_map(|node| node.text())
        .collect::<String>();
    Some(DocumentDrawingShape {
        text,
        preset,
        fill,
        line,
        line_width_emu,
        rotation: xfrm
            .and_then(|node| node.attribute("rot"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
    })
}

fn parse_drawing_rgb_color(node: roxmltree::Node<'_, '_>) -> Option<[u8; 3]> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == "srgbClr")
        .and_then(|child| child.attribute("val"))
        .and_then(parse_rgb_hex)
}

fn parse_document_drawing_image(
    container: roxmltree::Node<'_, '_>,
    relationships: &BTreeMap<String, String>,
) -> Option<DocumentDrawingImage> {
    let blip = container
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "blip")?;
    let relationship_id = blip
        .attribute((
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            "embed",
        ))
        .or_else(|| blip.attribute("embed"))?
        .to_string();
    let target = relationships
        .get(&relationship_id)
        .cloned()
        .unwrap_or_else(|| relationship_id.clone());
    let raster_id = normalize_document_image_target(&target);
    let name = container
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "cNvPr")
        .and_then(|node| node.attribute("name"))
        .map(str::to_string);
    Some(DocumentDrawingImage { raster_id, name })
}

fn normalize_document_image_target(target: &str) -> String {
    target
        .strip_prefix("../")
        .unwrap_or(target)
        .strip_prefix("word/")
        .unwrap_or_else(|| target.strip_prefix('/').unwrap_or(target))
        .to_string()
}

fn normalize_document_part_target(target: &str) -> String {
    let target = target.strip_prefix("../").unwrap_or(target);
    let target = target.strip_prefix('/').unwrap_or(target);
    if target.starts_with("word/") {
        target.to_string()
    } else {
        format!("word/{target}")
    }
}

fn parse_document_drawing_chart(xml: &[u8]) -> anyhow::Result<DocumentDrawingChart> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document chart XML is not UTF-8")?,
    )?;
    let chart = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "chart")
        .context("document chart element is missing")?;
    let title = chart
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "title")
        .map(|node| {
            node.descendants()
                .filter(|child| child.is_element() && child.tag_name().name() == "t")
                .filter_map(|child| child.text())
                .collect::<String>()
        })
        .unwrap_or_default();
    let bar_chart = chart
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "barChart")
        .context("only bar charts are supported by this DOCY slice")?;
    let mut categories = Vec::new();
    let mut series = Vec::new();
    for series_node in bar_chart
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "ser")
    {
        let index = chart_child_value(series_node, "idx")
            .and_then(|value| value.parse().ok())
            .unwrap_or(series.len() as u32);
        let tx = series_node
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "tx");
        let name_formula = tx
            .and_then(|node| chart_descendant_text(node, "f"))
            .unwrap_or_default();
        let name = tx
            .and_then(|node| chart_descendant_text(node, "v"))
            .unwrap_or_default();
        let category = series_node
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "cat");
        let category_formula = category
            .and_then(|node| chart_descendant_text(node, "f"))
            .unwrap_or_default();
        if categories.is_empty() {
            categories = category
                .map(|node| chart_cache_values(node, "strCache"))
                .unwrap_or_default();
        }
        let value = series_node
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "val");
        let value_formula = value
            .and_then(|node| chart_descendant_text(node, "f"))
            .unwrap_or_default();
        let values = value
            .map(|node| chart_cache_values(node, "numCache"))
            .unwrap_or_default();
        let fill = series_node
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "spPr")
            .and_then(|node| {
                node.descendants()
                    .find(|child| child.is_element() && child.tag_name().name() == "srgbClr")
            })
            .and_then(|node| node.attribute("val"))
            .and_then(parse_rgb_hex);
        series.push(DocumentDrawingChartSeries {
            index,
            name_formula,
            name,
            category_formula,
            value_formula,
            values,
            fill,
        });
    }
    let axis_ids = bar_chart
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "axId")
        .filter_map(|node| node.attribute("val"))
        .filter_map(|value| value.parse::<i32>().ok())
        .collect::<Vec<_>>();
    Ok(DocumentDrawingChart {
        title,
        categories,
        series,
        category_axis_id: axis_ids.first().copied().unwrap_or(-2_068_027_336),
        value_axis_id: axis_ids.get(1).copied().unwrap_or(-2_113_994_440),
    })
}

fn chart_child_value<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.attribute("val"))
}

fn chart_descendant_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(str::to_string)
}

fn chart_cache_values(node: roxmltree::Node<'_, '_>, cache_name: &str) -> Vec<String> {
    let Some(cache) = node
        .descendants()
        .find(|child| child.is_element() && child.tag_name().name() == cache_name)
    else {
        return Vec::new();
    };
    cache
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "pt")
        .filter_map(|point| chart_descendant_text(point, "v"))
        .collect()
}

fn attach_document_drawing_charts(
    blocks: &mut [DocumentBlock],
    chart_parts: &BTreeMap<String, DocumentDrawingChart>,
) {
    for block in blocks {
        match block {
            DocumentBlock::Paragraph(paragraph) => {
                attach_document_drawing_charts_to_runs(&mut paragraph.runs, chart_parts)
            }
            DocumentBlock::Table(table) => {
                for cell in table.rows.iter_mut().flatten() {
                    attach_document_drawing_charts(&mut cell.blocks, chart_parts);
                }
            }
        }
    }
}

fn attach_document_drawing_charts_to_runs(
    runs: &mut [DocumentRun],
    chart_parts: &BTreeMap<String, DocumentDrawingChart>,
) {
    for run in runs {
        match run {
            DocumentRun::Drawing(drawing) => {
                drawing.chart = drawing
                    .chart_target
                    .as_ref()
                    .and_then(|target| chart_parts.get(target))
                    .cloned();
            }
            DocumentRun::Hyperlink { runs, .. } | DocumentRun::Revision { runs, .. } => {
                attach_document_drawing_charts_to_runs(runs, chart_parts)
            }
            _ => {}
        }
    }
}

fn word_attribute<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attribute((
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        name,
    ))
    .or_else(|| node.attribute(name))
}

fn relationship_attribute<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<&'a str> {
    node.attribute((
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        name,
    ))
    .or_else(|| node.attribute(name))
}

fn child_attribute<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    child_name: &str,
    attribute: &str,
) -> Option<&'a str> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == child_name)
        .and_then(|child| word_attribute(child, attribute))
}

fn parse_document_relationships(xml: &[u8]) -> anyhow::Result<BTreeMap<String, String>> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document relationships XML is not UTF-8")?,
    )?;
    let mut relationships = BTreeMap::new();
    for relationship in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Relationship")
    {
        let Some(id) = relationship.attribute("Id") else {
            continue;
        };
        let Some(target) = relationship.attribute("Target") else {
            continue;
        };
        relationships.insert(id.to_string(), target.to_string());
    }
    Ok(relationships)
}

#[derive(Debug, Clone, Default)]
struct DocumentHeaderFooterRelationshipParts {
    headers: Vec<DocumentHeaderFooterRelationshipPart>,
    footers: Vec<DocumentHeaderFooterRelationshipPart>,
}

#[derive(Debug, Clone)]
struct DocumentHeaderFooterRelationshipPart {
    id: String,
    path: String,
}

fn parse_document_header_footer_relationship_parts(
    xml: &[u8],
) -> anyhow::Result<DocumentHeaderFooterRelationshipParts> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document relationships XML is not UTF-8")?,
    )?;
    let mut parts = DocumentHeaderFooterRelationshipParts::default();
    for relationship in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Relationship")
    {
        let Some(id) = relationship.attribute("Id") else {
            continue;
        };
        let Some(target) = relationship.attribute("Target") else {
            continue;
        };
        let relationship_type = relationship.attribute("Type").unwrap_or_default();
        let path = normalize_document_part_target(target);
        if relationship_type.ends_with("/header")
            || (path.starts_with("word/header") && path.ends_with(".xml"))
        {
            parts.headers.push(DocumentHeaderFooterRelationshipPart {
                id: id.to_string(),
                path,
            });
        } else if relationship_type.ends_with("/footer")
            || (path.starts_with("word/footer") && path.ends_with(".xml"))
        {
            parts.footers.push(DocumentHeaderFooterRelationshipPart {
                id: id.to_string(),
                path,
            });
        }
    }
    Ok(parts)
}

fn parse_document_alignment(value: &str) -> Option<u8> {
    match value {
        "right" => Some(0),
        "left" => Some(1),
        "center" => Some(2),
        "both" | "distribute" => Some(3),
        _ => None,
    }
}

fn parse_document_run_properties(
    run: roxmltree::Node<'_, '_>,
    theme_fonts: &DocumentThemeFonts,
) -> DocumentRunProperties {
    let Some(properties) = run
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "rPr")
    else {
        return DocumentRunProperties::default();
    };
    parse_document_run_property_node(properties, theme_fonts)
}

fn parse_document_run_property_node(
    properties: roxmltree::Node<'_, '_>,
    theme_fonts: &DocumentThemeFonts,
) -> DocumentRunProperties {
    let fonts = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "rFonts");
    let font_ascii =
        fonts.and_then(|node| resolve_document_font(node, "ascii", "asciiTheme", theme_fonts));
    let font_hansi =
        fonts.and_then(|node| resolve_document_font(node, "hAnsi", "hAnsiTheme", theme_fonts));
    let bold = properties.children().any(|node| {
        node.is_element()
            && node.tag_name().name() == "b"
            && word_attribute(node, "val") != Some("0")
    });
    let italic = properties.children().any(|node| {
        node.is_element()
            && node.tag_name().name() == "i"
            && word_attribute(node, "val") != Some("0")
    });
    let font_size_half_points = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "sz")
        .and_then(|node| word_attribute(node, "val"))
        .and_then(|value| value.parse().ok());
    let color = properties
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "color")
        .and_then(|node| word_attribute(node, "val"))
        .and_then(parse_rgb_hex);
    DocumentRunProperties {
        font_ascii,
        font_hansi,
        bold,
        italic,
        font_size_half_points,
        color,
    }
}

fn resolve_document_font(
    fonts: roxmltree::Node<'_, '_>,
    direct_attribute: &str,
    theme_attribute: &str,
    theme_fonts: &DocumentThemeFonts,
) -> Option<String> {
    word_attribute(fonts, direct_attribute)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let theme = word_attribute(fonts, theme_attribute)?;
            match theme {
                "majorHAnsi" | "majorAscii" | "majorEastAsia" | "majorBidi" => {
                    theme_fonts.major_latin.clone()
                }
                "minorHAnsi" | "minorAscii" | "minorEastAsia" | "minorBidi" => {
                    theme_fonts.minor_latin.clone()
                }
                _ => None,
            }
        })
}

fn merge_document_run_properties(
    defaults: &DocumentRunProperties,
    direct: DocumentRunProperties,
) -> DocumentRunProperties {
    DocumentRunProperties {
        font_ascii: direct.font_ascii.or_else(|| defaults.font_ascii.clone()),
        font_hansi: direct.font_hansi.or_else(|| defaults.font_hansi.clone()),
        bold: direct.bold || defaults.bold,
        italic: direct.italic || defaults.italic,
        font_size_half_points: direct
            .font_size_half_points
            .or(defaults.font_size_half_points),
        color: direct.color.or(defaults.color),
    }
}

fn parse_rgb_hex(value: &str) -> Option<[u8; 3]> {
    if value.len() != 6 || value.eq_ignore_ascii_case("auto") {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

#[derive(Debug, Clone, Default)]
struct DocumentNumbering {
    abstracts: Vec<DocumentAbstractNumbering>,
    nums: Vec<DocumentNumberingInstance>,
}

#[derive(Debug, Clone)]
struct DocumentAbstractNumbering {
    id: u32,
    levels: Vec<DocumentNumberingLevel>,
}

#[derive(Debug, Clone)]
struct DocumentNumberingLevel {
    level: u32,
    format: u8,
    text: String,
    start: u32,
    paragraph_style: Option<String>,
    left_twips: Option<u32>,
    hanging_twips: Option<u32>,
    font_ascii: Option<String>,
    font_hansi: Option<String>,
}

#[derive(Debug, Clone)]
struct DocumentNumberingInstance {
    id: u32,
    abstract_id: u32,
}

fn parse_document_numbering(xml: &[u8]) -> anyhow::Result<DocumentNumbering> {
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("document numbering XML is not UTF-8")?,
    )?;
    let mut numbering = DocumentNumbering::default();
    for abstract_node in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "abstractNum")
    {
        let Some(id) = word_attribute(abstract_node, "abstractNumId")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let mut levels = Vec::new();
        for level_node in abstract_node
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "lvl")
        {
            let level = word_attribute(level_node, "ilvl")
                .and_then(|value| value.parse().ok())
                .unwrap_or(levels.len() as u32);
            let format = child_attribute(level_node, "numFmt", "val")
                .and_then(document_number_format)
                .unwrap_or(13);
            let text = child_attribute(level_node, "lvlText", "val")
                .unwrap_or("%1.")
                .to_string();
            let start = child_attribute(level_node, "start", "val")
                .and_then(|value| value.parse().ok())
                .unwrap_or(1);
            let paragraph_style = child_attribute(level_node, "pStyle", "val").map(str::to_string);
            let paragraph_properties = level_node
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "pPr");
            let indent = paragraph_properties.and_then(|properties| {
                properties
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "ind")
            });
            let run_properties = level_node
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "rPr");
            let fonts = run_properties.and_then(|properties| {
                properties
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "rFonts")
            });
            levels.push(DocumentNumberingLevel {
                level,
                format,
                text,
                start,
                paragraph_style,
                left_twips: parse_u32_attribute(indent, "left"),
                hanging_twips: parse_u32_attribute(indent, "hanging"),
                font_ascii: fonts
                    .and_then(|node| word_attribute(node, "ascii"))
                    .map(str::to_string),
                font_hansi: fonts
                    .and_then(|node| word_attribute(node, "hAnsi"))
                    .map(str::to_string),
            });
        }
        if levels.len() == 1 && levels[0].format == 5 {
            let template = levels[0].clone();
            for level in 1..9u32 {
                let mut generated = template.clone();
                generated.level = level;
                generated.paragraph_style = None;
                generated.left_twips = Some(360 * (level + 1));
                levels.push(generated);
            }
        }
        numbering
            .abstracts
            .push(DocumentAbstractNumbering { id, levels });
    }
    for num_node in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "num")
    {
        let Some(id) =
            word_attribute(num_node, "numId").and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(abstract_id) = child_attribute(num_node, "abstractNumId", "val")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        numbering
            .nums
            .push(DocumentNumberingInstance { id, abstract_id });
    }
    Ok(numbering)
}

fn document_number_format(value: &str) -> Option<u8> {
    match value {
        "bullet" => Some(5),
        "decimal" => Some(13),
        _ => None,
    }
}

fn write_document_numbering_table(numbering: &DocumentNumbering) -> Vec<u8> {
    let mut content = Vec::new();
    if !numbering.abstracts.is_empty() {
        let mut abstracts = Vec::new();
        for abstract_numbering in &numbering.abstracts {
            let mut abstract_content = Vec::new();
            write_item(
                &mut abstract_content,
                2,
                &abstract_numbering.id.to_le_bytes(),
            );
            let mut levels = Vec::new();
            for level in &abstract_numbering.levels {
                write_item(&mut levels, 5, &write_document_numbering_level(level));
            }
            write_item(&mut abstract_content, 4, &levels);
            write_item(&mut abstracts, 1, &abstract_content);
        }
        write_item(&mut content, 0, &abstracts);
    }
    if !numbering.nums.is_empty() {
        let mut nums = Vec::new();
        for num in &numbering.nums {
            let mut num_content = Vec::new();
            write_fixed_property(&mut num_content, 19, 4, &num.abstract_id.to_le_bytes());
            write_fixed_property(&mut num_content, 20, 4, &num.id.to_le_bytes());
            write_item(&mut nums, 18, &num_content);
        }
        write_item(&mut content, 17, &nums);
    }
    let mut table = Vec::new();
    table.extend_from_slice(&(content.len() as u32).to_le_bytes());
    table.extend_from_slice(&content);
    table
}

fn write_document_numbering_level(level: &DocumentNumberingLevel) -> Vec<u8> {
    let mut output = Vec::new();
    let mut num_format = Vec::new();
    write_item(&mut num_format, 25, &[level.format]);
    output.push(24);
    output.push(6);
    output.extend_from_slice(&(num_format.len() as u32).to_le_bytes());
    output.extend_from_slice(&num_format);
    write_fixed_property(&mut output, 37, 1, &[10]);

    let mut level_text = Vec::new();
    for part in split_numbering_level_text(&level.text) {
        let mut item = Vec::new();
        match part {
            NumberingLevelTextPart::Text(value) => {
                item.push(10);
                write_utf16_string(&mut item, &value);
            }
            NumberingLevelTextPart::Number(value) => {
                item.push(11);
                item.extend_from_slice(&1u32.to_le_bytes());
                item.push(value);
            }
        }
        write_item(&mut level_text, 9, &item);
    }
    output.push(8);
    output.push(6);
    output.extend_from_slice(&(level_text.len() as u32).to_le_bytes());
    output.extend_from_slice(&level_text);
    write_fixed_property(&mut output, 13, 4, &level.start.to_le_bytes());
    if let Some(style) = &level.paragraph_style {
        write_variable_string_property(&mut output, 21, style);
    }
    if level.left_twips.is_some() || level.hanging_twips.is_some() {
        let mut indent = Vec::new();
        if let Some(left) = level.left_twips {
            write_fixed_property(&mut indent, 36, 4, &left.to_le_bytes());
        }
        if let Some(hanging) = level.hanging_twips {
            let first_line = (-(hanging as i32)) as u32;
            write_fixed_property(&mut indent, 38, 4, &first_line.to_le_bytes());
        }
        let mut paragraph_properties = Vec::new();
        paragraph_properties.push(1);
        paragraph_properties.push(6);
        paragraph_properties.extend_from_slice(&(indent.len() as u32).to_le_bytes());
        paragraph_properties.extend_from_slice(&indent);
        output.push(15);
        output.push(6);
        output.extend_from_slice(&(paragraph_properties.len() as u32).to_le_bytes());
        output.extend_from_slice(&paragraph_properties);
    }
    if level.font_ascii.is_some() || level.font_hansi.is_some() {
        let properties = DocumentRunProperties {
            font_ascii: level.font_ascii.clone(),
            font_hansi: level.font_hansi.clone(),
            ..DocumentRunProperties::default()
        };
        let run_properties = write_document_run_properties(&properties);
        output.push(16);
        output.push(6);
        output.extend_from_slice(&(run_properties.len() as u32).to_le_bytes());
        output.extend_from_slice(&run_properties);
    }
    write_fixed_property(&mut output, 29, 4, &level.level.to_le_bytes());
    output
}

enum NumberingLevelTextPart {
    Text(String),
    Number(u8),
}

fn split_numbering_level_text(value: &str) -> Vec<NumberingLevelTextPart> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '%' {
            if let Some(next) = characters.peek().copied() {
                if let Some(number) = next.to_digit(10) {
                    characters.next();
                    if !text.is_empty() {
                        parts.push(NumberingLevelTextPart::Text(std::mem::take(&mut text)));
                    }
                    parts.push(NumberingLevelTextPart::Number(
                        number.saturating_sub(1) as u8
                    ));
                    continue;
                }
            }
        }
        text.push(character);
    }
    if !text.is_empty() {
        parts.push(NumberingLevelTextPart::Text(text));
    }
    parts
}

fn write_document_styles_table(styles: &DocumentStyleContext) -> Vec<u8> {
    let mut content = Vec::new();
    let default_paragraph = DocumentParagraphStyle {
        after_twips: styles.default_after_twips,
        line_twips: styles.default_line_twips,
        line_rule: styles.default_line_rule,
        ..DocumentParagraphStyle::default()
    };
    let default_paragraph_properties =
        write_document_paragraph_style_properties(&default_paragraph);
    if !default_paragraph_properties.is_empty() {
        write_item(&mut content, 0, &default_paragraph_properties);
    }
    let default_run_properties = write_document_run_properties(&styles.default_run);
    if !default_run_properties.is_empty() {
        write_item(&mut content, 1, &default_run_properties);
    }
    if !styles.definitions.is_empty() {
        let mut style_records = Vec::new();
        for style in &styles.definitions {
            write_item(
                &mut style_records,
                0,
                &write_document_style_definition(style),
            );
        }
        write_item(&mut content, 2, &style_records);
    }

    let mut table = Vec::new();
    table.extend_from_slice(&(content.len() as u32).to_le_bytes());
    table.extend_from_slice(&content);
    table
}

fn write_document_comments_table(comments: &[DocumentComment]) -> Option<Vec<u8>> {
    if comments.is_empty() {
        return None;
    }
    let mut content = Vec::new();
    for comment in comments {
        write_item(&mut content, 0, &write_document_comment(comment, true));
    }
    let mut table = Vec::new();
    table.extend_from_slice(&(content.len() as u32).to_le_bytes());
    table.extend_from_slice(&content);
    Some(table)
}

fn write_document_comment(comment: &DocumentComment, include_id: bool) -> Vec<u8> {
    let mut output = Vec::new();
    if include_id {
        write_item(&mut output, 1, &comment.id.to_le_bytes());
    }
    if let Some(author) = comment.author.as_deref().filter(|value| !value.is_empty()) {
        write_raw_utf16_string_entry(&mut output, 3, author);
    }
    if let Some(initials) = comment
        .initials
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        write_raw_utf16_string_entry(&mut output, 2, initials);
    }
    if let Some(date) = comment.date.as_deref().filter(|value| !value.is_empty()) {
        write_raw_utf16_string_entry(&mut output, 5, date);
    }
    write_item(&mut output, 8, &[u8::from(comment.solved)]);
    if !comment.text.is_empty() {
        write_raw_utf16_string_entry(&mut output, 6, &comment.text);
    }
    if !comment.replies.is_empty() {
        let mut replies = Vec::new();
        for reply in &comment.replies {
            write_item(&mut replies, 0, &write_document_comment(reply, false));
        }
        write_item(&mut output, 9, &replies);
    }
    output
}

fn write_document_style_definition(style: &DocumentStyleDefinition) -> Vec<u8> {
    let mut output = Vec::new();
    write_style_string_item(&mut output, 1, &style.id);
    write_style_string_item(&mut output, 2, &style.name);
    if let Some(value) = &style.based_on {
        write_style_string_item(&mut output, 3, value);
    }
    if let Some(value) = &style.next {
        write_style_string_item(&mut output, 4, value);
    }
    if let Some(run) = &style.run {
        let run_properties = write_document_run_properties(run);
        if !run_properties.is_empty() {
            write_item(&mut output, 5, &run_properties);
        }
    }
    if let Some(paragraph) = &style.paragraph {
        let paragraph_properties = write_document_paragraph_style_properties(paragraph);
        if !paragraph_properties.is_empty() {
            write_item(&mut output, 6, &paragraph_properties);
        }
    }
    if style.is_default {
        write_item(&mut output, 8, &[1]);
    }
    write_item(&mut output, 9, &[style.style_type]);
    if let Some(value) = style.q_format {
        write_item(&mut output, 10, &[u8::from(value)]);
    }
    if let Some(value) = style.ui_priority {
        write_item(&mut output, 11, &value.to_le_bytes());
    }
    if let Some(value) = style.hidden {
        write_item(&mut output, 12, &[u8::from(value)]);
    }
    if let Some(value) = style.semi_hidden {
        write_item(&mut output, 13, &[u8::from(value)]);
    }
    if let Some(value) = style.unhide_when_used {
        write_item(&mut output, 14, &[u8::from(value)]);
    }
    if let Some(value) = &style.link {
        write_style_string_item(&mut output, 18, value);
    }
    if style.custom_style {
        write_item(&mut output, 19, &[1]);
    }
    output
}

fn write_document_paragraph_style_properties(style: &DocumentParagraphStyle) -> Vec<u8> {
    let mut paragraph = DocumentParagraph {
        style_id: None,
        num_id: style.num_id,
        num_level: style.num_level,
        alignment: style.alignment,
        spacing_before_twips: style.before_twips,
        spacing_after_twips: style.after_twips,
        spacing_line_twips: style.line_twips,
        spacing_line_rule: style.line_rule,
        contextual_spacing: style.contextual_spacing,
        left_indent_twips: style.left_indent_twips,
        right_indent_twips: style.right_indent_twips,
        first_line_indent_twips: style.first_line_indent_twips,
        bottom_border: style.bottom_border.clone(),
        section: None,
        runs: Vec::new(),
    };
    write_document_paragraph_properties(&mut paragraph)
}

fn write_document_table(source: &DocumentSource) -> Vec<u8> {
    let mut content = write_document_blocks(&source.blocks);
    if let Some(section) = &source.section {
        write_item(&mut content, 4, &write_document_section(section));
    }
    let mut table = Vec::new();
    table.extend_from_slice(&(content.len() as u32).to_le_bytes());
    table.extend_from_slice(&content);
    table
}

fn write_document_header_footer_table(source: &DocumentSource) -> Option<Vec<u8>> {
    let parts = &source.header_footers;
    if parts.headers.is_empty() && parts.footers.is_empty() {
        return None;
    }
    let (header_types, footer_types) = document_header_footer_role_types(source);
    let mut content = Vec::new();
    if !parts.headers.is_empty() {
        write_item(
            &mut content,
            0,
            &write_document_header_footer_content(&parts.headers, &header_types),
        );
    }
    if !parts.footers.is_empty() {
        write_item(
            &mut content,
            1,
            &write_document_header_footer_content(&parts.footers, &footer_types),
        );
    }
    let mut table = Vec::new();
    table.extend_from_slice(&(content.len() as u32).to_le_bytes());
    table.extend_from_slice(&content);
    Some(table)
}

fn write_document_header_footer_content(
    parts: &[DocumentHeaderFooterPart],
    role_types: &[u8],
) -> Vec<u8> {
    let mut content = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let item = write_document_header_footer_item(part);
        write_item(
            &mut content,
            role_types.get(index).copied().unwrap_or(4),
            &item,
        );
    }
    content
}

fn write_document_header_footer_item(part: &DocumentHeaderFooterPart) -> Vec<u8> {
    let mut item = Vec::new();
    write_item(&mut item, 5, &write_document_blocks(&part.blocks));
    item
}

fn document_header_footer_role_types(source: &DocumentSource) -> (Vec<u8>, Vec<u8>) {
    let mut headers = vec![4; source.header_footers.headers.len()];
    let mut footers = vec![4; source.header_footers.footers.len()];
    collect_document_header_footer_role_types_from_blocks(
        &source.blocks,
        &mut headers,
        &mut footers,
    );
    if let Some(section) = &source.section {
        collect_document_header_footer_role_types_from_section(section, &mut headers, &mut footers);
    }
    (headers, footers)
}

fn collect_document_header_footer_role_types_from_blocks(
    blocks: &[DocumentBlock],
    headers: &mut [u8],
    footers: &mut [u8],
) {
    for block in blocks {
        match block {
            DocumentBlock::Paragraph(paragraph) => {
                if let Some(section) = &paragraph.section {
                    collect_document_header_footer_role_types_from_section(
                        section, headers, footers,
                    );
                }
            }
            DocumentBlock::Table(table) => {
                for row in &table.rows {
                    for cell in row {
                        collect_document_header_footer_role_types_from_blocks(
                            &cell.blocks,
                            headers,
                            footers,
                        );
                    }
                }
            }
        }
    }
}

fn collect_document_header_footer_role_types_from_section(
    section: &DocumentSection,
    headers: &mut [u8],
    footers: &mut [u8],
) {
    if let Some(index) = section
        .header_default
        .and_then(|index| headers.get_mut(index))
    {
        *index = 4;
    }
    if let Some(index) = section.header_even.and_then(|index| headers.get_mut(index)) {
        *index = 3;
    }
    if let Some(index) = section
        .header_first
        .and_then(|index| headers.get_mut(index))
    {
        *index = 2;
    }
    if let Some(index) = section
        .footer_default
        .and_then(|index| footers.get_mut(index))
    {
        *index = 4;
    }
    if let Some(index) = section.footer_even.and_then(|index| footers.get_mut(index)) {
        *index = 3;
    }
    if let Some(index) = section
        .footer_first
        .and_then(|index| footers.get_mut(index))
    {
        *index = 2;
    }
}

fn write_document_section(section: &DocumentSection) -> Vec<u8> {
    let mut output = Vec::new();
    let mut size = Vec::new();
    write_fixed_property(&mut size, 3, 4, &section.width_twips.to_le_bytes());
    write_fixed_property(&mut size, 4, 4, &section.height_twips.to_le_bytes());
    write_fixed_property(&mut size, 2, 1, &[section.orientation]);
    write_item(&mut output, 0, &size);

    let mut margins = Vec::new();
    for (property, value) in (6u8..=12).zip(section.margins_twips) {
        write_fixed_property(&mut margins, property, 4, &value.to_le_bytes());
    }
    write_item(&mut output, 1, &margins);
    if section.title_page || section.break_type.is_some() {
        let mut settings = Vec::new();
        if section.title_page {
            write_fixed_property(&mut settings, 0, 1, &[1]);
        }
        if let Some(value) = section.break_type {
            write_fixed_property(&mut settings, 2, 1, &[value]);
        }
        write_item(&mut output, 2, &settings);
    }
    let headers = write_document_section_header_footer_refs(&[
        section.header_default,
        section.header_even,
        section.header_first,
    ]);
    if !headers.is_empty() {
        write_item(&mut output, 3, &headers);
    }
    let footers = write_document_section_header_footer_refs(&[
        section.footer_default,
        section.footer_even,
        section.footer_first,
    ]);
    if !footers.is_empty() {
        write_item(&mut output, 4, &footers);
    }
    output
}

fn write_document_section_header_footer_refs(indices: &[Option<usize>; 3]) -> Vec<u8> {
    let mut output = Vec::new();
    for index in indices.iter().flatten() {
        write_item(&mut output, 5, &(*index as u32).to_le_bytes());
    }
    output
}

fn write_document_blocks(blocks: &[DocumentBlock]) -> Vec<u8> {
    let mut content = Vec::new();
    for block in blocks {
        match block {
            DocumentBlock::Paragraph(paragraph) => {
                write_document_paragraph(&mut content, paragraph)
            }
            DocumentBlock::Table(table) => {
                content.push(3);
                let table_record = write_document_table_record(table);
                content.extend_from_slice(&(table_record.len() as u32).to_le_bytes());
                content.extend_from_slice(&table_record);
            }
        }
    }
    content
}

fn write_document_paragraph(content: &mut Vec<u8>, paragraph: &DocumentParagraph) {
    let mut paragraph_content = Vec::new();
    let paragraph_properties = write_document_paragraph_properties(paragraph);
    if !paragraph_properties.is_empty() {
        write_item(&mut paragraph_content, 1, &paragraph_properties);
    }
    if !paragraph.runs.is_empty() {
        let mut runs = Vec::new();
        write_document_runs(&mut runs, &paragraph.runs);
        write_item(&mut paragraph_content, 2, &runs);
    }
    content.push(0);
    content.extend_from_slice(&(paragraph_content.len() as u32).to_le_bytes());
    content.extend_from_slice(&paragraph_content);
}

fn write_document_runs(output: &mut Vec<u8>, runs: &[DocumentRun]) {
    write_document_runs_with_revision(output, runs, false);
}

fn write_document_runs_with_revision(output: &mut Vec<u8>, runs: &[DocumentRun], deleted: bool) {
    for run in runs {
        match run {
            DocumentRun::Hyperlink {
                value,
                anchor,
                tooltip,
                runs,
            } => {
                let mut hyperlink = Vec::new();
                if let Some(anchor) = anchor.as_deref().filter(|value| !value.is_empty()) {
                    write_raw_utf16_string_entry(&mut hyperlink, 2, anchor);
                }
                if !value.is_empty() || anchor.as_deref().unwrap_or_default().is_empty() {
                    write_raw_utf16_string_entry(&mut hyperlink, 1, value);
                }
                if let Some(tooltip) = tooltip.as_deref().filter(|value| !value.is_empty()) {
                    write_raw_utf16_string_entry(&mut hyperlink, 3, tooltip);
                }
                write_item(&mut hyperlink, 4, &[1]);
                let mut nested = Vec::new();
                write_document_runs_with_revision(&mut nested, runs, deleted);
                write_item(&mut hyperlink, 0, &nested);
                write_item(output, 10, &hyperlink);
                continue;
            }
            DocumentRun::Bookmark { id, name, start } => {
                let mut bookmark = Vec::new();
                write_item(&mut bookmark, 0, &id.to_le_bytes());
                if *start {
                    if let Some(name) = name {
                        write_raw_utf16_string_entry(&mut bookmark, 1, name);
                    }
                }
                write_item(output, if *start { 23 } else { 24 }, &bookmark);
                continue;
            }
            DocumentRun::CommentStart(id) | DocumentRun::CommentEnd(id) => {
                let mut comment = Vec::new();
                write_item(&mut comment, 1, &id.to_le_bytes());
                write_item(
                    output,
                    if matches!(run, DocumentRun::CommentStart(_)) {
                        6
                    } else {
                        7
                    },
                    &comment,
                );
                continue;
            }
            DocumentRun::Revision {
                kind,
                id,
                author,
                date,
                runs,
            } => {
                let mut revision = Vec::new();
                write_item(&mut revision, 2, &id.to_le_bytes());
                if let Some(author) = author.as_deref().filter(|value| !value.is_empty()) {
                    write_raw_utf16_string_entry(&mut revision, 0, author);
                }
                if let Some(date) = date.as_deref().filter(|value| !value.is_empty()) {
                    write_raw_utf16_string_entry(&mut revision, 1, date);
                }
                let mut nested = Vec::new();
                write_document_runs_with_revision(&mut nested, runs, *kind == 12);
                write_item(&mut revision, 4, &nested);
                write_item(output, *kind, &revision);
                continue;
            }
            _ => {}
        }

        let mut run_content = Vec::new();
        let properties = match run {
            DocumentRun::Text { value, properties } => {
                run_content.push(if deleted { 15 } else { 0 });
                write_utf16_string(&mut run_content, value);
                Some(properties)
            }
            DocumentRun::InstructionText { value, properties } => {
                run_content.push(30);
                write_utf16_string(&mut run_content, value);
                Some(properties)
            }
            DocumentRun::FieldChar(kind) => {
                let mut field = Vec::new();
                write_item(&mut field, 3, &[*kind]);
                write_item(&mut run_content, 29, &field);
                None
            }
            DocumentRun::Drawing(drawing) => {
                write_item(&mut run_content, 12, &write_document_drawing(drawing));
                None
            }
            DocumentRun::PageBreak => {
                run_content.push(4);
                run_content.extend_from_slice(&0u32.to_le_bytes());
                None
            }
            DocumentRun::LineBreak => {
                run_content.push(5);
                run_content.extend_from_slice(&0u32.to_le_bytes());
                None
            }
            DocumentRun::Tab => {
                run_content.push(2);
                run_content.extend_from_slice(&0u32.to_le_bytes());
                None
            }
            DocumentRun::CommentReference(id) => {
                let mut reference = Vec::new();
                write_item(&mut reference, 1, &id.to_le_bytes());
                write_item(&mut run_content, 11, &reference);
                None
            }
            DocumentRun::Hyperlink { .. }
            | DocumentRun::Bookmark { .. }
            | DocumentRun::CommentStart(_)
            | DocumentRun::CommentEnd(_)
            | DocumentRun::Revision { .. } => unreachable!(),
        };
        let mut run_record = Vec::new();
        if let Some(properties) = properties {
            let run_properties = write_document_run_properties(properties);
            if !run_properties.is_empty() {
                write_item(&mut run_record, 1, &run_properties);
            }
        }
        write_item(&mut run_record, 8, &run_content);
        write_item(output, 5, &run_record);
    }
}

fn write_document_table_record(table: &DocumentTable) -> Vec<u8> {
    let mut output = Vec::new();
    let mut table_properties = Vec::new();
    write_item(&mut table_properties, 12, &[2]);
    write_item(&mut output, 0, &table_properties);
    if !table.grid_twips.is_empty() {
        let mut grid = Vec::new();
        for width in &table.grid_twips {
            write_fixed_property(&mut grid, 13, 4, &width.to_le_bytes());
        }
        write_item(&mut output, 1, &grid);
    }
    let mut rows = Vec::new();
    for row in &table.rows {
        let mut cells = Vec::new();
        for cell in row {
            let mut cell_record = Vec::new();
            let cell_properties = write_document_cell_properties(cell);
            if !cell_properties.is_empty() {
                write_item(&mut cell_record, 7, &cell_properties);
            }
            let cell_blocks = cell
                .blocks
                .iter()
                .map(clone_document_block)
                .collect::<Vec<_>>();
            write_item(&mut cell_record, 8, &write_document_blocks(&cell_blocks));
            write_item(&mut cells, 6, &cell_record);
        }
        let mut row_record = Vec::new();
        write_item(&mut row_record, 5, &cells);
        write_item(&mut rows, 4, &row_record);
    }
    write_item(&mut output, 3, &rows);
    output
}

fn write_document_cell_properties(cell: &DocumentCell) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(fill) = cell.fill {
        let mut shading = Vec::new();
        write_fixed_property(&mut shading, 3, 3, &fill);
        output.push(1);
        output.push(6);
        output.extend_from_slice(&(shading.len() as u32).to_le_bytes());
        output.extend_from_slice(&shading);
    }
    if let Some(width) = cell.width_twips {
        let mut measurement = Vec::new();
        write_fixed_property(&mut measurement, 0, 1, &[1]);
        write_fixed_property(&mut measurement, 2, 4, &width.to_le_bytes());
        output.push(3);
        output.push(6);
        output.extend_from_slice(&(measurement.len() as u32).to_le_bytes());
        output.extend_from_slice(&measurement);
    }
    output
}

fn clone_document_block(block: &DocumentBlock) -> DocumentBlock {
    match block {
        DocumentBlock::Paragraph(paragraph) => {
            DocumentBlock::Paragraph(clone_document_paragraph(paragraph))
        }
        DocumentBlock::Table(table) => DocumentBlock::Table(clone_document_table(table)),
    }
}

fn clone_document_table(table: &DocumentTable) -> DocumentTable {
    DocumentTable {
        grid_twips: table.grid_twips.clone(),
        rows: table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| DocumentCell {
                        width_twips: cell.width_twips,
                        fill: cell.fill,
                        blocks: cell.blocks.iter().map(clone_document_block).collect(),
                    })
                    .collect()
            })
            .collect(),
    }
}

fn clone_document_paragraph(paragraph: &DocumentParagraph) -> DocumentParagraph {
    DocumentParagraph {
        style_id: paragraph.style_id.clone(),
        num_id: paragraph.num_id,
        num_level: paragraph.num_level,
        alignment: paragraph.alignment,
        spacing_before_twips: paragraph.spacing_before_twips,
        spacing_after_twips: paragraph.spacing_after_twips,
        spacing_line_twips: paragraph.spacing_line_twips,
        spacing_line_rule: paragraph.spacing_line_rule,
        contextual_spacing: paragraph.contextual_spacing,
        left_indent_twips: paragraph.left_indent_twips,
        right_indent_twips: paragraph.right_indent_twips,
        first_line_indent_twips: paragraph.first_line_indent_twips,
        bottom_border: paragraph.bottom_border.clone(),
        section: paragraph.section.clone(),
        runs: paragraph.runs.iter().map(clone_document_run).collect(),
    }
}

fn clone_document_run(run: &DocumentRun) -> DocumentRun {
    match run {
        DocumentRun::Text { value, properties } => DocumentRun::Text {
            value: value.clone(),
            properties: properties.clone(),
        },
        DocumentRun::Drawing(drawing) => DocumentRun::Drawing(drawing.clone()),
        DocumentRun::PageBreak => DocumentRun::PageBreak,
        DocumentRun::LineBreak => DocumentRun::LineBreak,
        DocumentRun::Tab => DocumentRun::Tab,
        DocumentRun::Hyperlink {
            value,
            anchor,
            tooltip,
            runs,
        } => DocumentRun::Hyperlink {
            value: value.clone(),
            anchor: anchor.clone(),
            tooltip: tooltip.clone(),
            runs: runs.iter().map(clone_document_run).collect(),
        },
        DocumentRun::Bookmark { id, name, start } => DocumentRun::Bookmark {
            id: *id,
            name: name.clone(),
            start: *start,
        },
        DocumentRun::FieldChar(kind) => DocumentRun::FieldChar(*kind),
        DocumentRun::InstructionText { value, properties } => DocumentRun::InstructionText {
            value: value.clone(),
            properties: properties.clone(),
        },
        DocumentRun::CommentStart(id) => DocumentRun::CommentStart(*id),
        DocumentRun::CommentEnd(id) => DocumentRun::CommentEnd(*id),
        DocumentRun::CommentReference(id) => DocumentRun::CommentReference(*id),
        DocumentRun::Revision {
            kind,
            id,
            author,
            date,
            runs,
        } => DocumentRun::Revision {
            kind: *kind,
            id: *id,
            author: author.clone(),
            date: date.clone(),
            runs: runs.iter().map(clone_document_run).collect(),
        },
    }
}

fn write_document_paragraph_properties(paragraph: &DocumentParagraph) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(style_id) = &paragraph.style_id {
        write_variable_string_property(&mut output, 21, style_id);
    }
    if paragraph.left_indent_twips.is_some()
        || paragraph.right_indent_twips.is_some()
        || paragraph.first_line_indent_twips.is_some()
    {
        let mut indent = Vec::new();
        if let Some(value) = paragraph.left_indent_twips {
            write_fixed_property(&mut indent, 36, 4, &value.to_le_bytes());
        }
        if let Some(value) = paragraph.right_indent_twips {
            write_fixed_property(&mut indent, 37, 4, &value.to_le_bytes());
        }
        if let Some(value) = paragraph.first_line_indent_twips {
            write_fixed_property(&mut indent, 38, 4, &value.to_le_bytes());
        }
        output.push(1);
        output.push(6);
        output.extend_from_slice(&(indent.len() as u32).to_le_bytes());
        output.extend_from_slice(&indent);
    }
    if paragraph.num_id.is_some() || paragraph.num_level.is_some() {
        let mut num_pr = Vec::new();
        if let Some(level) = paragraph.num_level {
            write_fixed_property(&mut num_pr, 23, 4, &level.to_le_bytes());
        }
        if let Some(num_id) = paragraph.num_id {
            write_fixed_property(&mut num_pr, 24, 4, &num_id.to_le_bytes());
        }
        output.push(22);
        output.push(6);
        output.extend_from_slice(&(num_pr.len() as u32).to_le_bytes());
        output.extend_from_slice(&num_pr);
    }
    if paragraph.contextual_spacing {
        write_fixed_property(&mut output, 0, 1, &[1]);
    }
    if let Some(alignment) = paragraph.alignment {
        write_fixed_property(&mut output, 5, 1, &[alignment]);
    }
    if paragraph.spacing_before_twips.is_some()
        || paragraph.spacing_after_twips.is_some()
        || paragraph.spacing_line_twips.is_some()
        || paragraph.spacing_line_rule.is_some()
    {
        let mut spacing = Vec::new();
        if let Some(value) = paragraph.spacing_line_twips {
            write_fixed_property(&mut spacing, 39, 4, &value.to_le_bytes());
        }
        if let Some(value) = paragraph.spacing_line_rule {
            write_fixed_property(&mut spacing, 11, 1, &[value]);
        }
        if let Some(value) = paragraph.spacing_before_twips {
            write_fixed_property(&mut spacing, 40, 4, &value.to_le_bytes());
        }
        if let Some(value) = paragraph.spacing_after_twips {
            write_fixed_property(&mut spacing, 41, 4, &value.to_le_bytes());
        }
        output.push(9);
        output.push(6);
        output.extend_from_slice(&(spacing.len() as u32).to_le_bytes());
        output.extend_from_slice(&spacing);
    }
    if let Some(border) = &paragraph.bottom_border {
        let mut border_properties = Vec::new();
        write_fixed_property(&mut border_properties, 0, 3, &border.color);
        write_fixed_property(
            &mut border_properties,
            5,
            4,
            &border.space_points.to_le_bytes(),
        );
        write_fixed_property(
            &mut border_properties,
            6,
            4,
            &border.size_eighth_points.to_le_bytes(),
        );
        write_fixed_property(&mut border_properties, 3, 1, &[1]);
        write_fixed_property(&mut border_properties, 7, 4, &1u32.to_le_bytes());
        let mut borders = Vec::new();
        write_item(&mut borders, 3, &border_properties);
        output.push(27);
        output.push(6);
        output.extend_from_slice(&(borders.len() as u32).to_le_bytes());
        output.extend_from_slice(&borders);
    }
    if let Some(section) = &paragraph.section {
        write_variable_property(&mut output, 31, &write_document_section(section));
    }
    output
}

fn write_document_run_properties(properties: &DocumentRunProperties) -> Vec<u8> {
    let mut output = Vec::new();
    if properties.bold {
        write_fixed_property(&mut output, 0, 1, &[1]);
    }
    if properties.italic {
        write_fixed_property(&mut output, 1, 1, &[1]);
    }
    if let Some(value) = &properties.font_ascii {
        write_variable_string_property(&mut output, 4, value);
    }
    if let Some(value) = &properties.font_hansi {
        write_variable_string_property(&mut output, 5, value);
    }
    if let Some(value) = properties.font_size_half_points {
        write_fixed_property(&mut output, 8, 4, &value.to_le_bytes());
    }
    if let Some(value) = properties.color {
        write_fixed_property(&mut output, 9, 3, &value);
    }
    output
}

fn write_document_drawing(drawing: &DocumentDrawing) -> Vec<u8> {
    // Euro-Office sdkjs Serialize2.js writes para_Drawing as c_oSerRunType.pptxDrawing (12)
    // and then a c_oSerImageType2 property stream. This deliberately covers the
    // OOXML placement/docPr metadata first; PptxData/image-shape serialization
    // remains the next required step before document.images-positioning can pass
    // visual differential review.
    let mut output = Vec::new();
    write_fixed_property(&mut output, 0, 1, &[if drawing.inline { 0 } else { 1 }]);
    if let Some((cx, cy)) = drawing.extent_emu {
        let mut extent = Vec::new();
        write_fixed_property(&mut extent, 2, 4, &cx.to_le_bytes());
        write_fixed_property(&mut extent, 3, 4, &cy.to_le_bytes());
        write_variable_property(&mut output, 14, &extent);
    }
    if let Some(doc_pr) = &drawing.doc_pr {
        let mut properties = Vec::new();
        if let Some(id) = doc_pr.id {
            write_item(&mut properties, 0, &id.to_le_bytes());
        }
        if let Some(name) = &doc_pr.name {
            write_raw_utf16_string_entry(&mut properties, 1, name);
        }
        if let Some(descr) = &doc_pr.descr {
            write_raw_utf16_string_entry(&mut properties, 4, descr);
        }
        if !properties.is_empty() {
            write_variable_property(&mut output, 31, &properties);
        }
    }
    if let Some(chart) = &drawing.chart {
        write_variable_property(&mut output, 25, &write_document_chart_binary(chart));
    } else if let Some(shape) = &drawing.shape {
        write_variable_property(
            &mut output,
            1,
            &write_document_drawing_shape_pptx_data(drawing, shape),
        );
    } else if let Some(image) = &drawing.image {
        let pptx_data = write_document_drawing_image_pptx_data(drawing, image);
        write_variable_property(&mut output, 1, &pptx_data);
    }
    output
}

fn write_document_drawing_shape_pptx_data(
    drawing: &DocumentDrawing,
    shape: &DocumentDrawingShape,
) -> Vec<u8> {
    // refs: sdkjs/common/Shapes/SerializeWriter.js:5588-5718
    let mut output = Vec::new();
    ppty_record(&mut output, 0, |outer| {
        ppty_record(outer, 1, |drawing_record| {
            ppty_record(drawing_record, 1, |shape_record| {
                write_ppty_attr_start(shape_record);
                write_ppty_attr_end(shape_record);
                ppty_record(shape_record, 0, |nv| {
                    ppty_record(nv, 0, |cnv| {
                        write_ppty_attr_start(cnv);
                        write_ppty_u32_attr(cnv, 0, 0);
                        write_ppty_string_attr(
                            cnv,
                            1,
                            drawing
                                .doc_pr
                                .as_ref()
                                .and_then(|value| value.name.as_deref())
                                .unwrap_or("Shape"),
                        );
                        write_ppty_attr_end(cnv);
                    });
                    ppty_record(nv, 1, |properties| {
                        write_ppty_attr_start(properties);
                        write_ppty_attr_end(properties);
                    });
                    ppty_record(nv, 2, |locks| {
                        write_ppty_attr_start(locks);
                        write_ppty_attr_end(locks);
                        ppty_record(locks, 1, |_| {});
                        ppty_record(locks, 2, |object_type| {
                            object_type.extend_from_slice(&0u32.to_le_bytes());
                        });
                    });
                });
                ppty_record(shape_record, 1, |sp_pr| {
                    write_ppty_attr_start(sp_pr);
                    write_ppty_attr_end(sp_pr);
                    ppty_record(sp_pr, 0, |xfrm| {
                        write_ppty_attr_start(xfrm);
                        write_ppty_i32_attr(xfrm, 0, 0);
                        write_ppty_i32_attr(xfrm, 1, 0);
                        if let Some((cx, cy)) = drawing.extent_emu {
                            write_ppty_i32_attr(xfrm, 2, cx.min(i32::MAX as u32) as i32);
                            write_ppty_i32_attr(xfrm, 3, cy.min(i32::MAX as u32) as i32);
                        }
                        write_ppty_i32_attr(xfrm, 10, shape.rotation);
                        write_ppty_attr_end(xfrm);
                    });
                    ppty_record(sp_pr, 1, |geometry| {
                        ppty_record(geometry, 1, |preset| {
                            write_ppty_attr_start(preset);
                            write_ppty_string_attr(preset, 0, &shape.preset);
                            write_ppty_attr_end(preset);
                            ppty_record(preset, 0, |adjustments| {
                                adjustments.extend_from_slice(&0u32.to_le_bytes());
                            });
                        });
                    });
                    if let Some(color) = shape.fill {
                        write_ppty_solid_fill(sp_pr, 2, color);
                    }
                    if let Some(color) = shape.line {
                        ppty_record(sp_pr, 3, |line| {
                            write_ppty_attr_start(line);
                            if let Some(width) = shape.line_width_emu {
                                write_ppty_i32_attr(line, 3, width.min(i32::MAX as u32) as i32);
                            }
                            write_ppty_attr_end(line);
                            write_ppty_solid_fill(line, 0, color);
                        });
                    }
                    ppty_record(sp_pr, 4, |_| {});
                });
                if !shape.text.is_empty() {
                    ppty_record(shape_record, 4, |text_box| {
                        let paragraph = DocumentParagraph {
                            style_id: None,
                            num_id: None,
                            num_level: None,
                            alignment: None,
                            spacing_before_twips: None,
                            spacing_after_twips: None,
                            spacing_line_twips: None,
                            spacing_line_rule: None,
                            contextual_spacing: false,
                            left_indent_twips: None,
                            right_indent_twips: None,
                            first_line_indent_twips: None,
                            bottom_border: None,
                            section: None,
                            runs: vec![DocumentRun::Text {
                                value: shape.text.clone(),
                                properties: DocumentRunProperties::default(),
                            }],
                        };
                        let content = write_document_blocks(&[DocumentBlock::Paragraph(paragraph)]);
                        text_box.extend_from_slice(&(content.len() as u32).to_le_bytes());
                        text_box.extend_from_slice(&content);
                    });
                    ppty_record(shape_record, 5, |body_pr| {
                        write_ppty_attr_start(body_pr);
                        write_ppty_i32_attr(body_pr, 3, 12_700);
                        write_ppty_i32_attr(body_pr, 8, 12_700);
                        write_ppty_i32_attr(body_pr, 10, 12_700);
                        write_ppty_i32_attr(body_pr, 15, 12_700);
                        write_ppty_bool_attr(body_pr, 16, true);
                        write_ppty_bool_attr(body_pr, 19, true);
                        write_ppty_attr_end(body_pr);
                        ppty_record(body_pr, 1, |text_fit| {
                            write_ppty_attr_start(text_fit);
                            write_ppty_u32_attr(text_fit, 0, 0);
                            write_ppty_attr_end(text_fit);
                        });
                    });
                }
            });
        });
    });
    output
}

fn write_ppty_solid_fill(output: &mut Vec<u8>, record_type: u8, color: [u8; 3]) {
    ppty_record(output, record_type, |fill| {
        ppty_record(fill, 3, |solid| {
            ppty_record(solid, 0, |unicolor| {
                ppty_record(unicolor, 1, |rgb| {
                    write_ppty_attr_start(rgb);
                    rgb.extend_from_slice(&[0, color[0], 1, color[1], 2, color[2]]);
                    write_ppty_attr_end(rgb);
                });
            });
        });
    });
}

fn write_document_chart_binary(chart: &DocumentDrawingChart) -> Vec<u8> {
    // refs: sdkjs/common/SerializeChart.js:1451-1545,4511-4670,5571-5665,5882-5965
    let mut chart_space = Vec::new();
    write_item(&mut chart_space, 0, &chart_boolean(false));
    write_item(&mut chart_space, 8, &write_document_chart(chart));
    chart_space
}

fn write_document_chart(chart: &DocumentDrawingChart) -> Vec<u8> {
    let mut output = Vec::new();
    if !chart.title.is_empty() {
        write_item(&mut output, 0, &write_document_chart_title(&chart.title));
    }
    write_item(&mut output, 1, &chart_boolean(false));
    let mut plot_area = Vec::new();
    write_item(&mut plot_area, 5, &write_document_bar_chart(chart));
    write_item(
        &mut plot_area,
        19,
        &write_document_category_axis(chart.category_axis_id, chart.value_axis_id),
    );
    write_item(
        &mut plot_area,
        22,
        &write_document_value_axis(chart.value_axis_id, chart.category_axis_id),
    );
    write_item(&mut output, 7, &plot_area);
    write_item(&mut output, 9, &chart_boolean(true));
    write_item(&mut output, 10, &chart_enum(1));
    write_item(&mut output, 11, &chart_boolean(false));
    output
}

fn write_document_bar_chart(chart: &DocumentDrawingChart) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &chart_enum(1));
    write_item(&mut output, 1, &chart_enum(1));
    for series in &chart.series {
        write_item(
            &mut output,
            3,
            &write_document_bar_chart_series(chart, series),
        );
    }
    write_item(
        &mut output,
        8,
        &chart_unsigned(chart.category_axis_id as u32),
    );
    write_item(&mut output, 8, &chart_unsigned(chart.value_axis_id as u32));
    output
}

fn write_document_bar_chart_series(
    chart: &DocumentDrawingChart,
    series: &DocumentDrawingChartSeries,
) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &chart_unsigned(series.index));
    write_item(&mut output, 1, &chart_unsigned(series.index));
    let mut tx = Vec::new();
    write_item(
        &mut tx,
        0,
        &write_document_chart_string_ref(&series.name_formula, std::slice::from_ref(&series.name)),
    );
    write_item(&mut output, 2, &tx);
    if let Some(fill) = series.fill {
        write_item(
            &mut output,
            3,
            &write_document_chart_series_shape_properties(fill),
        );
    }
    let mut categories = Vec::new();
    write_item(
        &mut categories,
        4,
        &write_document_chart_string_ref(&series.category_formula, &chart.categories),
    );
    write_item(&mut output, 10, &categories);
    let mut values = Vec::new();
    write_item(
        &mut values,
        1,
        &write_document_chart_number_ref(&series.value_formula, &series.values),
    );
    write_item(&mut output, 11, &values);
    output
}

fn write_document_chart_series_shape_properties(fill: [u8; 3]) -> Vec<u8> {
    let mut output = Vec::new();
    ppty_record(&mut output, 0, |shape_properties| {
        write_ppty_attr_start(shape_properties);
        write_ppty_attr_end(shape_properties);
        ppty_record(shape_properties, 1, |_| {});
        write_ppty_solid_fill(shape_properties, 2, fill);
        ppty_record(shape_properties, 4, |_| {});
    });
    output
}

fn write_document_chart_string_ref(formula: &str, values: &[String]) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &utf16_bytes(formula));
    let mut cache = Vec::new();
    write_item(&mut cache, 0, &chart_unsigned(values.len() as u32));
    for (index, value) in values.iter().enumerate() {
        let mut point = Vec::new();
        write_item(&mut point, 0, &utf16_bytes(value));
        write_item(&mut point, 1, &(index as u32).to_le_bytes());
        write_item(&mut cache, 1, &point);
    }
    write_item(&mut output, 1, &cache);
    output
}

fn write_document_chart_number_ref(formula: &str, values: &[String]) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &utf16_bytes(formula));
    let mut cache = Vec::new();
    write_item(&mut cache, 0, &utf16_bytes("General"));
    write_item(&mut cache, 1, &chart_unsigned(values.len() as u32));
    for (index, value) in values.iter().enumerate() {
        let mut point = Vec::new();
        write_item(&mut point, 0, &utf16_bytes(value));
        write_item(&mut point, 1, &(index as u32).to_le_bytes());
        write_item(&mut cache, 2, &point);
    }
    write_item(&mut output, 1, &cache);
    output
}

fn write_document_category_axis(axis_id: i32, cross_axis_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &chart_unsigned(axis_id as u32));
    let mut scaling = Vec::new();
    write_item(&mut scaling, 1, &chart_enum(1));
    write_item(&mut output, 1, &scaling);
    write_item(&mut output, 2, &chart_boolean(false));
    write_item(&mut output, 3, &chart_enum(0));
    write_item(&mut output, 8, &chart_enum(3));
    write_item(&mut output, 9, &chart_enum(2));
    write_item(&mut output, 10, &chart_enum(2));
    write_item(&mut output, 13, &chart_unsigned(cross_axis_id as u32));
    write_item(&mut output, 14, &chart_enum(0));
    write_item(&mut output, 16, &chart_boolean(true));
    write_item(&mut output, 17, &chart_enum(0));
    write_item(&mut output, 18, &chart_string("100"));
    write_item(&mut output, 21, &chart_boolean(false));
    output
}

fn write_document_value_axis(axis_id: i32, cross_axis_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &chart_unsigned(axis_id as u32));
    write_item(&mut output, 1, &[]);
    write_item(&mut output, 2, &chart_boolean(false));
    write_item(&mut output, 3, &chart_enum(1));
    write_item(&mut output, 4, &[]);
    write_item(&mut output, 8, &chart_enum(3));
    write_item(&mut output, 9, &chart_enum(2));
    write_item(&mut output, 10, &chart_enum(2));
    write_item(&mut output, 13, &chart_unsigned(cross_axis_id as u32));
    write_item(&mut output, 14, &chart_enum(0));
    output
}

fn chart_boolean(value: bool) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &[u8::from(value)]);
    output
}

fn chart_enum(value: u8) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &[value]);
    output
}

fn chart_unsigned(value: u32) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &value.to_le_bytes());
    output
}

fn chart_string(value: &str) -> Vec<u8> {
    let mut output = Vec::new();
    write_item(&mut output, 0, &utf16_bytes(value));
    output
}

fn utf16_bytes(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn write_document_chart_title(value: &str) -> Vec<u8> {
    // ref: sdkjs/common/SerializeChart.js WriteCT_Title/WriteCT_Tx/WriteTxBody
    // A simple rich-text title is a stable PPTY CTextBody skeleton whose only
    // variable field is the UTF-16 run. Keeping the upstream record boundaries
    // exact is load-bearing: a generic record wrapper cannot represent the
    // record-0 shortcuts used by the PPTY writer.
    let text = utf16_bytes(value);
    let chars = u32::try_from(value.encode_utf16().count()).unwrap_or(u32::MAX);
    let bytes = u32::try_from(text.len()).unwrap_or(u32::MAX);
    let mut output = vec![
        0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x73, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00,
        0x00, 0x0e, 0x00, 0x00, 0x00, 0xfa, 0xfb, 0x01, 0x07, 0x00, 0x00, 0x00, 0xfa, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xfb, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x51, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x02, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x3a, 0x00, 0x00, 0x00, 0x01, 0x35, 0x00, 0x00, 0x00, 0xfa, 0x00, 0x17,
        0x00, 0x00, 0x00,
    ];
    for (position, base) in [
        (1usize, 74u32),
        (6, 69),
        (11, 64),
        (40, 35),
        (49, 26),
        (54, 21),
        (63, 12),
        (68, 7),
    ] {
        output[position..position + 4].copy_from_slice(&(base + bytes).to_le_bytes());
    }
    output[74..78].copy_from_slice(&chars.to_le_bytes());
    output.extend_from_slice(&text);
    output.extend_from_slice(&[
        0xfb, 0x01, 0, 0, 0, 0, 0x02, 0x06, 0, 0, 0, 0, 0x01, 0, 0, 0, 0,
    ]);
    output
}

fn write_document_drawing_image_pptx_data(
    drawing: &DocumentDrawing,
    image: &DocumentDrawingImage,
) -> Vec<u8> {
    // ref: sdkjs/common/Shapes/SerializeWriter.js:5588-5815
    // ref: sdkjs/common/Shapes/Serialize.js:10913-11445
    //
    // The Word DOCY c_oSerImageType2.PptxData value is a PPTY drawing tree.
    // Minimal renderable image shape:
    //   record 0
    //     record 1
    //       record 2 (CImageShape)
    //         record 0 nvPicPr
    //         record 1 blipFill
    //         record 2 spPr/xfrm/prstGeom
    let mut output = Vec::new();
    ppty_record(&mut output, 0, |outer| {
        ppty_record(outer, 1, |drawing_record| {
            ppty_record(drawing_record, 2, |pic| {
                write_ppty_image_nv_pr(pic, image);
                write_ppty_image_blip_fill(pic, image);
                write_ppty_image_shape_properties(pic, drawing);
            });
        });
    });
    output
}

fn write_ppty_image_nv_pr(output: &mut Vec<u8>, image: &DocumentDrawingImage) {
    ppty_record(output, 0, |nv| {
        ppty_record(nv, 0, |cnv| {
            write_ppty_attr_start(cnv);
            write_ppty_u32_attr(cnv, 0, 0);
            write_ppty_string_attr(cnv, 1, image.name.as_deref().unwrap_or("Picture"));
            write_ppty_attr_end(cnv);
        });
    });
}

fn write_ppty_image_blip_fill(output: &mut Vec<u8>, image: &DocumentDrawingImage) {
    ppty_record(output, 1, |fill_record| {
        // c_oAscFill.FILL_TYPE_BLIP = 1.
        ppty_record(fill_record, 1, |blip_fill| {
            write_ppty_attr_start(blip_fill);
            blip_fill.push(1);
            blip_fill.push(1); // rotWithShape = true
            write_ppty_attr_end(blip_fill);

            ppty_record(blip_fill, 0, |blip| {
                write_ppty_attr_start(blip);
                write_ppty_attr_end(blip);

                ppty_record(blip, 2, |effects| {
                    effects.extend_from_slice(&0u32.to_le_bytes());
                });
                ppty_record(blip, 3, |path| {
                    write_ppty_attr_start(path);
                    write_ppty_string_attr(path, 0, &image.raster_id);
                    write_ppty_attr_end(path);
                });
            });

            ppty_record(blip_fill, 3, |stretch| {
                // Empty stretch record matches pic:blipFill/a:stretch/a:fillRect
                // closely enough for sdkjs to create a CBlipFillStretch.
                let _ = stretch;
            });
        });
    });
}

fn write_ppty_image_shape_properties(output: &mut Vec<u8>, drawing: &DocumentDrawing) {
    ppty_record(output, 2, |sp_pr| {
        write_ppty_attr_start(sp_pr);
        write_ppty_attr_end(sp_pr);
        if let Some((cx, cy)) = drawing.extent_emu {
            ppty_record(sp_pr, 0, |xfrm| {
                write_ppty_attr_start(xfrm);
                write_ppty_i32_attr(xfrm, 0, 0);
                write_ppty_i32_attr(xfrm, 1, 0);
                write_ppty_i32_attr(xfrm, 2, cx.min(i32::MAX as u32) as i32);
                write_ppty_i32_attr(xfrm, 3, cy.min(i32::MAX as u32) as i32);
                write_ppty_attr_end(xfrm);
            });
        }
        ppty_record(sp_pr, 1, |geometry| {
            ppty_record(geometry, 1, |preset| {
                write_ppty_attr_start(preset);
                write_ppty_string_attr(preset, 0, "rect");
                write_ppty_attr_end(preset);
            });
        });
    });
}

fn ppty_record(output: &mut Vec<u8>, record_type: u8, write: impl FnOnce(&mut Vec<u8>)) {
    output.push(record_type);
    let length_position = output.len();
    output.extend_from_slice(&0u32.to_le_bytes());
    let content_start = output.len();
    write(output);
    let length = u32::try_from(output.len() - content_start).unwrap_or(u32::MAX);
    output[length_position..length_position + 4].copy_from_slice(&length.to_le_bytes());
}

fn write_ppty_attr_start(output: &mut Vec<u8>) {
    output.push(0xFA);
}

fn write_ppty_attr_end(output: &mut Vec<u8>) {
    output.push(0xFB);
}

fn write_ppty_u32_attr(output: &mut Vec<u8>, attribute_type: u8, value: u32) {
    output.push(attribute_type);
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_ppty_i32_attr(output: &mut Vec<u8>, attribute_type: u8, value: i32) {
    output.push(attribute_type);
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_ppty_bool_attr(output: &mut Vec<u8>, attribute_type: u8, value: bool) {
    output.push(attribute_type);
    output.push(u8::from(value));
}

fn write_ppty_string_attr(output: &mut Vec<u8>, attribute_type: u8, value: &str) {
    output.push(attribute_type);
    write_ppty_string2(output, value);
}

fn write_ppty_string2(output: &mut Vec<u8>, value: &str) {
    let utf16 = value.encode_utf16().collect::<Vec<_>>();
    output.extend_from_slice(&(utf16.len() as u32).to_le_bytes());
    for code in utf16 {
        output.extend_from_slice(&code.to_le_bytes());
    }
}

fn write_fixed_property(output: &mut Vec<u8>, property_type: u8, length_type: u8, bytes: &[u8]) {
    output.push(property_type);
    output.push(length_type);
    output.extend_from_slice(bytes);
}

fn write_variable_property(output: &mut Vec<u8>, property_type: u8, bytes: &[u8]) {
    output.push(property_type);
    output.push(6);
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(bytes);
}

fn write_variable_string_property(output: &mut Vec<u8>, property_type: u8, value: &str) {
    output.push(property_type);
    output.push(6);
    write_utf16_string(output, value);
}

fn write_raw_utf16_string_entry(output: &mut Vec<u8>, property_type: u8, value: &str) {
    output.push(property_type);
    write_utf16_string(output, value);
}

fn write_style_string_item(output: &mut Vec<u8>, item_type: u8, value: &str) {
    let bytes = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    write_item(output, item_type, &bytes);
}

fn write_item(output: &mut Vec<u8>, record_type: u8, content: &[u8]) {
    output.push(record_type);
    output.extend_from_slice(&(content.len() as u32).to_le_bytes());
    output.extend_from_slice(content);
}

fn write_utf16_string(output: &mut Vec<u8>, value: &str) {
    let bytes = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(&bytes);
}

/// Build the prepared-state contract once a native converter has produced an
/// editor payload. Keeping this boundary separate prevents OOXML bytes from
/// being mislabeled as the Euro-Office binary protocol.
pub fn prepare_with_editor_payload(
    kind: OfficeKind,
    source_bytes: &[u8],
    editor_payload: &[u8],
    options: PrepareOptions,
) -> anyhow::Result<PreparedEditorPayload> {
    let manifest = inspect(kind, source_bytes)?;
    let editor_manifest = inspect_editor_payload(kind, editor_payload)?;
    Ok(PreparedEditorPayload {
        kind,
        protocol: editor_manifest.protocol.clone(),
        protocol_version: editor_manifest.protocol_version,
        source_sha256: sha256_hex(source_bytes),
        editor_sha256: editor_manifest.payload_sha256.clone(),
        editor_payload: editor_payload.to_vec(),
        manifest,
        editor_manifest: Some(editor_manifest),
        implemented_features: options.implemented_features,
        diagnostics: vec![OfficeDiagnostic {
            level: "info".to_string(),
            code: "office.native-editor-payload.validated".to_string(),
            message: "The native Euro-Office editor payload passed Rust protocol and table-directory validation.".to_string(),
        }],
    })
}

#[derive(Debug)]
struct SpreadsheetSourceSheet {
    name: String,
    sheet_id: u32,
    visibility: u8,
    default_row_height: f64,
    columns: Vec<SpreadsheetSourceColumn>,
    rows: Vec<SpreadsheetSourceRow>,
    merged_cells: Vec<String>,
    frozen_pane: Option<EditorFrozenPaneManifest>,
    tables: Vec<EditorTableManifest>,
    data_validations: Vec<EditorDataValidationManifest>,
    conditional_formats: Vec<EditorConditionalFormatManifest>,
    protection: Option<EditorSheetProtectionManifest>,
    comments: Vec<EditorSpreadsheetCommentManifest>,
    drawings: Vec<SpreadsheetSourceDrawing>,
    print_layout: Option<SpreadsheetPrintLayout>,
    view: Option<u8>,
    pivot_tables: Vec<SpreadsheetPivotTable>,
}

#[derive(Debug, Clone)]
struct SpreadsheetPivotTable {
    cache_id: u32,
    xml: Vec<u8>,
}

#[derive(Debug, Clone)]
struct SpreadsheetPivotCache {
    id: u32,
    definition_xml: Vec<u8>,
    records_xml: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
struct SpreadsheetPrintLayout {
    fit_to_page: Option<bool>,
    margins: [Option<f64>; 6],
    paper_size: Option<u8>,
    orientation: Option<u8>,
    fit_to_height: Option<u32>,
    fit_to_width: Option<u32>,
    first_page_number: Option<u32>,
    use_first_page_number: Option<bool>,
    horizontal_dpi: Option<u32>,
    vertical_dpi: Option<u32>,
    print_options: [Option<bool>; 5],
    header_footer: [Option<String>; 6],
    align_with_margins: Option<bool>,
    row_breaks: Option<SpreadsheetBreaks>,
    col_breaks: Option<SpreadsheetBreaks>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SpreadsheetBreaks {
    count: u32,
    manual_count: u32,
    breaks: Vec<SpreadsheetBreak>,
}

#[derive(Debug, Clone, PartialEq)]
struct SpreadsheetBreak {
    id: u32,
    manual: bool,
    min: u32,
    max: u32,
}

#[derive(Debug, Clone)]
struct SpreadsheetSourceDrawing {
    name: String,
    from_col: u32,
    from_col_off_mm: f64,
    from_row: u32,
    from_row_off_mm: f64,
    to_col: u32,
    to_col_off_mm: f64,
    to_row: u32,
    to_row_off_mm: f64,
    xfrm_emu: [i32; 4],
    chart: DocumentDrawingChart,
}

#[derive(Debug, Default)]
struct SpreadsheetSourceWorkbook {
    defined_names: Vec<EditorDefinedNameManifest>,
    protection: Option<EditorWorkbookProtectionManifest>,
    pivot_caches: Vec<SpreadsheetPivotCache>,
}

#[derive(Debug)]
struct SpreadsheetSourceColumn {
    min: u32,
    max: u32,
    width: f64,
    custom_width: bool,
}

#[derive(Debug)]
struct SpreadsheetSourceRow {
    index: u32,
    height_twips: u16,
    custom_height: bool,
    hidden: bool,
    cells: Vec<SpreadsheetSourceCell>,
}

#[derive(Debug)]
struct SpreadsheetSourceCell {
    column: u32,
    style_id: u32,
    value: SpreadsheetSourceValue,
    formula: Option<String>,
}

#[derive(Debug)]
enum SpreadsheetSourceValue {
    SharedString(u32),
    String(String),
    Number(f64),
    Boolean(bool),
    Error(String),
    Blank,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SpreadsheetSourceStyles {
    number_formats: BTreeMap<u32, String>,
    fonts: Vec<SpreadsheetSourceFont>,
    fills: Vec<SpreadsheetSourceFill>,
    cell_style_xfs: Vec<SpreadsheetSourceXf>,
    cell_xfs: Vec<SpreadsheetSourceXf>,
    differential_formats: Vec<EditorDifferentialStyleManifest>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SpreadsheetSourceFont {
    bold: bool,
    italic: bool,
    color: Option<u32>,
    size: Option<f64>,
    name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SpreadsheetSourceFill {
    pattern: u8,
    foreground: Option<u32>,
    background: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SpreadsheetSourceXf {
    font_id: u32,
    fill_id: u32,
    border_id: u32,
    num_fmt_id: u32,
    xf_id: Option<u32>,
    apply_font: bool,
    apply_fill: bool,
    apply_alignment: bool,
    horizontal_alignment: Option<u8>,
}

/// First native Rust OOXML -> XLSY writer slice. It intentionally accepts only
/// the cell families implemented below and rejects formulas/errors/rich inline
/// strings instead of silently flattening them.
// ref: sdkjs/cell/model/Serialize.js:7759-7865,5283-5335,6795-6847
pub fn transcode_spreadsheet_to_editor_payload(source_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    inspect(OfficeKind::Spreadsheet, source_bytes)?;
    let (shared_strings, styles, workbook, sheets) = read_spreadsheet_source(source_bytes)?;
    let header = b"XLSY;v10;0;";
    let table_types = [1u8, 2, 3, 4, 0];
    let directory_bytes = 1 + table_types.len() * 5;
    let mut xlsb_streams = Vec::with_capacity(sheets.len());
    let mut xlsb_offsets = Vec::with_capacity(sheets.len());
    let mut next_offset = header.len() + directory_bytes;
    for sheet in &sheets {
        let stream = write_xlsb_sheet(sheet)?;
        xlsb_offsets.push(next_offset as u32);
        next_offset += stream.len();
        xlsb_streams.push(stream);
    }
    let tables = [
        (1u8, write_shared_strings_table(&shared_strings)),
        (2u8, write_styles_table(&styles)),
        (3u8, write_workbook_table(&workbook)),
        (4u8, write_worksheets_table(&sheets, &xlsb_offsets)),
        (0u8, length_prefix(&[])),
    ];
    let mut table_offset = next_offset;
    let mut directory = Vec::with_capacity(directory_bytes);
    directory.push(tables.len() as u8);
    for (table_type, table) in &tables {
        directory.push(*table_type);
        directory.extend_from_slice(&(table_offset as u32).to_le_bytes());
        table_offset += table.len();
    }
    let mut output = Vec::with_capacity(table_offset);
    output.extend_from_slice(header);
    output.extend_from_slice(&directory);
    for stream in xlsb_streams {
        output.extend_from_slice(&stream);
    }
    for (_, table) in tables {
        output.extend_from_slice(&table);
    }
    inspect_editor_payload(OfficeKind::Spreadsheet, &output)?;
    Ok(output)
}

fn read_spreadsheet_source(
    source_bytes: &[u8],
) -> anyhow::Result<(
    Vec<String>,
    SpreadsheetSourceStyles,
    SpreadsheetSourceWorkbook,
    Vec<SpreadsheetSourceSheet>,
)> {
    let mut archive =
        ZipArchive::new(Cursor::new(source_bytes)).context("open spreadsheet OOXML")?;
    let workbook = read_zip_part(&mut archive, "xl/workbook.xml")?;
    let relationships = read_zip_part(&mut archive, "xl/_rels/workbook.xml.rels")?;
    let shared_strings = match read_optional_zip_part(&mut archive, "xl/sharedStrings.xml")? {
        Some(xml) => parse_ooxml_shared_strings(&xml)?,
        None => Vec::new(),
    };
    let styles = parse_ooxml_styles(&read_zip_part(&mut archive, "xl/styles.xml")?)?;
    let relationship_document = roxmltree::Document::parse(
        std::str::from_utf8(&relationships).context("workbook relationships are not UTF-8")?,
    )
    .context("parse workbook relationships")?;
    let targets = relationship_document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Relationship")
        .filter_map(|node| {
            Some((
                node.attribute("Id")?.to_string(),
                node.attribute("Target")?.to_string(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let workbook_document = roxmltree::Document::parse(
        std::str::from_utf8(&workbook).context("workbook is not UTF-8")?,
    )
    .context("parse workbook")?;
    let workbook_source = SpreadsheetSourceWorkbook {
        defined_names: parse_ooxml_defined_names(&workbook_document)?,
        protection: parse_ooxml_workbook_protection(&workbook_document)?,
        pivot_caches: read_spreadsheet_pivot_caches(&mut archive, &workbook_document, &targets)?,
    };
    let mut sheets = Vec::new();
    for sheet in workbook_document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "sheet")
    {
        let name = sheet
            .attribute("name")
            .context("workbook sheet has no name")?
            .to_string();
        let sheet_id = sheet
            .attribute("sheetId")
            .context("workbook sheet has no sheetId")?
            .parse::<u32>()
            .context("workbook sheetId is invalid")?;
        let relationship_id = sheet
            .attributes()
            .find(|attribute| attribute.name() == "id")
            .map(|attribute| attribute.value())
            .context("workbook sheet has no relationship id")?;
        let target = targets
            .get(relationship_id)
            .with_context(|| format!("worksheet relationship is missing: {relationship_id}"))?;
        ensure!(
            !target.contains(".."),
            "unsafe worksheet relationship target: {target}"
        );
        let path = if target.starts_with("xl/") {
            target.clone()
        } else {
            format!("xl/{target}")
        };
        let worksheet = read_zip_part(&mut archive, &path)?;
        let tables = read_spreadsheet_source_tables(&mut archive, &path)?;
        let comments = read_spreadsheet_source_comments(&mut archive, &path)?;
        let drawings = read_spreadsheet_source_drawings(&mut archive, &path, &worksheet)?;
        let pivot_tables = read_spreadsheet_pivot_tables(&mut archive, &path, &worksheet)?;
        sheets.push(SpreadsheetSourceSheet {
            name,
            sheet_id,
            visibility: match sheet.attribute("state") {
                Some("hidden") => 0,
                Some("veryHidden") => 1,
                Some("visible") | None => 2,
                Some(value) => anyhow::bail!("unsupported worksheet visibility: {value}"),
            },
            default_row_height: parse_ooxml_default_row_height(&worksheet)?,
            columns: parse_ooxml_columns(&worksheet)?,
            rows: parse_ooxml_worksheet(&worksheet, shared_strings.len())?,
            merged_cells: parse_ooxml_merged_cells(&worksheet)?,
            frozen_pane: parse_ooxml_frozen_pane(&worksheet)?,
            tables,
            data_validations: parse_ooxml_data_validations(&worksheet)?,
            conditional_formats: parse_ooxml_conditional_formats(
                &worksheet,
                &styles.differential_formats,
            )?,
            protection: parse_ooxml_sheet_protection(&worksheet)?,
            comments,
            drawings,
            print_layout: parse_ooxml_print_layout(&worksheet)?,
            view: parse_ooxml_sheet_view(&worksheet)?,
            pivot_tables,
        });
    }
    ensure!(!sheets.is_empty(), "spreadsheet workbook has no sheets");
    Ok((shared_strings, styles, workbook_source, sheets))
}

fn parse_ooxml_sheet_view(xml: &[u8]) -> anyhow::Result<Option<u8>> {
    let document = roxmltree::Document::parse(std::str::from_utf8(xml)?)?;
    Ok(document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "sheetView")
        .and_then(|node| node.attribute("view"))
        .map(|value| match value {
            "normal" => Ok(0),
            "pageBreakPreview" => Ok(1),
            "pageLayout" => Ok(2),
            _ => anyhow::bail!("unsupported sheet view: {value}"),
        })
        .transpose()?)
}

fn read_spreadsheet_pivot_caches<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    workbook: &roxmltree::Document<'_>,
    targets: &BTreeMap<String, String>,
) -> anyhow::Result<Vec<SpreadsheetPivotCache>> {
    let mut caches = Vec::new();
    for node in workbook
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "pivotCache")
    {
        let id = node
            .attribute("cacheId")
            .context("pivot cache has no cacheId")?
            .parse()?;
        let relationship = node
            .attributes()
            .find(|attribute| attribute.name() == "id")
            .context("pivot cache has no relationship")?
            .value();
        let target = targets
            .get(relationship)
            .with_context(|| format!("pivot cache relationship is missing: {relationship}"))?;
        let path = if target.starts_with("xl/") {
            target.clone()
        } else {
            format!("xl/{target}")
        };
        let definition_xml = read_zip_part(archive, &path)?;
        let records_xml = spreadsheet_related_part(archive, &path, "/pivotCacheRecords")?
            .map(|record_path| read_zip_part(archive, &record_path))
            .transpose()?;
        caches.push(SpreadsheetPivotCache {
            id,
            definition_xml,
            records_xml,
        });
    }
    Ok(caches)
}

fn read_spreadsheet_pivot_tables<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    worksheet_xml: &[u8],
) -> anyhow::Result<Vec<SpreadsheetPivotTable>> {
    let document = roxmltree::Document::parse(std::str::from_utf8(worksheet_xml)?)?;
    let mut result = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "pivotTablePart")
    {
        let id = node
            .attributes()
            .find(|attribute| attribute.name() == "id")
            .context("pivot table part has no relationship")?
            .value();
        let path = spreadsheet_relationship_target(archive, worksheet_path, id)?;
        let xml = read_zip_part(archive, &path)?;
        let parsed = roxmltree::Document::parse(std::str::from_utf8(&xml)?)?;
        let cache_id = parsed
            .root_element()
            .attribute("cacheId")
            .context("pivot table has no cacheId")?
            .parse()?;
        result.push(SpreadsheetPivotTable { cache_id, xml });
    }
    Ok(result)
}

fn spreadsheet_related_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    source_path: &str,
    relationship_suffix: &str,
) -> anyhow::Result<Option<String>> {
    let source = Path::new(source_path);
    let parent = source
        .parent()
        .and_then(|v| v.to_str())
        .context("related part parent missing")?;
    let filename = source
        .file_name()
        .and_then(|v| v.to_str())
        .context("related part filename missing")?;
    let rels_path = format!("{parent}/_rels/{filename}.rels");
    let Some(rels) = read_optional_zip_part(archive, &rels_path)? else {
        return Ok(None);
    };
    let document = roxmltree::Document::parse(std::str::from_utf8(&rels)?)?;
    let Some(target) = document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node
                    .attribute("Type")
                    .is_some_and(|value| value.ends_with(relationship_suffix))
        })
        .and_then(|node| node.attribute("Target"))
    else {
        return Ok(None);
    };
    Ok(Some(normalize_ooxml_relationship_target(parent, target)?))
}

fn spreadsheet_relationship_target<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    source_path: &str,
    relationship_id: &str,
) -> anyhow::Result<String> {
    let source = Path::new(source_path);
    let parent = source
        .parent()
        .and_then(|v| v.to_str())
        .context("relationship parent missing")?;
    let filename = source
        .file_name()
        .and_then(|v| v.to_str())
        .context("relationship filename missing")?;
    let rels = read_zip_part(archive, &format!("{parent}/_rels/{filename}.rels"))?;
    let document = roxmltree::Document::parse(std::str::from_utf8(&rels)?)?;
    let target = document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node.attribute("Id") == Some(relationship_id)
        })
        .and_then(|node| node.attribute("Target"))
        .context("relationship target missing")?;
    normalize_ooxml_relationship_target(parent, target)
}

fn parse_ooxml_print_layout(xml: &[u8]) -> anyhow::Result<Option<SpreadsheetPrintLayout>> {
    let text = std::str::from_utf8(xml).context("worksheet print layout is not UTF-8")?;
    let document = roxmltree::Document::parse(text).context("parse worksheet print layout")?;
    let element = |name: &str| {
        document
            .descendants()
            .find(move |node| node.is_element() && node.tag_name().name() == name)
    };
    if [
        "pageMargins",
        "pageSetup",
        "printOptions",
        "headerFooter",
        "rowBreaks",
        "colBreaks",
    ]
    .iter()
    .all(|name| element(name).is_none())
    {
        return Ok(None);
    }
    let mut result = SpreadsheetPrintLayout::default();
    if let Some(node) = element("pageSetUpPr") {
        result.fit_to_page = node
            .attribute("fitToPage")
            .map(parse_ooxml_bool)
            .transpose()?;
    }
    if let Some(node) = element("pageMargins") {
        for (index, name) in ["left", "top", "right", "bottom", "header", "footer"]
            .iter()
            .enumerate()
        {
            result.margins[index] = node
                .attribute(*name)
                .map(str::parse)
                .transpose()
                .with_context(|| format!("invalid page margin {name}"))?;
        }
    }
    if let Some(node) = element("pageSetup") {
        result.paper_size = node.attribute("paperSize").map(str::parse).transpose()?;
        result.orientation = match node.attribute("orientation") {
            Some("landscape") => Some(0),
            Some("portrait") => Some(1),
            None => None,
            Some(value) => anyhow::bail!("unsupported page orientation: {value}"),
        };
        result.fit_to_height = node.attribute("fitToHeight").map(str::parse).transpose()?;
        result.fit_to_width = node.attribute("fitToWidth").map(str::parse).transpose()?;
        result.first_page_number = node
            .attribute("firstPageNumber")
            .map(str::parse)
            .transpose()?;
        result.use_first_page_number = node
            .attribute("useFirstPageNumber")
            .map(parse_ooxml_bool)
            .transpose()?;
        result.horizontal_dpi = node
            .attribute("horizontalDpi")
            .map(str::parse)
            .transpose()?;
        result.vertical_dpi = node.attribute("verticalDpi").map(str::parse).transpose()?;
    }
    if let Some(node) = element("printOptions") {
        for (index, name) in [
            "gridLines",
            "headings",
            "gridLinesSet",
            "horizontalCentered",
            "verticalCentered",
        ]
        .iter()
        .enumerate()
        {
            result.print_options[index] =
                node.attribute(*name).map(parse_ooxml_bool).transpose()?;
        }
    }
    if let Some(node) = element("headerFooter") {
        result.align_with_margins = node
            .attribute("alignWithMargins")
            .map(parse_ooxml_bool)
            .transpose()?;
        for (index, name) in [
            "evenFooter",
            "evenHeader",
            "firstFooter",
            "firstHeader",
            "oddFooter",
            "oddHeader",
        ]
        .iter()
        .enumerate()
        {
            result.header_footer[index] = node
                .children()
                .find(|child| child.is_element() && child.tag_name().name() == *name)
                .and_then(|child| child.text())
                .map(str::to_string);
        }
    }
    result.row_breaks = parse_ooxml_breaks(&document, "rowBreaks")?;
    result.col_breaks = parse_ooxml_breaks(&document, "colBreaks")?;
    Ok(Some(result))
}

fn parse_ooxml_bool(value: &str) -> anyhow::Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => anyhow::bail!("invalid OOXML boolean: {value}"),
    }
}

fn parse_ooxml_breaks(
    document: &roxmltree::Document<'_>,
    name: &str,
) -> anyhow::Result<Option<SpreadsheetBreaks>> {
    let Some(node) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == name)
    else {
        return Ok(None);
    };
    let mut result = SpreadsheetBreaks {
        count: node.attribute("count").unwrap_or("0").parse()?,
        manual_count: node.attribute("manualBreakCount").unwrap_or("0").parse()?,
        breaks: Vec::new(),
    };
    for child in node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "brk")
    {
        result.breaks.push(SpreadsheetBreak {
            id: child
                .attribute("id")
                .context("print break has no id")?
                .parse()?,
            manual: child
                .attribute("man")
                .map(parse_ooxml_bool)
                .transpose()?
                .unwrap_or(false),
            min: child.attribute("min").unwrap_or("0").parse()?,
            max: child.attribute("max").unwrap_or("0").parse()?,
        });
    }
    Ok(Some(result))
}

fn read_spreadsheet_source_drawings<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    worksheet_xml: &[u8],
) -> anyhow::Result<Vec<SpreadsheetSourceDrawing>> {
    let worksheet_document = roxmltree::Document::parse(
        std::str::from_utf8(worksheet_xml).context("worksheet is not UTF-8")?,
    )?;
    let Some(drawing_id) = worksheet_document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "drawing")
        .and_then(|node| node.attributes().find(|attribute| attribute.name() == "id"))
        .map(|attribute| attribute.value().to_string())
    else {
        return Ok(Vec::new());
    };
    let worksheet = Path::new(worksheet_path);
    let filename = worksheet
        .file_name()
        .and_then(|value| value.to_str())
        .context("worksheet path has no filename")?;
    let parent = worksheet
        .parent()
        .and_then(|value| value.to_str())
        .context("worksheet path has no parent")?;
    let relationships_path = format!("{parent}/_rels/{filename}.rels");
    let relationships = read_zip_part(archive, &relationships_path)?;
    let relationships_document = roxmltree::Document::parse(
        std::str::from_utf8(&relationships).context("worksheet relationships are not UTF-8")?,
    )?;
    let target = relationships_document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node.attribute("Id") == Some(drawing_id.as_str())
        })
        .and_then(|node| node.attribute("Target"))
        .context("worksheet drawing relationship is missing")?;
    let drawing_path = normalize_ooxml_relationship_target(parent, target)?;
    let drawing_xml = read_zip_part(archive, &drawing_path)?;
    let drawing_document = roxmltree::Document::parse(
        std::str::from_utf8(&drawing_xml).context("spreadsheet drawing is not UTF-8")?,
    )?;
    let drawing_parent = Path::new(&drawing_path)
        .parent()
        .and_then(|value| value.to_str())
        .context("drawing path has no parent")?;
    let drawing_filename = Path::new(&drawing_path)
        .file_name()
        .and_then(|value| value.to_str())
        .context("drawing path has no filename")?;
    let drawing_relationships_path = format!("{drawing_parent}/_rels/{drawing_filename}.rels");
    let drawing_relationships = read_zip_part(archive, &drawing_relationships_path)?;
    let drawing_relationships_document = roxmltree::Document::parse(
        std::str::from_utf8(&drawing_relationships)
            .context("drawing relationships are not UTF-8")?,
    )?;
    let mut drawings = Vec::new();
    for anchor in drawing_document
        .descendants()
        .filter(|node| node.is_element() && matches!(node.tag_name().name(), "twoCellAnchor"))
    {
        let graphic_frame = anchor
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "graphicFrame")
            .context("chart anchor has no graphicFrame")?;
        let chart_id = graphic_frame
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "chart")
            .and_then(|node| node.attributes().find(|attribute| attribute.name() == "id"))
            .map(|attribute| attribute.value().to_string())
            .context("chart graphicFrame has no relationship id")?;
        let chart_target = drawing_relationships_document
            .descendants()
            .find(|node| {
                node.is_element()
                    && node.tag_name().name() == "Relationship"
                    && node.attribute("Id") == Some(chart_id.as_str())
            })
            .and_then(|node| node.attribute("Target"))
            .context("chart relationship is missing")?;
        let chart_path = normalize_ooxml_relationship_target(drawing_parent, chart_target)?;
        let chart = parse_document_drawing_chart(&read_zip_part(archive, &chart_path)?)?;
        let point = |name: &str| -> anyhow::Result<(u32, f64, u32, f64)> {
            let node = anchor
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == name)
                .with_context(|| format!("chart anchor has no {name}"))?;
            let value = |child_name: &str| -> anyhow::Result<u32> {
                node.children()
                    .find(|child| child.is_element() && child.tag_name().name() == child_name)
                    .and_then(|child| child.text())
                    .with_context(|| format!("chart anchor {name} has no {child_name}"))?
                    .parse()
                    .context("chart anchor coordinate is invalid")
            };
            Ok((
                value("col")?,
                f64::from(value("colOff")?) / 36_000.0,
                value("row")?,
                f64::from(value("rowOff")?) / 36_000.0,
            ))
        };
        let (from_col, from_col_off_mm, from_row, from_row_off_mm) = point("from")?;
        let (to_col, to_col_off_mm, to_row, to_row_off_mm) = point("to")?;
        let name = graphic_frame
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "cNvPr")
            .and_then(|node| node.attribute("name"))
            .unwrap_or("Chart")
            .to_string();
        let transform = graphic_frame
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "xfrm")
            .context("chart graphicFrame has no transform")?;
        let off = transform
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "off")
            .context("chart transform has no offset")?;
        let ext = transform
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "ext")
            .context("chart transform has no extent")?;
        let xfrm_emu = [
            off.attribute("x")
                .context("chart transform has no x")?
                .parse()?,
            off.attribute("y")
                .context("chart transform has no y")?
                .parse()?,
            ext.attribute("cx")
                .context("chart transform has no cx")?
                .parse()?,
            ext.attribute("cy")
                .context("chart transform has no cy")?
                .parse()?,
        ];
        drawings.push(SpreadsheetSourceDrawing {
            name,
            from_col,
            from_col_off_mm,
            from_row,
            from_row_off_mm,
            to_col,
            to_col_off_mm,
            to_row,
            to_row_off_mm,
            xfrm_emu,
            chart,
        });
    }
    Ok(drawings)
}

fn read_spreadsheet_source_tables<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
) -> anyhow::Result<Vec<EditorTableManifest>> {
    let worksheet = Path::new(worksheet_path);
    let filename = worksheet
        .file_name()
        .and_then(|value| value.to_str())
        .context("worksheet path has no filename")?;
    let parent = worksheet
        .parent()
        .and_then(|value| value.to_str())
        .context("worksheet path has no parent")?;
    let relationships_path = format!("{parent}/_rels/{filename}.rels");
    let Some(relationships) = read_optional_zip_part(archive, &relationships_path)? else {
        return Ok(Vec::new());
    };
    let document = roxmltree::Document::parse(
        std::str::from_utf8(&relationships).context("worksheet relationships are not UTF-8")?,
    )?;
    let mut tables = Vec::new();
    for relationship in document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "Relationship"
            && node
                .attribute("Type")
                .is_some_and(|value| value.ends_with("/table"))
    }) {
        let target = relationship
            .attribute("Target")
            .context("table relationship has no target")?;
        let path = normalize_ooxml_relationship_target(parent, target)?;
        tables.push(parse_ooxml_table(&read_zip_part(archive, &path)?)?);
    }
    Ok(tables)
}

fn read_spreadsheet_source_comments<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
) -> anyhow::Result<Vec<EditorSpreadsheetCommentManifest>> {
    let worksheet = Path::new(worksheet_path);
    let filename = worksheet
        .file_name()
        .and_then(|value| value.to_str())
        .context("worksheet path has no filename")?;
    let parent = worksheet
        .parent()
        .and_then(|value| value.to_str())
        .context("worksheet path has no parent")?;
    let relationships_path = format!("{parent}/_rels/{filename}.rels");
    let Some(relationships) = read_optional_zip_part(archive, &relationships_path)? else {
        return Ok(Vec::new());
    };
    let relationships = roxmltree::Document::parse(
        std::str::from_utf8(&relationships).context("worksheet relationships are not UTF-8")?,
    )?;
    let Some(target) = relationships
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node
                    .attribute("Type")
                    .is_some_and(|value| value.ends_with("/comments"))
        })
        .and_then(|node| node.attribute("Target"))
    else {
        return Ok(Vec::new());
    };
    let path = normalize_ooxml_relationship_target(parent, target)?;
    let comments_xml = read_zip_part(archive, &path)?;
    let document = roxmltree::Document::parse(
        std::str::from_utf8(&comments_xml).context("spreadsheet comments are not UTF-8")?,
    )?;
    let authors = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "author")
        .map(|node| node.text().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "comment")
        .enumerate()
        .map(|(index, node)| {
            let reference = node
                .attribute("ref")
                .context("spreadsheet comment has no reference")?
                .to_string();
            let author_id = node.attribute("authorId").unwrap_or("0").parse::<usize>()?;
            let text = node
                .descendants()
                .filter(|child| child.is_element() && child.tag_name().name() == "t")
                .filter_map(|child| child.text())
                .collect::<String>();
            Ok(EditorSpreadsheetCommentManifest {
                reference,
                text,
                author: authors.get(author_id).cloned().unwrap_or_default(),
                guid: format!("00000000-0000-4000-8000-{index:012x}"),
            })
        })
        .collect()
}

fn parse_ooxml_defined_names(
    document: &roxmltree::Document<'_>,
) -> anyhow::Result<Vec<EditorDefinedNameManifest>> {
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "definedName")
        .map(|node| {
            Ok(EditorDefinedNameManifest {
                name: node
                    .attribute("name")
                    .context("defined name has no name")?
                    .to_string(),
                reference: node.text().unwrap_or_default().to_string(),
                local_sheet_id: node.attribute("localSheetId").map(str::parse).transpose()?,
                hidden: parse_ooxml_bool_attribute(node, "hidden", false)?,
            })
        })
        .collect()
}

fn parse_ooxml_workbook_protection(
    document: &roxmltree::Document<'_>,
) -> anyhow::Result<Option<EditorWorkbookProtectionManifest>> {
    let Some(node) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "workbookProtection")
    else {
        return Ok(None);
    };
    Ok(Some(EditorWorkbookProtectionManifest {
        lock_structure: parse_ooxml_bool_attribute(node, "lockStructure", false)?,
        lock_windows: parse_ooxml_bool_attribute(node, "lockWindows", false)?,
        lock_revision: parse_ooxml_bool_attribute(node, "lockRevision", false)?,
        password: node.attribute("workbookPassword").map(str::to_string),
    }))
}

fn parse_ooxml_sheet_protection(
    xml: &[u8],
) -> anyhow::Result<Option<EditorSheetProtectionManifest>> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)?;
    let Some(node) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "sheetProtection")
    else {
        return Ok(None);
    };
    Ok(Some(EditorSheetProtectionManifest {
        password: node.attribute("password").map(str::to_string),
        sheet: parse_ooxml_bool_attribute(node, "sheet", false)?,
        objects: parse_ooxml_bool_attribute(node, "objects", false)?,
        scenarios: parse_ooxml_bool_attribute(node, "scenarios", false)?,
        format_cells: parse_ooxml_bool_attribute(node, "formatCells", false)?,
        format_columns: parse_ooxml_bool_attribute(node, "formatColumns", false)?,
        format_rows: parse_ooxml_bool_attribute(node, "formatRows", false)?,
        insert_columns: parse_ooxml_bool_attribute(node, "insertColumns", false)?,
        insert_hyperlinks: parse_ooxml_bool_attribute(node, "insertHyperlinks", false)?,
        insert_rows: parse_ooxml_bool_attribute(node, "insertRows", false)?,
        delete_columns: parse_ooxml_bool_attribute(node, "deleteColumns", false)?,
        delete_rows: parse_ooxml_bool_attribute(node, "deleteRows", false)?,
        select_locked_cells: parse_ooxml_bool_attribute(node, "selectLockedCells", false)?,
        sort: parse_ooxml_bool_attribute(node, "sort", false)?,
        auto_filter: parse_ooxml_bool_attribute(node, "autoFilter", false)?,
        pivot_tables: parse_ooxml_bool_attribute(node, "pivotTables", false)?,
        select_unlocked_cells: parse_ooxml_bool_attribute(node, "selectUnlockedCells", false)?,
    }))
}

fn parse_ooxml_bool_attribute(
    node: roxmltree::Node<'_, '_>,
    name: &str,
    default: bool,
) -> anyhow::Result<bool> {
    match node.attribute(name) {
        None => Ok(default),
        Some("1" | "true" | "on") => Ok(true),
        Some("0" | "false" | "off") => Ok(false),
        Some(value) => anyhow::bail!("invalid OOXML boolean {name}={value}"),
    }
}

fn normalize_ooxml_relationship_target(parent: &str, target: &str) -> anyhow::Result<String> {
    ensure!(
        !target.starts_with('/'),
        "absolute OOXML relationship target is unsafe"
    );
    let mut components = parent.split('/').map(str::to_string).collect::<Vec<_>>();
    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                ensure!(
                    components.len() > 1,
                    "OOXML relationship escapes xl package root"
                );
                components.pop();
            }
            value => components.push(value.to_string()),
        }
    }
    let path = components.join("/");
    ensure!(
        path.starts_with("xl/"),
        "OOXML relationship escapes xl package root"
    );
    Ok(path)
}

fn parse_ooxml_table(xml: &[u8]) -> anyhow::Result<EditorTableManifest> {
    let document = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("spreadsheet table is not UTF-8")?,
    )?;
    let root = document.root_element();
    let auto_filter = root
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "autoFilter");
    let filters = auto_filter
        .into_iter()
        .flat_map(|node| node.children())
        .filter(|node| node.is_element() && node.tag_name().name() == "filterColumn")
        .map(|column| {
            Ok(EditorFilterColumnManifest {
                column_id: column.attribute("colId").unwrap_or("0").parse()?,
                values: column
                    .descendants()
                    .filter(|node| node.is_element() && node.tag_name().name() == "filter")
                    .filter_map(|node| node.attribute("val").map(str::to_string))
                    .collect(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let sort_node = auto_filter
        .and_then(|node| {
            node.children()
                .find(|child| child.is_element() && child.tag_name().name() == "sortState")
        })
        .or_else(|| {
            root.children()
                .find(|node| node.is_element() && node.tag_name().name() == "sortState")
        });
    let sort = sort_node.map(|node| {
        let condition = node
            .descendants()
            .find(|child| child.is_element() && child.tag_name().name() == "sortCondition");
        EditorSortManifest {
            reference: node.attribute("ref").unwrap_or("").to_string(),
            condition_reference: condition
                .and_then(|value| value.attribute("ref"))
                .unwrap_or("")
                .to_string(),
            descending: condition
                .and_then(|value| value.attribute("descending"))
                .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        }
    });
    let style = root
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "tableStyleInfo");
    Ok(EditorTableManifest {
        reference: root.attribute("ref").unwrap_or("").to_string(),
        display_name: root
            .attribute("displayName")
            .or_else(|| root.attribute("name"))
            .unwrap_or("")
            .to_string(),
        style_name: style
            .and_then(|node| node.attribute("name"))
            .unwrap_or("")
            .to_string(),
        show_column_stripes: style
            .and_then(|node| node.attribute("showColumnStripes"))
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        show_row_stripes: style
            .and_then(|node| node.attribute("showRowStripes"))
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        show_first_column: style
            .and_then(|node| node.attribute("showFirstColumn"))
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        show_last_column: style
            .and_then(|node| node.attribute("showLastColumn"))
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        columns: root
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "tableColumn")
            .filter_map(|node| node.attribute("name").map(str::to_string))
            .collect(),
        filters,
        sort,
    })
}

fn parse_ooxml_merged_cells(xml: &[u8]) -> anyhow::Result<Vec<String>> {
    let document = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("spreadsheet worksheet is not UTF-8")?,
    )?;
    Ok(document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "mergeCell")
        .filter_map(|node| node.attribute("ref").map(str::to_string))
        .collect())
}

fn parse_ooxml_frozen_pane(xml: &[u8]) -> anyhow::Result<Option<EditorFrozenPaneManifest>> {
    let document = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("spreadsheet worksheet is not UTF-8")?,
    )?;
    let Some(pane) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "pane")
    else {
        return Ok(None);
    };
    if pane.attribute("state") != Some("frozen") {
        return Ok(None);
    }
    Ok(Some(EditorFrozenPaneManifest {
        active_pane: pane.attribute("activePane").unwrap_or("").to_string(),
        state: "frozen".to_string(),
        top_left_cell: pane.attribute("topLeftCell").unwrap_or("A1").to_string(),
        x_split: pane.attribute("xSplit").unwrap_or("0").parse()?,
        y_split: pane.attribute("ySplit").unwrap_or("0").parse()?,
    }))
}

fn parse_ooxml_data_validations(xml: &[u8]) -> anyhow::Result<Vec<EditorDataValidationManifest>> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)?;
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "dataValidation")
        .map(|node| {
            let text = |name: &str| {
                node.children()
                    .find(|child| child.is_element() && child.tag_name().name() == name)
                    .and_then(|child| child.text())
                    .map(str::to_string)
            };
            Ok(EditorDataValidationManifest {
                reference: node
                    .attribute("sqref")
                    .context("data validation has no sqref")?
                    .to_string(),
                validation_type: node.attribute("type").unwrap_or("none").to_string(),
                operator: node.attribute("operator").map(str::to_string),
                allow_blank: ooxml_bool_attribute(node.attribute("allowBlank")),
                show_error_message: ooxml_bool_attribute(node.attribute("showErrorMessage")),
                error_style: node.attribute("errorStyle").map(str::to_string),
                error_title: node.attribute("errorTitle").map(str::to_string),
                error: node.attribute("error").map(str::to_string),
                formula1: text("formula1"),
                formula2: text("formula2"),
            })
        })
        .collect()
}

fn parse_ooxml_conditional_formats(
    xml: &[u8],
    differential_formats: &[EditorDifferentialStyleManifest],
) -> anyhow::Result<Vec<EditorConditionalFormatManifest>> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)?;
    let mut output = Vec::new();
    for container in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "conditionalFormatting")
    {
        let reference = container
            .attribute("sqref")
            .context("conditional formatting has no sqref")?;
        for rule in container
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "cfRule")
        {
            let mut thresholds = Vec::new();
            let mut colors = Vec::new();
            if let Some(scale) = rule
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "colorScale")
            {
                for threshold in scale
                    .children()
                    .filter(|node| node.is_element() && node.tag_name().name() == "cfvo")
                {
                    thresholds.push(EditorConditionalThresholdManifest {
                        threshold_type: threshold
                            .attribute("type")
                            .context("conditional threshold has no type")?
                            .to_string(),
                        value: threshold.attribute("val").map(str::to_string),
                    });
                }
                colors.extend(
                    scale
                        .children()
                        .filter(|node| node.is_element() && node.tag_name().name() == "color")
                        .filter_map(|node| node.attribute("rgb"))
                        .map(str::to_string),
                );
            }
            let dxf = rule
                .attribute("dxfId")
                .map(str::parse::<usize>)
                .transpose()?
                .map(|index| {
                    differential_formats
                        .get(index)
                        .cloned()
                        .with_context(|| format!("conditional dxfId is out of range: {index}"))
                })
                .transpose()?;
            output.push(EditorConditionalFormatManifest {
                reference: reference.to_string(),
                rule_type: rule
                    .attribute("type")
                    .context("conditional rule has no type")?
                    .to_string(),
                priority: rule.attribute("priority").unwrap_or("0").parse()?,
                operator: rule.attribute("operator").map(str::to_string),
                formulas: rule
                    .children()
                    .filter(|node| node.is_element() && node.tag_name().name() == "formula")
                    .filter_map(|node| node.text())
                    .map(str::to_string)
                    .collect(),
                thresholds,
                colors,
                differential_style: dxf,
            });
        }
    }
    Ok(output)
}

fn ooxml_bool_attribute(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}

fn parse_ooxml_styles(xml: &[u8]) -> anyhow::Result<SpreadsheetSourceStyles> {
    let document = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("spreadsheet styles are not UTF-8")?,
    )
    .context("parse spreadsheet styles")?;
    let root = document.root_element();
    let child = |name: &str| {
        root.children()
            .find(|node| node.is_element() && node.tag_name().name() == name)
    };
    let fonts = child("fonts")
        .into_iter()
        .flat_map(|node| node.children())
        .filter(|node| node.is_element() && node.tag_name().name() == "font")
        .map(|font| {
            Ok(SpreadsheetSourceFont {
                bold: font
                    .children()
                    .any(|node| node.is_element() && node.tag_name().name() == "b"),
                italic: font
                    .children()
                    .any(|node| node.is_element() && node.tag_name().name() == "i"),
                color: font
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "color")
                    .and_then(|node| node.attribute("rgb"))
                    .map(parse_argb)
                    .transpose()?,
                size: font
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "sz")
                    .and_then(|node| node.attribute("val"))
                    .map(str::parse)
                    .transpose()?,
                name: font
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "name")
                    .and_then(|node| node.attribute("val"))
                    .map(str::to_string),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let fills = child("fills")
        .into_iter()
        .flat_map(|node| node.children())
        .filter(|node| node.is_element() && node.tag_name().name() == "fill")
        .map(|fill| {
            let pattern = fill
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "patternFill")
                .context("only pattern fills are implemented")?;
            let color = |name: &str| -> anyhow::Result<Option<u32>> {
                pattern
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == name)
                    .and_then(|node| node.attribute("rgb"))
                    .map(parse_argb)
                    .transpose()
            };
            Ok(SpreadsheetSourceFill {
                pattern: match pattern.attribute("patternType").unwrap_or("none") {
                    "none" => 17,
                    "gray125" => 8,
                    "solid" => 18,
                    value => anyhow::bail!("unsupported spreadsheet fill pattern: {value}"),
                },
                foreground: color("fgColor")?,
                background: color("bgColor")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let parse_xfs =
        |container: Option<roxmltree::Node<'_, '_>>| -> anyhow::Result<Vec<SpreadsheetSourceXf>> {
            container
                .into_iter()
                .flat_map(|node| node.children())
                .filter(|node| node.is_element() && node.tag_name().name() == "xf")
                .map(|xf| {
                    let alignment = xf
                        .children()
                        .find(|node| node.is_element() && node.tag_name().name() == "alignment");
                    Ok(SpreadsheetSourceXf {
                        font_id: xf.attribute("fontId").unwrap_or("0").parse()?,
                        fill_id: xf.attribute("fillId").unwrap_or("0").parse()?,
                        border_id: xf.attribute("borderId").unwrap_or("0").parse()?,
                        num_fmt_id: xf.attribute("numFmtId").unwrap_or("0").parse()?,
                        xf_id: xf.attribute("xfId").map(str::parse).transpose()?,
                        apply_font: xf.attribute("applyFont") == Some("1"),
                        apply_fill: xf.attribute("applyFill") == Some("1"),
                        apply_alignment: xf.attribute("applyAlignment") == Some("1"),
                        horizontal_alignment: alignment
                            .and_then(|node| node.attribute("horizontal"))
                            .map(|value| match value {
                                "center" => Ok(0),
                                "justify" => Ok(5),
                                "left" => Ok(6),
                                "right" => Ok(7),
                                "centerContinuous" => Ok(8),
                                other => anyhow::bail!("unsupported horizontal alignment: {other}"),
                            })
                            .transpose()?,
                    })
                })
                .collect()
        };
    let styles = SpreadsheetSourceStyles {
        number_formats: child("numFmts")
            .into_iter()
            .flat_map(|node| node.children())
            .filter(|node| node.is_element() && node.tag_name().name() == "numFmt")
            .map(|node| {
                Ok((
                    node.attribute("numFmtId")
                        .context("number format has no id")?
                        .parse()?,
                    node.attribute("formatCode")
                        .context("number format has no code")?
                        .to_string(),
                ))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()?,
        fonts,
        fills,
        cell_style_xfs: parse_xfs(child("cellStyleXfs"))?,
        cell_xfs: parse_xfs(child("cellXfs"))?,
        differential_formats: child("dxfs")
            .into_iter()
            .flat_map(|node| node.children())
            .filter(|node| node.is_element() && node.tag_name().name() == "dxf")
            .map(|dxf| {
                let fill_rgb = dxf
                    .descendants()
                    .find(|node| node.is_element() && node.tag_name().name() == "fgColor")
                    .and_then(|node| node.attribute("rgb"))
                    .map(str::to_string);
                let font_rgb = dxf
                    .children()
                    .find(|node| node.is_element() && node.tag_name().name() == "font")
                    .and_then(|font| {
                        font.children()
                            .find(|node| node.is_element() && node.tag_name().name() == "color")
                    })
                    .and_then(|node| node.attribute("rgb"))
                    .map(str::to_string);
                Ok(EditorDifferentialStyleManifest { fill_rgb, font_rgb })
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
    };
    ensure!(!styles.fonts.is_empty(), "spreadsheet styles have no fonts");
    ensure!(!styles.fills.is_empty(), "spreadsheet styles have no fills");
    ensure!(
        !styles.cell_xfs.is_empty(),
        "spreadsheet styles have no cellXfs"
    );
    Ok(styles)
}

fn parse_argb(value: &str) -> anyhow::Result<u32> {
    ensure!(value.len() == 8, "ARGB color must contain eight hex digits");
    u32::from_str_radix(value, 16).context("ARGB color is invalid")
}

fn parse_ooxml_default_row_height(xml: &[u8]) -> anyhow::Result<f64> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)?;
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "sheetFormatPr")
        .and_then(|node| node.attribute("defaultRowHeight"))
        .unwrap_or("15")
        .parse::<f64>()
        .context("worksheet default row height is invalid")
}

fn parse_ooxml_columns(xml: &[u8]) -> anyhow::Result<Vec<SpreadsheetSourceColumn>> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)?;
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "col")
        .map(|node| {
            Ok(SpreadsheetSourceColumn {
                min: node
                    .attribute("min")
                    .context("worksheet column has no min")?
                    .parse()?,
                max: node
                    .attribute("max")
                    .context("worksheet column has no max")?
                    .parse()?,
                width: node
                    .attribute("width")
                    .context("worksheet column has no width")?
                    .parse()?,
                custom_width: node.attribute("customWidth") == Some("1"),
            })
        })
        .collect()
}

fn read_zip_part<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut entry = archive
        .by_name(path)
        .with_context(|| format!("read OOXML part {path}"))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_optional_zip_part<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    match archive.by_name(path) {
        Ok(mut entry) => {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read optional OOXML part {path}")),
    }
}

fn parse_ooxml_shared_strings(xml: &[u8]) -> anyhow::Result<Vec<String>> {
    let document = roxmltree::Document::parse(
        std::str::from_utf8(xml).context("shared strings are not UTF-8")?,
    )
    .context("parse shared strings")?;
    Ok(document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "si")
        .map(|node| {
            node.descendants()
                .filter(|child| child.is_element() && child.tag_name().name() == "t")
                .filter_map(|child| child.text())
                .collect::<String>()
        })
        .collect())
}

fn parse_ooxml_worksheet(
    xml: &[u8],
    shared_string_count: usize,
) -> anyhow::Result<Vec<SpreadsheetSourceRow>> {
    let document =
        roxmltree::Document::parse(std::str::from_utf8(xml).context("worksheet is not UTF-8")?)
            .context("parse worksheet")?;
    let mut rows = Vec::new();
    for row in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "row")
    {
        let index = row
            .attribute("r")
            .context("worksheet row has no index")?
            .parse::<u32>()
            .context("worksheet row index is invalid")?
            .checked_sub(1)
            .context("worksheet row index must be one-based")?;
        let height_twips = row
            .attribute("ht")
            .map(|value| {
                value
                    .parse::<f64>()
                    .context("worksheet row height is invalid")
            })
            .transpose()?
            .map(|value| (value * 20.0).round() as u16)
            .unwrap_or(0);
        let mut cells = Vec::new();
        for cell in row
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "c")
        {
            let formula = cell
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "f")
                .and_then(|node| node.text())
                .map(str::to_string);
            let reference = cell
                .attribute("r")
                .context("worksheet cell has no reference")?;
            let column = parse_cell_column(reference)?;
            let style_id = cell.attribute("s").unwrap_or("0").parse::<u32>()?;
            let raw = cell
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "v")
                .and_then(|node| node.text())
                .unwrap_or("");
            let value = match cell.attribute("t") {
                Some("s") => {
                    let index = raw
                        .parse::<u32>()
                        .context("shared string index is invalid")?;
                    ensure!(
                        (index as usize) < shared_string_count,
                        "shared string index is out of range"
                    );
                    SpreadsheetSourceValue::SharedString(index)
                }
                Some("b") => SpreadsheetSourceValue::Boolean(raw == "1"),
                Some("e") => SpreadsheetSourceValue::Error(raw.to_string()),
                Some("str") => SpreadsheetSourceValue::String(raw.to_string()),
                Some(value) => anyhow::bail!("unsupported OOXML cell type in XLSY writer: {value}"),
                None if raw.is_empty() => SpreadsheetSourceValue::Blank,
                None => SpreadsheetSourceValue::Number(
                    raw.parse::<f64>().context("numeric cell is invalid")?,
                ),
            };
            cells.push(SpreadsheetSourceCell {
                column,
                style_id,
                value,
                formula,
            });
        }
        rows.push(SpreadsheetSourceRow {
            index,
            height_twips,
            custom_height: row.attribute("customHeight") == Some("1"),
            hidden: matches!(row.attribute("hidden"), Some("1" | "true")),
            cells,
        });
    }
    Ok(rows)
}

fn parse_cell_column(reference: &str) -> anyhow::Result<u32> {
    let mut column = 0u32;
    let mut letters = 0usize;
    for byte in reference.bytes() {
        if !byte.is_ascii_alphabetic() {
            break;
        }
        column = column
            .checked_mul(26)
            .and_then(|value| value.checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1)))
            .context("cell column overflow")?;
        letters += 1;
    }
    ensure!(
        letters > 0 && column > 0,
        "invalid cell reference: {reference}"
    );
    Ok(column - 1)
}

fn write_xlsb_sheet(sheet: &SpreadsheetSourceSheet) -> anyhow::Result<Vec<u8>> {
    let mut output = Vec::new();
    write_xlsb_record(&mut output, 145, &[]);
    for row in &sheet.rows {
        let mut row_payload = Vec::with_capacity(17);
        row_payload.extend_from_slice(&(row.index & 0x000f_ffff).to_le_bytes());
        row_payload.extend_from_slice(&0u32.to_le_bytes());
        row_payload.extend_from_slice(&row.height_twips.to_le_bytes());
        row_payload.push(0);
        row_payload
            .push(if row.custom_height { 0x20 } else { 0 } | if row.hidden { 0x10 } else { 0 });
        row_payload.push(0);
        row_payload.extend_from_slice(&0u32.to_le_bytes());
        write_xlsb_record(&mut output, 0, &row_payload);
        for cell in &row.cells {
            let formula = cell.formula.as_deref();
            let (record_type, value) = match (&cell.value, formula.is_some()) {
                (SpreadsheetSourceValue::SharedString(index), false) => {
                    (7, index.to_le_bytes().to_vec())
                }
                (SpreadsheetSourceValue::String(value), false) => {
                    let mut bytes = Vec::new();
                    write_xlsb_string(&mut bytes, value);
                    (6, bytes)
                }
                (SpreadsheetSourceValue::Number(number), false) => {
                    (5, number.to_le_bytes().to_vec())
                }
                (SpreadsheetSourceValue::Boolean(value), false) => (4, vec![u8::from(*value)]),
                (SpreadsheetSourceValue::Error(value), false) => {
                    (3, vec![spreadsheet_error_code(value)])
                }
                (SpreadsheetSourceValue::Blank, false) => (1, Vec::new()),
                (SpreadsheetSourceValue::Number(number), true) => {
                    (9, number.to_le_bytes().to_vec())
                }
                (SpreadsheetSourceValue::Boolean(value), true) => (10, vec![u8::from(*value)]),
                (SpreadsheetSourceValue::Error(value), true) => {
                    (11, vec![spreadsheet_error_code(value)])
                }
                (SpreadsheetSourceValue::String(value), true) => {
                    let mut bytes = Vec::new();
                    write_xlsb_string(&mut bytes, value);
                    (8, bytes)
                }
                (SpreadsheetSourceValue::Blank, true) => {
                    let mut bytes = Vec::new();
                    write_xlsb_string(&mut bytes, "");
                    (8, bytes)
                }
                (SpreadsheetSourceValue::SharedString(_), true) => {
                    anyhow::bail!("formula cache cannot be a shared-string index")
                }
            };
            let mut cell_payload = Vec::new();
            cell_payload.extend_from_slice(&(cell.column & 0x3fff).to_le_bytes());
            cell_payload.extend_from_slice(&cell.style_id.to_le_bytes());
            cell_payload.extend_from_slice(&value);
            if let Some(formula) = formula {
                // ref: sdkjs/cell/model/Workbook.js:18217-18272
                cell_payload.extend_from_slice(&0u16.to_le_bytes()); // flags
                cell_payload.extend_from_slice(&0u32.to_le_bytes()); // cce
                cell_payload.extend_from_slice(&0u32.to_le_bytes()); // cb
                cell_payload.extend_from_slice(&6u16.to_le_bytes()); // normal formula + text
                write_xlsb_string(&mut cell_payload, formula);
            } else {
                cell_payload.extend_from_slice(&0u16.to_le_bytes());
            }
            write_xlsb_record(&mut output, record_type, &cell_payload);
        }
    }
    write_xlsb_record(&mut output, 146, &[]);
    Ok(output)
}

fn spreadsheet_error_code(value: &str) -> u8 {
    match value {
        "#NULL!" => 0x00,
        "#DIV/0!" => 0x07,
        "#VALUE!" => 0x0f,
        "#REF!" => 0x17,
        "#NAME?" => 0x1d,
        "#NUM!" => 0x24,
        "#N/A" => 0x2a,
        _ => 0x2b,
    }
}

fn spreadsheet_error_text(value: u8) -> &'static str {
    match value {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0f => "#VALUE!",
        0x17 => "#REF!",
        0x1d => "#NAME?",
        0x24 => "#NUM!",
        0x2a => "#N/A",
        _ => "#GETTING_DATA",
    }
}

fn write_xlsb_record(output: &mut Vec<u8>, record_type: u32, payload: &[u8]) {
    write_xlsb_varint(output, record_type);
    write_xlsb_varint(output, payload.len() as u32);
    output.extend_from_slice(payload);
}

fn write_xlsb_string(output: &mut Vec<u8>, value: &str) {
    let encoded = value.encode_utf16().collect::<Vec<_>>();
    output.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    output.extend(encoded.into_iter().flat_map(u16::to_le_bytes));
}

fn write_xlsb_varint(output: &mut Vec<u8>, mut value: u32) {
    loop {
        let part = (value & 0x7f) as u8;
        value >>= 7;
        output.push(if value == 0 { part } else { part | 0x80 });
        if value == 0 {
            break;
        }
    }
}

fn write_shared_strings_table(strings: &[String]) -> Vec<u8> {
    let mut content = Vec::new();
    for string in strings {
        write_binary_item(&mut content, 0, |item| {
            write_binary_item(item, 3, |text| append_utf16_le(text, string));
        });
    }
    length_prefix(&content)
}

// ref: sdkjs/cell/model/Serialize.js:3050-3528
fn write_styles_table(styles: &SpreadsheetSourceStyles) -> Vec<u8> {
    let mut content = Vec::new();
    write_binary_item(&mut content, 0, |borders| {
        write_binary_item(borders, 1, |_| {});
    });
    write_binary_item(&mut content, 4, |fills| {
        for fill in &styles.fills {
            write_binary_item(fills, 5, |item| {
                write_binary_item(item, 0, |pattern| {
                    write_binary_item(pattern, 2, |value| value.push(fill.pattern));
                    if let Some(color) = fill.foreground {
                        write_binary_item(pattern, 3, |value| write_spreadsheet_rgb(value, color));
                    }
                    if let Some(color) = fill.background {
                        write_binary_item(pattern, 4, |value| write_spreadsheet_rgb(value, color));
                    }
                });
            });
        }
    });
    write_binary_item(&mut content, 6, |fonts| {
        for font in &styles.fonts {
            write_binary_item(fonts, 7, |item| {
                if font.bold {
                    item.extend_from_slice(&[0, 1, 1]);
                }
                if font.italic {
                    item.extend_from_slice(&[3, 1, 1]);
                }
                if let Some(color) = font.color {
                    item.extend_from_slice(&[1, 6]);
                    let mut encoded = Vec::new();
                    write_spreadsheet_rgb(&mut encoded, color);
                    item.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
                    item.extend_from_slice(&encoded);
                }
                if let Some(name) = &font.name {
                    item.extend_from_slice(&[4, 6]);
                    write_utf16_le(item, name);
                }
                if let Some(size) = font.size {
                    item.extend_from_slice(&[6, 5]);
                    item.extend_from_slice(&size.to_le_bytes());
                }
            });
        }
    });
    write_binary_item(&mut content, 14, |xfs| {
        for xf in &styles.cell_style_xfs {
            write_binary_item(xfs, 3, |item| write_spreadsheet_xf(item, xf, true));
        }
    });
    write_binary_item(&mut content, 2, |xfs| {
        for xf in &styles.cell_xfs {
            write_binary_item(xfs, 3, |item| write_spreadsheet_xf(item, xf, false));
        }
    });
    write_binary_item(&mut content, 8, |_| {});
    if !styles.number_formats.is_empty() {
        let position = content.len();
        content.truncate(position - 5);
        write_binary_item(&mut content, 8, |formats| {
            for (id, code) in &styles.number_formats {
                write_binary_item(formats, 9, |item| {
                    item.extend_from_slice(&[0, 6]);
                    write_utf16_le(item, code);
                    item.extend_from_slice(&[1, 4]);
                    item.extend_from_slice(&id.to_le_bytes());
                });
            }
        });
    }
    length_prefix(&content)
}

fn decode_spreadsheet_editor_styles(
    editor_payload: &[u8],
    manifest: &EditorPayloadManifest,
) -> anyhow::Result<SpreadsheetSourceStyles> {
    let table = manifest
        .tables
        .iter()
        .find(|table| table.table_type == 2)
        .context("XLSY styles table is missing")?;
    let start = table.offset as usize;
    let end = start
        .checked_add(table.bytes as usize)
        .context("XLSY styles boundary overflow")?;
    let content = length_prefixed_content(
        editor_payload
            .get(start..end)
            .context("XLSY styles table is outside payload")?,
        "styles table",
    )?;
    let mut styles = SpreadsheetSourceStyles::default();
    for (table_type, table_item) in length_prefixed_items(content, "styles table")? {
        match table_type {
            6 => {
                for (item_type, item) in length_prefixed_items(table_item, "font table")? {
                    if item_type != 7 {
                        continue;
                    }
                    let mut font = SpreadsheetSourceFont::default();
                    for (property_type, payload) in spreadsheet_binary_properties(item, "font")? {
                        match property_type {
                            0 if payload.len() == 1 => font.bold = payload[0] != 0,
                            1 if payload.len() >= 6 && payload[0] == 0 && payload[1] == 4 => {
                                let rgb =
                                    u32::from_le_bytes(payload[2..6].try_into().expect("font RGB"));
                                font.color = Some(if rgb & 0xff00_0000 == 0 {
                                    rgb | 0xff00_0000
                                } else {
                                    rgb
                                });
                            }
                            3 if payload.len() == 1 => font.italic = payload[0] != 0,
                            4 => font.name = Some(decode_utf16_le(payload, "font name")?),
                            6 if payload.len() == 8 => {
                                font.size =
                                    Some(f64::from_le_bytes(payload.try_into().expect("font size")))
                            }
                            _ => {}
                        }
                    }
                    styles.fonts.push(font);
                }
            }
            8 => {
                for (item_type, item) in length_prefixed_items(table_item, "number formats")? {
                    if item_type != 9 {
                        continue;
                    }
                    let mut id = None;
                    let mut code = None;
                    for (property_type, payload) in
                        spreadsheet_binary_properties(item, "number format")?
                    {
                        match property_type {
                            0 => code = Some(decode_utf16_le(payload, "number format code")?),
                            1 if payload.len() == 4 => {
                                id = Some(u32::from_le_bytes(
                                    payload.try_into().expect("number format id"),
                                ))
                            }
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(code)) = (id, code) {
                        styles.number_formats.insert(id, code);
                    }
                }
            }
            14 | 2 => {
                let target = if table_type == 14 {
                    &mut styles.cell_style_xfs
                } else {
                    &mut styles.cell_xfs
                };
                for (item_type, item) in length_prefixed_items(table_item, "cell formats")? {
                    if item_type != 3 {
                        continue;
                    }
                    let mut xf = SpreadsheetSourceXf::default();
                    for (property_type, payload) in
                        spreadsheet_binary_properties(item, "cell format")?
                    {
                        match property_type {
                            0 if payload.len() == 1 => xf.apply_alignment = payload[0] != 0,
                            2 if payload.len() == 1 => xf.apply_fill = payload[0] != 0,
                            3 if payload.len() == 1 => xf.apply_font = payload[0] != 0,
                            6 if payload.len() == 4 => {
                                xf.border_id =
                                    u32::from_le_bytes(payload.try_into().expect("border id"))
                            }
                            7 if payload.len() == 4 => {
                                xf.fill_id =
                                    u32::from_le_bytes(payload.try_into().expect("fill id"))
                            }
                            8 if payload.len() == 4 => {
                                xf.font_id =
                                    u32::from_le_bytes(payload.try_into().expect("font id"))
                            }
                            9 if payload.len() == 4 => {
                                xf.num_fmt_id = u32::from_le_bytes(
                                    payload.try_into().expect("number format id"),
                                )
                            }
                            12 if payload.len() == 4 => {
                                xf.xf_id =
                                    Some(u32::from_le_bytes(payload.try_into().expect("xf id")))
                            }
                            _ => {}
                        }
                    }
                    target.push(xf);
                }
            }
            _ => {}
        }
    }
    Ok(styles)
}

fn write_spreadsheet_rgb(output: &mut Vec<u8>, color: u32) {
    output.extend_from_slice(&[0, 4]);
    output.extend_from_slice(&color.to_le_bytes());
}

fn write_spreadsheet_xf(output: &mut Vec<u8>, xf: &SpreadsheetSourceXf, is_cell_style: bool) {
    if xf.border_id != 0 {
        output.extend_from_slice(&[1, 1, 1]);
    }
    output.extend_from_slice(&[6, 4]);
    output.extend_from_slice(&xf.border_id.to_le_bytes());
    if xf.apply_fill || xf.fill_id != 0 {
        output.extend_from_slice(&[2, 1, 1]);
    }
    output.extend_from_slice(&[7, 4]);
    output.extend_from_slice(&xf.fill_id.to_le_bytes());
    if xf.apply_font || xf.font_id != 0 {
        output.extend_from_slice(&[3, 1, 1]);
    }
    output.extend_from_slice(&[8, 4]);
    output.extend_from_slice(&xf.font_id.to_le_bytes());
    if xf.num_fmt_id != 0 {
        output.extend_from_slice(&[4, 1, 1]);
    }
    output.extend_from_slice(&[9, 4]);
    output.extend_from_slice(&xf.num_fmt_id.to_le_bytes());
    if xf.apply_alignment || xf.horizontal_alignment.is_some() {
        output.extend_from_slice(&[0, 1, 1]);
    }
    if let Some(horizontal) = xf.horizontal_alignment {
        output.extend_from_slice(&[13, 6]);
        let alignment = [0, 1, horizontal];
        output.extend_from_slice(&(alignment.len() as u32).to_le_bytes());
        output.extend_from_slice(&alignment);
    }
    if !is_cell_style {
        if let Some(xf_id) = xf.xf_id {
            output.extend_from_slice(&[12, 4]);
            output.extend_from_slice(&xf_id.to_le_bytes());
        }
    }
}

fn write_workbook_table(workbook: &SpreadsheetSourceWorkbook) -> Vec<u8> {
    let mut content = Vec::new();
    write_binary_item(&mut content, 1, |book_views| {
        write_binary_item(book_views, 2, |view| {
            view.extend_from_slice(&[0, 4]);
            view.extend_from_slice(&0u32.to_le_bytes());
        });
    });
    write_binary_item(&mut content, 3, |defined_names| {
        for name in &workbook.defined_names {
            write_binary_item(defined_names, 4, |record| {
                record.push(0);
                write_utf16_le(record, &name.name);
                record.push(1);
                write_utf16_le(record, &name.reference);
                if let Some(local_sheet_id) = name.local_sheet_id {
                    write_binary_item(record, 2, |value| {
                        value.extend_from_slice(&local_sheet_id.to_le_bytes())
                    });
                }
                if name.hidden {
                    write_binary_item(record, 3, |value| value.push(1));
                }
            });
        }
    });
    if !workbook.pivot_caches.is_empty() {
        write_binary_item(&mut content, 7, |caches| {
            for cache in &workbook.pivot_caches {
                write_binary_item(caches, 8, |record| {
                    write_binary_item(record, 0, |value| {
                        value.extend_from_slice(&cache.id.to_le_bytes())
                    });
                    write_binary_item(record, 1, |value| {
                        value.extend_from_slice(&cache.definition_xml)
                    });
                    if let Some(records) = &cache.records_xml {
                        write_binary_item(record, 2, |value| value.extend_from_slice(records));
                    }
                });
            }
        });
    }
    if let Some(protection) = &workbook.protection {
        write_binary_item(&mut content, 21, |record| {
            write_spreadsheet_property(record, 4, &[u8::from(protection.lock_structure)]);
            write_spreadsheet_property(record, 5, &[u8::from(protection.lock_windows)]);
            if let Some(password) = protection.password.as_deref() {
                write_spreadsheet_variable_property(record, 6, password);
            }
            write_spreadsheet_property(record, 11, &[u8::from(protection.lock_revision)]);
        });
    }
    length_prefix(&content)
}

fn write_worksheets_table(sheets: &[SpreadsheetSourceSheet], offsets: &[u32]) -> Vec<u8> {
    let mut content = Vec::new();
    for (sheet, offset) in sheets.iter().zip(offsets) {
        write_binary_item(&mut content, 0, |worksheet| {
            write_binary_item(worksheet, 1, |properties| {
                properties.extend_from_slice(&[0, 6]);
                write_utf16_le(properties, &sheet.name);
                properties.extend_from_slice(&[1, 4]);
                properties.extend_from_slice(&sheet.sheet_id.to_le_bytes());
                if sheet.visibility != 2 {
                    properties.extend_from_slice(&[2, 1, sheet.visibility]);
                }
            });
            if !sheet.columns.is_empty() {
                write_binary_item(worksheet, 2, |columns| {
                    for column in &sheet.columns {
                        write_binary_item(columns, 3, |item| {
                            item.extend_from_slice(&[2, 4]);
                            item.extend_from_slice(&column.max.to_le_bytes());
                            item.extend_from_slice(&[3, 4]);
                            item.extend_from_slice(&column.min.to_le_bytes());
                            item.extend_from_slice(&[5, 5]);
                            item.extend_from_slice(&column.width.to_le_bytes());
                            if column.custom_width {
                                item.extend_from_slice(&[6, 1, 1]);
                            }
                        });
                    }
                });
            }
            if let Some(pane) = &sheet.frozen_pane {
                write_binary_item(worksheet, 22, |views| {
                    write_binary_item(views, 23, |view| {
                        if let Some(sheet_view) = sheet.view {
                            write_binary_item(view, 12, |value| value.push(sheet_view));
                        }
                        write_binary_item(view, 19, |pane_bytes| {
                            if !pane.active_pane.is_empty() {
                                write_binary_item(pane_bytes, 0, |value| {
                                    value.push(match pane.active_pane.as_str() {
                                        "bottomLeft" => 0,
                                        "bottomRight" => 1,
                                        "topLeft" => 2,
                                        _ => 3,
                                    });
                                });
                            }
                            write_binary_item(pane_bytes, 1, |value| {
                                append_utf16_le(value, "frozen");
                            });
                            write_binary_item(pane_bytes, 2, |value| {
                                append_utf16_le(value, &pane.top_left_cell);
                            });
                            if pane.x_split > 0.0 {
                                write_binary_item(pane_bytes, 3, |value| {
                                    value.extend_from_slice(&pane.x_split.to_le_bytes());
                                });
                            }
                            if pane.y_split > 0.0 {
                                write_binary_item(pane_bytes, 4, |value| {
                                    value.extend_from_slice(&pane.y_split.to_le_bytes());
                                });
                            }
                        });
                    });
                });
            } else if let Some(sheet_view) = sheet.view {
                write_binary_item(worksheet, 22, |views| {
                    write_binary_item(views, 23, |view| {
                        write_binary_item(view, 12, |value| value.push(sheet_view));
                    });
                });
            }
            write_binary_item(worksheet, 11, |format| {
                format.extend_from_slice(&[1, 5]);
                format.extend_from_slice(&sheet.default_row_height.to_le_bytes());
            });
            if let Some(print) = &sheet.print_layout {
                if let Some(fit_to_page) = print.fit_to_page {
                    write_binary_item(worksheet, 24, |properties| {
                        write_binary_item(properties, 10, |page_setup| {
                            write_binary_item(page_setup, 12, |value| {
                                value.push(u8::from(fit_to_page))
                            });
                        });
                    });
                }
                write_binary_item(worksheet, 14, |value| {
                    for (property, margin) in print.margins.iter().enumerate() {
                        if let Some(margin) = margin {
                            write_spreadsheet_property(
                                value,
                                property as u8,
                                &margin.to_le_bytes(),
                            );
                        }
                    }
                });
                write_binary_item(worksheet, 15, |value| {
                    write_spreadsheet_page_setup(value, print)
                });
                write_binary_item(worksheet, 16, |value| {
                    for (property, option) in print.print_options.iter().enumerate() {
                        if let Some(option) = option {
                            write_spreadsheet_property(value, property as u8, &[u8::from(*option)]);
                        }
                    }
                });
            }
            write_binary_item(worksheet, 9, |sheet_data| {
                write_binary_item(sheet_data, 35, |position| {
                    position.extend_from_slice(&offset.to_le_bytes());
                });
            });
            if !sheet.merged_cells.is_empty() {
                write_binary_item(worksheet, 7, |merged| {
                    for reference in &sheet.merged_cells {
                        write_binary_item(merged, 8, |value| append_utf16_le(value, reference));
                    }
                });
            }
            if !sheet.tables.is_empty() {
                write_binary_item(worksheet, 18, |tables| {
                    for table in &sheet.tables {
                        write_spreadsheet_table(tables, table);
                    }
                });
            }
            if !sheet.comments.is_empty() {
                write_binary_item(worksheet, 19, |comments| {
                    for comment in &sheet.comments {
                        write_binary_item(comments, 20, |record| {
                            write_spreadsheet_comment(record, comment)
                        });
                    }
                });
            }
            if !sheet.drawings.is_empty() {
                write_binary_item(worksheet, 12, |drawings| {
                    for drawing in &sheet.drawings {
                        write_binary_item(drawings, 13, |record| {
                            write_spreadsheet_chart_drawing(record, drawing)
                        });
                    }
                });
            }
            for conditional in &sheet.conditional_formats {
                write_binary_item(worksheet, 21, |value| {
                    write_spreadsheet_conditional_format(value, conditional)
                });
            }
            for pivot in &sheet.pivot_tables {
                write_binary_item(worksheet, 26, |record| {
                    write_binary_item(record, 3, |value| {
                        value.extend_from_slice(&pivot.cache_id.to_le_bytes())
                    });
                    write_binary_item(record, 4, |value| value.extend_from_slice(&pivot.xml));
                });
            }
            if !sheet.data_validations.is_empty() {
                write_binary_item(worksheet, 32, |value| {
                    write_spreadsheet_data_validations(value, &sheet.data_validations)
                });
            }
            if let Some(protection) = &sheet.protection {
                write_binary_item(worksheet, 41, |value| {
                    write_spreadsheet_sheet_protection(value, protection)
                });
            }
            if let Some(print) = &sheet.print_layout {
                if print.align_with_margins.is_some()
                    || print.header_footer.iter().any(Option::is_some)
                {
                    write_binary_item(worksheet, 27, |value| {
                        write_spreadsheet_header_footer(value, print)
                    });
                }
                if let Some(breaks) = &print.row_breaks {
                    write_binary_item(worksheet, 30, |value| {
                        write_spreadsheet_breaks(value, breaks)
                    });
                }
                if let Some(breaks) = &print.col_breaks {
                    write_binary_item(worksheet, 31, |value| {
                        write_spreadsheet_breaks(value, breaks)
                    });
                }
            }
        });
    }
    length_prefix(&content)
}

fn write_spreadsheet_page_setup(output: &mut Vec<u8>, print: &SpreadsheetPrintLayout) {
    for (property, value) in [
        (1, print.paper_size.map(u32::from)),
        (7, print.first_page_number),
        (8, print.fit_to_height),
        (9, print.fit_to_width),
        (10, print.horizontal_dpi),
        (18, print.vertical_dpi),
    ] {
        if let Some(value) = value {
            let bytes = value.to_le_bytes();
            if property == 1 {
                write_spreadsheet_property(output, property, &[value as u8]);
            } else {
                write_spreadsheet_property(output, property, &bytes);
            }
        }
    }
    if let Some(value) = print.orientation {
        write_spreadsheet_property(output, 0, &[value]);
    }
    if let Some(value) = print.use_first_page_number {
        write_spreadsheet_property(output, 16, &[u8::from(value)]);
    }
}

fn write_spreadsheet_header_footer(output: &mut Vec<u8>, print: &SpreadsheetPrintLayout) {
    if let Some(value) = print.align_with_margins {
        write_binary_item(output, 0, |item| item.push(u8::from(value)));
    }
    for (property, text) in print.header_footer.iter().enumerate() {
        if let Some(text) = text {
            output.push((property + 4) as u8);
            let bytes = text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            output.extend_from_slice(&bytes);
        }
    }
}

fn write_spreadsheet_breaks(output: &mut Vec<u8>, breaks: &SpreadsheetBreaks) {
    write_binary_item(output, 0, |value| {
        value.extend_from_slice(&breaks.count.to_le_bytes())
    });
    write_binary_item(output, 1, |value| {
        value.extend_from_slice(&breaks.manual_count.to_le_bytes())
    });
    for brk in &breaks.breaks {
        write_binary_item(output, 2, |value| {
            write_binary_item(value, 3, |item| {
                item.extend_from_slice(&brk.id.to_le_bytes())
            });
            write_binary_item(value, 4, |item| item.push(u8::from(brk.manual)));
            write_binary_item(value, 5, |item| {
                item.extend_from_slice(&brk.max.to_le_bytes())
            });
            write_binary_item(value, 6, |item| {
                item.extend_from_slice(&brk.min.to_le_bytes())
            });
        });
    }
}

// refs: sdkjs/cell/model/Serialize.js:6609-6731 and
// sdkjs/common/Shapes/SerializeWriter.js:3840-3945,5627-5661
fn write_spreadsheet_chart_drawing(output: &mut Vec<u8>, drawing: &SpreadsheetSourceDrawing) {
    write_binary_item(output, 0, |value| value.push(2)); // two-cell anchor
    write_binary_item(output, 12, |value| value.push(1)); // editAs one-cell
    write_binary_item(output, 1, |value| {
        write_spreadsheet_anchor_point(
            value,
            drawing.from_col,
            drawing.from_col_off_mm,
            drawing.from_row,
            drawing.from_row_off_mm,
        )
    });
    write_binary_item(output, 2, |value| {
        write_spreadsheet_anchor_point(
            value,
            drawing.to_col,
            drawing.to_col_off_mm,
            drawing.to_row,
            drawing.to_row_off_mm,
        )
    });
    write_binary_item(output, 9, |value| {
        write_spreadsheet_chart_ppty(value, drawing)
    });
}

fn write_spreadsheet_anchor_point(
    output: &mut Vec<u8>,
    column: u32,
    column_offset_mm: f64,
    row: u32,
    row_offset_mm: f64,
) {
    write_spreadsheet_property(output, 0, &column.to_le_bytes());
    write_spreadsheet_property(output, 1, &column_offset_mm.to_le_bytes());
    write_spreadsheet_property(output, 2, &row.to_le_bytes());
    write_spreadsheet_property(output, 3, &row_offset_mm.to_le_bytes());
}

fn write_spreadsheet_chart_ppty(output: &mut Vec<u8>, drawing: &SpreadsheetSourceDrawing) {
    ppty_record(output, 0, |outer| {
        ppty_record(outer, 1, |drawing_record| {
            ppty_record(drawing_record, 5, |graphic_frame| {
                write_ppty_attr_start(graphic_frame);
                write_ppty_attr_end(graphic_frame);
                ppty_record(graphic_frame, 0, |non_visual| {
                    ppty_record(non_visual, 0, |common_non_visual| {
                        write_ppty_attr_start(common_non_visual);
                        write_ppty_i32_attr(common_non_visual, 0, 2);
                        write_ppty_string_attr(common_non_visual, 1, &drawing.name);
                        write_ppty_attr_end(common_non_visual);
                    });
                });
                ppty_record(graphic_frame, 1, |transform| {
                    write_ppty_attr_start(transform);
                    for (attribute, value) in drawing.xfrm_emu.into_iter().enumerate() {
                        write_ppty_i32_attr(transform, attribute as u8, value);
                    }
                    write_ppty_attr_end(transform);
                });
                ppty_record(graphic_frame, 3, |chart| {
                    chart.extend_from_slice(&write_document_chart_binary(&drawing.chart))
                });
            });
        });
    });
}

fn write_spreadsheet_comment(output: &mut Vec<u8>, comment: &EditorSpreadsheetCommentManifest) {
    let column = parse_cell_column(&comment.reference).expect("validated OOXML comment column");
    let row = comment
        .reference
        .chars()
        .skip_while(|value| value.is_ascii_alphabetic())
        .collect::<String>()
        .parse::<u32>()
        .expect("validated OOXML comment row")
        - 1;
    write_spreadsheet_property(output, 0, &row.to_le_bytes());
    write_spreadsheet_property(output, 1, &column.to_le_bytes());
    let mut datas = Vec::new();
    write_binary_item(&mut datas, 3, |data| {
        data.push(0);
        write_utf16_le(data, &comment.text);
        data.push(3);
        write_utf16_le(data, &comment.author);
    });
    output.push(2);
    output.push(6);
    output.extend_from_slice(&(datas.len() as u32).to_le_bytes());
    output.extend_from_slice(&datas);
}

fn write_spreadsheet_sheet_protection(
    output: &mut Vec<u8>,
    protection: &EditorSheetProtectionManifest,
) {
    if let Some(password) = protection.password.as_deref() {
        write_spreadsheet_variable_property(output, 4, password);
    }
    for (property_type, value) in [
        (5, protection.auto_filter),
        (7, protection.delete_columns),
        (8, protection.delete_rows),
        (9, protection.format_cells),
        (10, protection.format_columns),
        (11, protection.format_rows),
        (12, protection.insert_columns),
        (13, protection.insert_hyperlinks),
        (14, protection.insert_rows),
        (15, protection.objects),
        (16, protection.pivot_tables),
        (17, protection.scenarios),
        (18, protection.select_locked_cells),
        (19, protection.select_unlocked_cells),
        (20, protection.sheet),
        (21, protection.sort),
    ] {
        write_spreadsheet_property(output, property_type, &[u8::from(value)]);
    }
}

// ref: sdkjs/cell/model/Serialize.js:5494-5598
fn write_spreadsheet_data_validations(
    output: &mut Vec<u8>,
    validations: &[EditorDataValidationManifest],
) {
    write_binary_item(output, 0, |list| {
        for validation in validations {
            write_binary_item(list, 1, |record| {
                write_spreadsheet_property(record, 6, &[u8::from(validation.allow_blank)]);
                write_spreadsheet_property(
                    record,
                    5,
                    &[
                        spreadsheet_validation_type_code(&validation.validation_type)
                            .expect("validated data validation type"),
                    ],
                );
                write_optional_variable_property(record, 7, validation.error.as_deref());
                write_optional_variable_property(record, 8, validation.error_title.as_deref());
                if let Some(style) = &validation.error_style {
                    write_spreadsheet_property(
                        record,
                        9,
                        &[spreadsheet_validation_error_style_code(style)
                            .expect("validated validation error style")],
                    );
                }
                if let Some(operator) = &validation.operator {
                    write_spreadsheet_property(
                        record,
                        11,
                        &[spreadsheet_validation_operator_code(operator)
                            .expect("validated validation operator")],
                    );
                }
                write_spreadsheet_property(record, 15, &[u8::from(validation.show_error_message)]);
                write_spreadsheet_variable_property(record, 17, &validation.reference);
                write_optional_variable_property(record, 18, validation.formula1.as_deref());
                write_optional_variable_property(record, 19, validation.formula2.as_deref());
            });
        }
    });
}

// ref: sdkjs/cell/model/Serialize.js:7104-7179
fn write_spreadsheet_conditional_format(
    output: &mut Vec<u8>,
    conditional: &EditorConditionalFormatManifest,
) {
    write_binary_item(output, 1, |value| {
        append_utf16_le(value, &conditional.reference)
    });
    write_binary_item(output, 2, |rule| {
        if let Some(operator) = &conditional.operator {
            write_binary_item(rule, 4, |value| {
                value.push(
                    spreadsheet_conditional_operator_code(operator)
                        .expect("validated conditional operator"),
                )
            });
        }
        write_binary_item(rule, 6, |value| {
            value.extend_from_slice(&conditional.priority.to_le_bytes())
        });
        write_binary_item(rule, 12, |value| {
            value.push(
                spreadsheet_conditional_type_code(&conditional.rule_type)
                    .expect("validated conditional type"),
            )
        });
        if let Some(style) = &conditional.differential_style {
            write_binary_item(rule, 18, |value| {
                write_spreadsheet_differential_style(value, style)
            });
        }
        if conditional.rule_type == "colorScale" {
            write_binary_item(rule, 14, |scale| {
                for threshold in &conditional.thresholds {
                    write_binary_item(scale, 0, |value| {
                        write_binary_item(value, 1, |kind| {
                            kind.push(
                                spreadsheet_conditional_threshold_type_code(
                                    &threshold.threshold_type,
                                )
                                .expect("validated conditional threshold type"),
                            )
                        });
                        if let Some(formula) = &threshold.value {
                            write_binary_item(value, 3, |text| append_utf16_le(text, formula));
                        }
                    });
                }
                for color in &conditional.colors {
                    write_binary_item(scale, 1, |value| {
                        write_spreadsheet_rgb(
                            value,
                            u32::from_str_radix(color, 16).expect("validated conditional RGB"),
                        )
                    });
                }
            });
        }
        for formula in &conditional.formulas {
            write_binary_item(rule, 16, |value| append_utf16_le(value, formula));
        }
    });
}

fn write_spreadsheet_differential_style(
    output: &mut Vec<u8>,
    style: &EditorDifferentialStyleManifest,
) {
    if let Some(fill) = &style.fill_rgb {
        write_binary_item(output, 2, |fill_record| {
            write_binary_item(fill_record, 0, |pattern| {
                write_binary_item(pattern, 2, |value| value.push(18));
                for item_type in [3, 4] {
                    write_binary_item(pattern, item_type, |value| {
                        write_spreadsheet_rgb(
                            value,
                            u32::from_str_radix(fill, 16).expect("validated differential fill RGB"),
                        )
                    });
                }
            });
        });
    }
    if let Some(font) = &style.font_rgb {
        write_binary_item(output, 3, |font_record| {
            let mut color = Vec::new();
            write_spreadsheet_rgb(
                &mut color,
                u32::from_str_radix(font, 16).expect("validated differential font RGB"),
            );
            font_record.extend_from_slice(&[1, 6]);
            font_record.extend_from_slice(&(color.len() as u32).to_le_bytes());
            font_record.extend_from_slice(&color);
        });
    }
}

fn write_spreadsheet_property(output: &mut Vec<u8>, property_type: u8, value: &[u8]) {
    output.push(property_type);
    output.push(match value.len() {
        1 => 1,
        4 => 4,
        8 => 5,
        _ => panic!("unsupported fixed spreadsheet property length"),
    });
    output.extend_from_slice(value);
}

fn write_spreadsheet_variable_property(output: &mut Vec<u8>, property_type: u8, value: &str) {
    let mut encoded = Vec::new();
    append_utf16_le(&mut encoded, value);
    output.extend_from_slice(&[property_type, 6]);
    output.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    output.extend_from_slice(&encoded);
}

fn write_optional_variable_property(output: &mut Vec<u8>, property_type: u8, value: Option<&str>) {
    if let Some(value) = value {
        write_spreadsheet_variable_property(output, property_type, value);
    }
}

// ref: sdkjs/cell/model/Serialize.js:2187-2289,2487-2640
fn write_spreadsheet_table(output: &mut Vec<u8>, table: &EditorTableManifest) {
    write_binary_item(output, 0, |record| {
        write_binary_item(record, 1, |value| append_utf16_le(value, &table.reference));
        write_binary_item(record, 3, |value| {
            append_utf16_le(value, &table.display_name)
        });
        write_binary_item(record, 4, |auto_filter| {
            write_binary_item(auto_filter, 0, |value| {
                append_utf16_le(value, &table.reference)
            });
            if !table.filters.is_empty() {
                write_binary_item(auto_filter, 1, |columns| {
                    for column in &table.filters {
                        write_binary_item(columns, 2, |record| {
                            write_binary_item(record, 0, |value| {
                                value.extend_from_slice(&column.column_id.to_le_bytes());
                            });
                            write_binary_item(record, 1, |filters| {
                                for filter in &column.values {
                                    write_binary_item(filters, 2, |value| {
                                        write_binary_item(value, 0, |text| {
                                            append_utf16_le(text, filter);
                                        });
                                    });
                                }
                            });
                        });
                    }
                });
            }
            if let Some(sort) = &table.sort {
                write_binary_item(auto_filter, 3, |value| {
                    write_spreadsheet_sort_state(value, sort);
                });
            }
        });
        if !table.columns.is_empty() {
            write_binary_item(record, 6, |columns| {
                for column in &table.columns {
                    write_binary_item(columns, 0, |value| {
                        write_binary_item(value, 1, |name| append_utf16_le(name, column));
                    });
                }
            });
        }
        write_binary_item(record, 7, |style| {
            style.extend_from_slice(&[0, 6]);
            write_utf16_le(style, &table.style_name);
            style.extend_from_slice(&[1, 1, u8::from(table.show_column_stripes)]);
            style.extend_from_slice(&[2, 1, u8::from(table.show_row_stripes)]);
            style.extend_from_slice(&[3, 1, u8::from(table.show_first_column)]);
            style.extend_from_slice(&[4, 1, u8::from(table.show_last_column)]);
        });
    });
}

fn write_spreadsheet_sort_state(output: &mut Vec<u8>, sort: &EditorSortManifest) {
    write_binary_item(output, 0, |value| append_utf16_le(value, &sort.reference));
    write_binary_item(output, 2, |conditions| {
        write_binary_item(conditions, 3, |condition| {
            condition.extend_from_slice(&[4, 6]);
            write_utf16_le(condition, &sort.condition_reference);
            condition.extend_from_slice(&[6, 1, u8::from(sort.descending)]);
        });
    });
}

fn write_binary_item(output: &mut Vec<u8>, item_type: u8, write: impl FnOnce(&mut Vec<u8>)) {
    let mut content = Vec::new();
    write(&mut content);
    output.push(item_type);
    output.extend_from_slice(&(content.len() as u32).to_le_bytes());
    output.extend_from_slice(&content);
}

fn length_prefix(content: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(4 + content.len());
    output.extend_from_slice(&(content.len() as u32).to_le_bytes());
    output.extend_from_slice(content);
    output
}

fn write_utf16_le(output: &mut Vec<u8>, value: &str) {
    let encoded = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    output.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    output.extend_from_slice(&encoded);
}

fn append_utf16_le(output: &mut Vec<u8>, value: &str) {
    output.extend(value.encode_utf16().flat_map(u16::to_le_bytes));
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
    if kind == OfficeKind::Document && changed_payload.starts_with(b"DOCY;") {
        ensure!(
            base_payload.starts_with(b"DOCY;"),
            "base editor payload is not DOCY"
        );
        let editor_manifest = inspect_editor_payload(kind, changed_payload)?;
        let primary_text = decode_document_binary_paragraphs(changed_payload)?.join("\n");
        return Ok(PreparedEditorPayload {
            kind,
            protocol: DOCUMENT_EDITOR_PROTOCOL.to_string(),
            protocol_version: DOCUMENT_EDITOR_PROTOCOL_VERSION,
            source_sha256: sha256_hex(base_payload),
            editor_sha256: sha256_hex(changed_payload),
            editor_payload: changed_payload.to_vec(),
            manifest: SemanticManifest {
                schema_version: "ctox-office-semantic-manifest-v1".to_string(),
                kind,
                package_sha256: sha256_hex(changed_payload),
                parts: Vec::new(),
                relationship_parts: 0,
                content_types_present: false,
                primary_part: "word/document.xml".to_string(),
                primary_text,
            },
            editor_manifest: Some(editor_manifest),
            implemented_features: options.implemented_features,
            diagnostics: vec![OfficeDiagnostic {
                level: "info".to_string(),
                code: "office.document.docy-complete-payload-change".to_string(),
                message: "The editor supplied a complete DOCY v10 payload for conflict-checked canonical export.".to_string(),
            }],
        });
    }
    if kind == OfficeKind::Spreadsheet && changed_payload.starts_with(b"XLSY;") {
        ensure!(
            base_payload.starts_with(b"XLSY;"),
            "base editor payload is not XLSY"
        );
        let editor_manifest = inspect_editor_payload(kind, changed_payload)?;
        let primary_text = editor_manifest
            .worksheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(PreparedEditorPayload {
            kind,
            protocol: SPREADSHEET_EDITOR_PROTOCOL.to_string(),
            protocol_version: SPREADSHEET_EDITOR_PROTOCOL_VERSION,
            source_sha256: sha256_hex(base_payload),
            editor_sha256: sha256_hex(changed_payload),
            editor_payload: changed_payload.to_vec(),
            manifest: SemanticManifest {
                schema_version: "ctox-office-semantic-manifest-v1".to_string(),
                kind,
                package_sha256: sha256_hex(changed_payload),
                parts: Vec::new(),
                relationship_parts: 0,
                content_types_present: false,
                primary_part: "xl/workbook.xml".to_string(),
                primary_text,
            },
            editor_manifest: Some(editor_manifest),
            implemented_features: options.implemented_features,
            diagnostics: vec![OfficeDiagnostic {
                level: "info".to_string(),
                code: "office.spreadsheet.xlsy-complete-payload-change".to_string(),
                message: "The editor supplied a complete XLSY v10 payload for conflict-checked canonical export.".to_string(),
            }],
        });
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
    let is_document_binary = kind == OfficeKind::Document && editor_payload.starts_with(b"DOCY;");
    let is_spreadsheet_binary =
        kind == OfficeKind::Spreadsheet && editor_payload.starts_with(b"XLSY;");
    if is_document_binary || is_spreadsheet_binary {
        inspect_editor_payload(kind, editor_payload)?;
    } else {
        inspect(kind, editor_payload)?;
    }
    let bytes = match (is_document_binary, is_spreadsheet_binary, original_package) {
        (true, false, Some(original)) => export_document_binary(editor_payload, original)?,
        (true, false, None) => {
            anyhow::bail!("DOCY export requires the original DOCX escrow package")
        }
        (false, true, Some(original)) => export_spreadsheet_binary(editor_payload, original)?,
        (false, true, None) => {
            anyhow::bail!("XLSY export requires the original XLSX escrow package")
        }
        (false, false, Some(original)) => merge_understood_parts(kind, editor_payload, original)?,
        (false, false, None) => editor_payload.to_vec(),
        (true, true, _) => unreachable!("an editor payload cannot be DOCY and XLSY"),
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

#[derive(Debug, Clone, PartialEq)]
struct DecodedSpreadsheetChartDrawing {
    from_col: u32,
    from_col_off_mm: f64,
    from_row: u32,
    from_row_off_mm: f64,
    to_col: u32,
    to_col_off_mm: f64,
    to_row: u32,
    to_row_off_mm: f64,
    style: Option<u8>,
    xfrm_emu: Option<[i32; 4]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DecodedSpreadsheetPrintPivot {
    view: Option<u8>,
    fit_to_page: Option<bool>,
    margins: [Option<f64>; 6],
    page_setup: BTreeMap<u8, Vec<u8>>,
    print_options: [Option<bool>; 5],
    header_footer_flags: BTreeMap<u8, bool>,
    header_footer_text: BTreeMap<u8, String>,
    row_breaks: Option<SpreadsheetBreaks>,
    col_breaks: Option<SpreadsheetBreaks>,
    pivot_tables: Vec<(u32, Vec<u8>)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DecodedSpreadsheetPivotCaches(Vec<(u32, Vec<u8>, Option<Vec<u8>>)>);

fn decode_spreadsheet_print_pivot(
    editor_payload: &[u8],
) -> anyhow::Result<BTreeMap<String, DecodedSpreadsheetPrintPivot>> {
    let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, editor_payload)?;
    let table = manifest
        .tables
        .iter()
        .find(|table| table.table_type == 4)
        .context("XLSY has no worksheets table")?;
    let content = length_prefixed_content(
        &editor_payload[table.offset as usize..(table.offset + table.bytes) as usize],
        "worksheets table",
    )?;
    let mut result = BTreeMap::new();
    let mut index = 0usize;
    for (item_type, worksheet) in length_prefixed_items(content, "worksheets table")? {
        if item_type != 0 {
            continue;
        }
        let name = manifest
            .worksheets
            .get(index)
            .context("worksheet manifest is shorter than table")?
            .name
            .clone();
        index += 1;
        let mut decoded = DecodedSpreadsheetPrintPivot::default();
        for (kind, value) in length_prefixed_items(worksheet, "worksheet record")? {
            match kind {
                22 => {
                    for (_, sheet_view) in length_prefixed_items(value, "sheet views")? {
                        decoded.view = length_prefixed_items(sheet_view, "sheet view")?
                            .into_iter()
                            .find(|(property, _)| *property == 12)
                            .and_then(|(_, bytes)| bytes.first().copied());
                    }
                }
                24 => {
                    for (property, sheet_pr) in length_prefixed_items(value, "sheet properties")? {
                        if property == 10 {
                            decoded.fit_to_page =
                                length_prefixed_items(sheet_pr, "page setup properties")?
                                    .into_iter()
                                    .find(|(kind, _)| *kind == 12)
                                    .and_then(|(_, bytes)| bytes.first().map(|value| *value != 0));
                        }
                    }
                }
                14 => {
                    for (property, bytes) in spreadsheet_binary_properties(value, "page margins")? {
                        if usize::from(property) < 6 && bytes.len() == 8 {
                            decoded.margins[property as usize] =
                                Some(f64::from_le_bytes(bytes.try_into().unwrap()));
                        }
                    }
                }
                15 => {
                    for (property, bytes) in spreadsheet_binary_properties(value, "page setup")? {
                        decoded.page_setup.insert(property, bytes.to_vec());
                    }
                }
                16 => {
                    for (property, bytes) in spreadsheet_binary_properties(value, "print options")?
                    {
                        if usize::from(property) < 5 {
                            decoded.print_options[property as usize] =
                                bytes.first().map(|value| *value != 0);
                        }
                    }
                }
                26 => {
                    let mut cache_id = None;
                    let mut xml = None;
                    for (property, bytes) in length_prefixed_items(value, "pivot table")? {
                        if property == 3 && bytes.len() == 4 {
                            cache_id = Some(u32::from_le_bytes(bytes.try_into().unwrap()));
                        }
                        if property == 4 {
                            xml = Some(bytes.to_vec());
                        }
                    }
                    decoded.pivot_tables.push((
                        cache_id.context("pivot table cacheId missing")?,
                        xml.context("pivot table XML missing")?,
                    ));
                }
                27 => {
                    for (property, bytes) in length_prefixed_items(value, "header footer")? {
                        if property < 4 {
                            decoded
                                .header_footer_flags
                                .insert(property, bytes.first().copied().unwrap_or(0) != 0);
                        } else {
                            decoded
                                .header_footer_text
                                .insert(property, decode_utf16_le(bytes, "header/footer text")?);
                        }
                    }
                }
                30 => decoded.row_breaks = Some(decode_spreadsheet_breaks(value)?),
                31 => decoded.col_breaks = Some(decode_spreadsheet_breaks(value)?),
                _ => {}
            }
        }
        result.insert(name, decoded);
    }
    Ok(result)
}

fn decode_spreadsheet_breaks(value: &[u8]) -> anyhow::Result<SpreadsheetBreaks> {
    let mut result = SpreadsheetBreaks::default();
    for (kind, bytes) in length_prefixed_items(value, "page breaks")? {
        match kind {
            0 if bytes.len() == 4 => result.count = u32::from_le_bytes(bytes.try_into().unwrap()),
            1 if bytes.len() == 4 => {
                result.manual_count = u32::from_le_bytes(bytes.try_into().unwrap())
            }
            2 => {
                let mut item = SpreadsheetBreak {
                    id: 0,
                    manual: false,
                    min: 0,
                    max: 0,
                };
                for (property, data) in length_prefixed_items(bytes, "page break")? {
                    match property {
                        3 if data.len() == 4 => {
                            item.id = u32::from_le_bytes(data.try_into().unwrap())
                        }
                        4 => item.manual = data.first().copied().unwrap_or(0) != 0,
                        5 if data.len() == 4 => {
                            item.max = u32::from_le_bytes(data.try_into().unwrap())
                        }
                        6 if data.len() == 4 => {
                            item.min = u32::from_le_bytes(data.try_into().unwrap())
                        }
                        _ => {}
                    }
                }
                result.breaks.push(item);
            }
            _ => {}
        }
    }
    Ok(result)
}

fn decode_spreadsheet_pivot_caches(
    editor_payload: &[u8],
) -> anyhow::Result<DecodedSpreadsheetPivotCaches> {
    let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, editor_payload)?;
    let table = manifest
        .tables
        .iter()
        .find(|table| table.table_type == 3)
        .context("XLSY has no workbook table")?;
    let content = length_prefixed_content(
        &editor_payload[table.offset as usize..(table.offset + table.bytes) as usize],
        "workbook table",
    )?;
    let mut result = Vec::new();
    for (kind, value) in length_prefixed_items(content, "workbook table")? {
        if kind != 7 {
            continue;
        }
        for (cache_kind, cache) in length_prefixed_items(value, "pivot caches")? {
            if cache_kind != 8 {
                continue;
            }
            let mut id = None;
            let mut definition = None;
            let mut records = None;
            for (property, bytes) in length_prefixed_items(cache, "pivot cache")? {
                match property {
                    0 if bytes.len() == 4 => {
                        id = Some(u32::from_le_bytes(bytes.try_into().unwrap()))
                    }
                    1 => definition = Some(bytes.to_vec()),
                    2 => records = Some(bytes.to_vec()),
                    _ => {}
                }
            }
            result.push((
                id.context("pivot cache id missing")?,
                definition.context("pivot cache definition missing")?,
                records,
            ));
        }
    }
    Ok(DecodedSpreadsheetPivotCaches(result))
}

fn decode_spreadsheet_chart_drawings(
    editor_payload: &[u8],
) -> anyhow::Result<BTreeMap<String, Vec<DecodedSpreadsheetChartDrawing>>> {
    let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, editor_payload)?;
    let table = manifest
        .tables
        .iter()
        .find(|table| table.table_type == 4)
        .context("XLSY has no worksheets table")?;
    let start = table.offset as usize;
    let end = start + table.bytes as usize;
    let content = length_prefixed_content(&editor_payload[start..end], "worksheets table")?;
    let worksheet_records = length_prefixed_items(content, "worksheets table")?;
    let mut result = BTreeMap::new();
    let mut worksheet_index = 0usize;
    for (item_type, worksheet) in worksheet_records {
        if item_type != 0 {
            continue;
        }
        let name = manifest
            .worksheets
            .get(worksheet_index)
            .context("XLSY worksheet manifest is shorter than its table")?
            .name
            .clone();
        worksheet_index += 1;
        let mut drawings = Vec::new();
        for (worksheet_type, worksheet_item) in
            length_prefixed_items(worksheet, "worksheet record")?
        {
            if worksheet_type != 12 {
                continue;
            }
            for (drawing_type, drawing_item) in
                length_prefixed_items(worksheet_item, "worksheet drawings")?
            {
                if drawing_type != 13 {
                    continue;
                }
                if let Some(drawing) = decode_spreadsheet_chart_drawing(drawing_item)? {
                    drawings.push(drawing);
                }
            }
        }
        result.insert(name, drawings);
    }
    Ok(result)
}

fn decode_spreadsheet_chart_drawing(
    value: &[u8],
) -> anyhow::Result<Option<DecodedSpreadsheetChartDrawing>> {
    let mut from = None;
    let mut to = None;
    let mut style = None;
    let mut xfrm_emu = None;
    for (item_type, item) in length_prefixed_items(value, "spreadsheet drawing")? {
        match item_type {
            1 => from = Some(decode_spreadsheet_anchor_point(item)?),
            2 => to = Some(decode_spreadsheet_anchor_point(item)?),
            9 => {
                let metadata = decode_spreadsheet_chart_metadata(item)?;
                style = metadata.0;
                xfrm_emu = metadata.1;
            }
            _ => {}
        }
    }
    let (Some(from), Some(to)) = (from, to) else {
        return Ok(None);
    };
    Ok(Some(DecodedSpreadsheetChartDrawing {
        from_col: from.0,
        from_col_off_mm: from.1,
        from_row: from.2,
        from_row_off_mm: from.3,
        to_col: to.0,
        to_col_off_mm: to.1,
        to_row: to.2,
        to_row_off_mm: to.3,
        style,
        xfrm_emu,
    }))
}

fn decode_spreadsheet_anchor_point(value: &[u8]) -> anyhow::Result<(u32, f64, u32, f64)> {
    let mut result = (0, 0.0, 0, 0.0);
    for (property_type, property) in spreadsheet_binary_properties(value, "drawing anchor")? {
        match property_type {
            0 if property.len() == 4 => result.0 = u32::from_le_bytes(property.try_into().unwrap()),
            1 if property.len() == 8 => result.1 = f64::from_le_bytes(property.try_into().unwrap()),
            2 if property.len() == 4 => result.2 = u32::from_le_bytes(property.try_into().unwrap()),
            3 if property.len() == 8 => result.3 = f64::from_le_bytes(property.try_into().unwrap()),
            _ => {}
        }
    }
    Ok(result)
}

fn decode_spreadsheet_chart_metadata(
    value: &[u8],
) -> anyhow::Result<(Option<u8>, Option<[i32; 4]>)> {
    let record0 = length_prefixed_items(value, "pptx drawing")?
        .into_iter()
        .find(|(kind, _)| *kind == 0)
        .map(|(_, value)| value);
    let Some(record0) = record0 else {
        return Ok((None, None));
    };
    let record1 = length_prefixed_items(record0, "pptx drawing root")?
        .into_iter()
        .find(|(kind, _)| *kind == 1)
        .map(|(_, value)| value);
    let Some(record1) = record1 else {
        return Ok((None, None));
    };
    let frame = length_prefixed_items(record1, "pptx drawing object")?
        .into_iter()
        .find(|(kind, _)| *kind == 5)
        .map(|(_, value)| value);
    let Some(frame) = frame else {
        return Ok((None, None));
    };
    let records_start = frame
        .iter()
        .position(|value| *value == 0xfb)
        .map(|value| value + 1)
        .context("chart graphicFrame attribute terminator is missing")?;
    let frame_records = length_prefixed_items(&frame[records_start..], "chart graphicFrame")?;
    let xfrm_emu = frame_records
        .iter()
        .find(|(kind, _)| *kind == 1)
        .map(|(_, value)| decode_spreadsheet_chart_xfrm(value))
        .transpose()?;
    let chart_space = frame_records
        .into_iter()
        .find(|(kind, _)| *kind == 3)
        .map(|(_, value)| value);
    let Some(chart_space) = chart_space else {
        return Ok((None, xfrm_emu));
    };
    let alternate = length_prefixed_items(chart_space, "chart space")?
        .into_iter()
        .find(|(kind, _)| *kind == 3)
        .map(|(_, value)| value);
    let Some(alternate) = alternate else {
        return Ok((None, xfrm_emu));
    };
    let fallback = length_prefixed_items(alternate, "chart alternate content")?
        .into_iter()
        .find(|(kind, _)| *kind == 1)
        .map(|(_, value)| value);
    let Some(fallback) = fallback else {
        return Ok((None, xfrm_emu));
    };
    let style = length_prefixed_items(fallback, "chart style fallback")?
        .into_iter()
        .find(|(kind, _)| *kind == 0)
        .map(|(_, value)| value);
    let Some(style) = style else {
        return Ok((None, xfrm_emu));
    };
    let value = length_prefixed_items(style, "chart style")?
        .into_iter()
        .find(|(kind, _)| *kind == 0)
        .and_then(|(_, value)| value.first().copied());
    Ok((value, xfrm_emu))
}

fn decode_spreadsheet_chart_xfrm(value: &[u8]) -> anyhow::Result<[i32; 4]> {
    let start = value
        .iter()
        .position(|value| *value == 0xfa)
        .map(|value| value + 1)
        .context("chart xfrm attribute start is missing")?;
    let end = value[start..]
        .iter()
        .position(|value| *value == 0xfb)
        .map(|value| start + value)
        .context("chart xfrm attribute end is missing")?;
    let mut result = [0i32; 4];
    let mut seen = [false; 4];
    let mut position = start;
    while position < end {
        let attribute = value[position] as usize;
        position += 1;
        if attribute <= 7 || attribute == 10 {
            ensure!(end - position >= 4, "chart xfrm integer is truncated");
            if attribute < 4 {
                result[attribute] =
                    i32::from_le_bytes(value[position..position + 4].try_into().unwrap());
                seen[attribute] = true;
            }
            position += 4;
        } else if matches!(attribute, 8 | 9) {
            ensure!(end - position >= 1, "chart xfrm boolean is truncated");
            position += 1;
        } else {
            anyhow::bail!("unsupported chart xfrm attribute: {attribute}");
        }
    }
    ensure!(
        seen.into_iter().all(|value| value),
        "chart xfrm is incomplete"
    );
    Ok(result)
}

/// Materialize the cell values understood by the XLSY v10 decoder back into
/// the original XLSX escrow. Unchanged package parts, and unchanged XML nodes
/// inside understood parts, are copied from the escrow verbatim.
fn export_spreadsheet_binary(
    editor_payload: &[u8],
    original_package: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let changed = inspect_editor_payload(OfficeKind::Spreadsheet, editor_payload)?;
    let original_editor = transcode_spreadsheet_to_editor_payload(original_package)?;
    let original = inspect_editor_payload(OfficeKind::Spreadsheet, &original_editor)?;
    let original_styles = decode_spreadsheet_editor_styles(&original_editor, &original)?;
    let changed_styles = decode_spreadsheet_editor_styles(editor_payload, &changed)?;
    let original_chart_drawings = decode_spreadsheet_chart_drawings(&original_editor)?;
    let changed_chart_drawings = decode_spreadsheet_chart_drawings(editor_payload)?;
    let original_print_pivot = decode_spreadsheet_print_pivot(&original_editor)?;
    let changed_print_pivot = decode_spreadsheet_print_pivot(editor_payload)?;
    let original_pivot_caches = decode_spreadsheet_pivot_caches(&original_editor)?;
    let changed_pivot_caches = decode_spreadsheet_pivot_caches(editor_payload)?;
    ensure!(
        changed.worksheets.len() == original.worksheets.len(),
        "XLSY worksheet add/remove export is not implemented"
    );

    let mut archive =
        ZipArchive::new(Cursor::new(original_package)).context("open original XLSX escrow")?;
    let worksheet_paths = spreadsheet_worksheet_paths(&mut archive)?;
    let table_paths = spreadsheet_table_paths(&mut archive, &worksheet_paths)?;
    let shared_xml = read_zip_part(&mut archive, "xl/sharedStrings.xml")?;
    let mut replacements = BTreeMap::new();
    let source_cells_changed =
        original
            .worksheets
            .iter()
            .zip(&changed.worksheets)
            .any(|(before, after)| {
                before.cells.len() != after.cells.len()
                    || before
                        .cells
                        .iter()
                        .zip(&after.cells)
                        .any(|(before, after)| {
                            before.reference != after.reference
                                || before.value_type != after.value_type
                                || before.display != after.display
                                || before.formula != after.formula
                        })
            });
    ensure!(
        original_pivot_caches.0.len() == changed_pivot_caches.0.len(),
        "XLSY pivot cache add/remove export is not implemented"
    );
    for (
        (before_id, before_definition, before_records),
        (_after_id, after_definition, after_records),
    ) in original_pivot_caches.0.iter().zip(&changed_pivot_caches.0)
    {
        if source_cells_changed && before_definition != after_definition {
            let path = spreadsheet_workbook_pivot_cache_path(&mut archive, *before_id)?;
            replacements.insert(path, after_definition.clone());
        }
        if source_cells_changed && before_records != after_records {
            let definition_path = spreadsheet_workbook_pivot_cache_path(&mut archive, *before_id)?;
            let records_path =
                spreadsheet_related_part(&mut archive, &definition_path, "/pivotCacheRecords")?
                    .context("changed pivot cache has no records relationship")?;
            replacements.insert(
                records_path,
                after_records
                    .clone()
                    .context("pivot cache records removal is not implemented")?,
            );
        }
    }
    let changed_defined_names = normalize_spreadsheet_builtin_defined_names(&changed.defined_names);
    if original.defined_names != changed_defined_names
        || original.workbook_protection != changed.workbook_protection
    {
        let workbook_xml = read_zip_part(&mut archive, "xl/workbook.xml")?;
        replacements.insert(
            "xl/workbook.xml".to_string(),
            replace_spreadsheet_workbook_metadata(
                workbook_xml,
                &changed_defined_names,
                changed.workbook_protection.as_ref(),
            )?,
        );
    }
    let mut changed_style_ids = BTreeSet::new();
    for (before_sheet, after_sheet) in original.worksheets.iter().zip(&changed.worksheets) {
        let before_cells = before_sheet
            .cells
            .iter()
            .map(|cell| (cell.reference.as_str(), cell))
            .collect::<BTreeMap<_, _>>();
        for after_cell in &after_sheet.cells {
            let before_style_id = before_cells
                .get(after_cell.reference.as_str())
                .map(|cell| cell.style_id)
                .unwrap_or(0);
            if !spreadsheet_styles_equivalent(
                before_style_id,
                &original_styles,
                after_cell.style_id,
                &changed_styles,
            )? {
                changed_style_ids.insert(after_cell.style_id);
            }
        }
    }
    let styles_xml = read_zip_part(&mut archive, "xl/styles.xml")?;
    let source_differential_formats = parse_ooxml_styles(&styles_xml)?.differential_formats;
    let (materialized_styles, style_map) =
        materialize_spreadsheet_styles(&styles_xml, &changed_styles, &changed_style_ids)?;
    if materialized_styles != styles_xml {
        replacements.insert("xl/styles.xml".to_string(), materialized_styles);
    }
    let materialized_shared_strings = materialize_spreadsheet_shared_strings(&original, &changed)?;
    let updated_shared = replace_changed_shared_strings(
        &shared_xml,
        &original.shared_strings,
        &materialized_shared_strings,
    )?;
    if updated_shared != shared_xml {
        replacements.insert("xl/sharedStrings.xml".to_string(), updated_shared);
    }

    for (sheet_index, (before, after)) in original
        .worksheets
        .iter()
        .zip(&changed.worksheets)
        .enumerate()
    {
        ensure!(
            before.name == after.name && before.sheet_id == after.sheet_id,
            "XLSY worksheet identity changed at index {sheet_index}"
        );
        let path = worksheet_paths
            .get(&before.name)
            .with_context(|| format!("worksheet path is missing: {}", before.name))?;
        let worksheet_xml = read_zip_part(&mut archive, path)?;
        let updated = replace_changed_worksheet_cells(
            &worksheet_xml,
            before,
            after,
            &original.shared_strings,
            &materialized_shared_strings,
            &original_styles,
            &changed_styles,
            &style_map,
        )?;
        let updated = replace_spreadsheet_validation_conditional(
            updated,
            before,
            after,
            &source_differential_formats,
        )?;
        let updated = if before.protection != after.protection {
            replace_spreadsheet_sheet_protection(updated, after.protection.as_ref())?
        } else {
            updated
        };
        let before_print = original_print_pivot
            .get(&before.name)
            .cloned()
            .unwrap_or_default();
        let after_print = changed_print_pivot
            .get(&after.name)
            .cloned()
            .unwrap_or_default();
        let updated = if before_print != DecodedSpreadsheetPrintPivot::default()
            && before_print != after_print
        {
            replace_spreadsheet_print_layout(updated, &after_print)?
        } else {
            updated
        };
        ensure!(
            before_print.pivot_tables.len() == after_print.pivot_tables.len(),
            "XLSY pivot table add/remove export is not implemented"
        );
        for (pivot_index, ((before_cache, before_xml), (after_cache, after_xml))) in before_print
            .pivot_tables
            .iter()
            .zip(&after_print.pivot_tables)
            .enumerate()
        {
            if before_xml != after_xml {
                let pivot_path = spreadsheet_worksheet_pivot_paths(&mut archive, path)?
                    .get(pivot_index)
                    .cloned()
                    .context("pivot table relationship is missing")?;
                replacements.insert(
                    pivot_path,
                    normalize_pivot_cache_id(after_xml, *after_cache, *before_cache)?,
                );
            }
        }
        if updated != worksheet_xml {
            replacements.insert(path.clone(), updated);
        }
        if before.comments != after.comments {
            let comments_path = spreadsheet_comment_path(&mut archive, path)?
                .context("XLSY changed comments but worksheet has no comments relationship")?;
            replacements.insert(
                comments_path,
                write_ooxml_spreadsheet_comments(&after.comments),
            );
            let vml_path = spreadsheet_vml_path(&mut archive, path)?
                .context("XLSY changed comments but worksheet has no VML relationship")?;
            replacements.insert(
                vml_path,
                write_ooxml_spreadsheet_comment_vml(&after.comments)?,
            );
        }
        let before_drawings = original_chart_drawings
            .get(&before.name)
            .cloned()
            .unwrap_or_default();
        let after_drawings = changed_chart_drawings
            .get(&after.name)
            .cloned()
            .unwrap_or_default();
        ensure!(
            before_drawings.len() == after_drawings.len(),
            "XLSY chart add/remove export is not implemented"
        );
        if before_drawings != after_drawings && !after_drawings.is_empty() {
            ensure!(
                after_drawings.len() == 1,
                "XLSY chart export currently supports one chart per worksheet"
            );
            let drawing_path =
                spreadsheet_worksheet_relationship_path(&mut archive, path, "/drawing")?
                    .context("XLSY changed chart but worksheet has no drawing relationship")?;
            let drawing_xml = read_zip_part(&mut archive, &drawing_path)?;
            replacements.insert(
                drawing_path.clone(),
                replace_spreadsheet_drawing_anchor(drawing_xml, &after_drawings[0])?,
            );
            if before_drawings[0].style != after_drawings[0].style {
                let chart_path =
                    spreadsheet_worksheet_relationship_path(&mut archive, &drawing_path, "/chart")?
                        .context(
                            "XLSY changed chart style but drawing has no chart relationship",
                        )?;
                let chart_xml = read_zip_part(&mut archive, &chart_path)?;
                replacements.insert(
                    chart_path,
                    replace_spreadsheet_chart_style(chart_xml, after_drawings[0].style)?,
                );
            }
        }
        ensure!(
            before.tables.len() == after.tables.len(),
            "XLSY table add/remove export is not implemented"
        );
        let paths = table_paths.get(&before.name).cloned().unwrap_or_default();
        ensure!(
            paths.len() == before.tables.len(),
            "worksheet table relationship count does not match XLSY"
        );
        for ((before_table, after_table), table_path) in
            before.tables.iter().zip(&after.tables).zip(paths)
        {
            ensure!(
                before_table.display_name == after_table.display_name
                    && before_table.reference == after_table.reference,
                "XLSY table identity changed"
            );
            let table_xml = read_zip_part(&mut archive, &table_path)?;
            let updated_table =
                replace_spreadsheet_table_filter_sort(&table_xml, before_table, after_table)?;
            if updated_table != table_xml {
                replacements.insert(table_path, updated_table);
            }
        }
    }
    drop(archive);
    replace_package_parts(original_package, replacements)
}

fn normalize_spreadsheet_builtin_defined_names(
    names: &[EditorDefinedNameManifest],
) -> Vec<EditorDefinedNameManifest> {
    names
        .iter()
        .cloned()
        .map(|mut name| {
            name.name = match name.name.as_str() {
                "Print_Area" => "_xlnm.Print_Area".to_string(),
                "Print_Titles" => "_xlnm.Print_Titles".to_string(),
                _ => name.name,
            };
            name
        })
        .collect()
}

fn normalize_pivot_cache_id(xml: &[u8], from: u32, to: u32) -> anyhow::Result<Vec<u8>> {
    if from == to {
        return Ok(xml.to_vec());
    }
    let source = std::str::from_utf8(xml).context("pivot table XML is not UTF-8")?;
    let pattern = Regex::new(&format!(r#"\bcacheId="{}""#, from))?;
    ensure!(
        pattern.is_match(source),
        "changed pivot table XML has no expected cacheId"
    );
    Ok(pattern
        .replace(source, format!(r#"cacheId="{}""#, to))
        .into_owned()
        .into_bytes())
}

fn spreadsheet_workbook_pivot_cache_path<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    cache_id: u32,
) -> anyhow::Result<String> {
    let workbook = read_zip_part(archive, "xl/workbook.xml")?;
    let document = roxmltree::Document::parse(std::str::from_utf8(&workbook)?)?;
    let node = document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "pivotCache"
                && node
                    .attribute("cacheId")
                    .and_then(|value| value.parse::<u32>().ok())
                    == Some(cache_id)
        })
        .context("workbook pivot cache is missing")?;
    let relationship = node
        .attributes()
        .find(|attribute| attribute.name() == "id")
        .context("pivot cache relationship id missing")?
        .value();
    spreadsheet_relationship_target(archive, "xl/workbook.xml", relationship)
}

fn spreadsheet_worksheet_pivot_paths<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
) -> anyhow::Result<Vec<String>> {
    let worksheet = read_zip_part(archive, worksheet_path)?;
    let document = roxmltree::Document::parse(std::str::from_utf8(&worksheet)?)?;
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "pivotTablePart")
        .map(|node| {
            let relationship = node
                .attributes()
                .find(|attribute| attribute.name() == "id")
                .context("pivotTablePart relationship missing")?
                .value();
            spreadsheet_relationship_target(archive, worksheet_path, relationship)
        })
        .collect()
}

fn replace_spreadsheet_print_layout(
    xml: Vec<u8>,
    print: &DecodedSpreadsheetPrintPivot,
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from_utf8(xml).context("worksheet print layout is not UTF-8")?;
    output = replace_or_insert_worksheet_node(
        output,
        "sheetPr",
        &spreadsheet_sheet_pr_xml(print),
        "<dimension",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "sheetViews",
        &spreadsheet_sheet_views_xml(print),
        "<sheetFormatPr",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "printOptions",
        &spreadsheet_print_options_xml(print),
        "<pageMargins",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "pageMargins",
        &spreadsheet_page_margins_xml(print),
        "<pageSetup",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "pageSetup",
        &spreadsheet_page_setup_xml(print),
        "<headerFooter",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "headerFooter",
        &spreadsheet_header_footer_xml(print),
        "<rowBreaks",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "rowBreaks",
        &spreadsheet_breaks_xml("rowBreaks", print.row_breaks.as_ref()),
        "<colBreaks",
    )?;
    output = replace_or_insert_worksheet_node(
        output,
        "colBreaks",
        &spreadsheet_breaks_xml("colBreaks", print.col_breaks.as_ref()),
        "<pivotTableParts",
    )?;
    Ok(output.into_bytes())
}

fn replace_or_insert_worksheet_node(
    mut xml: String,
    name: &str,
    replacement: &str,
    before: &str,
) -> anyhow::Result<String> {
    let regex = Regex::new(&format!(r#"(?s)<{name}\b[^>]*(?:/>|>.*?</{name}>)"#))?;
    if regex.is_match(&xml) {
        return Ok(regex.replace(&xml, replacement).into_owned());
    }
    if replacement.is_empty() {
        return Ok(xml);
    }
    if let Some(position) = xml.find(before) {
        xml.insert_str(position, replacement);
        return Ok(xml);
    }
    let position = xml
        .rfind("</worksheet>")
        .context("worksheet closing element missing")?;
    xml.insert_str(position, replacement);
    Ok(xml)
}

fn spreadsheet_sheet_pr_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    print
        .fit_to_page
        .map(|value| {
            format!(
                r#"<sheetPr><pageSetUpPr fitToPage="{}"/></sheetPr>"#,
                u8::from(value)
            )
        })
        .unwrap_or_default()
}

fn spreadsheet_sheet_views_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    print
        .view
        .map(|view| {
            format!(
                r#"<sheetViews><sheetView workbookViewId="0" view="{}"/></sheetViews>"#,
                ["normal", "pageBreakPreview", "pageLayout"]
                    .get(view as usize)
                    .unwrap_or(&"normal")
            )
        })
        .unwrap_or_default()
}

fn spreadsheet_print_options_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    let names = [
        "gridLines",
        "headings",
        "gridLinesSet",
        "horizontalCentered",
        "verticalCentered",
    ];
    let attrs = print
        .print_options
        .iter()
        .zip(names)
        .filter_map(|(value, name)| value.map(|value| format!(r#" {name}="{}""#, u8::from(value))))
        .collect::<String>();
    if attrs.is_empty() {
        String::new()
    } else {
        format!("<printOptions{attrs}/>")
    }
}

fn spreadsheet_page_margins_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    let names = ["left", "top", "right", "bottom", "header", "footer"];
    let attrs = print
        .margins
        .iter()
        .zip(names)
        .filter_map(|(value, name)| value.map(|value| format!(r#" {name}="{value}""#)))
        .collect::<String>();
    if attrs.is_empty() {
        String::new()
    } else {
        format!("<pageMargins{attrs}/>")
    }
}

fn spreadsheet_page_setup_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    let mut attrs = String::new();
    for (property, value) in &print.page_setup {
        let (name, text) = match (*property, value.as_slice()) {
            (0, [value]) => (
                "orientation",
                if *value == 0 {
                    "landscape".to_string()
                } else {
                    "portrait".to_string()
                },
            ),
            (1, [value]) => ("paperSize", value.to_string()),
            (2, [value]) => ("blackAndWhite", u8::from(*value != 0).to_string()),
            (3, [value]) => ("cellComments", value.to_string()),
            (4 | 7 | 8 | 9 | 10 | 15 | 18, bytes) if bytes.len() == 4 => (
                {
                    let name = match property {
                        4 => "copies",
                        7 => "firstPageNumber",
                        8 => "fitToHeight",
                        9 => "fitToWidth",
                        10 => "horizontalDpi",
                        15 => "scale",
                        _ => "verticalDpi",
                    };
                    name
                },
                u32::from_le_bytes(bytes.try_into().unwrap()).to_string(),
            ),
            (5, [value]) => ("draft", u8::from(*value != 0).to_string()),
            (6, [value]) => ("errors", value.to_string()),
            (11, [value]) => (
                "pageOrder",
                if *value == 0 {
                    "downThenOver".to_string()
                } else {
                    "overThenDown".to_string()
                },
            ),
            (16, [value]) => ("useFirstPageNumber", u8::from(*value != 0).to_string()),
            (17, [value]) => ("usePrinterDefaults", u8::from(*value != 0).to_string()),
            _ => continue,
        };
        attrs.push_str(&format!(r#" {name}="{text}""#));
    }
    if attrs.is_empty() {
        String::new()
    } else {
        format!("<pageSetup{attrs}/>")
    }
}

fn spreadsheet_header_footer_xml(print: &DecodedSpreadsheetPrintPivot) -> String {
    if print.header_footer_flags.is_empty() && print.header_footer_text.is_empty() {
        return String::new();
    }
    let flag_names = [
        "alignWithMargins",
        "differentFirst",
        "differentOddEven",
        "scaleWithDoc",
    ];
    let text_names = [
        "",
        "",
        "",
        "",
        "evenFooter",
        "evenHeader",
        "firstFooter",
        "firstHeader",
        "oddFooter",
        "oddHeader",
    ];
    let flags = print
        .header_footer_flags
        .iter()
        .filter_map(|(property, value)| {
            flag_names
                .get(*property as usize)
                .map(|name| format!(r#" {name}="{}""#, u8::from(*value)))
        })
        .collect::<String>();
    let children = print
        .header_footer_text
        .iter()
        .filter_map(|(property, value)| {
            text_names
                .get(*property as usize)
                .filter(|name| !name.is_empty())
                .map(|name| format!("<{name}>{}</{name}>", xml_escape_text(value)))
        })
        .collect::<String>();
    format!("<headerFooter{flags}>{children}</headerFooter>")
}

fn spreadsheet_breaks_xml(name: &str, breaks: Option<&SpreadsheetBreaks>) -> String {
    let Some(breaks) = breaks else {
        return String::new();
    };
    let children = breaks
        .breaks
        .iter()
        .map(|item| {
            format!(
                r#"<brk id="{}" min="{}" max="{}" man="{}"/>"#,
                item.id,
                item.min,
                item.max,
                u8::from(item.manual)
            )
        })
        .collect::<String>();
    format!(
        r#"<{name} count="{}" manualBreakCount="{}">{children}</{name}>"#,
        breaks.count, breaks.manual_count
    )
}

fn replace_spreadsheet_drawing_anchor(
    xml: Vec<u8>,
    drawing: &DecodedSpreadsheetChartDrawing,
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from_utf8(xml).context("spreadsheet drawing is not UTF-8")?;
    let from = spreadsheet_drawing_point_xml(
        "from",
        drawing.from_col,
        drawing.from_col_off_mm,
        drawing.from_row,
        drawing.from_row_off_mm,
    );
    let to = spreadsheet_drawing_point_xml(
        "to",
        drawing.to_col,
        drawing.to_col_off_mm,
        drawing.to_row,
        drawing.to_row_off_mm,
    );
    let from_regex = Regex::new(r#"(?s)<xdr:from>.*?</xdr:from>"#)?;
    let to_regex = Regex::new(r#"(?s)<xdr:to>.*?</xdr:to>"#)?;
    ensure!(
        from_regex.is_match(&output),
        "drawing has no xdr:from anchor"
    );
    ensure!(to_regex.is_match(&output), "drawing has no xdr:to anchor");
    output = from_regex.replace(&output, from).into_owned();
    output = to_regex.replace(&output, to).into_owned();
    if let Some([off_x, off_y, ext_x, ext_y]) = drawing.xfrm_emu {
        let off_regex = Regex::new(r#"<a:off\b[^>]*/>"#)?;
        let ext_regex = Regex::new(r#"<a:ext\b[^>]*/>"#)?;
        ensure!(
            off_regex.is_match(&output),
            "drawing has no a:off transform"
        );
        ensure!(
            ext_regex.is_match(&output),
            "drawing has no a:ext transform"
        );
        output = off_regex
            .replace(&output, format!(r#"<a:off x="{off_x}" y="{off_y}"/>"#))
            .into_owned();
        output = ext_regex
            .replace(&output, format!(r#"<a:ext cx="{ext_x}" cy="{ext_y}"/>"#))
            .into_owned();
    }
    Ok(output.into_bytes())
}

fn spreadsheet_drawing_point_xml(
    name: &str,
    column: u32,
    column_offset_mm: f64,
    row: u32,
    row_offset_mm: f64,
) -> String {
    let to_emu = |value: f64| (value * 36_000.0).round() as i64;
    format!(
        "<xdr:{name}><xdr:col>{column}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:{name}>",
        to_emu(column_offset_mm),
        to_emu(row_offset_mm),
    )
}

fn replace_spreadsheet_chart_style(xml: Vec<u8>, style: Option<u8>) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from_utf8(xml).context("spreadsheet chart is not UTF-8")?;
    let style_regex = Regex::new(r#"<c:style\b[^>]*/>"#)?;
    output = style_regex.replace_all(&output, "").into_owned();
    if let Some(style) = style {
        let position = output
            .find("<c:chart>")
            .context("chart space has no c:chart element")?;
        output.insert_str(position, &format!("<c:style val=\"{style}\"/>"));
    }
    Ok(output.into_bytes())
}

fn replace_spreadsheet_workbook_metadata(
    xml: Vec<u8>,
    names: &[EditorDefinedNameManifest],
    protection: Option<&EditorWorkbookProtectionManifest>,
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from_utf8(xml).context("workbook is not UTF-8")?;
    let names_regex = Regex::new(r#"(?s)<definedNames\b[^>]*>.*?</definedNames>"#)?;
    output = names_regex.replace_all(&output, "").into_owned();
    let protection_regex = Regex::new(r#"<workbookProtection\b[^>]*/>"#)?;
    output = protection_regex.replace_all(&output, "").into_owned();
    let mut replacement = String::new();
    if let Some(value) = protection {
        replacement.push_str("<workbookProtection");
        if let Some(password) = value.password.as_deref() {
            replacement.push_str(&format!(
                " workbookPassword=\"{}\"",
                escape_xml_attribute(password)
            ));
        }
        for (name, enabled) in [
            ("lockStructure", value.lock_structure),
            ("lockWindows", value.lock_windows),
            ("lockRevision", value.lock_revision),
        ] {
            if enabled {
                replacement.push_str(&format!(" {name}=\"1\""));
            }
        }
        replacement.push_str("/>");
    }
    if !names.is_empty() {
        replacement.push_str("<definedNames>");
        for name in names {
            replacement.push_str(&format!(
                "<definedName name=\"{}\"",
                escape_xml_attribute(&name.name)
            ));
            if let Some(local) = name.local_sheet_id {
                replacement.push_str(&format!(" localSheetId=\"{local}\""));
            }
            if name.hidden {
                replacement.push_str(" hidden=\"1\"");
            }
            replacement.push_str(&format!(
                ">{}</definedName>",
                escape_xml_text(&name.reference)
            ));
        }
        replacement.push_str("</definedNames>");
    }
    let position = output
        .find("</workbook>")
        .context("workbook closing element is missing")?;
    output.insert_str(position, &replacement);
    Ok(output.into_bytes())
}

fn replace_spreadsheet_sheet_protection(
    xml: Vec<u8>,
    protection: Option<&EditorSheetProtectionManifest>,
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from_utf8(xml).context("worksheet is not UTF-8")?;
    let regex = Regex::new(r#"<sheetProtection\b[^>]*/>"#)?;
    let Some(value) = protection else {
        return Ok(regex.replace_all(&output, "").into_owned().into_bytes());
    };
    let mut element = String::from("<sheetProtection");
    if let Some(password) = value.password.as_deref() {
        element.push_str(&format!(" password=\"{}\"", escape_xml_attribute(password)));
    }
    for (name, enabled) in [
        ("sheet", value.sheet),
        ("objects", value.objects),
        ("scenarios", value.scenarios),
        ("formatCells", value.format_cells),
        ("formatColumns", value.format_columns),
        ("formatRows", value.format_rows),
        ("insertColumns", value.insert_columns),
        ("insertHyperlinks", value.insert_hyperlinks),
        ("insertRows", value.insert_rows),
        ("deleteColumns", value.delete_columns),
        ("deleteRows", value.delete_rows),
        ("selectLockedCells", value.select_locked_cells),
        ("sort", value.sort),
        ("autoFilter", value.auto_filter),
        ("pivotTables", value.pivot_tables),
        ("selectUnlockedCells", value.select_unlocked_cells),
    ] {
        element.push_str(&format!(" {name}=\"{}\"", u8::from(enabled)));
    }
    element.push_str("/>");
    if regex.is_match(&output) {
        return Ok(regex
            .replace_all(&output, element.as_str())
            .into_owned()
            .into_bytes());
    }
    let position = output
        .find("</worksheet>")
        .context("worksheet closing element is missing")?;
    output.insert_str(position, &element);
    Ok(output.into_bytes())
}

fn spreadsheet_comment_path<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
) -> anyhow::Result<Option<String>> {
    spreadsheet_worksheet_relationship_path(archive, worksheet_path, "/comments")
}

fn spreadsheet_vml_path<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
) -> anyhow::Result<Option<String>> {
    spreadsheet_worksheet_relationship_path(archive, worksheet_path, "/vmlDrawing")
}

fn spreadsheet_worksheet_relationship_path<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    relationship_suffix: &str,
) -> anyhow::Result<Option<String>> {
    let worksheet = Path::new(worksheet_path);
    let filename = worksheet
        .file_name()
        .and_then(|value| value.to_str())
        .context("worksheet path has no filename")?;
    let parent = worksheet
        .parent()
        .and_then(|value| value.to_str())
        .context("worksheet path has no parent")?;
    let Some(xml) = read_optional_zip_part(archive, &format!("{parent}/_rels/{filename}.rels"))?
    else {
        return Ok(None);
    };
    let document = roxmltree::Document::parse(
        std::str::from_utf8(&xml).context("worksheet relationships are not UTF-8")?,
    )?;
    document
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node
                    .attribute("Type")
                    .is_some_and(|value| value.ends_with(relationship_suffix))
        })
        .and_then(|node| node.attribute("Target"))
        .map(|target| normalize_ooxml_relationship_target(parent, target))
        .transpose()
}

fn write_ooxml_spreadsheet_comment_vml(
    comments: &[EditorSpreadsheetCommentManifest],
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from(
        r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t" o:connecttype="rect"/></v:shapetype>"#,
    );
    for (index, comment) in comments.iter().enumerate() {
        let column = parse_cell_column(&comment.reference)?;
        let row = comment
            .reference
            .chars()
            .skip_while(|value| value.is_ascii_alphabetic())
            .collect::<String>()
            .parse::<u32>()
            .context("spreadsheet comment row is invalid")?
            .checked_sub(1)
            .context("spreadsheet comment row must be positive")?;
        output.push_str(&format!(
            r##"<v:shape id="_x0000_s{}" type="#_x0000_t202" style="position:absolute;margin-left:80pt;margin-top:5pt;width:108pt;height:59pt;z-index:{};visibility:hidden" fillcolor="#ffffe1" o:insetmode="auto"><v:fill color2="#ffffe1"/><v:shadow on="t" color="black" obscured="t"/><v:path o:connecttype="none"/><v:textbox style="mso-direction-alt:auto"><div style="text-align:left"/></v:textbox><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 2, {}, 31, {}, 1</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{}</x:Column></x:ClientData></v:shape>"##,
            1025 + index,
            index + 1,
            column,
            row,
            column + 2,
            row + 3,
            row,
            column,
        ));
    }
    output.push_str("</xml>");
    Ok(output.into_bytes())
}

fn write_ooxml_spreadsheet_comments(comments: &[EditorSpreadsheetCommentManifest]) -> Vec<u8> {
    let mut authors = Vec::<String>::new();
    for comment in comments {
        if !authors.contains(&comment.author) {
            authors.push(comment.author.clone());
        }
    }
    let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors>");
    for author in &authors {
        output.push_str(&format!("<author>{}</author>", escape_xml_text(author)));
    }
    output.push_str("</authors><commentList>");
    for comment in comments {
        let author_id = authors
            .iter()
            .position(|author| author == &comment.author)
            .unwrap_or(0);
        output.push_str(&format!(
            "<comment ref=\"{}\" authorId=\"{author_id}\"><text><t>{}</t></text></comment>",
            escape_xml_attribute(&comment.reference),
            escape_xml_text(&comment.text)
        ));
    }
    output.push_str("</commentList></comments>");
    output.into_bytes()
}

fn replace_spreadsheet_validation_conditional(
    xml: Vec<u8>,
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
    differential_formats: &[EditorDifferentialStyleManifest],
) -> anyhow::Result<Vec<u8>> {
    if before.data_validations == after.data_validations
        && before.conditional_formats == after.conditional_formats
    {
        return Ok(xml);
    }
    let mut output = String::from_utf8(xml).context("worksheet is not UTF-8")?;
    let conditional_regex =
        Regex::new(r#"(?s)<conditionalFormatting\b[^>]*>.*?</conditionalFormatting>"#)?;
    output = conditional_regex.replace_all(&output, "").into_owned();
    let validation_regex = Regex::new(r#"(?s)<dataValidations\b[^>]*>.*?</dataValidations>"#)?;
    output = validation_regex.replace_all(&output, "").into_owned();
    let mut replacement = String::new();
    for conditional in &after.conditional_formats {
        replacement.push_str(&write_ooxml_conditional_format(
            conditional,
            differential_formats,
        )?);
    }
    if !after.data_validations.is_empty() {
        replacement.push_str(&format!(
            "<dataValidations count=\"{}\">{}</dataValidations>",
            after.data_validations.len(),
            after
                .data_validations
                .iter()
                .map(write_ooxml_data_validation)
                .collect::<String>()
        ));
    }
    let position = output
        .find("</worksheet>")
        .context("worksheet closing element is missing")?;
    output.insert_str(position, &replacement);
    Ok(output.into_bytes())
}

fn write_ooxml_data_validation(validation: &EditorDataValidationManifest) -> String {
    let mut attributes = vec![
        format!(
            "type=\"{}\"",
            escape_xml_attribute(&validation.validation_type)
        ),
        format!("allowBlank=\"{}\"", u8::from(validation.allow_blank)),
        format!(
            "showErrorMessage=\"{}\"",
            u8::from(validation.show_error_message)
        ),
        format!("sqref=\"{}\"", escape_xml_attribute(&validation.reference)),
    ];
    for (name, value) in [
        ("operator", validation.operator.as_deref()),
        ("errorStyle", validation.error_style.as_deref()),
        ("errorTitle", validation.error_title.as_deref()),
        ("error", validation.error.as_deref()),
    ] {
        if let Some(value) = value {
            attributes.push(format!("{name}=\"{}\"", escape_xml_attribute(value)));
        }
    }
    let formulas = [
        ("formula1", validation.formula1.as_deref()),
        ("formula2", validation.formula2.as_deref()),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        value.map(|value| format!("<{name}>{}</{name}>", escape_xml_text(value)))
    })
    .collect::<String>();
    format!(
        "<dataValidation {}>{formulas}</dataValidation>",
        attributes.join(" ")
    )
}

fn write_ooxml_conditional_format(
    conditional: &EditorConditionalFormatManifest,
    differential_formats: &[EditorDifferentialStyleManifest],
) -> anyhow::Result<String> {
    let mut attributes = vec![
        format!("type=\"{}\"", escape_xml_attribute(&conditional.rule_type)),
        format!("priority=\"{}\"", conditional.priority),
    ];
    if let Some(operator) = &conditional.operator {
        attributes.push(format!("operator=\"{}\"", escape_xml_attribute(operator)));
    }
    if let Some(style) = &conditional.differential_style {
        let dxf_id = differential_formats
            .iter()
            .position(|candidate| differential_styles_rgb_equal(candidate, style))
            .context("conditional differential style changed but XLSX dxf materialization is not implemented")?;
        attributes.push(format!("dxfId=\"{dxf_id}\""));
    }
    let mut body = conditional
        .formulas
        .iter()
        .map(|formula| format!("<formula>{}</formula>", escape_xml_text(formula)))
        .collect::<String>();
    if conditional.rule_type == "colorScale" {
        body.push_str("<colorScale>");
        for threshold in &conditional.thresholds {
            body.push_str(&format!(
                "<cfvo type=\"{}\"{} />",
                escape_xml_attribute(&threshold.threshold_type),
                threshold
                    .value
                    .as_ref()
                    .map_or_else(String::new, |value| format!(
                        " val=\"{}\"",
                        escape_xml_attribute(value)
                    ))
            ));
        }
        for color in &conditional.colors {
            body.push_str(&format!(
                "<color rgb=\"{}\"/>",
                escape_xml_attribute(&ooxml_argb(color))
            ));
        }
        body.push_str("</colorScale>");
    }
    Ok(format!(
        "<conditionalFormatting sqref=\"{}\"><cfRule {}>{body}</cfRule></conditionalFormatting>",
        escape_xml_attribute(&conditional.reference),
        attributes.join(" ")
    ))
}

fn differential_styles_rgb_equal(
    left: &EditorDifferentialStyleManifest,
    right: &EditorDifferentialStyleManifest,
) -> bool {
    normalized_rgb(&left.fill_rgb) == normalized_rgb(&right.fill_rgb)
        && normalized_rgb(&left.font_rgb) == normalized_rgb(&right.font_rgb)
}

fn normalized_rgb(value: &Option<String>) -> Option<String> {
    value.as_deref().map(|value| {
        value
            .get(value.len().saturating_sub(6)..)
            .unwrap_or(value)
            .to_string()
    })
}

fn ooxml_argb(value: &str) -> String {
    let rgb = value.get(value.len().saturating_sub(6)..).unwrap_or(value);
    format!("FF{rgb}")
}

fn spreadsheet_table_paths<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_paths: &BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let mut result = BTreeMap::new();
    for (sheet_name, worksheet_path) in worksheet_paths {
        let worksheet = Path::new(worksheet_path);
        let filename = worksheet
            .file_name()
            .and_then(|value| value.to_str())
            .context("worksheet path has no filename")?;
        let parent = worksheet
            .parent()
            .and_then(|value| value.to_str())
            .context("worksheet path has no parent")?;
        let relationships_path = format!("{parent}/_rels/{filename}.rels");
        let Some(relationships) = read_optional_zip_part(archive, &relationships_path)? else {
            result.insert(sheet_name.clone(), Vec::new());
            continue;
        };
        let document = roxmltree::Document::parse(
            std::str::from_utf8(&relationships).context("worksheet relationships are not UTF-8")?,
        )?;
        let paths = document
            .descendants()
            .filter(|node| {
                node.is_element()
                    && node.tag_name().name() == "Relationship"
                    && node
                        .attribute("Type")
                        .is_some_and(|value| value.ends_with("/table"))
            })
            .map(|node| {
                normalize_ooxml_relationship_target(
                    parent,
                    node.attribute("Target")
                        .context("table relationship has no target")?,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        result.insert(sheet_name.clone(), paths);
    }
    Ok(result)
}

fn replace_spreadsheet_table_filter_sort(
    xml: &[u8],
    before: &EditorTableManifest,
    after: &EditorTableManifest,
) -> anyhow::Result<Vec<u8>> {
    if before.filters == after.filters && before.sort == after.sort {
        return Ok(xml.to_vec());
    }
    let mut output = std::str::from_utf8(xml)
        .context("spreadsheet table is not UTF-8")?
        .to_string();
    let filters = after
        .filters
        .iter()
        .map(|column| {
            format!(
                "<filterColumn colId=\"{}\"><filters>{}</filters></filterColumn>",
                column.column_id,
                column
                    .values
                    .iter()
                    .map(|value| format!("<filter val=\"{}\"/>", escape_xml_attribute(value)))
                    .collect::<String>()
            )
        })
        .collect::<String>();
    let auto_filter = if filters.is_empty() {
        format!(
            "<autoFilter ref=\"{}\"/>",
            escape_xml_attribute(&after.reference)
        )
    } else {
        format!(
            "<autoFilter ref=\"{}\">{filters}</autoFilter>",
            escape_xml_attribute(&after.reference)
        )
    };
    let auto_filter_regex = Regex::new(r#"(?s)<autoFilter\b[^>]*(?:/>|>.*?</autoFilter>)"#)?;
    let found = auto_filter_regex
        .find(&output)
        .context("table autoFilter element is missing")?;
    output.replace_range(found.range(), &auto_filter);

    let sort_regex = Regex::new(r#"(?s)<sortState\b[^>]*>.*?</sortState>|<sortState\b[^>]*/>"#)?;
    if let Some(found) = sort_regex.find(&output) {
        output.replace_range(found.range(), "");
    }
    if let Some(sort) = &after.sort {
        let sort_xml = format!(
            "<sortState ref=\"{}\"><sortCondition{} ref=\"{}\"/></sortState>",
            escape_xml_attribute(&sort.reference),
            if sort.descending {
                " descending=\"1\""
            } else {
                ""
            },
            escape_xml_attribute(&sort.condition_reference)
        );
        let position = output
            .find("<tableColumns")
            .context("tableColumns element is missing")?;
        output.insert_str(position, &sort_xml);
    }
    Ok(output.into_bytes())
}

fn spreadsheet_styles_equivalent(
    before_id: u32,
    before: &SpreadsheetSourceStyles,
    after_id: u32,
    after: &SpreadsheetSourceStyles,
) -> anyhow::Result<bool> {
    let before_xf = before
        .cell_xfs
        .get(before_id as usize)
        .with_context(|| format!("original XLSY style is missing: {before_id}"))?;
    let after_xf = after
        .cell_xfs
        .get(after_id as usize)
        .with_context(|| format!("changed XLSY style is missing: {after_id}"))?;
    let before_font = before
        .fonts
        .get(before_xf.font_id as usize)
        .with_context(|| format!("original XLSY font is missing: {}", before_xf.font_id))?;
    let after_font = after
        .fonts
        .get(after_xf.font_id as usize)
        .with_context(|| format!("changed XLSY font is missing: {}", after_xf.font_id))?;
    let before_num = before.number_formats.get(&before_xf.num_fmt_id);
    let after_num = after.number_formats.get(&after_xf.num_fmt_id);
    let number_format_equal =
        if before_xf.num_fmt_id == after_xf.num_fmt_id && before_xf.num_fmt_id < 164 {
            true
        } else {
            before_num == after_num
        };
    Ok(spreadsheet_fonts_equivalent(before_font, after_font)
        && number_format_equal
        && before_xf.fill_id == after_xf.fill_id
        && before_xf.border_id == after_xf.border_id
        && before_xf.horizontal_alignment == after_xf.horizontal_alignment)
}

fn materialize_spreadsheet_styles(
    xml: &[u8],
    changed: &SpreadsheetSourceStyles,
    needed: &BTreeSet<u32>,
) -> anyhow::Result<(Vec<u8>, BTreeMap<u32, u32>)> {
    let mut source = std::str::from_utf8(xml)
        .context("spreadsheet styles are not UTF-8")?
        .to_string();
    let mut materialized = parse_ooxml_styles(xml)?;
    let mut style_map = BTreeMap::new();
    let mut font_entries = Vec::new();
    let mut format_entries = Vec::new();
    let mut xf_entries = Vec::new();
    for style_id in needed {
        let changed_xf = changed
            .cell_xfs
            .get(*style_id as usize)
            .with_context(|| format!("changed XLSY style is missing: {style_id}"))?
            .clone();
        let changed_font = changed
            .fonts
            .get(changed_xf.font_id as usize)
            .with_context(|| format!("changed XLSY font is missing: {}", changed_xf.font_id))?
            .clone();
        let font_id = if let Some(index) = materialized
            .fonts
            .iter()
            .position(|font| spreadsheet_fonts_equivalent(font, &changed_font))
        {
            index as u32
        } else {
            let index = materialized.fonts.len() as u32;
            font_entries.push(spreadsheet_font_xml(&changed_font));
            materialized.fonts.push(changed_font);
            index
        };
        let num_fmt_id = if let Some(code) = changed.number_formats.get(&changed_xf.num_fmt_id) {
            if let Some((id, _)) = materialized
                .number_formats
                .iter()
                .find(|(_, existing)| *existing == code)
            {
                *id
            } else {
                materialized
                    .number_formats
                    .insert(changed_xf.num_fmt_id, code.clone());
                format_entries.push(format!(
                    "<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
                    changed_xf.num_fmt_id,
                    escape_xml_attribute(code)
                ));
                changed_xf.num_fmt_id
            }
        } else {
            changed_xf.num_fmt_id
        };
        let mut mapped = changed_xf;
        mapped.font_id = font_id;
        mapped.num_fmt_id = num_fmt_id;
        if let Some(index) = materialized.cell_xfs.iter().position(|xf| xf == &mapped) {
            style_map.insert(*style_id, index as u32);
        } else {
            let index = materialized.cell_xfs.len() as u32;
            xf_entries.push(spreadsheet_xf_xml(&mapped));
            materialized.cell_xfs.push(mapped);
            style_map.insert(*style_id, index);
        }
    }
    if !format_entries.is_empty() {
        if source.contains("</numFmts>") {
            source = append_spreadsheet_style_entries(&source, "numFmts", &format_entries)?;
        } else {
            let root_start = source
                .find("<styleSheet")
                .context("spreadsheet style root is missing")?;
            let root_end = root_start
                + source[root_start..]
                    .find('>')
                    .context("spreadsheet style root is truncated")?
                + 1;
            source.insert_str(
                root_end,
                &format!(
                    "<numFmts count=\"{}\">{}</numFmts>",
                    format_entries.len(),
                    format_entries.join("")
                ),
            );
        }
    }
    if !font_entries.is_empty() {
        source = append_spreadsheet_style_entries(&source, "fonts", &font_entries)?;
    }
    if !xf_entries.is_empty() {
        source = append_spreadsheet_style_entries(&source, "cellXfs", &xf_entries)?;
    }
    Ok((source.into_bytes(), style_map))
}

fn append_spreadsheet_style_entries(
    source: &str,
    tag: &str,
    entries: &[String],
) -> anyhow::Result<String> {
    let closing = format!("</{tag}>");
    let position = source
        .find(&closing)
        .with_context(|| format!("spreadsheet style collection is missing: {tag}"))?;
    let mut output = source.to_string();
    output.insert_str(position, &entries.join(""));
    let opening = Regex::new(&format!(r#"<{tag}\b[^>]*\bcount=\"(\d+)\"[^>]*>"#))
        .context("compile style count regex")?;
    let capture = opening
        .captures(&output)
        .with_context(|| format!("spreadsheet style count is missing: {tag}"))?;
    let count = capture
        .get(1)
        .expect("count capture")
        .as_str()
        .parse::<usize>()?
        + entries.len();
    let range = capture.get(1).expect("count capture").range();
    output.replace_range(range, &count.to_string());
    Ok(output)
}

fn spreadsheet_font_xml(font: &SpreadsheetSourceFont) -> String {
    let mut body = String::new();
    if font.bold {
        body.push_str("<b/>");
    }
    if font.italic {
        body.push_str("<i/>");
    }
    if let Some(color) = font.color {
        body.push_str(&format!("<color rgb=\"{color:08X}\"/>"));
    }
    if let Some(size) = font.size {
        body.push_str(&format!(
            "<sz val=\"{}\"/>",
            format_spreadsheet_number(size)
        ));
    }
    if let Some(name) = &font.name {
        body.push_str(&format!("<name val=\"{}\"/>", escape_xml_attribute(name)));
    }
    format!("<font>{body}</font>")
}

fn spreadsheet_fonts_equivalent(
    left: &SpreadsheetSourceFont,
    right: &SpreadsheetSourceFont,
) -> bool {
    left.bold == right.bold
        && left.italic == right.italic
        && left.size == right.size
        && left.name == right.name
}

fn spreadsheet_xf_xml(xf: &SpreadsheetSourceXf) -> String {
    let mut attributes = format!(
        "fontId=\"{}\" fillId=\"{}\" borderId=\"{}\" numFmtId=\"{}\"",
        xf.font_id, xf.fill_id, xf.border_id, xf.num_fmt_id
    );
    if let Some(xf_id) = xf.xf_id {
        attributes.push_str(&format!(" xfId=\"{xf_id}\""));
    }
    if xf.apply_font || xf.font_id != 0 {
        attributes.push_str(" applyFont=\"1\"");
    }
    if xf.apply_fill || xf.fill_id != 0 {
        attributes.push_str(" applyFill=\"1\"");
    }
    if xf.num_fmt_id != 0 {
        attributes.push_str(" applyNumberFormat=\"1\"");
    }
    format!("<xf {attributes}/>")
}

fn escape_xml_attribute(value: &str) -> String {
    escape_xml_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn materialize_spreadsheet_shared_strings(
    original: &EditorPayloadManifest,
    changed: &EditorPayloadManifest,
) -> anyhow::Result<Vec<String>> {
    let mut output = original.shared_strings.clone();
    let used_after = changed
        .worksheets
        .iter()
        .flat_map(|sheet| &sheet.cells)
        .filter(|cell| cell.value_type == "shared_string")
        .map(|cell| cell.display.clone())
        .collect::<BTreeSet<_>>();
    for (before_sheet, after_sheet) in original.worksheets.iter().zip(&changed.worksheets) {
        ensure!(
            before_sheet.name == after_sheet.name,
            "XLSY worksheet order changed while materializing shared strings"
        );
        let before_cells = before_sheet
            .cells
            .iter()
            .map(|cell| (cell.reference.as_str(), cell))
            .collect::<BTreeMap<_, _>>();
        for after_cell in &after_sheet.cells {
            if after_cell.value_type != "shared_string"
                || output.iter().any(|value| value == &after_cell.display)
            {
                continue;
            }
            let before_cell = before_cells
                .get(after_cell.reference.as_str())
                .with_context(|| format!("original cell is missing: {}", after_cell.reference))?;
            let reusable = before_cell.value_type == "shared_string"
                && !used_after.contains(&before_cell.display);
            if reusable {
                let index = output
                    .iter()
                    .position(|value| value == &before_cell.display)
                    .with_context(|| {
                        format!("original shared string is missing: {}", before_cell.display)
                    })?;
                output[index] = after_cell.display.clone();
            } else {
                output.push(after_cell.display.clone());
            }
        }
    }
    Ok(output)
}

fn spreadsheet_worksheet_paths<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> anyhow::Result<BTreeMap<String, String>> {
    let workbook = read_zip_part(archive, "xl/workbook.xml")?;
    let relationships = read_zip_part(archive, "xl/_rels/workbook.xml.rels")?;
    let relationship_document = roxmltree::Document::parse(
        std::str::from_utf8(&relationships).context("workbook relationships are not UTF-8")?,
    )?;
    let targets = relationship_document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Relationship")
        .filter_map(|node| Some((node.attribute("Id")?, node.attribute("Target")?)))
        .collect::<BTreeMap<_, _>>();
    let workbook_document = roxmltree::Document::parse(
        std::str::from_utf8(&workbook).context("workbook is not UTF-8")?,
    )?;
    workbook_document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "sheet")
        .map(|sheet| {
            let name = sheet
                .attribute("name")
                .context("workbook sheet has no name")?;
            let relationship_id = sheet
                .attributes()
                .find(|attribute| attribute.name() == "id")
                .map(|attribute| attribute.value())
                .context("workbook sheet has no relationship id")?;
            let target = targets
                .get(relationship_id)
                .with_context(|| format!("worksheet relationship is missing: {relationship_id}"))?;
            ensure!(!target.contains(".."), "unsafe worksheet target: {target}");
            let path = if target.starts_with("xl/") {
                (*target).to_string()
            } else {
                format!("xl/{target}")
            };
            Ok((name.to_string(), path))
        })
        .collect()
}

fn replace_changed_shared_strings(
    xml: &[u8],
    before: &[String],
    after: &[String],
) -> anyhow::Result<Vec<u8>> {
    let source = std::str::from_utf8(xml)
        .context("shared strings are not UTF-8")?
        .to_string();
    let item = Regex::new(r"(?s)<si(?:\s[^>]*)?>.*?</si>").expect("shared-string regex");
    let ranges = item
        .find_iter(&source)
        .map(|value| value.range())
        .collect::<Vec<_>>();
    ensure!(
        ranges.len() == before.len(),
        "sharedStrings.xml item count does not match XLSY"
    );
    let first = ranges
        .first()
        .context("sharedStrings.xml has no si elements")?;
    let last = ranges.last().expect("non-empty shared string ranges");
    let mut items = String::new();
    for value in after {
        if let Some(index) = before.iter().position(|candidate| candidate == value) {
            items.push_str(&source[ranges[index].clone()]);
        } else {
            items.push_str(&format!("<si><t>{}</t></si>", escape_xml_text(value)));
        }
    }
    let mut output = String::with_capacity(source.len() + items.len());
    output.push_str(&source[..first.start]);
    output.push_str(&items);
    output.push_str(&source[last.end..]);
    let unique_count = Regex::new(r#"uniqueCount="\d+""#).expect("uniqueCount regex");
    output = unique_count
        .replace(&output, format!("uniqueCount=\"{}\"", after.len()))
        .into_owned();
    Ok(output.into_bytes())
}

fn replace_changed_worksheet_cells(
    xml: &[u8],
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
    original_shared_strings: &[String],
    shared_strings: &[String],
    original_styles: &SpreadsheetSourceStyles,
    changed_styles: &SpreadsheetSourceStyles,
    style_map: &BTreeMap<u32, u32>,
) -> anyhow::Result<Vec<u8>> {
    let before_cells = before
        .cells
        .iter()
        .map(|cell| (cell.reference.as_str(), cell))
        .collect::<BTreeMap<_, _>>();
    let after_cells = after
        .cells
        .iter()
        .map(|cell| (cell.reference.as_str(), cell))
        .collect::<BTreeMap<_, _>>();
    let mut output = std::str::from_utf8(xml)
        .context("worksheet is not UTF-8")?
        .to_string();
    for (reference, changed_cell) in &after_cells {
        let original_cell = before_cells.get(reference).copied();
        let shared_string_index_changed = original_cell
            .is_some_and(|cell| cell.value_type == "shared_string")
            && changed_cell.value_type == "shared_string"
            && original_shared_strings
                .iter()
                .position(|value| value == &original_cell.unwrap().display)
                != shared_strings
                    .iter()
                    .position(|value| value == &changed_cell.display);
        let style_changed = !spreadsheet_styles_equivalent(
            original_cell.map(|cell| cell.style_id).unwrap_or(0),
            original_styles,
            changed_cell.style_id,
            changed_styles,
        )?;
        if let Some(original_cell) = original_cell {
            if original_cell.value_type == changed_cell.value_type
                && original_cell.display == changed_cell.display
                && original_cell.formula == changed_cell.formula
                && !shared_string_index_changed
                && !style_changed
            {
                continue;
            }
        }
        let pattern = format!(
            r#"(?s)<c\b[^>]*\br="{}"[^>]*>.*?</c>"#,
            regex::escape(reference)
        );
        let cell_regex = Regex::new(&pattern).context("compile worksheet cell regex")?;
        let found = cell_regex.find(&output);
        let materialized_style = if style_changed {
            *style_map
                .get(&changed_cell.style_id)
                .with_context(|| format!("materialized style is missing for {reference}"))?
        } else {
            original_cell.map(|cell| cell.style_id).unwrap_or(0)
        };
        let style = if materialized_style == 0 {
            String::new()
        } else {
            format!(" s=\"{materialized_style}\"")
        };
        let formula = changed_cell
            .formula
            .as_deref()
            .map(|value| {
                format!(
                    "<f>{}</f>",
                    escape_xml_text(value.strip_prefix('=').unwrap_or(value))
                )
            })
            .unwrap_or_default();
        let replacement = match changed_cell.value_type.as_str() {
            "shared_string" => {
                ensure!(
                    changed_cell.formula.is_none(),
                    "formula cache cannot use shared strings"
                );
                let index = shared_strings
                    .iter()
                    .position(|value| value == &changed_cell.display)
                    .with_context(|| format!("shared string is missing for {reference}"))?;
                format!("<c r=\"{reference}\"{style} t=\"s\"><v>{index}</v></c>")
            }
            "number" => format!(
                "<c r=\"{reference}\"{style}>{formula}<v>{}</v></c>",
                escape_xml_text(&changed_cell.display)
            ),
            "boolean" => format!(
                "<c r=\"{reference}\"{style} t=\"b\">{formula}<v>{}</v></c>",
                if changed_cell.display.eq_ignore_ascii_case("true") {
                    "1"
                } else {
                    "0"
                }
            ),
            "error" => format!(
                "<c r=\"{reference}\"{style} t=\"e\">{formula}<v>{}</v></c>",
                escape_xml_text(&changed_cell.display)
            ),
            "string" => format!(
                "<c r=\"{reference}\"{style} t=\"str\">{formula}<v>{}</v></c>",
                escape_xml_text(&changed_cell.display)
            ),
            "blank" => format!("<c r=\"{reference}\"{style}/>"),
            value => anyhow::bail!("unsupported XLSY cell value type for export: {value}"),
        };
        if let Some(found) = found {
            output.replace_range(found.range(), &replacement);
        } else {
            let row_number =
                reference.trim_start_matches(|character: char| character.is_ascii_alphabetic());
            ensure!(
                !row_number.is_empty(),
                "invalid worksheet cell reference: {reference}"
            );
            let row_regex = Regex::new(&format!(
                r#"(?s)<row\b[^>]*\br=\"{}\"[^>]*>.*?</row>"#,
                regex::escape(row_number)
            ))?;
            let row = row_regex.find(&output).with_context(|| {
                format!("worksheet row is missing for inserted cell: {reference}")
            })?;
            let insert_at = row.end() - "</row>".len();
            output.insert_str(insert_at, &replacement);
        }
    }
    for (reference, _) in before_cells {
        if after_cells.contains_key(reference) {
            continue;
        }
        let pattern = format!(
            r#"(?s)<c\b[^>]*\br=\"{}\"[^>]*(?:/>|>.*?</c>)"#,
            regex::escape(reference)
        );
        let cell_regex = Regex::new(&pattern)?;
        let found = cell_regex
            .find(&output)
            .with_context(|| format!("worksheet removed cell is missing: {reference}"))?;
        output.replace_range(found.range(), "");
    }
    output = replace_spreadsheet_row_layout(output, before, after)?;
    output = replace_spreadsheet_column_layout(output, before, after)?;
    output = replace_spreadsheet_merges(output, before, after)?;
    output = replace_spreadsheet_frozen_pane(output, before, after)?;
    Ok(output.into_bytes())
}

fn replace_spreadsheet_merges(
    mut xml: String,
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
) -> anyhow::Result<String> {
    if before.merged_cells == after.merged_cells {
        return Ok(xml);
    }
    let replacement = if after.merged_cells.is_empty() {
        String::new()
    } else {
        format!(
            "<mergeCells count=\"{}\">{}</mergeCells>",
            after.merged_cells.len(),
            after
                .merged_cells
                .iter()
                .map(|reference| format!(
                    "<mergeCell ref=\"{}\"/>",
                    escape_xml_attribute(reference)
                ))
                .collect::<String>()
        )
    };
    let regex = Regex::new(r#"(?s)<mergeCells\b[^>]*>.*?</mergeCells>"#)?;
    if let Some(found) = regex.find(&xml) {
        xml.replace_range(found.range(), &replacement);
    } else if !replacement.is_empty() {
        let position = xml
            .find("</worksheet>")
            .context("worksheet closing element is missing")?;
        xml.insert_str(position, &replacement);
    }
    Ok(xml)
}

fn replace_spreadsheet_frozen_pane(
    mut xml: String,
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
) -> anyhow::Result<String> {
    if before.frozen_pane == after.frozen_pane {
        return Ok(xml);
    }
    let pane_regex = Regex::new(r#"<pane\b[^>]*/>"#)?;
    match (&before.frozen_pane, &after.frozen_pane) {
        (_, Some(pane)) => {
            let mut attributes = Vec::new();
            if pane.x_split > 0.0 {
                attributes.push(format!(
                    "xSplit=\"{}\"",
                    format_spreadsheet_number(pane.x_split)
                ));
            }
            if pane.y_split > 0.0 {
                attributes.push(format!(
                    "ySplit=\"{}\"",
                    format_spreadsheet_number(pane.y_split)
                ));
            }
            attributes.push(format!(
                "topLeftCell=\"{}\"",
                escape_xml_attribute(&pane.top_left_cell)
            ));
            if !pane.active_pane.is_empty() {
                attributes.push(format!(
                    "activePane=\"{}\"",
                    escape_xml_attribute(&pane.active_pane)
                ));
            }
            attributes.push("state=\"frozen\"".to_string());
            let replacement = format!("<pane {}/>", attributes.join(" "));
            if let Some(found) = pane_regex.find(&xml) {
                xml.replace_range(found.range(), &replacement);
            } else {
                let sheet_view = Regex::new(r#"<sheetView\b[^>]*>"#)?
                    .find(&xml)
                    .context("worksheet sheetView is missing")?;
                xml.insert_str(sheet_view.end(), &replacement);
            }
        }
        (Some(_), None) => {
            if let Some(found) = pane_regex.find(&xml) {
                xml.replace_range(found.range(), "");
            }
        }
        (None, None) => {}
    }
    Ok(xml)
}

fn replace_spreadsheet_row_layout(
    mut xml: String,
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
) -> anyhow::Result<String> {
    let before_rows = before
        .rows
        .iter()
        .map(|row| (row.index, row))
        .collect::<BTreeMap<_, _>>();
    for row in &after.rows {
        let Some(original) = before_rows.get(&row.index) else {
            continue;
        };
        let height_changed = row.custom_height && original.height != row.height;
        if original.custom_height == row.custom_height
            && original.hidden == row.hidden
            && !height_changed
        {
            continue;
        }
        let number = row.index + 1;
        let regex = Regex::new(&format!(r#"<row\b[^>]*\br=\"{number}\"[^>]*>"#))?;
        let found = regex
            .find(&xml)
            .with_context(|| format!("worksheet row is missing: {number}"))?;
        let mut opening = found.as_str().to_string();
        opening = set_xml_attribute(
            &opening,
            "ht",
            if row.custom_height {
                row.height.map(format_spreadsheet_number)
            } else {
                None
            },
        );
        opening = set_xml_attribute(
            &opening,
            "customHeight",
            row.custom_height.then(|| "1".to_string()),
        );
        opening = set_xml_attribute(&opening, "hidden", row.hidden.then(|| "1".to_string()));
        xml.replace_range(found.range(), &opening);
    }
    Ok(xml)
}

fn replace_spreadsheet_column_layout(
    mut xml: String,
    before: &EditorWorksheetManifest,
    after: &EditorWorksheetManifest,
) -> anyhow::Result<String> {
    let same_ranges = before.columns.len() == after.columns.len()
        && before
            .columns
            .iter()
            .zip(&after.columns)
            .all(|(before, after)| before.min == after.min && before.max == after.max);
    if !same_ranges {
        let columns = after
            .columns
            .iter()
            .map(|column| {
                format!(
                    r#"<col min="{}" max="{}" width="{}" customWidth="{}"/>"#,
                    column.min,
                    column.max,
                    column.width,
                    u8::from(column.custom_width)
                )
            })
            .collect::<String>();
        let replacement = format!("<cols>{columns}</cols>");
        let regex = Regex::new(r#"(?s)<cols\b[^>]*>.*?</cols>"#)?;
        if regex.is_match(&xml) {
            return Ok(regex.replace(&xml, replacement).into_owned());
        }
        let position = xml
            .find("<sheetData")
            .context("worksheet has no sheetData for column insertion")?;
        xml.insert_str(position, &replacement);
        return Ok(xml);
    }
    for (index, column) in after.columns.iter().enumerate() {
        let original = before
            .columns
            .get(index)
            .with_context(|| format!("original worksheet column is missing at index {index}"))?;
        if original.width == column.width && original.custom_width == column.custom_width {
            continue;
        }
        let regex = Regex::new(&format!(
            r#"<col\b[^>]*\bmin=\"{}\"[^>]*\bmax=\"{}\"[^>]*/>"#,
            original.min, original.max
        ))?;
        let found = regex.find(&xml).with_context(|| {
            format!(
                "worksheet column is missing: {}-{}",
                original.min, original.max
            )
        })?;
        let mut element = found.as_str().to_string();
        element = set_xml_attribute(
            &element,
            "width",
            Some(format_spreadsheet_number(column.width)),
        );
        element = set_xml_attribute(
            &element,
            "customWidth",
            column.custom_width.then(|| "1".to_string()),
        );
        xml.replace_range(found.range(), &element);
    }
    Ok(xml)
}

fn set_xml_attribute(element: &str, name: &str, value: Option<String>) -> String {
    let regex = Regex::new(&format!(r#"\s{name}=\"[^\"]*\""#)).expect("XML attribute regex");
    let stripped = regex.replace(element, "").into_owned();
    let Some(value) = value else {
        return stripped;
    };
    let position = stripped.find('>').expect("XML element terminator");
    let insert_at = if stripped.as_bytes().get(position.wrapping_sub(1)) == Some(&b'/') {
        position - 1
    } else {
        position
    };
    let mut output = stripped;
    output.insert_str(
        insert_at,
        &format!(" {name}=\"{}\"", escape_xml_attribute(&value)),
    );
    output
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn export_document_binary(
    editor_payload: &[u8],
    original_package: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let blocks = decode_document_binary_document(editor_payload)?;
    let paragraphs = flatten_decoded_document_paragraphs(&blocks);
    let mut archive =
        ZipArchive::new(Cursor::new(original_package)).context("open original DOCX escrow")?;
    let document_xml = read_zip_part(&mut archive, "word/document.xml")?;
    let document_relationships_xml =
        read_optional_zip_part(&mut archive, "word/_rels/document.xml.rels")?;
    let content_types_xml = read_zip_part(&mut archive, "[Content_Types].xml")?;
    let header_footer_relationship_parts = document_relationships_xml
        .as_deref()
        .map(parse_document_header_footer_relationship_parts)
        .transpose()?
        .unwrap_or_default();
    let mut header_footer_relationships = DecodedDocumentHeaderFooterRelationshipIds {
        headers: header_footer_relationship_parts
            .headers
            .iter()
            .map(|part| part.id.clone())
            .collect(),
        footers: header_footer_relationship_parts
            .footers
            .iter()
            .map(|part| part.id.clone())
            .collect(),
    };
    let styles_xml = read_optional_zip_part(&mut archive, "word/styles.xml")?;
    let numbering_xml = read_optional_zip_part(&mut archive, "word/numbering.xml")?;
    let mut chart_paths = archive
        .file_names()
        .filter(|path| is_primary_chart_part(path))
        .map(str::to_string)
        .collect::<Vec<_>>();
    chart_paths.sort();
    let mut chart_parts = Vec::with_capacity(chart_paths.len());
    for path in &chart_paths {
        chart_parts.push(read_zip_part(&mut archive, path)?);
    }
    drop(archive);
    let style_ids = decode_document_binary_style_ids(
        editor_payload,
        styles_xml.as_deref().unwrap_or_default(),
    )?;
    let header_footer_parts = decode_document_binary_header_footer_parts(editor_payload)?;
    let comments = decode_document_binary_comments(editor_payload)?;
    let mut replacements = prepare_document_binary_header_footer_package_replacements(
        document_relationships_xml.as_deref(),
        &content_types_xml,
        &header_footer_relationship_parts,
        &header_footer_parts,
        &style_ids,
        &mut header_footer_relationships,
    )?;
    let replacement = if decoded_document_blocks_have_complex_content(&blocks) {
        replace_document_body_from_decoded_blocks(
            &document_xml,
            &blocks,
            &style_ids,
            &header_footer_relationships,
        )?
    } else {
        replace_document_paragraph_formatting(
            &document_xml,
            &paragraphs,
            &style_ids,
            &header_footer_relationships,
        )
        .or_else(|error| {
            if error.to_string().contains("DOCY paragraph count") {
                return replace_document_body_from_decoded_blocks(
                    &document_xml,
                    &blocks,
                    &style_ids,
                    &header_footer_relationships,
                );
            }
            Err(error)
        })?
    };
    let relationships_for_hyperlinks = replacements
        .get("word/_rels/document.xml.rels")
        .map(Vec::as_slice)
        .or(document_relationships_xml.as_deref());
    let (replacement, hyperlink_relationships) =
        materialize_document_hyperlink_relationships(&replacement, relationships_for_hyperlinks)?;
    if let Some(relationships) = hyperlink_relationships {
        replacements.insert("word/_rels/document.xml.rels".to_string(), relationships);
    }
    replacements.insert("word/document.xml".to_string(), replacement);
    if let Some(numbering_xml) = numbering_xml {
        let numbering = extend_document_numbering_levels(&numbering_xml, &paragraphs)?;
        if numbering != numbering_xml {
            replacements.insert("word/numbering.xml".to_string(), numbering);
        }
    }
    if !comments.is_empty() {
        add_document_comment_package_replacements(
            &mut replacements,
            document_relationships_xml.as_deref(),
            &content_types_xml,
            &comments,
        )?;
    }
    let chart_styles = collect_decoded_document_chart_styles(&blocks);
    ensure!(
        chart_styles.len() <= chart_parts.len(),
        "DOCY contains {} styled charts but the original DOCX has only {} chart parts",
        chart_styles.len(),
        chart_parts.len()
    );
    for ((path, chart_xml), style) in chart_paths.into_iter().zip(chart_parts).zip(chart_styles) {
        replacements.insert(path, replace_document_chart_style(&chart_xml, style)?);
    }
    replace_package_parts(original_package, replacements)
}

fn is_primary_chart_part(path: &str) -> bool {
    path.starts_with("word/charts/chart") && path.ends_with(".xml")
}

fn collect_decoded_document_chart_styles(blocks: &[DecodedDocumentBlock]) -> Vec<u8> {
    let mut styles = Vec::new();
    collect_decoded_document_chart_styles_into(blocks, &mut styles);
    styles
}

fn collect_decoded_document_chart_styles_into(
    blocks: &[DecodedDocumentBlock],
    styles: &mut Vec<u8>,
) {
    for block in blocks {
        match block {
            DecodedDocumentBlock::Paragraph(paragraph) => {
                for run in &paragraph.runs {
                    if let Some(style) =
                        run.drawing.as_ref().and_then(|drawing| drawing.chart_style)
                    {
                        styles.push(style);
                    }
                }
            }
            DecodedDocumentBlock::Table(table) => {
                for cell in table.rows.iter().flatten() {
                    collect_decoded_document_chart_styles_into(&cell.blocks, styles);
                }
            }
        }
    }
}

fn replace_document_chart_style(chart_xml: &[u8], style: u8) -> anyhow::Result<Vec<u8>> {
    let source = std::str::from_utf8(chart_xml).context("chart XML is not UTF-8")?;
    let style_element = format!(r#"<c:style val="{style}"/>"#);
    let existing = Regex::new(r#"<c:style\b[^>]*/>"#)?;
    if existing.is_match(source) {
        return Ok(existing
            .replace(source, style_element)
            .into_owned()
            .into_bytes());
    }
    let chart = Regex::new(r"<c:chart\b")?;
    ensure!(
        chart.is_match(source),
        "chart XML c:chart element is missing"
    );
    Ok(chart
        .replace(source, format!("{style_element}<c:chart"))
        .into_owned()
        .into_bytes())
}

fn add_document_comment_package_replacements(
    replacements: &mut BTreeMap<String, Vec<u8>>,
    original_relationships_xml: Option<&[u8]>,
    original_content_types_xml: &[u8],
    comments: &[DecodedDocumentComment],
) -> anyhow::Result<()> {
    let (comments_xml, comments_extended_xml) = render_decoded_document_comments(comments);
    replacements.insert("word/comments.xml".to_string(), comments_xml);
    replacements.insert(
        "word/commentsExtended.xml".to_string(),
        comments_extended_xml,
    );

    let current_relationships = replacements
        .get("word/_rels/document.xml.rels")
        .map(Vec::as_slice)
        .or(original_relationships_xml);
    let relationships_text = current_relationships
        .map(std::str::from_utf8)
        .transpose()
        .context("document relationships XML is not UTF-8")?
        .unwrap_or_default();
    let mut additions = Vec::new();
    let mut next_id = next_document_relationship_id(current_relationships.unwrap_or_default())?;
    if !relationships_text.contains("/relationships/comments\"") {
        additions.push((
            format!("rId{next_id}"),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments"
                .to_string(),
            "comments.xml".to_string(),
        ));
        next_id += 1;
    }
    if !relationships_text.contains("/relationships/commentsExtended\"") {
        additions.push((
            format!("rId{next_id}"),
            "http://schemas.microsoft.com/office/2011/relationships/commentsExtended".to_string(),
            "commentsExtended.xml".to_string(),
        ));
    }
    if !additions.is_empty() {
        replacements.insert(
            "word/_rels/document.xml.rels".to_string(),
            add_document_relationships(current_relationships, &additions)?,
        );
    }

    let current_content_types = replacements
        .get("[Content_Types].xml")
        .map(Vec::as_slice)
        .unwrap_or(original_content_types_xml);
    replacements.insert(
        "[Content_Types].xml".to_string(),
        add_document_content_type_overrides(
            current_content_types,
            &[
                (
                    "/word/comments.xml".to_string(),
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
                        .to_string(),
                ),
                (
                    "/word/commentsExtended.xml".to_string(),
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml"
                        .to_string(),
                ),
            ],
        )?,
    );
    Ok(())
}

fn decoded_document_blocks_have_complex_content(blocks: &[DecodedDocumentBlock]) -> bool {
    blocks.iter().any(|block| match block {
        DecodedDocumentBlock::Paragraph(paragraph) => {
            paragraph.content.iter().any(|item| match item {
                DecodedDocumentInline::Run(run) => run.field_char.is_some() || run.instruction_text,
                DecodedDocumentInline::Hyperlink { .. }
                | DecodedDocumentInline::Bookmark { .. }
                | DecodedDocumentInline::CommentStart(_)
                | DecodedDocumentInline::CommentEnd(_)
                | DecodedDocumentInline::Revision { .. } => true,
            })
        }
        DecodedDocumentBlock::Table(table) => table
            .rows
            .iter()
            .flatten()
            .any(|cell| decoded_document_blocks_have_complex_content(&cell.blocks)),
    })
}

fn materialize_document_hyperlink_relationships(
    document_xml: &[u8],
    relationships_xml: Option<&[u8]>,
) -> anyhow::Result<(Vec<u8>, Option<Vec<u8>>)> {
    let mut document = std::str::from_utf8(document_xml)
        .context("rendered document XML is not UTF-8")?
        .to_string();
    let marker = Regex::new(r"__CTOX_HREF_([0-9a-f]+)__")?;
    let captures = marker
        .captures_iter(&document)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect::<Vec<_>>();
    if captures.is_empty() {
        return Ok((document_xml.to_vec(), None));
    }
    let current = relationships_xml.unwrap_or(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#,
    );
    let mut targets = BTreeMap::<String, String>::new();
    if !current.is_empty() {
        let tree = roxmltree::Document::parse(
            std::str::from_utf8(current).context("document relationships XML is not UTF-8")?,
        )?;
        for relationship in tree.descendants().filter(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node
                    .attribute("Type")
                    .is_some_and(|value| value.ends_with("/hyperlink"))
        }) {
            if let (Some(id), Some(target)) = (
                relationship.attribute("Id"),
                relationship.attribute("Target"),
            ) {
                targets.insert(target.to_string(), id.to_string());
            }
        }
    }
    let mut next_id = next_document_relationship_id(current)?;
    let mut additions = Vec::<(String, String)>::new();
    for encoded in captures {
        let target = String::from_utf8(hex_decode(&encoded)?)
            .context("DOCY hyperlink target is not UTF-8")?;
        let id = if let Some(id) = targets.get(&target) {
            id.clone()
        } else {
            let id = format!("rId{next_id}");
            next_id += 1;
            targets.insert(target.clone(), id.clone());
            additions.push((id.clone(), target));
            id
        };
        document = document.replace(&format!("__CTOX_HREF_{encoded}__"), &id);
    }
    let relationships = if additions.is_empty() {
        None
    } else {
        let mut xml = std::str::from_utf8(current)
            .context("document relationships XML is not UTF-8")?
            .to_string();
        let closing = xml
            .rfind("</Relationships>")
            .context("document relationships closing tag is missing")?;
        let mut inserted = String::new();
        for (id, target) in additions {
            inserted.push_str(&format!(
                "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
                xml_escape_attribute(&id),
                xml_escape_attribute(&target),
            ));
        }
        xml.insert_str(closing, &inserted);
        Some(xml.into_bytes())
    };
    Ok((document.into_bytes(), relationships))
}

fn prepare_document_binary_header_footer_package_replacements(
    document_relationships_xml: Option<&[u8]>,
    content_types_xml: &[u8],
    relationship_parts: &DocumentHeaderFooterRelationshipParts,
    header_footer_parts: &DecodedDocumentHeaderFooterParts,
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &mut DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    let mut replacements = BTreeMap::new();
    let mut new_relationships = Vec::new();
    let mut new_content_types = Vec::new();
    let mut next_rid = next_document_relationship_id(document_relationships_xml.unwrap_or(b""))?;
    let mut next_header_index = next_document_header_footer_part_index(&relationship_parts.headers);
    let mut next_footer_index = next_document_header_footer_part_index(&relationship_parts.footers);

    for (index, blocks) in header_footer_parts.headers.iter().enumerate() {
        let (relationship_id, path) = if let Some(part) = relationship_parts.headers.get(index) {
            (part.id.clone(), part.path.clone())
        } else {
            let relationship_id = format!("rId{next_rid}");
            next_rid += 1;
            let path = loop {
                let candidate = format!("word/header{next_header_index}.xml");
                next_header_index += 1;
                if !relationship_parts
                    .headers
                    .iter()
                    .any(|part| part.path == candidate)
                    && !replacements.contains_key(&candidate)
                {
                    break candidate;
                }
            };
            new_relationships.push((
                relationship_id.clone(),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
                    .to_string(),
                path.strip_prefix("word/").unwrap_or(&path).to_string(),
            ));
            new_content_types.push((
                format!("/{}", path),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
                    .to_string(),
            ));
            header_footer_relationships
                .headers
                .push(relationship_id.clone());
            (relationship_id, path)
        };
        if header_footer_relationships.headers.len() <= index {
            header_footer_relationships.headers.push(relationship_id);
        }
        replacements.insert(
            path,
            render_decoded_document_header_part(blocks, style_ids)?,
        );
    }

    for (index, blocks) in header_footer_parts.footers.iter().enumerate() {
        let (relationship_id, path) = if let Some(part) = relationship_parts.footers.get(index) {
            (part.id.clone(), part.path.clone())
        } else {
            let relationship_id = format!("rId{next_rid}");
            next_rid += 1;
            let path = loop {
                let candidate = format!("word/footer{next_footer_index}.xml");
                next_footer_index += 1;
                if !relationship_parts
                    .footers
                    .iter()
                    .any(|part| part.path == candidate)
                    && !replacements.contains_key(&candidate)
                {
                    break candidate;
                }
            };
            new_relationships.push((
                relationship_id.clone(),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
                    .to_string(),
                path.strip_prefix("word/").unwrap_or(&path).to_string(),
            ));
            new_content_types.push((
                format!("/{}", path),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
                    .to_string(),
            ));
            header_footer_relationships
                .footers
                .push(relationship_id.clone());
            (relationship_id, path)
        };
        if header_footer_relationships.footers.len() <= index {
            header_footer_relationships.footers.push(relationship_id);
        }
        replacements.insert(
            path,
            render_decoded_document_footer_part(blocks, style_ids)?,
        );
    }

    if !new_relationships.is_empty() {
        replacements.insert(
            "word/_rels/document.xml.rels".to_string(),
            add_document_relationships(document_relationships_xml, &new_relationships)?,
        );
    }
    if !new_content_types.is_empty() {
        replacements.insert(
            "[Content_Types].xml".to_string(),
            add_document_content_type_overrides(content_types_xml, &new_content_types)?,
        );
    }

    Ok(replacements)
}

fn next_document_relationship_id(xml: &[u8]) -> anyhow::Result<u32> {
    let text = std::str::from_utf8(xml).unwrap_or_default();
    let pattern = Regex::new(r#"Id="rId(\d+)""#)?;
    let max_id = pattern
        .captures_iter(text)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    Ok(max_id + 1)
}

fn next_document_header_footer_part_index(parts: &[DocumentHeaderFooterRelationshipPart]) -> u32 {
    let pattern = Regex::new(r#"(?:header|footer)(\d+)\.xml$"#).expect("static regex");
    parts
        .iter()
        .filter_map(|part| pattern.captures(&part.path))
        .filter_map(|captures| captures.get(1)?.as_str().parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn add_document_relationships(
    xml: Option<&[u8]>,
    additions: &[(String, String, String)],
) -> anyhow::Result<Vec<u8>> {
    let mut text = match xml {
        Some(xml) => std::str::from_utf8(xml)
            .context("document relationships XML is not UTF-8")?
            .to_string(),
        None => "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".to_string(),
    };
    let closing = text
        .rfind("</Relationships>")
        .context("document relationships closing tag is missing")?;
    let mut inserted = String::new();
    for (id, relationship_type, target) in additions {
        inserted.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
            xml_escape_attr(id),
            xml_escape_attr(relationship_type),
            xml_escape_attr(target)
        ));
    }
    text.insert_str(closing, &inserted);
    Ok(text.into_bytes())
}

fn add_document_content_type_overrides(
    xml: &[u8],
    additions: &[(String, String)],
) -> anyhow::Result<Vec<u8>> {
    let mut text = std::str::from_utf8(xml)
        .context("[Content_Types].xml is not UTF-8")?
        .to_string();
    let closing = text
        .rfind("</Types>")
        .context("[Content_Types].xml closing tag is missing")?;
    let mut inserted = String::new();
    for (part_name, content_type) in additions {
        if text.contains(&format!("PartName=\"{}\"", xml_escape_attr(part_name))) {
            continue;
        }
        inserted.push_str(&format!(
            "<Override PartName=\"{}\" ContentType=\"{}\"/>",
            xml_escape_attr(part_name),
            xml_escape_attr(content_type)
        ));
    }
    text.insert_str(closing, &inserted);
    Ok(text.into_bytes())
}

fn extend_document_numbering_levels(
    xml: &[u8],
    paragraphs: &[DecodedDocumentParagraph],
) -> anyhow::Result<Vec<u8>> {
    let source = std::str::from_utf8(xml).context("document numbering XML is not UTF-8")?;
    let mut requested = BTreeMap::<u32, u32>::new();
    for paragraph in paragraphs {
        if let (Some(num_id), Some(level)) = (paragraph.num_id, paragraph.num_level) {
            requested
                .entry(num_id)
                .and_modify(|current| *current = (*current).max(level))
                .or_insert(level);
        }
    }
    let mut abstract_requests = BTreeMap::<u32, u32>::new();
    for (num_id, level) in requested {
        let pattern = Regex::new(&format!(
            r#"(?s)<w:num\b[^>]*w:numId="{num_id}"[^>]*>.*?<w:abstractNumId\b[^>]*w:val="(\d+)"[^>]*/>.*?</w:num>"#
        ))?;
        let Some(captures) = pattern.captures(source) else {
            continue;
        };
        let abstract_id = captures[1].parse::<u32>()?;
        abstract_requests
            .entry(abstract_id)
            .and_modify(|current| *current = (*current).max(level))
            .or_insert(level);
    }
    let mut output = source.to_string();
    for (abstract_id, maximum_level) in abstract_requests {
        if maximum_level == 0 {
            continue;
        }
        let abstract_pattern = Regex::new(&format!(
            r#"(?s)<w:abstractNum\b[^>]*w:abstractNumId="{abstract_id}"[^>]*>.*?</w:abstractNum>"#
        ))?;
        let Some(found) = abstract_pattern.find(&output) else {
            continue;
        };
        let block = found.as_str();
        let level_pattern = Regex::new(r#"(?s)<w:lvl\b[^>]*w:ilvl="(\d+)"[^>]*>.*?</w:lvl>"#)?;
        let existing = level_pattern
            .captures_iter(block)
            .filter_map(|captures| captures[1].parse::<u32>().ok())
            .collect::<BTreeSet<_>>();
        let Some(level_zero) = level_pattern
            .captures_iter(block)
            .find(|captures| &captures[1] == "0")
            .and_then(|captures| captures.get(0))
            .map(|value| value.as_str().to_string())
        else {
            continue;
        };
        let mut additions = String::new();
        for level in 1..=maximum_level {
            if existing.contains(&level) {
                continue;
            }
            let mut generated =
                level_zero.replacen(r#"w:ilvl="0""#, &format!(r#"w:ilvl="{level}""#), 1);
            let left = 360 * (level + 1);
            let left_pattern = Regex::new(r#"w:left="\d+""#)?;
            generated = left_pattern
                .replace(&generated, format!(r#"w:left="{left}""#))
                .into_owned();
            additions.push_str(&generated);
        }
        if additions.is_empty() {
            continue;
        }
        let closing = block
            .rfind("</w:abstractNum>")
            .context("OOXML abstract numbering closing tag is missing")?;
        let expanded = format!("{}{}{}", &block[..closing], additions, &block[closing..]);
        output.replace_range(found.start()..found.end(), &expanded);
    }
    Ok(output.into_bytes())
}

fn decode_document_binary_paragraphs(payload: &[u8]) -> anyhow::Result<Vec<String>> {
    Ok(
        flatten_decoded_document_paragraphs(&decode_document_binary_document(payload)?)
            .into_iter()
            .map(|paragraph| paragraph.text)
            .collect(),
    )
}

#[derive(Debug, Clone)]
enum DecodedDocumentBlock {
    Paragraph(DecodedDocumentParagraph),
    Table(DecodedDocumentTable),
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentTable {
    rows: Vec<Vec<DecodedDocumentCell>>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentCell {
    blocks: Vec<DecodedDocumentBlock>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentParagraph {
    text: String,
    style_id: Option<String>,
    num_id: Option<u32>,
    num_level: Option<u32>,
    alignment: Option<u8>,
    left_indent_twips: Option<u32>,
    line_spacing_twips: Option<u32>,
    line_spacing_rule: Option<u8>,
    section: Option<DecodedDocumentSection>,
    runs: Vec<DecodedDocumentRun>,
    content: Vec<DecodedDocumentInline>,
}

#[derive(Debug, Clone)]
enum DecodedDocumentInline {
    Run(DecodedDocumentRun),
    Hyperlink {
        value: String,
        anchor: Option<String>,
        tooltip: Option<String>,
        runs: Vec<DecodedDocumentRun>,
    },
    Bookmark {
        id: u32,
        name: Option<String>,
        start: bool,
    },
    CommentStart(u32),
    CommentEnd(u32),
    Revision {
        kind: u8,
        id: u32,
        author: Option<String>,
        date: Option<String>,
        runs: Vec<DecodedDocumentRun>,
    },
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentSection {
    width_twips: Option<u32>,
    height_twips: Option<u32>,
    orientation: Option<u8>,
    margins_twips: [Option<u32>; 7],
    title_page: Option<bool>,
    break_type: Option<u8>,
    header_default: Option<usize>,
    header_even: Option<usize>,
    header_first: Option<usize>,
    footer_default: Option<usize>,
    footer_even: Option<usize>,
    footer_first: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentHeaderFooterRelationshipIds {
    headers: Vec<String>,
    footers: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentHeaderFooterRoleTypes {
    headers: Vec<u8>,
    footers: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentHeaderFooterParts {
    headers: Vec<Vec<DecodedDocumentBlock>>,
    footers: Vec<Vec<DecodedDocumentBlock>>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentComment {
    id: Option<u32>,
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
    text: String,
    solved: bool,
    replies: Vec<DecodedDocumentComment>,
}

fn decode_document_binary_comments(payload: &[u8]) -> anyhow::Result<Vec<DecodedDocumentComment>> {
    let manifest = inspect_editor_payload(OfficeKind::Document, payload)?;
    let Some(table) = manifest.tables.iter().find(|table| table.table_type == 8) else {
        return Ok(Vec::new());
    };
    let start = usize::try_from(table.offset)?;
    let end = start + usize::try_from(table.bytes)?;
    let content = length_prefixed_content(
        payload
            .get(start..end)
            .context("DOCY comments table is truncated")?,
        "DOCY comments table",
    )?;
    let mut comments = Vec::new();
    for (item_type, item) in length_prefixed_items(content, "DOCY comments")? {
        if item_type == 0 {
            comments.push(decode_document_binary_comment(item)?);
        }
    }
    Ok(comments)
}

fn decode_document_binary_comment(value: &[u8]) -> anyhow::Result<DecodedDocumentComment> {
    let mut comment = DecodedDocumentComment::default();
    for (item_type, item) in length_prefixed_items(value, "DOCY comment")? {
        match item_type {
            0 => comment.replies.push(decode_document_binary_comment(item)?),
            1 => comment.id = read_u32_property(item),
            2 => comment.initials = Some(decode_utf16_le(item, "DOCY comment initials")?),
            3 => comment.author = Some(decode_utf16_le(item, "DOCY comment author")?),
            5 | 14 => comment.date = Some(decode_utf16_le(item, "DOCY comment date")?),
            6 => comment.text = decode_utf16_le(item, "DOCY comment text")?,
            8 => comment.solved = item.first().copied().unwrap_or(0) != 0,
            9 => {
                for (reply_type, reply) in length_prefixed_items(item, "DOCY comment replies")? {
                    if reply_type == 0 {
                        comment.replies.push(decode_document_binary_comment(reply)?);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(comment)
}

#[derive(Debug)]
struct RenderedDocumentComment<'a> {
    id: u32,
    para_id: String,
    parent_para_id: Option<String>,
    comment: &'a DecodedDocumentComment,
}

fn render_decoded_document_comments(comments: &[DecodedDocumentComment]) -> (Vec<u8>, Vec<u8>) {
    let mut next_id = comments
        .iter()
        .filter_map(|comment| comment.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut rendered = Vec::new();
    for (ordinal, comment) in comments.iter().enumerate() {
        let id = comment.id.unwrap_or_else(|| {
            let id = next_id;
            next_id = next_id.saturating_add(1);
            id
        });
        let para_id = document_comment_para_id(ordinal as u32, id);
        rendered.push(RenderedDocumentComment {
            id,
            para_id: para_id.clone(),
            parent_para_id: None,
            comment,
        });
        flatten_rendered_document_comment_replies(
            &comment.replies,
            &para_id,
            ordinal as u32,
            &mut next_id,
            &mut rendered,
        );
    }

    let mut comments_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\">",
    );
    let mut extended_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w15:commentsEx xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\">",
    );
    for item in rendered {
        comments_xml.push_str(&format!("<w:comment w:id=\"{}\"", item.id));
        if let Some(author) = item.comment.author.as_deref() {
            comments_xml.push_str(&format!(" w:author=\"{}\"", xml_escape_attribute(author)));
        }
        if let Some(date) = item.comment.date.as_deref() {
            comments_xml.push_str(&format!(" w:date=\"{}\"", xml_escape_attribute(date)));
        }
        if let Some(initials) = item.comment.initials.as_deref() {
            comments_xml.push_str(&format!(
                " w:initials=\"{}\"",
                xml_escape_attribute(initials)
            ));
        }
        comments_xml.push_str(&format!(
            "><w:p w14:paraId=\"{}\"><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:comment>",
            item.para_id,
            xml_escape_text(&item.comment.text),
        ));

        extended_xml.push_str(&format!(
            "<w15:commentEx w15:paraId=\"{}\" w15:done=\"{}\"",
            item.para_id,
            u8::from(item.comment.solved),
        ));
        if let Some(parent_para_id) = item.parent_para_id {
            extended_xml.push_str(&format!(" w15:paraIdParent=\"{}\"", parent_para_id));
        }
        extended_xml.push_str("/>");
    }
    comments_xml.push_str("</w:comments>");
    extended_xml.push_str("</w15:commentsEx>");
    (comments_xml.into_bytes(), extended_xml.into_bytes())
}

fn flatten_rendered_document_comment_replies<'a>(
    replies: &'a [DecodedDocumentComment],
    parent_para_id: &str,
    root_ordinal: u32,
    next_id: &mut u32,
    output: &mut Vec<RenderedDocumentComment<'a>>,
) {
    for (reply_ordinal, reply) in replies.iter().enumerate() {
        let id = reply.id.unwrap_or_else(|| {
            let id = *next_id;
            *next_id = next_id.saturating_add(1);
            id
        });
        let para_id = document_comment_para_id(
            root_ordinal
                .wrapping_mul(0x10000)
                .wrapping_add(reply_ordinal as u32 + 1),
            id,
        );
        output.push(RenderedDocumentComment {
            id,
            para_id: para_id.clone(),
            parent_para_id: Some(parent_para_id.to_string()),
            comment: reply,
        });
        flatten_rendered_document_comment_replies(
            &reply.replies,
            &para_id,
            root_ordinal,
            next_id,
            output,
        );
    }
}

fn document_comment_para_id(ordinal: u32, id: u32) -> String {
    format!("{:08X}", 0x4354_4F58u32 ^ ordinal.rotate_left(11) ^ id)
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentRun {
    text: String,
    drawing: Option<DecodedDocumentDrawing>,
    page_break: bool,
    line_break: bool,
    tab: bool,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<u8>,
    font_size_half_points: Option<u32>,
    color: Option<[u8; 3]>,
    field_char: Option<u8>,
    instruction_text: bool,
    deleted_text: bool,
    comment_reference: Option<u32>,
}

#[derive(Debug, Clone, Default)]
struct DecodedDocumentDrawing {
    inline: Option<bool>,
    extent_emu: Option<(u32, u32)>,
    pptx_data_bytes: Option<usize>,
    chart2_data_bytes: Option<usize>,
    raster_ids: Vec<String>,
    shape_fill: Option<[u8; 3]>,
    shape_rotation: Option<i32>,
    chart_style: Option<u8>,
}

fn decode_document_binary_document(payload: &[u8]) -> anyhow::Result<Vec<DecodedDocumentBlock>> {
    let manifest = inspect_editor_payload(OfficeKind::Document, payload)?;
    let header_footer_roles = decode_document_binary_header_footer_role_types(payload)?;
    let table = manifest
        .tables
        .iter()
        .find(|table| table.table_type == 6)
        .context("DOCY document table is missing")?;
    let start = usize::try_from(table.offset).context("DOCY document table offset overflow")?;
    let end = start
        .checked_add(usize::try_from(table.bytes).context("DOCY document table size overflow")?)
        .context("DOCY document table range overflow")?;
    let content = length_prefixed_content(
        payload
            .get(start..end)
            .context("DOCY document table is truncated")?,
        "DOCY document table",
    )?;
    let blocks = decode_document_binary_blocks(content, &header_footer_roles)?;
    let paragraphs = flatten_decoded_document_paragraphs(&blocks);
    ensure!(
        !paragraphs.is_empty(),
        "DOCY document contains no paragraphs"
    );
    Ok(blocks)
}

fn flatten_decoded_document_paragraphs(
    blocks: &[DecodedDocumentBlock],
) -> Vec<DecodedDocumentParagraph> {
    let mut paragraphs = Vec::new();
    collect_decoded_document_paragraphs(blocks, &mut paragraphs);
    paragraphs
}

fn collect_decoded_document_paragraphs(
    blocks: &[DecodedDocumentBlock],
    paragraphs: &mut Vec<DecodedDocumentParagraph>,
) {
    for block in blocks {
        match block {
            DecodedDocumentBlock::Paragraph(paragraph) => paragraphs.push(paragraph.clone()),
            DecodedDocumentBlock::Table(table) => {
                for row in &table.rows {
                    for cell in row {
                        collect_decoded_document_paragraphs(&cell.blocks, paragraphs);
                    }
                }
            }
        }
    }
}

fn decode_document_binary_header_footer_role_types(
    payload: &[u8],
) -> anyhow::Result<DecodedDocumentHeaderFooterRoleTypes> {
    let manifest = inspect_editor_payload(OfficeKind::Document, payload)?;
    let Some(table) = manifest.tables.iter().find(|table| table.table_type == 4) else {
        return Ok(DecodedDocumentHeaderFooterRoleTypes::default());
    };
    let start = usize::try_from(table.offset).context("DOCY HdrFtr table offset overflow")?;
    let end = start
        .checked_add(usize::try_from(table.bytes).context("DOCY HdrFtr table size overflow")?)
        .context("DOCY HdrFtr table range overflow")?;
    let content = length_prefixed_content(
        payload
            .get(start..end)
            .context("DOCY HdrFtr table is truncated")?,
        "DOCY HdrFtr table",
    )?;
    let mut roles = DecodedDocumentHeaderFooterRoleTypes::default();
    for (kind, table_content) in length_prefixed_items(content, "DOCY HdrFtr content")? {
        let target = match kind {
            0 => &mut roles.headers,
            1 => &mut roles.footers,
            _ => continue,
        };
        for (role, _) in length_prefixed_items(table_content, "DOCY HdrFtr role content")? {
            target.push(role);
        }
    }
    Ok(roles)
}

fn decode_document_binary_header_footer_parts(
    payload: &[u8],
) -> anyhow::Result<DecodedDocumentHeaderFooterParts> {
    let manifest = inspect_editor_payload(OfficeKind::Document, payload)?;
    let header_footer_roles = decode_document_binary_header_footer_role_types(payload)?;
    let Some(table) = manifest.tables.iter().find(|table| table.table_type == 4) else {
        return Ok(DecodedDocumentHeaderFooterParts::default());
    };
    let start = usize::try_from(table.offset).context("DOCY HdrFtr table offset overflow")?;
    let end = start
        .checked_add(usize::try_from(table.bytes).context("DOCY HdrFtr table size overflow")?)
        .context("DOCY HdrFtr table range overflow")?;
    let content = length_prefixed_content(
        payload
            .get(start..end)
            .context("DOCY HdrFtr table is truncated")?,
        "DOCY HdrFtr table",
    )?;
    let mut parts = DecodedDocumentHeaderFooterParts::default();
    for (kind, table_content) in length_prefixed_items(content, "DOCY HdrFtr content")? {
        let target = match kind {
            0 => &mut parts.headers,
            1 => &mut parts.footers,
            _ => continue,
        };
        for (_, item_content) in length_prefixed_items(table_content, "DOCY HdrFtr role content")? {
            let mut blocks = Vec::new();
            for (item_type, value) in length_prefixed_items(item_content, "DOCY HdrFtr item")? {
                if item_type == 5 {
                    blocks = decode_document_binary_blocks(value, &header_footer_roles)?;
                    break;
                }
            }
            target.push(blocks);
        }
    }
    Ok(parts)
}

fn decode_document_binary_blocks(
    content: &[u8],
    header_footer_roles: &DecodedDocumentHeaderFooterRoleTypes,
) -> anyhow::Result<Vec<DecodedDocumentBlock>> {
    let mut blocks = Vec::new();
    for (record_type, record) in length_prefixed_items(content, "DOCY document content")? {
        match record_type {
            0 => blocks.push(DecodedDocumentBlock::Paragraph(
                decode_document_binary_paragraph(record, header_footer_roles)?,
            )),
            3 => blocks.push(DecodedDocumentBlock::Table(decode_document_binary_table(
                record,
                header_footer_roles,
            )?)),
            _ => {}
        }
    }
    Ok(blocks)
}

fn decode_document_binary_paragraph(
    record: &[u8],
    header_footer_roles: &DecodedDocumentHeaderFooterRoleTypes,
) -> anyhow::Result<DecodedDocumentParagraph> {
    let mut paragraph = DecodedDocumentParagraph::default();
    for (paragraph_type, paragraph_item) in length_prefixed_items(record, "DOCY paragraph")? {
        match paragraph_type {
            1 => decode_document_binary_paragraph_properties(
                paragraph_item,
                &mut paragraph,
                header_footer_roles,
            )?,
            2 => {
                for (content_type, content_item) in
                    length_prefixed_items(paragraph_item, "DOCY paragraph content")?
                {
                    match content_type {
                        5 => {
                            let run = decode_document_binary_run_formatting(content_item)?;
                            paragraph.text.push_str(&run.text);
                            paragraph.runs.push(run.clone());
                            paragraph.content.push(DecodedDocumentInline::Run(run));
                        }
                        10 => {
                            let hyperlink = decode_document_binary_hyperlink(content_item)?;
                            for run in &hyperlink.runs {
                                paragraph.text.push_str(&run.text);
                                paragraph.runs.push(run.clone());
                            }
                            paragraph.content.push(DecodedDocumentInline::Hyperlink {
                                value: hyperlink.value,
                                anchor: hyperlink.anchor,
                                tooltip: hyperlink.tooltip,
                                runs: hyperlink.runs,
                            });
                        }
                        23 | 24 => {
                            let (id, name) = decode_document_binary_bookmark(content_item)?;
                            paragraph.content.push(DecodedDocumentInline::Bookmark {
                                id,
                                name,
                                start: content_type == 23,
                            });
                        }
                        6 | 7 => {
                            let id = decode_document_comment_marker(content_item)?;
                            paragraph.content.push(if content_type == 6 {
                                DecodedDocumentInline::CommentStart(id)
                            } else {
                                DecodedDocumentInline::CommentEnd(id)
                            });
                        }
                        12 | 13 => {
                            let revision =
                                decode_document_binary_revision(content_item, content_type)?;
                            for run in &revision.runs {
                                paragraph.text.push_str(&run.text);
                                paragraph.runs.push(run.clone());
                            }
                            paragraph.content.push(DecodedDocumentInline::Revision {
                                kind: revision.kind,
                                id: revision.id,
                                author: revision.author,
                                date: revision.date,
                                runs: revision.runs,
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(paragraph)
}

fn decode_document_comment_marker(value: &[u8]) -> anyhow::Result<u32> {
    for (item_type, item) in length_prefixed_items(value, "DOCY comment marker")? {
        if item_type == 1 {
            return read_u32_property(item).context("DOCY comment marker id is truncated");
        }
    }
    anyhow::bail!("DOCY comment marker id is missing")
}

struct DecodedDocumentRevision {
    kind: u8,
    id: u32,
    author: Option<String>,
    date: Option<String>,
    runs: Vec<DecodedDocumentRun>,
}

fn decode_document_binary_revision(
    value: &[u8],
    kind: u8,
) -> anyhow::Result<DecodedDocumentRevision> {
    let mut revision = DecodedDocumentRevision {
        kind,
        id: 0,
        author: None,
        date: None,
        runs: Vec::new(),
    };
    for (item_type, item) in length_prefixed_items(value, "DOCY revision")? {
        match item_type {
            0 => revision.author = Some(decode_utf16_le(item, "DOCY revision author")?),
            1 => revision.date = Some(decode_utf16_le(item, "DOCY revision date")?),
            2 => revision.id = read_u32_property(item).unwrap_or(0),
            4 => {
                for (content_type, content_item) in
                    length_prefixed_items(item, "DOCY revision content")?
                {
                    if content_type == 5 {
                        revision
                            .runs
                            .push(decode_document_binary_run_formatting(content_item)?);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(revision)
}

#[derive(Debug)]
struct DecodedDocumentHyperlink {
    value: String,
    anchor: Option<String>,
    tooltip: Option<String>,
    runs: Vec<DecodedDocumentRun>,
}

fn decode_document_binary_hyperlink(value: &[u8]) -> anyhow::Result<DecodedDocumentHyperlink> {
    let mut hyperlink = DecodedDocumentHyperlink {
        value: String::new(),
        anchor: None,
        tooltip: None,
        runs: Vec::new(),
    };
    for (item_type, item) in length_prefixed_items(value, "DOCY hyperlink")? {
        match item_type {
            0 => {
                for (content_type, content_item) in
                    length_prefixed_items(item, "DOCY hyperlink content")?
                {
                    if content_type == 5 {
                        hyperlink
                            .runs
                            .push(decode_document_binary_run_formatting(content_item)?);
                    }
                }
            }
            1 => hyperlink.value = decode_utf16_le(item, "DOCY hyperlink value")?,
            2 => hyperlink.anchor = Some(decode_utf16_le(item, "DOCY hyperlink anchor")?),
            3 => hyperlink.tooltip = Some(decode_utf16_le(item, "DOCY hyperlink tooltip")?),
            _ => {}
        }
    }
    Ok(hyperlink)
}

fn decode_document_binary_bookmark(value: &[u8]) -> anyhow::Result<(u32, Option<String>)> {
    let mut id = None;
    let mut name = None;
    for (item_type, item) in length_prefixed_items(value, "DOCY bookmark")? {
        match item_type {
            0 => id = read_u32_property(item),
            1 => name = Some(decode_utf16_le(item, "DOCY bookmark name")?),
            _ => {}
        }
    }
    Ok((id.context("DOCY bookmark id is missing")?, name))
}

fn decode_document_binary_table(
    record: &[u8],
    header_footer_roles: &DecodedDocumentHeaderFooterRoleTypes,
) -> anyhow::Result<DecodedDocumentTable> {
    let mut table = DecodedDocumentTable::default();
    for (table_type, table_item) in length_prefixed_items(record, "DOCY table")? {
        if table_type != 3 {
            continue;
        }
        for (row_type, row_item) in length_prefixed_items(table_item, "DOCY table rows")? {
            if row_type != 4 {
                continue;
            }
            let mut row = Vec::new();
            for (row_content_type, row_content) in
                length_prefixed_items(row_item, "DOCY table row")?
            {
                if row_content_type != 5 {
                    continue;
                }
                for (cell_type, cell_item) in
                    length_prefixed_items(row_content, "DOCY table row cells")?
                {
                    if cell_type != 6 {
                        continue;
                    }
                    let mut cell = DecodedDocumentCell::default();
                    for (cell_content_type, cell_content) in
                        length_prefixed_items(cell_item, "DOCY table cell")?
                    {
                        if cell_content_type == 8 {
                            cell.blocks =
                                decode_document_binary_blocks(cell_content, header_footer_roles)?;
                        }
                    }
                    row.push(cell);
                }
            }
            table.rows.push(row);
        }
    }
    Ok(table)
}

fn decode_document_binary_paragraph_properties(
    properties: &[u8],
    paragraph: &mut DecodedDocumentParagraph,
    header_footer_roles: &DecodedDocumentHeaderFooterRoleTypes,
) -> anyhow::Result<()> {
    for (property_type, value) in binary_properties(properties, "DOCY paragraph properties")? {
        match property_type {
            1 => {
                for (indent_type, indent) in binary_properties(value, "DOCY paragraph indent")? {
                    if indent_type == 36 {
                        paragraph.left_indent_twips = read_u32_property(indent);
                    }
                }
            }
            5 => paragraph.alignment = value.first().copied(),
            9 => {
                for (spacing_type, spacing) in binary_properties(value, "DOCY paragraph spacing")? {
                    match spacing_type {
                        39 => paragraph.line_spacing_twips = read_u32_property(spacing),
                        11 => paragraph.line_spacing_rule = spacing.first().copied(),
                        _ => {}
                    }
                }
            }
            21 => {
                paragraph.style_id = Some(decode_utf16_le(value, "DOCY paragraph style")?);
            }
            22 => {
                for (numbering_type, numbering) in
                    binary_properties(value, "DOCY paragraph numbering")?
                {
                    match numbering_type {
                        23 => paragraph.num_level = read_u32_property(numbering),
                        24 => paragraph.num_id = read_u32_property(numbering),
                        _ => {}
                    }
                }
            }
            31 => {
                paragraph.section =
                    Some(decode_document_binary_section(value, header_footer_roles)?);
            }
            _ => {}
        }
    }
    Ok(())
}

fn decode_document_binary_section(
    value: &[u8],
    header_footer_roles: &DecodedDocumentHeaderFooterRoleTypes,
) -> anyhow::Result<DecodedDocumentSection> {
    let mut section = DecodedDocumentSection::default();
    for (section_type, section_value) in length_prefixed_items(value, "DOCY section properties")? {
        match section_type {
            0 => {
                for (size_type, size_value) in
                    binary_properties(section_value, "DOCY section page size")?
                {
                    match size_type {
                        2 => section.orientation = size_value.first().copied(),
                        3 => section.width_twips = read_u32_property(size_value),
                        4 => section.height_twips = read_u32_property(size_value),
                        _ => {}
                    }
                }
            }
            1 => {
                for (margin_type, margin_value) in
                    binary_properties(section_value, "DOCY section margins")?
                {
                    if (6..=12).contains(&margin_type) {
                        section.margins_twips[usize::from(margin_type - 6)] =
                            read_u32_property(margin_value);
                    }
                }
            }
            2 => {
                for (setting_type, setting_value) in
                    binary_properties(section_value, "DOCY section settings")?
                {
                    match setting_type {
                        0 => section.title_page = setting_value.first().map(|value| *value != 0),
                        2 => section.break_type = setting_value.first().copied(),
                        _ => {}
                    }
                }
            }
            3 => {
                for index in decode_document_binary_section_header_footer_refs(section_value)? {
                    match index
                        .and_then(|index| header_footer_roles.headers.get(index).copied())
                        .unwrap_or(4)
                    {
                        2 => section.header_first = index,
                        3 => section.header_even = index,
                        _ => section.header_default = index,
                    }
                }
            }
            4 => {
                for index in decode_document_binary_section_header_footer_refs(section_value)? {
                    match index
                        .and_then(|index| header_footer_roles.footers.get(index).copied())
                        .unwrap_or(4)
                    {
                        2 => section.footer_first = index,
                        3 => section.footer_even = index,
                        _ => section.footer_default = index,
                    }
                }
            }
            _ => {}
        }
    }
    Ok(section)
}

fn decode_document_binary_section_header_footer_refs(
    value: &[u8],
) -> anyhow::Result<Vec<Option<usize>>> {
    let mut refs = Vec::new();
    for (reference_type, reference_value) in
        length_prefixed_items(value, "DOCY section HdrFtr refs")?
    {
        if reference_type != 5 {
            continue;
        }
        refs.push(read_u32_property(reference_value).map(|value| value as usize));
    }
    Ok(refs)
}

fn decode_document_binary_run_formatting(record: &[u8]) -> anyhow::Result<DecodedDocumentRun> {
    let mut run = DecodedDocumentRun::default();
    for (run_type, run_item) in length_prefixed_items(record, "DOCY run")? {
        match run_type {
            1 => {
                for (property_type, value) in binary_properties(run_item, "DOCY run properties")? {
                    match property_type {
                        0 => run.bold = value.first().map(|value| *value != 0),
                        1 => run.italic = value.first().map(|value| *value != 0),
                        2 => run.underline = value.first().copied(),
                        8 => run.font_size_half_points = read_u32_property(value),
                        9 if value.len() >= 3 => run.color = Some([value[0], value[1], value[2]]),
                        _ => {}
                    }
                }
            }
            8 => decode_document_binary_run_content(run_item, &mut run)?,
            _ => {}
        }
    }
    Ok(run)
}

fn binary_properties<'a>(bytes: &'a [u8], label: &str) -> anyhow::Result<Vec<(u8, &'a [u8])>> {
    let mut properties = Vec::new();
    let mut position = 0usize;
    while position < bytes.len() {
        ensure!(position + 2 <= bytes.len(), "{label} header is truncated");
        let property_type = bytes[position];
        let length_type = bytes[position + 1];
        position += 2;
        let length = match length_type {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 | 7 | 8 => 8,
            6 => {
                ensure!(
                    position + 4 <= bytes.len(),
                    "{label} variable length is truncated"
                );
                let value = u32::from_le_bytes(bytes[position..position + 4].try_into().unwrap());
                position += 4;
                usize::try_from(value).context("DOCY property length overflow")?
            }
            other => anyhow::bail!("{label} has unsupported length type {other}"),
        };
        let end = position
            .checked_add(length)
            .context("DOCY property range overflow")?;
        let value = bytes
            .get(position..end)
            .with_context(|| format!("{label} is truncated"))?;
        properties.push((property_type, value));
        position = end;
    }
    Ok(properties)
}

fn read_u32_property(value: &[u8]) -> Option<u32> {
    value
        .get(..4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn decode_document_binary_run_content(
    run_item: &[u8],
    run: &mut DecodedDocumentRun,
) -> anyhow::Result<()> {
    let mut position = 0usize;
    while position < run_item.len() {
        let content_type = run_item[position];
        position += 1;
        ensure!(
            position + 4 <= run_item.len(),
            "DOCY run content length is truncated"
        );
        let length =
            u32::from_le_bytes(run_item[position..position + 4].try_into().unwrap()) as usize;
        position += 4;
        let end = position
            .checked_add(length)
            .context("DOCY run content overflow")?;
        let value = run_item
            .get(position..end)
            .context("DOCY run content is truncated")?;
        match content_type {
            0 => run.text.push_str(&decode_utf16_le(value, "DOCY run text")?),
            15 => {
                run.text
                    .push_str(&decode_utf16_le(value, "DOCY deleted run text")?);
                run.deleted_text = true;
            }
            30 => {
                run.text
                    .push_str(&decode_utf16_le(value, "DOCY instruction text")?);
                run.instruction_text = true;
            }
            29 => {
                for (field_type, field_value) in
                    length_prefixed_items(value, "DOCY field character")?
                {
                    if field_type == 3 {
                        run.field_char = field_value.first().copied();
                    }
                }
            }
            11 => run.comment_reference = Some(decode_document_comment_marker(value)?),
            2 => run.tab = true,
            4 => run.page_break = true,
            5 => run.line_break = true,
            12 => run.drawing = Some(decode_document_binary_drawing(value)?),
            _ => {}
        }
        position = end;
    }
    Ok(())
}

fn decode_document_binary_drawing(value: &[u8]) -> anyhow::Result<DecodedDocumentDrawing> {
    let mut drawing = DecodedDocumentDrawing::default();
    for (property_type, property) in binary_properties(value, "DOCY drawing")? {
        match property_type {
            0 => drawing.inline = property.first().map(|value| *value == 0),
            14 => {
                let mut cx = None;
                let mut cy = None;
                for (extent_type, extent_value) in
                    binary_properties(property, "DOCY drawing extent")?
                {
                    match extent_type {
                        2 => cx = read_u32_property(extent_value),
                        3 => cy = read_u32_property(extent_value),
                        _ => {}
                    }
                }
                drawing.extent_emu = cx.zip(cy);
            }
            1 => {
                drawing.pptx_data_bytes = Some(property.len());
                drawing.raster_ids = decode_ppty_raster_ids(property);
                if drawing.raster_ids.is_empty() {
                    let (fill, rotation) = decode_ppty_shape_properties(property)?;
                    drawing.shape_fill = fill;
                    drawing.shape_rotation = rotation;
                }
            }
            25 => {
                drawing.chart2_data_bytes = Some(property.len());
                drawing.chart_style = decode_document_chart_style(property)?;
            }
            _ => {}
        }
    }
    Ok(drawing)
}

fn decode_ppty_shape_properties(value: &[u8]) -> anyhow::Result<(Option<[u8; 3]>, Option<i32>)> {
    // refs: sdkjs/common/Shapes/SerializeWriter.js:5588-5718
    // The CTOX writer emits the same nested PPTY record tree as the upstream
    // shape writer: outer(0) -> drawing(1) -> shape(1) -> spPr(1).
    let outer = ppty_single_record(value, 0, "PPTY drawing outer")?;
    let drawing =
        ppty_find_record(outer, 0, 1, "PPTY drawing")?.context("PPTY drawing record is missing")?;
    let shape =
        ppty_find_record(drawing, 0, 1, "PPTY shape")?.context("PPTY shape record is missing")?;
    let shape_records_offset = ppty_attribute_prefix_end(shape, "PPTY shape")?;
    let shape_properties =
        ppty_find_record(shape, shape_records_offset, 1, "PPTY shape properties")?
            .context("PPTY shape properties record is missing")?;
    let properties_offset = ppty_attribute_prefix_end(shape_properties, "PPTY shape properties")?;

    let rotation = ppty_find_record(shape_properties, properties_offset, 0, "PPTY transform")?
        .and_then(decode_ppty_transform_rotation);
    let fill = ppty_find_record(shape_properties, properties_offset, 2, "PPTY shape fill")?
        .and_then(decode_ppty_solid_fill_rgb);
    Ok((fill, rotation))
}

fn ppty_single_record<'a>(
    value: &'a [u8],
    expected_type: u8,
    label: &str,
) -> anyhow::Result<&'a [u8]> {
    let (record_type, content, end) = ppty_record_at(value, 0, label)?;
    ensure!(
        record_type == expected_type,
        "{label} has type {record_type}, expected {expected_type}"
    );
    ensure!(end == value.len(), "{label} has trailing bytes");
    Ok(content)
}

fn ppty_record_at<'a>(
    value: &'a [u8],
    position: usize,
    label: &str,
) -> anyhow::Result<(u8, &'a [u8], usize)> {
    ensure!(position + 5 <= value.len(), "{label} header is truncated");
    let record_type = value[position];
    let length = u32::from_le_bytes(value[position + 1..position + 5].try_into().unwrap()) as usize;
    let start = position + 5;
    let end = start
        .checked_add(length)
        .context("PPTY record range overflow")?;
    let content = value
        .get(start..end)
        .with_context(|| format!("{label} is truncated"))?;
    Ok((record_type, content, end))
}

fn ppty_find_record<'a>(
    value: &'a [u8],
    mut position: usize,
    expected_type: u8,
    label: &str,
) -> anyhow::Result<Option<&'a [u8]>> {
    while position < value.len() {
        let (record_type, content, end) = ppty_record_at(value, position, label)?;
        if record_type == expected_type {
            return Ok(Some(content));
        }
        position = end;
    }
    Ok(None)
}

fn ppty_attribute_prefix_end(value: &[u8], label: &str) -> anyhow::Result<usize> {
    ensure!(
        value.first().copied() == Some(0xFA),
        "{label} attribute prefix is missing"
    );
    value
        .iter()
        .position(|byte| *byte == 0xFB)
        .map(|position| position + 1)
        .context(format!("{label} attribute terminator is missing"))
}

fn decode_ppty_transform_rotation(value: &[u8]) -> Option<i32> {
    if value.first().copied() != Some(0xFA) {
        return None;
    }
    let mut position = 1usize;
    while position < value.len() && value[position] != 0xFB {
        let attribute = value[position];
        position += 1;
        let bytes = value.get(position..position + 4)?;
        let number = i32::from_le_bytes(bytes.try_into().ok()?);
        if attribute == 10 {
            return Some(number);
        }
        position += 4;
    }
    None
}

fn decode_ppty_solid_fill_rgb(value: &[u8]) -> Option<[u8; 3]> {
    let solid = ppty_find_record(value, 0, 3, "PPTY solid fill").ok()??;
    let unicolor = ppty_find_record(solid, 0, 0, "PPTY unicolor").ok()??;
    let rgb = ppty_find_record(unicolor, 0, 1, "PPTY RGB color").ok()??;
    if rgb.first().copied() != Some(0xFA) {
        return None;
    }
    let mut color = [0u8; 3];
    let mut seen = [false; 3];
    let mut position = 1usize;
    while position + 1 < rgb.len() && rgb[position] != 0xFB {
        let attribute = usize::from(rgb[position]);
        let value = rgb[position + 1];
        if attribute < 3 {
            color[attribute] = value;
            seen[attribute] = true;
        }
        position += 2;
    }
    seen.iter().all(|value| *value).then_some(color)
}

fn decode_document_chart_style(value: &[u8]) -> anyhow::Result<Option<u8>> {
    // refs: sdkjs/common/SerializeChart.js:1451-1485,5999-6017
    // Built-in chart styles are stored as AlternateContent. The fallback
    // carries the OOXML c:style value, while the choice contains 100 + value.
    let alternate = length_prefixed_items(value, "DOCY chart space")?
        .into_iter()
        .find_map(|(kind, item)| (kind == 3).then_some(item));
    let Some(alternate) = alternate else {
        return Ok(None);
    };
    let fallback = length_prefixed_items(alternate, "DOCY chart alternate content")?
        .into_iter()
        .find_map(|(kind, item)| (kind == 1).then_some(item));
    let Some(fallback) = fallback else {
        return Ok(None);
    };
    let style = length_prefixed_items(fallback, "DOCY chart fallback")?
        .into_iter()
        .find_map(|(kind, item)| (kind == 0).then_some(item));
    let Some(style) = style else { return Ok(None) };
    let value = length_prefixed_items(style, "DOCY chart style")?
        .into_iter()
        .find_map(|(kind, item)| (kind == 0).then(|| item.first().copied()).flatten());
    Ok(value)
}

fn decode_ppty_raster_ids(value: &[u8]) -> Vec<String> {
    let mut ids = collect_ppty_string2_values(value)
        .into_iter()
        .filter(|value| value.starts_with("media/") || value.starts_with("word/media/"))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn collect_ppty_string2_values(value: &[u8]) -> Vec<String> {
    let mut values = Vec::new();
    for position in 0..value.len().saturating_sub(4) {
        let chars = u32::from_le_bytes(
            value[position..position + 4]
                .try_into()
                .expect("four-byte PPTY scan window"),
        ) as usize;
        if chars == 0 || chars > 512 {
            continue;
        }
        let Some(byte_len) = chars.checked_mul(2) else {
            continue;
        };
        let start = position + 4;
        let Some(end) = start.checked_add(byte_len) else {
            continue;
        };
        let Some(bytes) = value.get(start..end) else {
            continue;
        };
        let Ok(decoded) = decode_utf16_le(bytes, "PPTY string2") else {
            continue;
        };
        if decoded.chars().all(|ch| {
            ch == '/' || ch == '.' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric()
        }) {
            values.push(decoded);
        }
    }
    values
}

fn replace_document_paragraph_formatting(
    xml: &[u8],
    paragraphs: &[DecodedDocumentParagraph],
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<Vec<u8>> {
    let source = std::str::from_utf8(xml).context("document XML is not UTF-8")?;
    let paragraph_pattern = Regex::new(r"(?s)<w:p(?:\s[^>]*)?/>|<w:p(?:\s[^>]*)?>.*?</w:p>")?;
    let matches = paragraph_pattern.find_iter(source).collect::<Vec<_>>();
    ensure!(
        matches.len() == paragraphs.len(),
        "DOCY paragraph count {} does not match original DOCX paragraph count {}",
        paragraphs.len(),
        matches.len()
    );
    let mut output = String::with_capacity(source.len() + paragraphs.len() * 96);
    let mut cursor = 0usize;
    for (paragraph, found) in paragraphs.iter().zip(matches) {
        output.push_str(&source[cursor..found.start()]);
        output.push_str(&rewrite_document_paragraph_xml(
            found.as_str(),
            paragraph,
            style_ids,
            header_footer_relationships,
        )?);
        cursor = found.end();
    }
    output.push_str(&source[cursor..]);
    Ok(output.into_bytes())
}

fn replace_document_body_from_decoded_blocks(
    xml: &[u8],
    blocks: &[DecodedDocumentBlock],
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<Vec<u8>> {
    let source = std::str::from_utf8(xml).context("document XML is not UTF-8")?;
    let body_open = Regex::new(r"(?s)<w:body(?:\s[^>]*)?>")?
        .find(source)
        .context("document body opening tag is missing")?;
    let body_close = source
        .rfind("</w:body>")
        .context("document body closing tag is missing")?;
    ensure!(
        body_open.end() <= body_close,
        "document body range is invalid"
    );
    let body_content = &source[body_open.end()..body_close];
    let section_pattern = Regex::new(r"(?s)<w:sectPr(?:\s[^>]*)?>.*?</w:sectPr>")?;
    let section = section_pattern
        .find_iter(body_content)
        .last()
        .map(|found| found.as_str())
        .unwrap_or("");
    let mut replacement = String::with_capacity(source.len() + blocks.len() * 256);
    replacement.push_str(&source[..body_open.end()]);
    replacement.push_str(&render_decoded_document_blocks(
        blocks,
        style_ids,
        header_footer_relationships,
    )?);
    if !section.is_empty() {
        replacement.push_str(section);
    }
    replacement.push_str(&source[body_close..]);
    Ok(replacement.into_bytes())
}

fn render_decoded_document_blocks(
    blocks: &[DecodedDocumentBlock],
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<String> {
    let mut output = String::new();
    for block in blocks {
        match block {
            DecodedDocumentBlock::Paragraph(paragraph) => {
                output.push_str(&render_decoded_document_paragraph(
                    paragraph,
                    style_ids,
                    header_footer_relationships,
                ));
            }
            DecodedDocumentBlock::Table(table) => {
                output.push_str(&render_decoded_document_table(
                    table,
                    style_ids,
                    header_footer_relationships,
                )?);
            }
        }
    }
    Ok(output)
}

fn render_decoded_document_header_part(
    blocks: &[DecodedDocumentBlock],
    style_ids: &BTreeMap<String, String>,
) -> anyhow::Result<Vec<u8>> {
    render_decoded_document_header_footer_part("hdr", blocks, style_ids)
}

fn render_decoded_document_footer_part(
    blocks: &[DecodedDocumentBlock],
    style_ids: &BTreeMap<String, String>,
) -> anyhow::Result<Vec<u8>> {
    render_decoded_document_header_footer_part("ftr", blocks, style_ids)
}

fn render_decoded_document_header_footer_part(
    element: &str,
    blocks: &[DecodedDocumentBlock],
    style_ids: &BTreeMap<String, String>,
) -> anyhow::Result<Vec<u8>> {
    let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>");
    output.push_str(&format!(
        "<w:{element} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">"
    ));
    let header_footer_relationships = DecodedDocumentHeaderFooterRelationshipIds::default();
    if blocks.is_empty() {
        output.push_str("<w:p/>");
    } else {
        output.push_str(&render_decoded_document_blocks(
            blocks,
            style_ids,
            &header_footer_relationships,
        )?);
    }
    output.push_str(&format!("</w:{element}>"));
    Ok(output.into_bytes())
}

fn render_decoded_document_table(
    table: &DecodedDocumentTable,
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<String> {
    let column_count = table.rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let column_width = 9000u32 / u32::try_from(column_count).unwrap_or(1);
    let mut output = String::new();
    output.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/></w:tblBorders></w:tblPr><w:tblGrid>");
    for _ in 0..column_count {
        output.push_str(&format!("<w:gridCol w:w=\"{column_width}\"/>"));
    }
    output.push_str("</w:tblGrid>");
    for row in &table.rows {
        output.push_str("<w:tr>");
        for cell in row {
            output.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{column_width}\" w:type=\"dxa\"/></w:tcPr>"
            ));
            if cell.blocks.is_empty() {
                output.push_str("<w:p/>");
            } else {
                output.push_str(&render_decoded_document_blocks(
                    &cell.blocks,
                    style_ids,
                    header_footer_relationships,
                )?);
            }
            output.push_str("</w:tc>");
        }
        output.push_str("</w:tr>");
    }
    output.push_str("</w:tbl>");
    Ok(output)
}

fn render_decoded_document_paragraph(
    paragraph: &DecodedDocumentParagraph,
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> String {
    let mut output = String::new();
    output.push_str("<w:p>");
    let properties = render_decoded_document_paragraph_properties(
        paragraph,
        style_ids,
        header_footer_relationships,
    );
    if !properties.is_empty() {
        output.push_str("<w:pPr>");
        output.push_str(&properties);
        output.push_str("</w:pPr>");
    }
    if !paragraph.content.is_empty() {
        for item in &paragraph.content {
            output.push_str(&render_decoded_document_inline(item));
        }
    } else if paragraph.runs.is_empty() && !paragraph.text.is_empty() {
        output.push_str("<w:r><w:t>");
        output.push_str(&xml_escape_text(&paragraph.text));
        output.push_str("</w:t></w:r>");
    } else {
        for run in &paragraph.runs {
            output.push_str(&render_decoded_document_run(run));
        }
    }
    output.push_str("</w:p>");
    output
}

fn render_decoded_document_inline(item: &DecodedDocumentInline) -> String {
    match item {
        DecodedDocumentInline::Run(run) => render_decoded_document_run(run),
        DecodedDocumentInline::Bookmark { id, name, start } => {
            if *start {
                format!(
                    "<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/>",
                    xml_escape_attribute(name.as_deref().unwrap_or_default())
                )
            } else {
                format!("<w:bookmarkEnd w:id=\"{id}\"/>")
            }
        }
        DecodedDocumentInline::Hyperlink {
            value,
            anchor,
            tooltip,
            runs,
        } => {
            let mut output = String::from("<w:hyperlink");
            if !value.is_empty() {
                output.push_str(&format!(
                    " r:id=\"__CTOX_HREF_{}__\"",
                    hex_encode(value.as_bytes())
                ));
            }
            if let Some(anchor) = anchor.as_deref().filter(|value| !value.is_empty()) {
                output.push_str(&format!(" w:anchor=\"{}\"", xml_escape_attribute(anchor)));
            }
            if let Some(tooltip) = tooltip.as_deref().filter(|value| !value.is_empty()) {
                output.push_str(&format!(" w:tooltip=\"{}\"", xml_escape_attribute(tooltip)));
            }
            output.push('>');
            for run in runs {
                output.push_str(&render_decoded_document_run(run));
            }
            output.push_str("</w:hyperlink>");
            output
        }
        DecodedDocumentInline::CommentStart(id) => format!("<w:commentRangeStart w:id=\"{id}\"/>"),
        DecodedDocumentInline::CommentEnd(id) => format!("<w:commentRangeEnd w:id=\"{id}\"/>"),
        DecodedDocumentInline::Revision {
            kind,
            id,
            author,
            date,
            runs,
        } => {
            let tag = if *kind == 12 { "del" } else { "ins" };
            let mut output = format!("<w:{tag} w:id=\"{id}\"");
            if let Some(author) = author {
                output.push_str(&format!(" w:author=\"{}\"", xml_escape_attribute(author)));
            }
            if let Some(date) = date {
                output.push_str(&format!(" w:date=\"{}\"", xml_escape_attribute(date)));
            }
            output.push('>');
            for run in runs {
                output.push_str(&render_decoded_document_run(run));
            }
            output.push_str(&format!("</w:{tag}>"));
            output
        }
    }
}

fn render_decoded_document_paragraph_properties(
    paragraph: &DecodedDocumentParagraph,
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> String {
    let mut output = String::new();
    if let Some(style_id) = paragraph.style_id.as_deref() {
        let style_id = style_ids
            .get(style_id)
            .map(String::as_str)
            .unwrap_or_else(|| document_ooxml_style_id(style_id));
        output.push_str(&format!(
            "<w:pStyle w:val=\"{}\"/>",
            xml_escape_attr(style_id)
        ));
    }
    if paragraph.num_id.is_some() || paragraph.num_level.is_some() {
        output.push_str("<w:numPr>");
        if let Some(level) = paragraph.num_level {
            output.push_str(&format!("<w:ilvl w:val=\"{level}\"/>"));
        }
        if let Some(num_id) = paragraph.num_id {
            output.push_str(&format!("<w:numId w:val=\"{num_id}\"/>"));
        }
        output.push_str("</w:numPr>");
    }
    if let Some(alignment) = paragraph.alignment.and_then(document_alignment_name) {
        output.push_str(&format!("<w:jc w:val=\"{alignment}\"/>"));
    }
    if let Some(left) = paragraph.left_indent_twips {
        output.push_str(&format!("<w:ind w:left=\"{left}\"/>"));
    }
    if paragraph.line_spacing_twips.is_some() || paragraph.line_spacing_rule.is_some() {
        output.push_str("<w:spacing");
        if let Some(line) = paragraph.line_spacing_twips {
            output.push_str(&format!(" w:line=\"{line}\""));
        }
        if let Some(rule) = paragraph
            .line_spacing_rule
            .and_then(document_line_rule_name)
        {
            output.push_str(&format!(" w:lineRule=\"{rule}\""));
        }
        output.push_str("/>");
    }
    if let Some(section) = &paragraph.section {
        output.push_str(&render_decoded_document_section(
            section,
            "",
            header_footer_relationships,
        ));
    }
    output
}

fn render_decoded_document_run(run: &DecodedDocumentRun) -> String {
    let mut output = String::new();
    output.push_str("<w:r>");
    let properties = render_decoded_document_run_properties(run);
    if !properties.is_empty() {
        output.push_str("<w:rPr>");
        output.push_str(&properties);
        output.push_str("</w:rPr>");
    }
    if let Some(field_char) = run.field_char {
        let kind = match field_char {
            1 => "separate",
            2 => "end",
            _ => "begin",
        };
        output.push_str(&format!("<w:fldChar w:fldCharType=\"{kind}\"/>"));
        output.push_str("</w:r>");
        return output;
    }
    if let Some(comment_id) = run.comment_reference {
        output.push_str(&format!(
            "<w:commentReference w:id=\"{comment_id}\"/></w:r>"
        ));
        return output;
    }
    if run.page_break {
        output.push_str("<w:br w:type=\"page\"/></w:r>");
        return output;
    }
    if run.line_break {
        output.push_str("<w:br/></w:r>");
        return output;
    }
    if run.tab {
        output.push_str("<w:tab/></w:r>");
        return output;
    }
    output.push_str(if run.instruction_text {
        "<w:instrText"
    } else if run.deleted_text {
        "<w:delText"
    } else {
        "<w:t"
    });
    if run.text.starts_with(char::is_whitespace) || run.text.ends_with(char::is_whitespace) {
        output.push_str(" xml:space=\"preserve\"");
    }
    output.push('>');
    output.push_str(&xml_escape_text(&run.text));
    output.push_str(if run.instruction_text {
        "</w:instrText></w:r>"
    } else if run.deleted_text {
        "</w:delText></w:r>"
    } else {
        "</w:t></w:r>"
    });
    output
}

fn render_decoded_document_run_properties(run: &DecodedDocumentRun) -> String {
    let mut output = String::new();
    if let Some(value) = run.bold {
        output.push_str(if value {
            "<w:b/>"
        } else {
            "<w:b w:val=\"0\"/>"
        });
    }
    if let Some(value) = run.italic {
        output.push_str(if value {
            "<w:i/>"
        } else {
            "<w:i w:val=\"0\"/>"
        });
    }
    if let Some(value) = run.underline {
        output.push_str(if value == 0 {
            "<w:u w:val=\"none\"/>"
        } else {
            "<w:u w:val=\"single\"/>"
        });
    }
    if let Some(value) = run.font_size_half_points {
        output.push_str(&format!("<w:sz w:val=\"{value}\"/>"));
    }
    if let Some(color) = run.color {
        output.push_str(&format!(
            "<w:color w:val=\"{:02X}{:02X}{:02X}\"/>",
            color[0], color[1], color[2]
        ));
    }
    output
}

fn rewrite_document_paragraph_xml(
    paragraph_xml: &str,
    paragraph: &DecodedDocumentParagraph,
    style_ids: &BTreeMap<String, String>,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<String> {
    let normalized_paragraph;
    let paragraph_xml = if paragraph_xml.trim_end().ends_with("/>") {
        let end = paragraph_xml
            .rfind("/>")
            .context("OOXML self-closing paragraph end is missing")?;
        normalized_paragraph = format!("{}></w:p>", &paragraph_xml[..end]);
        normalized_paragraph.as_str()
    } else {
        paragraph_xml
    };
    let run_pattern = Regex::new(r"(?s)<w:r(?:\s[^>]*)?>.*?</w:r>")?;
    let run_matches = run_pattern
        .find_iter(paragraph_xml)
        .filter(|found| original_document_run_has_decoded_content(found.as_str()))
        .collect::<Vec<_>>();
    let decoded_runs = paragraph
        .runs
        .iter()
        .filter(|run| {
            !run.text.is_empty()
                || run.drawing.is_some()
                || run.page_break
                || run.line_break
                || run.tab
        })
        .collect::<Vec<_>>();
    ensure!(
        run_matches.len() == decoded_runs.len(),
        "DOCY run count {} does not match original DOCX run count {} for paragraph {:?}; original runs: {:?}",
        decoded_runs.len(),
        run_matches.len(),
        paragraph.text,
        run_matches.iter().map(|run| run.as_str()).collect::<Vec<_>>()
    );
    let mut rewritten = String::with_capacity(paragraph_xml.len() + 128);
    let mut cursor = 0usize;
    for (run, found) in decoded_runs.into_iter().zip(run_matches) {
        rewritten.push_str(&paragraph_xml[cursor..found.start()]);
        if let Some(drawing) = &run.drawing {
            rewritten.push_str(&rewrite_document_drawing_run_xml(found.as_str(), drawing)?);
        } else if run.page_break || run.line_break || run.tab {
            rewritten.push_str(found.as_str());
        } else {
            rewritten.push_str(&rewrite_document_run_xml(found.as_str(), run)?);
        }
        cursor = found.end();
    }
    rewritten.push_str(&paragraph_xml[cursor..]);

    if let Some(style_id) = paragraph.style_id.as_deref() {
        let style_id = style_ids
            .get(style_id)
            .map(String::as_str)
            .unwrap_or_else(|| document_ooxml_style_id(style_id));
        rewritten =
            upsert_empty_property(&rewritten, "p", "pPr", "pStyle", &[("w:val", style_id)])?;
    }
    if paragraph.num_id.is_some() || paragraph.num_level.is_some() {
        rewritten = upsert_document_numbering_properties(
            &rewritten,
            paragraph.num_id,
            paragraph.num_level,
        )?;
    }
    if let Some(alignment) = paragraph.alignment.and_then(document_alignment_name) {
        rewritten = upsert_empty_property(&rewritten, "p", "pPr", "jc", &[("w:val", alignment)])?;
    }
    if let Some(left) = paragraph.left_indent_twips {
        let value = left.to_string();
        rewritten = upsert_empty_property(&rewritten, "p", "pPr", "ind", &[("w:left", &value)])?;
    }
    if paragraph.line_spacing_twips.is_some() || paragraph.line_spacing_rule.is_some() {
        let line = paragraph.line_spacing_twips.map(|value| value.to_string());
        let rule = paragraph
            .line_spacing_rule
            .and_then(document_line_rule_name);
        let mut attributes = Vec::new();
        if let Some(line) = line.as_deref() {
            attributes.push(("w:line", line));
        }
        if let Some(rule) = rule {
            attributes.push(("w:lineRule", rule));
        }
        rewritten = upsert_empty_property(&rewritten, "p", "pPr", "spacing", &attributes)?;
    }
    if let Some(section) = &paragraph.section {
        rewritten =
            upsert_document_section_properties(&rewritten, section, header_footer_relationships)?;
    }
    Ok(rewritten)
}

fn original_document_run_has_decoded_content(run_xml: &str) -> bool {
    if [
        "<w:drawing",
        "<w:fldChar",
        "<w:instrText",
        "<w:delText",
        "<w:tab",
        "<w:br",
        "<w:commentReference",
    ]
    .iter()
    .any(|marker| run_xml.contains(marker))
    {
        return true;
    }
    let Ok(text) = Regex::new(r"(?s)<w:t(?:\s[^>]*)?>(.*?)</w:t>") else {
        return true;
    };
    let has_text = text
        .captures_iter(run_xml)
        .filter_map(|capture| capture.get(1))
        .any(|value| !value.as_str().is_empty());
    has_text
}

fn rewrite_document_drawing_run_xml(
    run_xml: &str,
    drawing: &DecodedDocumentDrawing,
) -> anyhow::Result<String> {
    let mut rewritten = run_xml.to_string();
    if let Some((cx, cy)) = drawing.extent_emu {
        rewritten = replace_xml_element_attribute(&rewritten, "wp:extent", "cx", &cx.to_string())?;
        rewritten = replace_xml_element_attribute(&rewritten, "wp:extent", "cy", &cy.to_string())?;
    }
    if let Some(rotation) = drawing.shape_rotation {
        rewritten =
            upsert_xml_element_attribute(&rewritten, "a:xfrm", "rot", &rotation.to_string())?;
    }
    if let Some(fill) = drawing.shape_fill {
        rewritten = replace_first_shape_fill_rgb(&rewritten, fill)?;
    }
    Ok(rewritten)
}

fn replace_xml_element_attribute(
    xml: &str,
    element: &str,
    attribute: &str,
    value: &str,
) -> anyhow::Result<String> {
    let pattern = Regex::new(&format!(
        r#"(?s)(<{}\b[^>]*\b{}=")[^"]*(")"#,
        regex::escape(element),
        regex::escape(attribute)
    ))?;
    if !pattern.is_match(xml) {
        return Ok(xml.to_string());
    }
    Ok(pattern
        .replace(xml, format!("${{1}}{value}${{2}}"))
        .into_owned())
}

fn upsert_xml_element_attribute(
    xml: &str,
    element: &str,
    attribute: &str,
    value: &str,
) -> anyhow::Result<String> {
    let attribute_pattern = Regex::new(&format!(
        r#"(?s)(<{}\b[^>]*\b{}=")[^"]*(")"#,
        regex::escape(element),
        regex::escape(attribute)
    ))?;
    if attribute_pattern.is_match(xml) {
        return Ok(attribute_pattern
            .replace(xml, format!("${{1}}{value}${{2}}"))
            .into_owned());
    }
    let pattern = Regex::new(&format!(r#"(<{}\b)"#, regex::escape(element)))?;
    ensure!(
        pattern.is_match(xml),
        "drawing XML element {element} is missing"
    );
    Ok(pattern
        .replace(xml, format!(r#"${{1}} {attribute}="{value}""#))
        .into_owned())
}

fn replace_first_shape_fill_rgb(xml: &str, color: [u8; 3]) -> anyhow::Result<String> {
    let properties_pattern = Regex::new(r"(?s)<wps:spPr\b[^>]*>.*?</wps:spPr>")?;
    let Some(properties) = properties_pattern.find(xml) else {
        return Ok(xml.to_string());
    };
    let properties_xml = properties.as_str();
    let color_pattern =
        Regex::new(r#"(?s)(<a:solidFill\b[^>]*>.*?<a:srgbClr\b[^>]*\bval=")[^"]*(")"#)?;
    ensure!(
        color_pattern.is_match(properties_xml),
        "shape solid RGB fill is missing"
    );
    let replacement = color_pattern
        .replace(
            properties_xml,
            format!(
                "${{1}}{:02X}{:02X}{:02X}${{2}}",
                color[0], color[1], color[2]
            ),
        )
        .into_owned();
    let mut output = String::with_capacity(xml.len());
    output.push_str(&xml[..properties.start()]);
    output.push_str(&replacement);
    output.push_str(&xml[properties.end()..]);
    Ok(output)
}

fn upsert_document_section_properties(
    paragraph_xml: &str,
    section: &DecodedDocumentSection,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> anyhow::Result<String> {
    let properties_pattern = Regex::new(r"(?s)<w:pPr(?:\s[^>]*)?>.*?</w:pPr>")?;
    let section_pattern = Regex::new(r"(?s)<w:sectPr(?:\s[^>]*)?>.*?</w:sectPr>")?;
    if let Some(properties) = properties_pattern.find(paragraph_xml) {
        let properties_xml = properties.as_str();
        let replaced_properties = if let Some(found) = section_pattern.find(properties_xml) {
            let existing_section = found.as_str();
            let inner_start = existing_section
                .find('>')
                .map(|position| position + 1)
                .context("OOXML sectPr opening tag is missing")?;
            let inner_end = existing_section
                .rfind("</w:sectPr>")
                .context("OOXML sectPr closing tag is missing")?;
            let preserved = preserve_document_section_children(
                &existing_section[inner_start..inner_end],
                decoded_document_section_has_header_footer_refs(section),
            )?;
            let rendered =
                render_decoded_document_section(section, &preserved, header_footer_relationships);
            format!(
                "{}{}{}",
                &properties_xml[..found.start()],
                rendered,
                &properties_xml[found.end()..]
            )
        } else {
            let closing = properties_xml
                .rfind("</w:pPr>")
                .context("OOXML pPr closing tag is missing")?;
            format!(
                "{}{}{}",
                &properties_xml[..closing],
                render_decoded_document_section(section, "", header_footer_relationships),
                &properties_xml[closing..]
            )
        };
        return Ok(format!(
            "{}{}{}",
            &paragraph_xml[..properties.start()],
            replaced_properties,
            &paragraph_xml[properties.end()..]
        ));
    }
    let opening = Regex::new(r"<w:p(?:\s[^>]*)?>")?
        .find(paragraph_xml)
        .context("OOXML paragraph opening tag is missing")?;
    Ok(format!(
        "{}<w:pPr>{}</w:pPr>{}",
        &paragraph_xml[..opening.end()],
        render_decoded_document_section(section, "", header_footer_relationships),
        &paragraph_xml[opening.end()..]
    ))
}

fn preserve_document_section_children(
    inner: &str,
    remove_header_footer_refs: bool,
) -> anyhow::Result<String> {
    let pattern = if remove_header_footer_refs {
        r#"(?s)<w:(?:pgSz|pgMar|titlePg|type|headerReference|footerReference)\b[^>]*(?:/>|>.*?</w:(?:pgSz|pgMar|titlePg|type|headerReference|footerReference)>)"#
    } else {
        r#"(?s)<w:(?:pgSz|pgMar|titlePg|type)\b[^>]*(?:/>|>.*?</w:(?:pgSz|pgMar|titlePg|type)>)"#
    };
    let removable = Regex::new(pattern)?;
    Ok(removable.replace_all(inner, "").into_owned())
}

fn render_decoded_document_section(
    section: &DecodedDocumentSection,
    preserved_children: &str,
    header_footer_relationships: &DecodedDocumentHeaderFooterRelationshipIds,
) -> String {
    let mut output = String::from("<w:sectPr>");
    render_decoded_document_header_footer_refs(
        &mut output,
        "headerReference",
        &header_footer_relationships.headers,
        [
            ("default", section.header_default),
            ("even", section.header_even),
            ("first", section.header_first),
        ],
    );
    render_decoded_document_header_footer_refs(
        &mut output,
        "footerReference",
        &header_footer_relationships.footers,
        [
            ("default", section.footer_default),
            ("even", section.footer_even),
            ("first", section.footer_first),
        ],
    );
    output.push_str(preserved_children);
    if let (Some(width), Some(height)) = (section.width_twips, section.height_twips) {
        output.push_str(&format!("<w:pgSz w:w=\"{width}\" w:h=\"{height}\""));
        if section.orientation == Some(1) {
            output.push_str(" w:orient=\"landscape\"");
        }
        output.push_str("/>");
    }
    if section.margins_twips.iter().any(Option::is_some) {
        output.push_str("<w:pgMar");
        let [left, top, right, bottom, header, footer, gutter] = section.margins_twips;
        for (name, value) in [
            ("top", top),
            ("right", right),
            ("bottom", bottom),
            ("left", left),
            ("header", header),
            ("footer", footer),
            ("gutter", gutter),
        ] {
            if let Some(value) = value {
                output.push_str(&format!(" w:{name}=\"{value}\""));
            }
        }
        output.push_str("/>");
    }
    if let Some(break_type) = section
        .break_type
        .and_then(document_section_break_type_name)
    {
        output.push_str(&format!("<w:type w:val=\"{break_type}\"/>"));
    }
    if section.title_page == Some(true) {
        output.push_str("<w:titlePg/>");
    }
    output.push_str("</w:sectPr>");
    output
}

fn decoded_document_section_has_header_footer_refs(section: &DecodedDocumentSection) -> bool {
    section.header_default.is_some()
        || section.header_even.is_some()
        || section.header_first.is_some()
        || section.footer_default.is_some()
        || section.footer_even.is_some()
        || section.footer_first.is_some()
}

fn render_decoded_document_header_footer_refs<const N: usize>(
    output: &mut String,
    element: &str,
    relationship_ids: &[String],
    refs: [(&str, Option<usize>); N],
) {
    for (role, index) in refs {
        let Some(relationship_id) = index.and_then(|index| relationship_ids.get(index)) else {
            continue;
        };
        output.push_str(&format!(
            "<w:{element} w:type=\"{role}\" r:id=\"{}\"/>",
            xml_escape_attr(relationship_id)
        ));
    }
}

fn decode_document_binary_style_ids(
    payload: &[u8],
    styles_xml: &[u8],
) -> anyhow::Result<BTreeMap<String, String>> {
    let manifest = inspect_editor_payload(OfficeKind::Document, payload)?;
    let Some(table) = manifest.tables.iter().find(|table| table.table_type == 5) else {
        return Ok(BTreeMap::new());
    };
    let start = usize::try_from(table.offset).context("DOCY styles table offset overflow")?;
    let end = start
        .checked_add(usize::try_from(table.bytes).context("DOCY styles table size overflow")?)
        .context("DOCY styles table range overflow")?;
    let content = length_prefixed_content(
        payload
            .get(start..end)
            .context("DOCY styles table is truncated")?,
        "DOCY styles table",
    )?;
    let mut binary_names = BTreeMap::new();
    for (record_type, records) in length_prefixed_items(content, "DOCY styles content")? {
        if record_type != 2 {
            continue;
        }
        for (style_type, style) in length_prefixed_items(records, "DOCY styles")? {
            if style_type != 0 {
                continue;
            }
            let mut id = None;
            let mut name = None;
            for (property_type, value) in length_prefixed_items(style, "DOCY style")? {
                match property_type {
                    1 => id = Some(decode_utf16_le(value, "DOCY style id")?),
                    2 => name = Some(decode_utf16_le(value, "DOCY style name")?),
                    _ => {}
                }
            }
            if let (Some(id), Some(name)) = (id, name) {
                binary_names.insert(id, name);
            }
        }
    }
    if styles_xml.is_empty() {
        return Ok(BTreeMap::new());
    }
    let tree = roxmltree::Document::parse(
        std::str::from_utf8(styles_xml).context("document styles XML is not UTF-8")?,
    )?;
    let mut ooxml_names = BTreeMap::new();
    for style in tree
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "style")
    {
        let Some(id) = word_attribute(style, "styleId") else {
            continue;
        };
        let Some(name) = child_attribute(style, "name", "val") else {
            continue;
        };
        ooxml_names.insert(name.to_ascii_lowercase(), id.to_string());
    }
    Ok(binary_names
        .into_iter()
        .filter_map(|(id, name)| {
            ooxml_names
                .get(&name.to_ascii_lowercase())
                .cloned()
                .map(|style_id| (id, style_id))
        })
        .collect())
}

fn document_ooxml_style_id(value: &str) -> &str {
    match value {
        "693" => "Heading1",
        "734" => "Quote",
        "723" => "ListBullet",
        "726" => "ListNumber",
        "713" => "ListParagraph",
        _ => value,
    }
}

fn upsert_document_numbering_properties(
    xml: &str,
    num_id: Option<u32>,
    level: Option<u32>,
) -> anyhow::Result<String> {
    let mut attributes = String::new();
    if let Some(level) = level {
        attributes.push_str(&format!("<w:ilvl w:val=\"{level}\"/>"));
    }
    if let Some(num_id) = num_id {
        attributes.push_str(&format!("<w:numId w:val=\"{num_id}\"/>"));
    }
    let num_pr = format!("<w:numPr>{attributes}</w:numPr>");
    let properties_pattern = Regex::new(r"(?s)<w:pPr(?:\s[^>]*)?>.*?</w:pPr>")?;
    if let Some(properties) = properties_pattern.find(xml) {
        let num_pr_pattern = Regex::new(r"(?s)<w:numPr(?:\s[^>]*)?>.*?</w:numPr>")?;
        let block = if let Some(found) = num_pr_pattern.find(properties.as_str()) {
            format!(
                "{}{}{}",
                &properties.as_str()[..found.start()],
                num_pr,
                &properties.as_str()[found.end()..]
            )
        } else {
            let closing = properties
                .as_str()
                .rfind("</w:pPr>")
                .context("OOXML paragraph properties closing tag is missing")?;
            format!(
                "{}{}{}",
                &properties.as_str()[..closing],
                num_pr,
                &properties.as_str()[closing..]
            )
        };
        return Ok(format!(
            "{}{}{}",
            &xml[..properties.start()],
            block,
            &xml[properties.end()..]
        ));
    }
    let opening = Regex::new(r"<w:p(?:\s[^>]*)?>")?
        .find(xml)
        .context("OOXML paragraph opening tag is missing")?;
    Ok(format!(
        "{}<w:pPr>{num_pr}</w:pPr>{}",
        &xml[..opening.end()],
        &xml[opening.end()..]
    ))
}

fn rewrite_document_run_xml(run_xml: &str, run: &DecodedDocumentRun) -> anyhow::Result<String> {
    let text_pattern = Regex::new(r"(?s)<w:t(?:\s[^>]*)?>.*?</w:t>")?;
    let text_match = text_pattern
        .find(run_xml)
        .context("DOCX run containing DOCY text has no w:t element")?;
    let opening_end = run_xml[text_match.start()..text_match.end()]
        .find('>')
        .context("DOCX w:t opening tag is malformed")?
        + text_match.start();
    let closing_start = run_xml[text_match.start()..text_match.end()]
        .rfind("</w:t>")
        .context("DOCX w:t closing tag is missing")?
        + text_match.start();
    let mut rewritten = String::with_capacity(run_xml.len() + 96);
    rewritten.push_str(&run_xml[..opening_end + 1]);
    rewritten.push_str(&xml_escape_text(&run.text));
    rewritten.push_str(&run_xml[closing_start..]);

    if let Some(value) = run.bold {
        rewritten = upsert_empty_property(
            &rewritten,
            "r",
            "rPr",
            "b",
            if value { &[] } else { &[("w:val", "0")] },
        )?;
    }
    if let Some(value) = run.italic {
        rewritten = upsert_empty_property(
            &rewritten,
            "r",
            "rPr",
            "i",
            if value { &[] } else { &[("w:val", "0")] },
        )?;
    }
    if let Some(value) = run.underline {
        rewritten = upsert_empty_property(
            &rewritten,
            "r",
            "rPr",
            "u",
            &[("w:val", if value == 0 { "none" } else { "single" })],
        )?;
    }
    if let Some(value) = run.font_size_half_points {
        let value = value.to_string();
        rewritten = upsert_empty_property(&rewritten, "r", "rPr", "sz", &[("w:val", &value)])?;
    }
    if let Some(color) = run.color {
        let value = format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        rewritten = upsert_empty_property(&rewritten, "r", "rPr", "color", &[("w:val", &value)])?;
    }
    Ok(rewritten)
}

fn upsert_empty_property(
    xml: &str,
    owner: &str,
    properties: &str,
    child: &str,
    attributes: &[(&str, &str)],
) -> anyhow::Result<String> {
    let properties_pattern = Regex::new(&format!(
        r"(?s)<w:{properties}(?:\s[^>]*)?>.*?</w:{properties}>"
    ))?;
    if let Some(found) = properties_pattern.find(xml) {
        let block = merge_empty_element_attributes(found.as_str(), child, attributes)?;
        return Ok(format!(
            "{}{}{}",
            &xml[..found.start()],
            block,
            &xml[found.end()..]
        ));
    }
    let opening_pattern = Regex::new(&format!(r"<w:{owner}(?:\s[^>]*)?>"))?;
    let opening = opening_pattern
        .find(xml)
        .context("OOXML owner opening tag is missing")?;
    let element = render_empty_element(child, attributes);
    Ok(format!(
        "{}<w:{properties}>{element}</w:{properties}>{}",
        &xml[..opening.end()],
        &xml[opening.end()..]
    ))
}

fn merge_empty_element_attributes(
    properties_xml: &str,
    child: &str,
    attributes: &[(&str, &str)],
) -> anyhow::Result<String> {
    let child_pattern = Regex::new(&format!(r"(?s)<w:{child}\b[^>]*(?:/>|>.*?</w:{child}>)"))?;
    if let Some(found) = child_pattern.find(properties_xml) {
        let mut merged = BTreeMap::<String, String>::new();
        let attribute_pattern = Regex::new(r#"([A-Za-z_][\w:.-]*)="([^"]*)""#)?;
        for captures in attribute_pattern.captures_iter(found.as_str()) {
            merged.insert(captures[1].to_string(), captures[2].to_string());
        }
        for (name, value) in attributes {
            merged.insert((*name).to_string(), (*value).to_string());
        }
        let rendered = render_empty_element_owned(child, &merged);
        Ok(format!(
            "{}{}{}",
            &properties_xml[..found.start()],
            rendered,
            &properties_xml[found.end()..]
        ))
    } else {
        let closing = format!(
            "</w:{}>",
            properties_xml[3..].split([' ', '>']).next().unwrap_or("")
        );
        let position = properties_xml
            .rfind(&closing)
            .context("OOXML properties closing tag is missing")?;
        let element = render_empty_element(child, attributes);
        Ok(format!(
            "{}{}{}",
            &properties_xml[..position],
            element,
            &properties_xml[position..]
        ))
    }
}

fn render_empty_element(child: &str, attributes: &[(&str, &str)]) -> String {
    let mut output = format!("<w:{child}");
    for (name, value) in attributes {
        output.push(' ');
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(value);
        output.push('"');
    }
    output.push_str("/>");
    output
}

fn render_empty_element_owned(child: &str, attributes: &BTreeMap<String, String>) -> String {
    let values = attributes
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    render_empty_element(child, &values)
}

fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attribute(value: &str) -> String {
    xml_escape_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(value: &str) -> anyhow::Result<Vec<u8>> {
    ensure!(value.len() % 2 == 0, "hex value has odd length");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).context("invalid hex value"))
        .collect()
}

fn xml_escape_attr(value: &str) -> String {
    xml_escape_text(value).replace('"', "&quot;")
}

fn document_alignment_name(value: u8) -> Option<&'static str> {
    match value {
        0 => Some("right"),
        1 => Some("left"),
        2 => Some("center"),
        3 => Some("both"),
        _ => None,
    }
}

fn document_line_rule_name(value: u8) -> Option<&'static str> {
    match value {
        0 => Some("atLeast"),
        1 => Some("auto"),
        2 => Some("exact"),
        _ => None,
    }
}

fn document_section_break_type_name(value: u8) -> Option<&'static str> {
    match value {
        0 => Some("continuous"),
        1 => Some("evenPage"),
        2 => Some("nextColumn"),
        3 => Some("nextPage"),
        4 => Some("oddPage"),
        _ => None,
    }
}

fn replace_package_parts(
    package: &[u8],
    mut replacements: BTreeMap<String, Vec<u8>>,
) -> anyhow::Result<Vec<u8>> {
    for (path, bytes) in &replacements {
        validate_xml(path, bytes)?;
    }
    let mut archive = ZipArchive::new(Cursor::new(package)).context("open OOXML escrow package")?;
    let output = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(output);
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read OOXML escrow entry")?;
        let path = entry.name().replace('\\', "/");
        ensure!(
            !path.starts_with('/') && !path.split('/').any(|segment| segment == ".."),
            "unsafe OOXML package path: {path}"
        );
        let options = SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer.add_directory(&path, options)?;
        } else {
            writer.start_file(&path, options)?;
            if let Some(replacement) = replacements.remove(path.as_str()) {
                writer.write_all(&replacement)?;
            } else {
                std::io::copy(&mut entry, &mut writer)?;
            }
        }
    }
    for (path, bytes) in replacements {
        writer.start_file(&path, SimpleFileOptions::default())?;
        writer.write_all(&bytes)?;
    }
    Ok(writer.finish()?.into_inner())
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

    fn office_fixture(relative: &str) -> std::path::PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        for repository_root in [manifest_dir.to_path_buf(), manifest_dir.join("../../..")] {
            let candidate = repository_root.join("tests/fixtures/office").join(relative);
            if candidate.exists() {
                return candidate;
            }
        }
        panic!(
            "Office fixture {relative} was not found from Cargo manifest directory {}",
            manifest_dir.display()
        );
    }

    fn synthetic_xlsy(sheet_name: &str) -> Vec<u8> {
        let mut properties = vec![0, 6];
        let name = sheet_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        properties.extend_from_slice(&(name.len() as u32).to_le_bytes());
        properties.extend_from_slice(&name);
        properties.extend_from_slice(&[1, 4]);
        properties.extend_from_slice(&1u32.to_le_bytes());
        let mut worksheet = vec![1];
        worksheet.extend_from_slice(&(properties.len() as u32).to_le_bytes());
        worksheet.extend_from_slice(&properties);
        let header_bytes = b"XLSY;v10;0;".len();
        let directory_bytes = 11usize;
        let xlsb_offset = header_bytes + directory_bytes;
        let mut sheet_data = vec![35];
        sheet_data.extend_from_slice(&4u32.to_le_bytes());
        sheet_data.extend_from_slice(&(xlsb_offset as u32).to_le_bytes());
        worksheet.push(9);
        worksheet.extend_from_slice(&(sheet_data.len() as u32).to_le_bytes());
        worksheet.extend_from_slice(&sheet_data);
        let mut worksheets_content = vec![0];
        worksheets_content.extend_from_slice(&(worksheet.len() as u32).to_le_bytes());
        worksheets_content.extend_from_slice(&worksheet);
        let mut worksheets = (worksheets_content.len() as u32).to_le_bytes().to_vec();
        worksheets.extend_from_slice(&worksheets_content);
        let mut xlsb = vec![0x91, 0x01, 0]; // BEGIN_SHEET_DATA
        xlsb.extend_from_slice(&[0, 17]); // ROW_HDR
        xlsb.extend_from_slice(&0u32.to_le_bytes());
        xlsb.extend_from_slice(&0u32.to_le_bytes());
        xlsb.extend_from_slice(&[0, 0, 0, 0, 0]);
        xlsb.extend_from_slice(&0u32.to_le_bytes());
        xlsb.extend_from_slice(&[5, 18]); // CELL_REAL
        xlsb.extend_from_slice(&0u32.to_le_bytes());
        xlsb.extend_from_slice(&0u32.to_le_bytes());
        xlsb.extend_from_slice(&42f64.to_le_bytes());
        xlsb.extend_from_slice(&0u16.to_le_bytes());
        xlsb.extend_from_slice(&[0x92, 0x01, 0]); // END_SHEET_DATA
        let workbook = [0, 0, 0, 0];
        let mut payload = b"XLSY;v10;0;".to_vec();
        let workbook_offset = payload.len() + directory_bytes + xlsb.len();
        let worksheets_offset = workbook_offset + workbook.len();
        payload.extend_from_slice(&[2, 3]);
        payload.extend_from_slice(&(workbook_offset as u32).to_le_bytes());
        payload.push(4);
        payload.extend_from_slice(&(worksheets_offset as u32).to_le_bytes());
        payload.extend_from_slice(&xlsb);
        payload.extend_from_slice(&workbook);
        payload.extend_from_slice(&worksheets);
        payload
    }

    fn docx(extra: Option<(&str, &[u8])>) -> Vec<u8> {
        docx_with_text("Hello CTOX", extra)
    }

    fn docx_with_text(text: &str, extra: Option<(&str, &[u8])>) -> Vec<u8> {
        docx_with_document_xml(
            &format!(
                r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
            ),
            extra,
        )
    }

    fn docx_with_document_xml(document_xml: &str, extra: Option<(&str, &[u8])>) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
        writer.start_file("_rels/.rels", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).unwrap();
        writer.start_file("word/document.xml", options).unwrap();
        writer.write_all(document_xml.as_bytes()).unwrap();
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
    fn document_prepare_writes_docy_and_canonical_export_preserves_docx() {
        let source = docx(Some((
            "customXml/item1.xml",
            br#"<?xml version="1.0"?><root value="keep"/>"#,
        )));
        let prepared = prepare(OfficeKind::Document, &source, PrepareOptions::default()).unwrap();
        assert!(prepared.editor_payload.starts_with(b"DOCY;v10;0;"));
        assert_eq!(prepared.protocol, DOCUMENT_EDITOR_PROTOCOL);
        assert_ne!(prepared.editor_sha256, prepared.source_sha256);
        assert_eq!(prepared.manifest.primary_text, "Hello CTOX");
        assert!(prepared
            .manifest
            .parts
            .iter()
            .any(|part| part.path == "customXml/item1.xml"));
        let exported = export(OfficeKind::Document, &source, Some(&source)).unwrap();
        assert_eq!(exported.bytes, source);
        assert_eq!(exported.sha256, sha256_hex(&source));
    }

    #[test]
    fn document_prepare_preserves_nested_tables_in_docy_roundtrip() {
        let source_path = office_fixture("document/tables.docx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        let exported = export(OfficeKind::Document, &editor, Some(&source)).unwrap();
        let manifest = inspect(OfficeKind::Document, &exported.bytes).unwrap();

        assert!(manifest.primary_text.contains("TABLE_NESTED_HOST"));
        assert!(manifest.primary_text.contains("NESTED_A1"));
        assert!(manifest.primary_text.contains("NESTED_B2"));
        assert!(manifest
            .parts
            .iter()
            .any(|part| part.path == "customXml/ctox-table-preserve.xml"));
    }

    #[test]
    fn document_prepare_and_export_preserve_hyperlinks_bookmarks_and_fields() {
        let source_path = office_fixture("document/links-bookmarks-fields.docx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        let blocks = decode_document_binary_document(&editor).unwrap();
        let paragraphs = flatten_decoded_document_paragraphs(&blocks);
        assert!(paragraphs.iter().any(|paragraph| paragraph.content.iter().any(
            |item| matches!(item, DecodedDocumentInline::Hyperlink { value, .. } if value == "https://ctox.dev/preserve-link")
        )));
        assert!(paragraphs.iter().any(|paragraph| paragraph.content.iter().any(
            |item| matches!(item, DecodedDocumentInline::Bookmark { name: Some(name), start: true, .. } if name == "ctox_existing_bookmark")
        )));
        assert!(paragraphs
            .iter()
            .flat_map(|paragraph| &paragraph.runs)
            .any(|run| run.instruction_text && run.text.contains("NUMPAGES")));

        let exported = export(OfficeKind::Document, &editor, Some(&source)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(exported.bytes)).unwrap();
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document_xml)
            .unwrap();
        assert!(document_xml.contains("ctox_existing_bookmark"));
        assert!(document_xml.contains("<w:instrText"));
        assert!(document_xml.contains("NUMPAGES"));
        let mut relationships = String::new();
        archive
            .by_name("word/_rels/document.xml.rels")
            .unwrap()
            .read_to_string(&mut relationships)
            .unwrap();
        assert!(relationships.contains("https://ctox.dev/preserve-link"));
        assert!(archive.by_name("customXml/ctox-links-preserve.xml").is_ok());
    }

    #[test]
    fn document_prepare_writes_imported_comments_and_revisions_to_docy() {
        let source_path = office_fixture("document/comments-track-changes.docx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        let manifest = inspect_editor_payload(OfficeKind::Document, &editor).unwrap();
        assert!(manifest
            .tables
            .iter()
            .any(|table| table.table_type == 8 && table.name == "comments"));
        for marker in [
            "CTOX_EXISTING_COMMENT_BODY",
            "CTOX_EXISTING_INSERTION",
            "CTOX_EXISTING_DELETION",
        ] {
            let encoded = marker
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            assert!(editor
                .windows(encoded.len())
                .any(|window| window == encoded));
        }

        let exported = export(OfficeKind::Document, &editor, Some(&source)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(exported.bytes)).unwrap();
        let mut comments = String::new();
        archive
            .by_name("word/comments.xml")
            .unwrap()
            .read_to_string(&mut comments)
            .unwrap();
        assert!(comments.contains("CTOX_EXISTING_COMMENT_BODY"));
        assert!(comments.contains("w14:paraId="));
        let mut comments_extended = String::new();
        archive
            .by_name("word/commentsExtended.xml")
            .unwrap()
            .read_to_string(&mut comments_extended)
            .unwrap();
        assert!(comments_extended.contains("w15:commentEx"));
        let mut relationships = String::new();
        archive
            .by_name("word/_rels/document.xml.rels")
            .unwrap()
            .read_to_string(&mut relationships)
            .unwrap();
        assert!(relationships.contains("/relationships/commentsExtended"));
        assert!(archive
            .by_name("customXml/ctox-comments-preserve.xml")
            .is_ok());
    }

    #[test]
    fn document_comment_renderer_flattens_replies_and_resolution_state() {
        let comments = vec![DecodedDocumentComment {
            id: Some(7),
            author: Some("Root & Reviewer".to_string()),
            initials: Some("RR".to_string()),
            date: Some("2026-07-12T00:00:00Z".to_string()),
            text: "Root <body>".to_string(),
            solved: true,
            replies: vec![DecodedDocumentComment {
                author: Some("Reply".to_string()),
                text: "Reply body".to_string(),
                ..DecodedDocumentComment::default()
            }],
        }];
        let (comments_xml, extended_xml) = render_decoded_document_comments(&comments);
        let comments_xml = String::from_utf8(comments_xml).unwrap();
        let extended_xml = String::from_utf8(extended_xml).unwrap();
        assert!(comments_xml.contains("w:id=\"7\""));
        assert!(comments_xml.contains("w:id=\"8\""));
        assert!(comments_xml.contains("Root &amp; Reviewer"));
        assert!(comments_xml.contains("Root &lt;body&gt;"));
        assert!(extended_xml.contains("w15:done=\"1\""));
        assert!(extended_xml.contains("w15:paraIdParent="));
    }

    #[test]
    fn document_prepare_preserves_drawing_runs_for_escrow_export() {
        let source_path = office_fixture("document/images-positioning.docx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        let blocks = decode_document_binary_document(&editor).unwrap();
        let paragraphs = flatten_decoded_document_paragraphs(&blocks);
        let drawings = paragraphs
            .iter()
            .flat_map(|paragraph| &paragraph.runs)
            .filter_map(|run| run.drawing.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(drawings.len(), 2);
        assert_eq!(drawings[0].inline, Some(true));
        assert_eq!(drawings[0].extent_emu, Some((1_828_800, 914_400)));
        assert!(drawings[0].pptx_data_bytes.unwrap_or_default() > 0);
        assert_eq!(drawings[0].raster_ids, vec!["media/image1.png"]);
        assert_eq!(drawings[1].inline, Some(false));
        assert_eq!(drawings[1].extent_emu, Some((1_143_000, 1_143_000)));
        assert!(drawings[1].pptx_data_bytes.unwrap_or_default() > 0);
        assert_eq!(drawings[1].raster_ids, vec!["media/image2.png"]);

        let exported = export(OfficeKind::Document, &editor, Some(&source)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(exported.bytes)).unwrap();
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document_xml)
            .unwrap();
        assert_eq!(document_xml.matches("<w:drawing>").count(), 2);
        assert!(document_xml.contains("CTOX_INLINE_IMAGE_TARGET"));
        assert!(document_xml.contains("CTOX_FLOATING_IMAGE_TARGET"));
    }

    #[test]
    fn document_drawings_chart_payload_decodes_and_rewrites_native_properties() {
        let source_path = office_fixture("document/drawings-charts.docx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        let blocks = decode_document_binary_document(&editor).unwrap();
        let drawings = flatten_decoded_document_paragraphs(&blocks)
            .iter()
            .flat_map(|paragraph| &paragraph.runs)
            .filter_map(|run| run.drawing.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(drawings.len(), 2);
        assert_eq!(drawings[0].shape_fill, Some([0xD9, 0xEA, 0xD3]));
        assert_eq!(drawings[0].shape_rotation, Some(0));
        assert!(drawings[0].pptx_data_bytes.unwrap_or_default() > 0);
        assert!(drawings[1].chart2_data_bytes.unwrap_or_default() > 0);

        let drawing = DecodedDocumentDrawing {
            extent_emu: Some((5_040_000, 2_520_000)),
            shape_fill: Some([0xF4, 0xB1, 0x83]),
            shape_rotation: Some(5_400_000),
            ..DecodedDocumentDrawing::default()
        };
        let rewritten = rewrite_document_drawing_run_xml(
            r#"<w:r><w:drawing><wp:inline><wp:extent cx="2286000" cy="685800"/><a:graphic><wps:wsp><wps:spPr><a:xfrm rot="0"/><a:solidFill><a:srgbClr val="D9EAD3"/></a:solidFill><a:ln><a:solidFill><a:srgbClr val="176B5B"/></a:solidFill></a:ln></wps:spPr></wps:wsp></a:graphic></wp:inline></w:drawing></w:r>"#,
            &drawing,
        )
        .unwrap();
        assert!(rewritten.contains(r#"wp:extent cx="5040000" cy="2520000""#));
        assert!(rewritten.contains(r#"a:xfrm rot="5400000""#));
        assert!(rewritten.contains(r#"a:srgbClr val="F4B183""#));
        assert!(rewritten.contains(r#"a:srgbClr val="176B5B""#));

        let mut style_value = Vec::new();
        write_item(&mut style_value, 0, &[2]);
        let mut fallback = Vec::new();
        write_item(&mut fallback, 0, &style_value);
        let mut alternate = Vec::new();
        write_item(&mut alternate, 1, &fallback);
        let mut chart_space = Vec::new();
        write_item(&mut chart_space, 3, &alternate);
        assert_eq!(decode_document_chart_style(&chart_space).unwrap(), Some(2));
        let chart = replace_document_chart_style(
            br#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart/></c:chartSpace>"#,
            2,
        )
        .unwrap();
        assert!(String::from_utf8(chart)
            .unwrap()
            .contains(r#"<c:style val="2"/>"#));
    }

    #[test]
    fn document_docy_table_structure_export_rebuilds_body_and_preserves_escrow() {
        let original = docx_with_text(
            "ORIGINAL_TABLE_PLACEHOLDER",
            Some((
                "customXml/preserve-tables.xml",
                br#"<?xml version="1.0"?><keep>tables</keep>"#,
            )),
        );
        let changed = docx_with_document_xml(
            r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>STRUCTURAL_TABLE_A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>STRUCTURAL_TABLE_B1</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>STRUCTURAL_TABLE_A2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>STRUCTURAL_TABLE_B2</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            None,
        );
        let editor = transcode_document_to_editor_payload(&changed).unwrap();
        let exported = export(OfficeKind::Document, &editor, Some(&original)).unwrap();
        assert!(exported
            .manifest
            .primary_text
            .contains("STRUCTURAL_TABLE_B2"));
        assert!(exported
            .manifest
            .parts
            .iter()
            .any(|part| part.path == "customXml/preserve-tables.xml"));
        let mut archive = ZipArchive::new(Cursor::new(exported.bytes)).unwrap();
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document_xml)
            .unwrap();
        assert!(document_xml.contains("<w:tbl>"));
        assert!(document_xml.contains("STRUCTURAL_TABLE_A1"));
    }

    #[test]
    fn document_docy_change_exports_primary_text_and_preserves_escrow() {
        let original = docx_with_text(
            "CTOX_EDIT_TARGET_ALPHA",
            Some(("customXml/preserve.xml", br#"<?xml version="1.0"?><keep/>"#)),
        );
        let changed_source = docx_with_text("CTOX_EDIT_RESULT_BRAVO_42", None);
        let base_editor = transcode_document_to_editor_payload(&original).unwrap();
        let changed_editor = transcode_document_to_editor_payload(&changed_source).unwrap();
        let prepared = apply_changes(
            OfficeKind::Document,
            &base_editor,
            &changed_editor,
            ApplyChangesOptions {
                expected_base_sha256: sha256_hex(&base_editor),
                implemented_features: vec!["document.edit-save".to_string()],
            },
        )
        .unwrap();
        assert_eq!(prepared.manifest.primary_text, "CTOX_EDIT_RESULT_BRAVO_42");
        let exported = export(
            OfficeKind::Document,
            &prepared.editor_payload,
            Some(&original),
        )
        .unwrap();
        assert_eq!(exported.manifest.primary_text, "CTOX_EDIT_RESULT_BRAVO_42");
        let mut archive = ZipArchive::new(Cursor::new(exported.bytes)).unwrap();
        let mut preserved = String::new();
        archive
            .by_name("customXml/preserve.xml")
            .unwrap()
            .read_to_string(&mut preserved)
            .unwrap();
        assert_eq!(preserved, r#"<?xml version="1.0"?><keep/>"#);
    }

    #[test]
    fn document_docy_formatting_decodes_and_rewrites_ooxml_properties() {
        let mut run_properties = Vec::new();
        write_fixed_property(&mut run_properties, 0, 1, &[1]);
        write_fixed_property(&mut run_properties, 1, 1, &[1]);
        write_fixed_property(&mut run_properties, 2, 1, &[1]);
        write_fixed_property(&mut run_properties, 8, 4, &36u32.to_le_bytes());
        write_fixed_property(&mut run_properties, 9, 3, &[0x95, 0x37, 0x35]);
        let mut run_content = vec![0];
        write_utf16_string(&mut run_content, "FORMAT_TARGET");
        let mut run_record = Vec::new();
        write_item(&mut run_record, 1, &run_properties);
        write_item(&mut run_record, 8, &run_content);
        let decoded_run = decode_document_binary_run_formatting(&run_record).unwrap();
        assert_eq!(decoded_run.text, "FORMAT_TARGET");
        assert_eq!(decoded_run.bold, Some(true));
        assert_eq!(decoded_run.italic, Some(true));
        assert_eq!(decoded_run.underline, Some(1));
        assert_eq!(decoded_run.font_size_half_points, Some(36));
        assert_eq!(decoded_run.color, Some([0x95, 0x37, 0x35]));

        let paragraph = DecodedDocumentParagraph {
            text: decoded_run.text.clone(),
            alignment: Some(2),
            left_indent_twips: Some(709),
            line_spacing_twips: Some(360),
            line_spacing_rule: Some(1),
            runs: vec![decoded_run],
            ..DecodedDocumentParagraph::default()
        };
        let original = r#"<w:p><w:pPr><w:spacing w:before="240"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial"/></w:rPr><w:t>OLD</w:t></w:r></w:p>"#;
        let rewritten = rewrite_document_paragraph_xml(
            original,
            &paragraph,
            &BTreeMap::new(),
            &DecodedDocumentHeaderFooterRelationshipIds::default(),
        )
        .unwrap();
        assert!(rewritten.contains(r#"<w:jc w:val="center"/>"#));
        assert!(rewritten.contains(r#"<w:ind w:left="709"/>"#));
        assert!(rewritten.contains(r#"w:before="240""#));
        assert!(rewritten.contains(r#"w:line="360""#));
        assert!(rewritten.contains(r#"w:lineRule="auto""#));
        assert!(rewritten.contains("<w:b/>"));
        assert!(rewritten.contains("<w:i/>"));
        assert!(rewritten.contains(r#"<w:u w:val="single"/>"#));
        assert!(rewritten.contains(r#"<w:sz w:val="36"/>"#));
        assert!(rewritten.contains(r#"<w:color w:val="953735"/>"#));
        assert!(rewritten.contains("<w:t>FORMAT_TARGET</w:t>"));
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
    fn inspects_xlsy_v10_table_directory_without_document_server() {
        let payload = synthetic_xlsy("Overview");
        let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, &payload).unwrap();
        assert_eq!(manifest.protocol, SPREADSHEET_EDITOR_PROTOCOL);
        assert_eq!(manifest.protocol_version, 10);
        assert_eq!(manifest.table_directory_bytes, 11);
        assert_eq!(manifest.tables.len(), 2);
        assert_eq!(manifest.tables[0].name, "workbook");
        assert!(manifest.tables[0].offset > 22);
        assert_eq!(manifest.tables[0].bytes, 4);
        assert_eq!(manifest.tables[1].name, "worksheets");
        assert_eq!(manifest.worksheets[0].name, "Overview");
        assert_eq!(manifest.worksheets[0].visibility, "visible");
        assert_eq!(manifest.worksheets[0].cells[0].reference, "A1");
        assert_eq!(manifest.worksheets[0].cells[0].display, "42");
    }

    #[test]
    fn inspects_docy_v10_table_directory_without_document_server() {
        let mut payload = b"DOCY;v10;0;".to_vec();
        payload.push(2);
        payload.push(0);
        payload.extend_from_slice(&22u32.to_le_bytes());
        payload.push(6);
        payload.extend_from_slice(&24u32.to_le_bytes());
        payload.extend_from_slice(&[1, 2, 3, 4, 5]);

        let manifest = inspect_editor_payload(OfficeKind::Document, &payload).unwrap();
        assert_eq!(manifest.protocol, DOCUMENT_EDITOR_PROTOCOL);
        assert_eq!(manifest.protocol_version, 10);
        assert_eq!(manifest.table_directory_bytes, 11);
        assert_eq!(manifest.tables.len(), 2);
        assert_eq!(manifest.tables[0].name, "signature");
        assert_eq!(manifest.tables[0].offset, 22);
        assert_eq!(manifest.tables[0].bytes, 2);
        assert_eq!(manifest.tables[1].name, "document");
        assert_eq!(manifest.tables[1].bytes, 3);
    }

    #[test]
    fn prepared_spreadsheet_keeps_ooxml_and_xlsy_as_distinct_payloads() {
        let source_path = office_fixture("spreadsheet/open-render-sheets.xlsx");
        let source = fs::read(source_path).unwrap();
        let editor = synthetic_xlsy("Overview");
        let prepared = prepare_with_editor_payload(
            OfficeKind::Spreadsheet,
            &source,
            &editor,
            PrepareOptions {
                implemented_features: vec!["spreadsheet.open-render-sheets".to_string()],
            },
        )
        .unwrap();
        assert_eq!(prepared.protocol, SPREADSHEET_EDITOR_PROTOCOL);
        assert_eq!(prepared.source_sha256, sha256_hex(&source));
        assert_eq!(prepared.editor_sha256, sha256_hex(&editor));
        assert_ne!(prepared.source_sha256, prepared.editor_sha256);
        assert_eq!(prepared.editor_payload, editor);
        assert_eq!(prepared.editor_manifest.unwrap().tables.len(), 2);
    }

    #[test]
    fn native_writer_transcodes_open_render_xlsx_to_xlsy_cells() {
        let source_path = office_fixture("spreadsheet/open-render-sheets.xlsx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        assert!(editor.starts_with(b"XLSY;v10;0;"));
        let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, &editor).unwrap();
        assert_eq!(manifest.shared_strings.len(), 18);
        assert_eq!(manifest.worksheets.len(), 3);
        assert_eq!(manifest.worksheets[0].name, "Overview");
        assert_eq!(manifest.worksheets[0].cells.len(), 8);
        assert_eq!(manifest.worksheets[0].cells[5].reference, "B4");
        assert_eq!(manifest.worksheets[0].cells[5].display, "125000");
        assert_eq!(manifest.worksheets[1].cells[5].display, "42");
        assert_eq!(manifest.worksheets[2].visibility, "hidden");
        assert_eq!(manifest.worksheets[2].cells[5].display, "Closed");
    }

    #[test]
    fn spreadsheet_xlsy_cell_edit_exports_only_shared_strings_and_preserves_escrow() {
        let source_path = office_fixture("spreadsheet/edit-save.xlsx");
        let source = fs::read(source_path).unwrap();
        let original_editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let mut changed_editor = original_editor.clone();
        let before = "CTOX_EDIT_CELL_ALPHA"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let after = "CTOX_EDIT_CELL_BRAVO"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(before.len(), after.len());
        let offset = changed_editor
            .windows(before.len())
            .position(|window| window == before)
            .expect("fixture shared string in XLSY");
        changed_editor[offset..offset + after.len()].copy_from_slice(&after);
        let changed_manifest =
            inspect_editor_payload(OfficeKind::Spreadsheet, &changed_editor).unwrap();
        assert_eq!(
            changed_manifest.worksheets[0].cells[1].display,
            "CTOX_EDIT_CELL_BRAVO"
        );

        let applied = apply_changes(
            OfficeKind::Spreadsheet,
            &original_editor,
            &changed_editor,
            ApplyChangesOptions {
                expected_base_sha256: sha256_hex(&original_editor),
                implemented_features: vec!["spreadsheet.edit-save".to_string()],
            },
        )
        .unwrap();
        assert_eq!(applied.editor_payload, changed_editor);
        let package = export(
            OfficeKind::Spreadsheet,
            &applied.editor_payload,
            Some(&source),
        )
        .unwrap();
        let mut original = ZipArchive::new(Cursor::new(&source)).unwrap();
        let mut exported = ZipArchive::new(Cursor::new(&package.bytes)).unwrap();
        assert_eq!(original.len(), exported.len());
        for index in 0..original.len() {
            let mut original_entry = original.by_index(index).unwrap();
            if original_entry.is_dir() {
                continue;
            }
            let path = original_entry.name().to_string();
            let mut before_bytes = Vec::new();
            original_entry.read_to_end(&mut before_bytes).unwrap();
            let mut after_bytes = Vec::new();
            exported
                .by_name(&path)
                .unwrap()
                .read_to_end(&mut after_bytes)
                .unwrap();
            if path == "xl/sharedStrings.xml" {
                assert_ne!(before_bytes, after_bytes);
                assert!(String::from_utf8(after_bytes)
                    .unwrap()
                    .contains("CTOX_EDIT_CELL_BRAVO"));
            } else {
                assert_eq!(before_bytes, after_bytes, "escrow part changed: {path}");
            }
        }
    }

    #[test]
    fn spreadsheet_formatting_materializes_styles_and_row_column_layout() {
        let source_path = office_fixture("spreadsheet/cell-format-rows-columns.xlsx");
        let source = fs::read(source_path).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let styles_xml = read_zip_part(&mut archive, "xl/styles.xml").unwrap();
        let worksheet_xml = read_zip_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        let escrow_xml =
            read_zip_part(&mut archive, "customXml/ctox-spreadsheet-preserve.xml").unwrap();
        let mut changed_styles = parse_ooxml_styles(&styles_xml).unwrap();
        changed_styles.fonts.push(SpreadsheetSourceFont {
            bold: true,
            italic: true,
            color: None,
            size: Some(11.0),
            name: Some("Arial".to_string()),
        });
        changed_styles.number_formats.insert(
            164,
            r#"_-* #,##0.00\ [$€-407]_-;\-* #,##0.00\ [$€-407]_-;_-* "-"??\ [$€-407]_-;_-@_-"#
                .to_string(),
        );
        changed_styles.cell_xfs.push(SpreadsheetSourceXf {
            font_id: (changed_styles.fonts.len() - 1) as u32,
            fill_id: 0,
            border_id: 0,
            num_fmt_id: 164,
            xf_id: Some(0),
            apply_font: true,
            apply_fill: false,
            apply_alignment: false,
            horizontal_alignment: None,
        });
        let style_id = (changed_styles.cell_xfs.len() - 1) as u32;
        let (materialized, style_map) = materialize_spreadsheet_styles(
            &styles_xml,
            &changed_styles,
            &BTreeSet::from([style_id]),
        )
        .unwrap();
        let materialized = String::from_utf8(materialized).unwrap();
        assert!(materialized.contains("<b/><i/>"));
        assert!(materialized.contains("numFmtId=\"164\""));
        assert!(materialized.contains("[$€-407]"));
        assert!(style_map.contains_key(&style_id));

        let before = EditorWorksheetManifest {
            name: "Overview".to_string(),
            sheet_id: 1,
            visibility: "visible".to_string(),
            xlsb_offset: 0,
            default_row_height: Some(15.0),
            columns: vec![EditorColumnManifest {
                min: 2,
                max: 2,
                width: 12.0,
                custom_width: true,
            }],
            rows: vec![EditorRowManifest {
                index: 3,
                height: Some(15.0),
                custom_height: false,
                hidden: false,
            }],
            merged_cells: Vec::new(),
            frozen_pane: None,
            tables: Vec::new(),
            data_validations: Vec::new(),
            conditional_formats: Vec::new(),
            protection: None,
            comments: Vec::new(),
            cells: Vec::new(),
        };
        let mut after = before.clone();
        after.columns[0].width = 33.25390625;
        after.rows[0].height = Some(27.75);
        after.rows[0].custom_height = true;
        let worksheet = String::from_utf8(worksheet_xml).unwrap();
        let worksheet = replace_spreadsheet_row_layout(worksheet, &before, &after).unwrap();
        let worksheet = replace_spreadsheet_column_layout(worksheet, &before, &after).unwrap();
        assert!(worksheet.contains("r=\"4\" ht=\"27.75\" customHeight=\"1\""));
        assert!(worksheet.contains("width=\"33.25390625\" customWidth=\"1\""));
        assert!(String::from_utf8(escrow_xml)
            .unwrap()
            .contains("SPREADSHEET_FORMAT_ESCROW_4C72"));
    }

    #[test]
    fn spreadsheet_formulas_roundtrip_text_cache_references_and_errors() {
        let source_path = office_fixture("spreadsheet/formulas-references.xlsx");
        let source = fs::read(source_path).unwrap();
        let editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let original = inspect_editor_payload(OfficeKind::Spreadsheet, &editor).unwrap();
        let overview = &original.worksheets[0];
        let cell = |reference: &str| {
            overview
                .cells
                .iter()
                .find(|cell| cell.reference == reference)
                .unwrap()
        };
        assert_eq!(cell("B3").formula.as_deref(), Some("=B2*2"));
        assert_eq!(cell("B4").formula.as_deref(), Some("=$B$2+5"));
        assert_eq!(cell("B5").formula.as_deref(), Some("=SUM(B2:B4)"));
        assert_eq!(cell("B6").formula.as_deref(), Some("='Details'!B4+1"));
        assert_eq!(cell("B8").formula.as_deref(), Some("=1/0"));
        assert_eq!(cell("B8").display, "#DIV/0!");

        let mut changed = original.clone();
        let changed_overview = &mut changed.worksheets[0];
        let d3 = changed_overview
            .cells
            .iter_mut()
            .find(|cell| cell.reference == "D3")
            .unwrap();
        d3.formula = Some("=D2*3".to_string());
        d3.display = "15".to_string();
        let c7 = changed_overview
            .cells
            .iter_mut()
            .find(|cell| cell.reference == "C7")
            .unwrap();
        c7.formula = Some("=C2+1".to_string());
        c7.display = "21".to_string();

        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let worksheet = read_zip_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        let styles = decode_spreadsheet_editor_styles(&editor, &original).unwrap();
        let updated = replace_changed_worksheet_cells(
            &worksheet,
            &original.worksheets[0],
            &changed.worksheets[0],
            &original.shared_strings,
            &original.shared_strings,
            &styles,
            &styles,
            &BTreeMap::new(),
        )
        .unwrap();
        let updated_text = String::from_utf8(updated.clone()).unwrap();
        assert!(updated_text.contains("<c r=\"D3\"><f>D2*3</f><v>15</v></c>"));
        assert!(updated_text.contains("<c r=\"C7\"><f>C2+1</f><v>21</v></c>"));
        assert!(updated_text.contains("<c r=\"B8\" t=\"e\"><f>1/0</f><v>#DIV/0!</v></c>"));

        drop(archive);
        let package = replace_package_parts(
            &source,
            BTreeMap::from([("xl/worksheets/sheet1.xml".to_string(), updated)]),
        )
        .unwrap();
        let reopened = inspect_editor_payload(
            OfficeKind::Spreadsheet,
            &transcode_spreadsheet_to_editor_payload(&package).unwrap(),
        )
        .unwrap();
        let reopened_overview = &reopened.worksheets[0];
        let reopened_cell = |reference: &str| {
            reopened_overview
                .cells
                .iter()
                .find(|cell| cell.reference == reference)
                .unwrap()
        };
        assert_eq!(reopened_cell("D3").formula.as_deref(), Some("=D2*3"));
        assert_eq!(reopened_cell("D3").display, "15");
        assert_eq!(reopened_cell("C7").formula.as_deref(), Some("=C2+1"));
        assert_eq!(reopened_cell("C7").display, "21");
        assert_eq!(reopened_cell("B8").display, "#DIV/0!");
    }

    #[test]
    fn spreadsheet_multi_sheet_merge_freeze_roundtrips_and_preserves_hidden_sheet() {
        let source_path = office_fixture("spreadsheet/multi-sheet-merge-freeze.xlsx");
        let source = fs::read(source_path).unwrap();
        let initial_editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let initial = inspect_editor_payload(OfficeKind::Spreadsheet, &initial_editor).unwrap();
        assert_eq!(initial.worksheets.len(), 3);
        assert_eq!(initial.worksheets[2].name, "Archive");
        assert_eq!(initial.worksheets[2].visibility, "hidden");
        assert_eq!(initial.worksheets[0].merged_cells, ["B2:C2"]);
        assert_eq!(
            initial.worksheets[0]
                .frozen_pane
                .as_ref()
                .unwrap()
                .top_left_cell,
            "B2"
        );

        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let worksheet = read_zip_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        drop(archive);
        let mut worksheet = String::from_utf8(worksheet).unwrap();
        worksheet = worksheet.replace(
            r#"<pane xSplit="1" ySplit="1" topLeftCell="B2" activePane="bottomRight" state="frozen"/>"#,
            r#"<pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/>"#,
        );
        worksheet = worksheet.replace(
            r#"<mergeCells count="1"><mergeCell ref="B2:C2"/></mergeCells>"#,
            r#"<mergeCells count="1"><mergeCell ref="B3:C3"/></mergeCells>"#,
        );
        worksheet = worksheet.replace(
            r#"<c r="B2" t="s"><v>2</v></c></row>"#,
            r#"<c r="B2" t="s"><v>2</v></c><c r="C2"/></row>"#,
        );
        let changed_package = replace_package_parts(
            &source,
            BTreeMap::from([(
                "xl/worksheets/sheet1.xml".to_string(),
                worksheet.into_bytes(),
            )]),
        )
        .unwrap();
        let changed_editor = transcode_spreadsheet_to_editor_payload(&changed_package).unwrap();
        let changed = inspect_editor_payload(OfficeKind::Spreadsheet, &changed_editor).unwrap();
        assert_eq!(changed.worksheets[0].merged_cells, ["B3:C3"]);
        assert_eq!(
            changed.worksheets[0].frozen_pane,
            Some(EditorFrozenPaneManifest {
                active_pane: "bottomRight".to_string(),
                state: "frozen".to_string(),
                top_left_cell: "B3".to_string(),
                x_split: 1.0,
                y_split: 2.0,
            })
        );
        assert_eq!(changed.worksheets[2].visibility, "hidden");

        let exported = export(OfficeKind::Spreadsheet, &changed_editor, Some(&source)).unwrap();
        let mut exported_archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let exported_sheet = String::from_utf8(
            read_zip_part(&mut exported_archive, "xl/worksheets/sheet1.xml").unwrap(),
        )
        .unwrap();
        assert!(exported_sheet.contains(r#"<mergeCell ref="B3:C3"/>"#));
        assert!(!exported_sheet.contains(r#"<mergeCell ref="B2:C2"/>"#));
        assert!(exported_sheet.contains(
            r#"<pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/>"#
        ));
        assert!(exported_sheet.contains(r#"<c r="C2"/>"#));
        let custom = read_zip_part(
            &mut exported_archive,
            "customXml/ctox-spreadsheet-preserve.xml",
        )
        .unwrap();
        assert!(String::from_utf8(custom)
            .unwrap()
            .contains("SPREADSHEET_MERGE_FREEZE_ESCROW_D52B"));
    }

    #[test]
    fn spreadsheet_sort_filter_table_roundtrips_native_xlsy_and_preserves_escrow() {
        let source_path = office_fixture("spreadsheet/sort-filter-tables.xlsx");
        let source = fs::read(source_path).unwrap();
        let initial_editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let initial = inspect_editor_payload(OfficeKind::Spreadsheet, &initial_editor).unwrap();
        let initial_table = &initial.worksheets[0].tables[0];
        assert_eq!(initial_table.display_name, "RevenueTable");
        assert_eq!(initial_table.reference, "A1:C6");
        assert_eq!(initial_table.style_name, "TableStyleMedium4");
        assert!(initial_table.filters.is_empty());
        assert!(initial_table.sort.is_none());

        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let sheet =
            String::from_utf8(read_zip_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let table = String::from_utf8(read_zip_part(&mut archive, "xl/tables/table1.xml").unwrap())
            .unwrap();
        drop(archive);
        let rows = r#"<row r="2"><c r="A2" t="s"><v>5</v></c><c r="B2" t="s"><v>6</v></c><c r="C2"><v>420</v></c></row><row r="3"><c r="A3" t="s"><v>5</v></c><c r="B3" t="s"><v>4</v></c><c r="C3"><v>310</v></c></row><row r="4" hidden="1"><c r="A4" t="s"><v>7</v></c><c r="B4" t="s"><v>8</v></c><c r="C4"><v>240</v></c></row><row r="5" hidden="1"><c r="A5" t="s"><v>9</v></c><c r="B5" t="s"><v>10</v></c><c r="C5"><v>180</v></c></row><row r="6" hidden="1"><c r="A6" t="s"><v>3</v></c><c r="B6" t="s"><v>4</v></c><c r="C6"><v>120</v></c></row>"#;
        let row_regex = Regex::new(r#"(?s)<row r="2">.*?</row><row r="3">.*?</row><row r="4">.*?</row><row r="5">.*?</row><row r="6">.*?</row>"#).unwrap();
        let changed_sheet = row_regex.replace(&sheet, rows).into_owned();
        let changed_table = table.replace(
            r#"<autoFilter ref="A1:C6"/>"#,
            r#"<autoFilter ref="A1:C6"><filterColumn colId="0"><filters><filter val="North"/></filters></filterColumn></autoFilter><sortState ref="A2:C6"><sortCondition descending="1" ref="C1:C6"/></sortState>"#,
        );
        let changed_package = replace_package_parts(
            &source,
            BTreeMap::from([
                (
                    "xl/worksheets/sheet1.xml".to_string(),
                    changed_sheet.into_bytes(),
                ),
                (
                    "xl/tables/table1.xml".to_string(),
                    changed_table.into_bytes(),
                ),
            ]),
        )
        .unwrap();
        let changed_editor = transcode_spreadsheet_to_editor_payload(&changed_package).unwrap();
        let changed = inspect_editor_payload(OfficeKind::Spreadsheet, &changed_editor).unwrap();
        let changed_sheet = &changed.worksheets[0];
        assert_eq!(
            changed_sheet
                .cells
                .iter()
                .find(|cell| cell.reference == "C2")
                .unwrap()
                .display,
            "420"
        );
        assert!(changed_sheet.rows[3..].iter().all(|row| row.hidden));
        assert_eq!(changed_sheet.tables[0].filters[0].values, ["North"]);
        assert_eq!(
            changed_sheet.tables[0]
                .sort
                .as_ref()
                .unwrap()
                .condition_reference,
            "C1:C6"
        );
        assert!(changed_sheet.tables[0].sort.as_ref().unwrap().descending);

        let exported = export(OfficeKind::Spreadsheet, &changed_editor, Some(&source)).unwrap();
        let reopened_editor = transcode_spreadsheet_to_editor_payload(&exported.bytes).unwrap();
        let reopened = inspect_editor_payload(OfficeKind::Spreadsheet, &reopened_editor).unwrap();
        let reopened_sheet = &reopened.worksheets[0];
        assert_eq!(
            reopened_sheet
                .cells
                .iter()
                .find(|cell| cell.reference == "C2")
                .unwrap()
                .display,
            "420"
        );
        assert!(reopened_sheet.rows[3..].iter().all(|row| row.hidden));
        assert_eq!(reopened_sheet.tables[0].filters[0].values, ["North"]);
        assert!(reopened_sheet.tables[0].sort.as_ref().unwrap().descending);
        let mut exported_archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let escrow = read_zip_part(
            &mut exported_archive,
            "customXml/ctox-spreadsheet-preserve.xml",
        )
        .unwrap();
        assert!(String::from_utf8(escrow)
            .unwrap()
            .contains("SPREADSHEET_TABLE_ESCROW_8B47"));
    }

    #[test]
    fn spreadsheet_validation_conditional_roundtrips_native_xlsy_and_preserves_escrow() {
        let source_path = office_fixture("spreadsheet/validation-conditional-formatting.xlsx");
        let source = fs::read(source_path).unwrap();
        let initial_editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let initial = inspect_editor_payload(OfficeKind::Spreadsheet, &initial_editor).unwrap();
        let overview = &initial.worksheets[0];
        assert_eq!(overview.data_validations.len(), 2);
        assert_eq!(overview.data_validations[0].validation_type, "list");
        assert_eq!(
            overview.data_validations[1].operator.as_deref(),
            Some("between")
        );
        assert_eq!(overview.conditional_formats.len(), 2);
        assert_eq!(overview.conditional_formats[0].rule_type, "colorScale");
        assert_eq!(overview.conditional_formats[0].colors.len(), 3);
        assert_eq!(overview.conditional_formats[1].rule_type, "cellIs");
        assert_eq!(
            overview.conditional_formats[1]
                .differential_style
                .as_ref()
                .unwrap()
                .fill_rgb
                .as_deref(),
            Some("FFC6EFCE")
        );

        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let shared =
            String::from_utf8(read_zip_part(&mut archive, "xl/sharedStrings.xml").unwrap())
                .unwrap();
        let sheet =
            String::from_utf8(read_zip_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        drop(archive);
        let shared = shared.replace("</sst>", "<si><t>Approved</t></si></sst>");
        let sheet = sheet
            .replace(
                r#"<c r="B2" t="s"><v>5</v></c>"#,
                r#"<c r="B2" t="s"><v>19</v></c>"#,
            )
            .replace(r#"<c r="C2"><v>5</v></c>"#, r#"<c r="C2"><v>8</v></c>"#)
            .replace(r#"<c r="E2"><v>20</v></c>"#, r#"<c r="E2"><v>80</v></c>"#)
            .replace("\"Draft,Review,Final\"", "\"Draft,Review,Final,Approved\"");
        let changed_package = replace_package_parts(
            &source,
            BTreeMap::from([
                ("xl/sharedStrings.xml".to_string(), shared.into_bytes()),
                ("xl/worksheets/sheet1.xml".to_string(), sheet.into_bytes()),
            ]),
        )
        .unwrap();
        let changed_editor = transcode_spreadsheet_to_editor_payload(&changed_package).unwrap();
        let changed = inspect_editor_payload(OfficeKind::Spreadsheet, &changed_editor).unwrap();
        let changed_overview = &changed.worksheets[0];
        assert_eq!(
            changed_overview
                .cells
                .iter()
                .find(|cell| cell.reference == "B2")
                .unwrap()
                .display,
            "Approved"
        );
        assert!(changed_overview.data_validations[0]
            .formula1
            .as_deref()
            .unwrap()
            .contains("Approved"));
        assert_eq!(changed_overview.conditional_formats.len(), 2);

        let exported = export(OfficeKind::Spreadsheet, &changed_editor, Some(&source)).unwrap();
        let reopened = inspect_editor_payload(
            OfficeKind::Spreadsheet,
            &transcode_spreadsheet_to_editor_payload(&exported.bytes).unwrap(),
        )
        .unwrap();
        let reopened_overview = &reopened.worksheets[0];
        assert_eq!(reopened_overview.conditional_formats.len(), 2);
        assert_eq!(
            reopened_overview.conditional_formats[1]
                .differential_style
                .as_ref()
                .unwrap()
                .font_rgb
                .as_deref(),
            Some("FF006100")
        );
        assert!(reopened_overview.data_validations[0]
            .formula1
            .as_deref()
            .unwrap()
            .contains("Approved"));
        let mut exported_archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
        let escrow = read_zip_part(
            &mut exported_archive,
            "customXml/ctox-spreadsheet-preserve.xml",
        )
        .unwrap();
        assert!(String::from_utf8(escrow)
            .unwrap()
            .contains("SPREADSHEET_VALIDATION_ESCROW_C19E"));
    }

    #[test]
    fn spreadsheet_comments_names_protection_roundtrip_native_xlsy_and_preserves_escrow() {
        let source_path = office_fixture("spreadsheet/comments-names-protection.xlsx");
        let source = fs::read(source_path).unwrap();
        let mut editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let manifest = inspect_editor_payload(OfficeKind::Spreadsheet, &editor).unwrap();
        assert_eq!(manifest.defined_names.len(), 2);
        assert_eq!(manifest.defined_names[0].name, "CTOX_Amount");
        assert_eq!(manifest.defined_names[1].local_sheet_id, Some(0));
        assert!(
            manifest
                .workbook_protection
                .as_ref()
                .unwrap()
                .lock_structure
        );
        let overview = &manifest.worksheets[0];
        assert!(overview.protection.as_ref().unwrap().sheet);
        assert_eq!(overview.comments.len(), 1);
        assert_eq!(overview.comments[0].reference, "B4");
        assert_eq!(overview.comments[0].text, "CTOX_EXISTING_CELL_COMMENT");

        let before = "CTOX_EXISTING_CELL_COMMENT"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let after = "CTOX_MODIFIED_CELL_COMMENT"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(before.len(), after.len());
        let position = editor
            .windows(before.len())
            .position(|window| window == before)
            .expect("comment text is present in XLSY");
        editor[position..position + before.len()].copy_from_slice(&after);

        let exported = export_spreadsheet_binary(&editor, &source).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&exported)).unwrap();
        let mut comments = String::new();
        archive
            .by_name("xl/comments1.xml")
            .unwrap()
            .read_to_string(&mut comments)
            .unwrap();
        assert!(comments.contains("CTOX_MODIFIED_CELL_COMMENT"));
        let mut vml = String::new();
        archive
            .by_name("xl/drawings/vmlDrawing1.vml")
            .unwrap()
            .read_to_string(&mut vml)
            .unwrap();
        assert_eq!(vml.matches("<v:shape ").count(), 1);
        assert!(vml.contains("<x:Row>3</x:Row><x:Column>1</x:Column>"));
        let mut escrow = String::new();
        archive
            .by_name("customXml/ctox-spreadsheet-preserve.xml")
            .unwrap()
            .read_to_string(&mut escrow)
            .unwrap();
        assert!(escrow.contains("SPREADSHEET_COMMENTS_NAMES_PROTECTION_ESCROW_6E21"));
        drop(archive);

        let reopened = transcode_spreadsheet_to_editor_payload(&exported).unwrap();
        let reopened = inspect_editor_payload(OfficeKind::Spreadsheet, &reopened).unwrap();
        assert_eq!(reopened.defined_names, manifest.defined_names);
        assert_eq!(reopened.workbook_protection, manifest.workbook_protection);
        assert_eq!(
            reopened.worksheets[0].comments[0].text,
            "CTOX_MODIFIED_CELL_COMMENT"
        );
        assert_eq!(
            reopened.worksheets[0].protection,
            manifest.worksheets[0].protection
        );
        let vml = String::from_utf8(
            write_ooxml_spreadsheet_comment_vml(&[
                EditorSpreadsheetCommentManifest {
                    reference: "B4".to_string(),
                    text: "Existing".to_string(),
                    author: "CTOX".to_string(),
                    guid: String::new(),
                },
                EditorSpreadsheetCommentManifest {
                    reference: "C4".to_string(),
                    text: "Added".to_string(),
                    author: "CTOX".to_string(),
                    guid: String::new(),
                },
            ])
            .unwrap(),
        )
        .unwrap();
        assert_eq!(vml.matches("<v:shape ").count(), 2);
        assert!(vml.contains("<x:Row>3</x:Row><x:Column>2</x:Column>"));
    }

    #[test]
    fn spreadsheet_charts_prepare_emits_native_chart_records_and_preserves_package() {
        let source_path = office_fixture("spreadsheet/charts.xlsx");
        let source = fs::read(source_path).unwrap();
        let (_, _, _, sheets) = read_spreadsheet_source(&source).unwrap();
        let drawing = &sheets[0].drawings[0];
        assert_eq!(drawing.name, "CTOX Revenue Chart");
        assert_eq!((drawing.from_col, drawing.from_row), (3, 1));
        assert_eq!((drawing.to_col, drawing.to_row), (10, 18));
        assert_eq!(drawing.chart.title, "CTOX Revenue 2026");
        assert_eq!(
            drawing.chart.categories,
            ["January", "February", "March", "April"]
        );
        assert_eq!(drawing.chart.series[0].values, ["120", "185", "160", "240"]);
        assert_eq!(drawing.chart.series[0].fill, Some([0x17, 0x6b, 0x5b]));

        let editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        assert!(editor.starts_with(b"XLSY;v10;0;"));
        assert!(contains_utf16le(&editor, "CTOX Revenue 2026"));
        assert!(
            editor.len() > 3_000,
            "chart records must be present in XLSY"
        );

        let exported = export_spreadsheet_binary(&editor, &source).unwrap();
        let mut original = ZipArchive::new(Cursor::new(&source)).unwrap();
        let mut roundtrip = ZipArchive::new(Cursor::new(&exported)).unwrap();
        for path in [
            "xl/charts/chart1.xml",
            "xl/drawings/drawing1.xml",
            "xl/drawings/_rels/drawing1.xml.rels",
            "customXml/ctox-spreadsheet-preserve.xml",
        ] {
            assert_eq!(
                read_zip_part(&mut original, path).unwrap(),
                read_zip_part(&mut roundtrip, path).unwrap(),
                "unchanged chart package part must remain byte-identical: {path}"
            );
        }
    }

    #[test]
    fn spreadsheet_pivot_print_layout_prepare_emits_native_records() {
        let source_path = office_fixture("spreadsheet/pivot-print-layout.xlsx");
        let source = fs::read(source_path).unwrap();
        let (_, _, workbook, sheets) = read_spreadsheet_source(&source).unwrap();
        assert_eq!(workbook.pivot_caches.len(), 1);
        assert_eq!(workbook.pivot_caches[0].id, 1);
        assert!(workbook.pivot_caches[0].records_xml.is_some());
        assert_eq!(sheets[0].view, Some(1));
        assert_eq!(sheets[0].pivot_tables.len(), 1);
        assert_eq!(sheets[0].pivot_tables[0].cache_id, 1);
        let print = sheets[0].print_layout.as_ref().unwrap();
        assert_eq!(print.fit_to_page, Some(true));
        assert_eq!(print.orientation, Some(0));
        assert_eq!(print.paper_size, Some(9));
        assert_eq!(print.row_breaks.as_ref().unwrap().breaks[0].id, 7);
        assert_eq!(print.col_breaks.as_ref().unwrap().breaks[0].id, 3);

        let editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        assert!(editor.starts_with(b"XLSY;v10;0;"));
        assert!(editor
            .windows(b"pivotTableDefinition".len())
            .any(|value| value == b"pivotTableDefinition"));
        assert!(editor
            .windows(b"pivotCacheDefinition".len())
            .any(|value| value == b"pivotCacheDefinition"));
        assert!(
            editor.len() > 5_000,
            "pivot and print records must be present in XLSY"
        );

        let exported = export_spreadsheet_binary(&editor, &source).unwrap();
        let mut original = ZipArchive::new(Cursor::new(&source)).unwrap();
        let mut roundtrip = ZipArchive::new(Cursor::new(&exported)).unwrap();
        for path in [
            "xl/pivotTables/pivotTable1.xml",
            "xl/pivotCache/pivotCacheDefinition1.xml",
            "xl/pivotCache/pivotCacheRecords1.xml",
            "customXml/ctox-spreadsheet-preserve.xml",
        ] {
            assert_eq!(
                read_zip_part(&mut original, path).unwrap(),
                read_zip_part(&mut roundtrip, path).unwrap(),
                "unchanged pivot package part must remain byte-identical: {path}"
            );
        }

        let mut renamed_editor = editor.clone();
        let before = b"name=\"CTOXRevenuePivot\"";
        let after = b"name=\"CTOXRevenueFY26X\"";
        assert_eq!(before.len(), after.len());
        let offset = renamed_editor
            .windows(before.len())
            .position(|value| value == before)
            .expect("native XLSY must contain the pivot name");
        renamed_editor[offset..offset + after.len()].copy_from_slice(after);
        let renamed_export = export_spreadsheet_binary(&renamed_editor, &source).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&renamed_export)).unwrap();
        let pivot = String::from_utf8(
            read_zip_part(&mut archive, "xl/pivotTables/pivotTable1.xml").unwrap(),
        )
        .unwrap();
        assert!(pivot.contains("name=\"CTOXRevenueFY26X\""));
        assert!(pivot.contains("cacheId=\"1\""));
        assert_eq!(
            read_zip_part(&mut archive, "xl/pivotCache/pivotCacheDefinition1.xml").unwrap(),
            {
                let mut original = ZipArchive::new(Cursor::new(&source)).unwrap();
                read_zip_part(&mut original, "xl/pivotCache/pivotCacheDefinition1.xml").unwrap()
            },
            "renaming a pivot must not normalize its unrelated cache definition"
        );
    }

    #[test]
    fn spreadsheet_roundtrip_corpus_preserves_every_identity_package_part() {
        let corpus_dir = office_fixture("spreadsheet");
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(corpus_dir.join("corpus.json")).unwrap()).unwrap();
        let entries = manifest["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 11);
        for entry in entries {
            let file = entry["file"].as_str().unwrap();
            let source = fs::read(corpus_dir.join(file)).unwrap();
            assert_eq!(
                source.len() as u64,
                entry["bytes"].as_u64().unwrap(),
                "{file}"
            );
            assert_eq!(
                sha256_hex(&source),
                entry["sha256"].as_str().unwrap(),
                "{file}"
            );
            let editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
            assert!(editor.starts_with(b"XLSY;v10;0;"), "{file}");
            let exported = export_spreadsheet_binary(&editor, &source).unwrap();
            inspect(OfficeKind::Spreadsheet, &exported).unwrap();
            let mut original = ZipArchive::new(Cursor::new(&source)).unwrap();
            let mut roundtrip = ZipArchive::new(Cursor::new(&exported)).unwrap();
            assert_eq!(
                original.len(),
                entry["parts"].as_u64().unwrap() as usize,
                "{file}"
            );
            assert_eq!(roundtrip.len(), original.len(), "{file}");
            for index in 0..original.len() {
                let path = original.by_index(index).unwrap().name().to_string();
                assert_eq!(
                    read_zip_part(&mut original, &path).unwrap(),
                    read_zip_part(&mut roundtrip, &path).unwrap(),
                    "identity XLSY roundtrip changed {file}:{path}"
                );
            }
        }
    }

    #[test]
    fn spreadsheet_shared_string_dedup_keeps_unrelated_sheet_indices_stable() {
        let source_path = office_fixture("spreadsheet/undo-clipboard-fill.xlsx");
        let source = fs::read(source_path).unwrap();
        let original_editor = transcode_spreadsheet_to_editor_payload(&source).unwrap();
        let original = inspect_editor_payload(OfficeKind::Spreadsheet, &original_editor).unwrap();
        let mut changed = original.clone();
        changed.shared_strings[1] = "UNDO_FILL_BASE_ONE".to_string();
        changed.shared_strings.remove(3);
        let overview = &mut changed.worksheets[0];
        overview
            .cells
            .iter_mut()
            .find(|cell| cell.reference == "A2")
            .unwrap()
            .display = "UNDO_FILL_BASE_ONE".to_string();
        overview
            .cells
            .iter_mut()
            .find(|cell| cell.reference == "B3")
            .unwrap()
            .display = "COPY_SOURCE_TEXT".to_string();
        overview
            .cells
            .iter_mut()
            .find(|cell| cell.reference == "B5")
            .unwrap()
            .display = "125000".to_string();

        let materialized = materialize_spreadsheet_shared_strings(&original, &changed).unwrap();
        assert_eq!(materialized.len(), original.shared_strings.len());
        assert_eq!(materialized[1], "UNDO_FILL_BASE_ONE");
        assert_eq!(materialized[3], "PASTE_TARGET_TEXT");

        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let paths = spreadsheet_worksheet_paths(&mut archive).unwrap();
        let shared = read_zip_part(&mut archive, "xl/sharedStrings.xml").unwrap();
        let styles = decode_spreadsheet_editor_styles(&original_editor, &original).unwrap();
        let overview_path = paths["Overview"].clone();
        let overview_xml = read_zip_part(&mut archive, &overview_path).unwrap();
        let mut replacements = BTreeMap::new();
        replacements.insert(
            "xl/sharedStrings.xml".to_string(),
            replace_changed_shared_strings(&shared, &original.shared_strings, &materialized)
                .unwrap(),
        );
        replacements.insert(
            overview_path,
            replace_changed_worksheet_cells(
                &overview_xml,
                &original.worksheets[0],
                &changed.worksheets[0],
                &original.shared_strings,
                &materialized,
                &styles,
                &styles,
                &BTreeMap::new(),
            )
            .unwrap(),
        );
        drop(archive);
        let package = replace_package_parts(&source, replacements).unwrap();
        let roundtrip = inspect_editor_payload(
            OfficeKind::Spreadsheet,
            &transcode_spreadsheet_to_editor_payload(&package).unwrap(),
        )
        .unwrap();
        assert_eq!(
            roundtrip.worksheets[0].cells[1].display,
            "UNDO_FILL_BASE_ONE"
        );
        assert_eq!(roundtrip.worksheets[0].cells[3].display, "COPY_SOURCE_TEXT");
        assert_eq!(roundtrip.worksheets[0].cells[7].display, "125000");
        assert_eq!(roundtrip.worksheets[1], original.worksheets[1]);
        assert_eq!(roundtrip.worksheets[2], original.worksheets[2]);
    }

    #[test]
    fn rejects_malformed_or_wrong_version_xlsy_payloads() {
        let xlsx = b"PK\x03\x04";
        assert!(inspect_editor_payload(OfficeKind::Spreadsheet, xlsx)
            .unwrap_err()
            .to_string()
            .contains("invalid XLSY signature"));
        let wrong_version = b"XLSY;v9;0;\x00";
        assert!(
            inspect_editor_payload(OfficeKind::Spreadsheet, wrong_version)
                .unwrap_err()
                .to_string()
                .contains("unsupported Euro-Office XLSY protocol version")
        );
        let truncated = b"XLSY;v10;0;\x01\x03";
        assert!(inspect_editor_payload(OfficeKind::Spreadsheet, truncated)
            .unwrap_err()
            .to_string()
            .contains("table directory is truncated"));
    }

    #[test]
    fn decodes_xlsy_shared_strings_including_rich_text_runs() {
        fn item(item_type: u8, content: &[u8]) -> Vec<u8> {
            let mut value = vec![item_type];
            value.extend_from_slice(&(content.len() as u32).to_le_bytes());
            value.extend_from_slice(content);
            value
        }
        let text = "CTOX"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let plain = item(0, &item(3, &text));
        let run_text = " Office"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let rich = item(0, &item(1, &item(3, &run_text)));
        let content = [plain, rich].concat();
        let mut table = (content.len() as u32).to_le_bytes().to_vec();
        table.extend_from_slice(&content);
        assert_eq!(
            decode_spreadsheet_shared_strings(&table).unwrap(),
            vec!["CTOX", " Office"]
        );
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
    fn document_roundtrip_corpus_uses_native_docy_and_preserves_package_parts() {
        let corpus_dir = office_fixture("document");
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
            let source_manifest = inspect(OfficeKind::Document, &source).unwrap();
            assert_eq!(
                source_manifest.parts.len() as u64,
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
            assert!(prepared.editor_payload.starts_with(b"DOCY;v10;0;"));
            let exported = export(
                OfficeKind::Document,
                &prepared.editor_payload,
                Some(&source),
            )
            .unwrap_or_else(|error| {
                panic!("native DOCY roundtrip failed for {file_name}: {error:#}")
            });
            assert_eq!(
                exported.manifest.primary_text, source_manifest.primary_text,
                "native DOCY roundtrip changed primary text for {file_name}"
            );
            let source_paths = source_manifest
                .parts
                .iter()
                .map(|part| part.path.as_str())
                .collect::<BTreeSet<_>>();
            let exported_paths = exported
                .manifest
                .parts
                .iter()
                .map(|part| part.path.as_str())
                .collect::<BTreeSet<_>>();
            assert!(
                source_paths.is_subset(&exported_paths),
                "native DOCY roundtrip removed package parts for {file_name}: {:?}",
                source_paths.difference(&exported_paths).collect::<Vec<_>>()
            );
            assert!(
                exported_paths
                    .difference(&source_paths)
                    .all(|path| is_understood_document_part(path)),
                "native DOCY roundtrip added an unrecognized package part for {file_name}: {:?}",
                exported_paths.difference(&source_paths).collect::<Vec<_>>()
            );
            let mut source_archive = ZipArchive::new(Cursor::new(&source)).unwrap();
            let mut exported_archive = ZipArchive::new(Cursor::new(&exported.bytes)).unwrap();
            for path in entry["must_preserve"].as_array().unwrap() {
                let path = path.as_str().unwrap();
                let mut original = Vec::new();
                source_archive
                    .by_name(path)
                    .unwrap_or_else(|_| {
                        panic!("missing source preservation part {path} in {file_name}")
                    })
                    .read_to_end(&mut original)
                    .unwrap();
                let mut preserved = Vec::new();
                exported_archive
                    .by_name(path)
                    .unwrap_or_else(|_| panic!("missing preservation part {path} in {file_name}"))
                    .read_to_end(&mut preserved)
                    .unwrap();
                assert_eq!(
                    preserved, original,
                    "native DOCY roundtrip changed escrow part {path} in {file_name}"
                );
            }
        }
    }

    #[test]
    fn spreadsheet_open_render_fixture_roundtrips_with_sheet_manifest() {
        let fixture = office_fixture("spreadsheet/open-render-sheets.xlsx");
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
        assert_eq!(prepared.protocol, SPREADSHEET_EDITOR_PROTOCOL);
        assert!(prepared.editor_payload.starts_with(b"XLSY;v10;0;"));
        assert_ne!(prepared.editor_sha256, prepared.source_sha256);
        assert_eq!(
            prepared.editor_manifest.as_ref().unwrap().worksheets.len(),
            3
        );
        let exported = export(OfficeKind::Spreadsheet, &source, None).unwrap();
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
        let fixture = office_fixture("spreadsheet/edit-save.xlsx");
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
    fn spreadsheet_export_merges_table_definition_and_preserves_escrow() {
        let fixture = office_fixture("spreadsheet/sort-filter-tables.xlsx");
        let source = fs::read(fixture).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&source)).unwrap();
        let output = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(output);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let path = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if path == "xl/tables/table1.xml" {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replace("<autoFilter ref=\"A1:C6\"/>", "<autoFilter ref=\"A1:C6\"><filterColumn colId=\"0\"><filters><filter val=\"North\"/></filters></filterColumn></autoFilter>")
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
        let mut table = String::new();
        merged
            .by_name("xl/tables/table1.xml")
            .unwrap()
            .read_to_string(&mut table)
            .unwrap();
        assert!(table.contains("filter val=\"North\""));
        let mut escrow = String::new();
        merged
            .by_name("customXml/ctox-spreadsheet-preserve.xml")
            .unwrap()
            .read_to_string(&mut escrow)
            .unwrap();
        assert!(escrow.contains("SPREADSHEET_TABLE_ESCROW_8B47"));
    }

    #[test]
    fn document_parse_preserves_paragraph_section_properties() {
        let xml = br#"<?xml version="1.0"?>
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                        xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
              <w:body>
                <w:p><w:r><w:t>SECTION_ONE</w:t></w:r></w:p>
                <w:p>
                  <w:pPr>
                    <w:sectPr>
                      <w:headerReference w:type="default" r:id="rId9"/>
                      <w:footerReference w:type="default" r:id="rId10"/>
                      <w:type w:val="nextPage"/>
                      <w:pgSz w:w="12240" w:h="15840"/>
                      <w:pgMar w:top="1152" w:right="1296" w:bottom="1152" w:left="1296" w:header="576" w:footer="648" w:gutter="0"/>
                      <w:titlePg/>
                    </w:sectPr>
                  </w:pPr>
                </w:p>
                <w:p><w:r><w:t>SECTION_TWO</w:t></w:r></w:p>
                <w:sectPr>
                  <w:pgSz w:w="15840" w:h="12240"/>
                  <w:pgMar w:top="1296" w:right="1152" w:bottom="1296" w:left="1152" w:header="709" w:footer="709" w:gutter="0"/>
                </w:sectPr>
              </w:body>
            </w:document>"#;
        let source = parse_document_source(
            xml,
            &DocumentStyleContext::default(),
            &BTreeMap::new(),
            &DocumentHeaderFooterParts::default(),
        )
        .unwrap();
        assert_eq!(source.blocks.len(), 3);
        let DocumentBlock::Paragraph(section_break) = &source.blocks[1] else {
            panic!("second block should be the empty section-break paragraph");
        };
        let paragraph_section = section_break
            .section
            .as_ref()
            .expect("paragraph sectPr should be preserved");
        assert_eq!(paragraph_section.width_twips, 12_240);
        assert_eq!(paragraph_section.height_twips, 15_840);
        assert_eq!(paragraph_section.orientation, 0);
        assert_eq!(paragraph_section.break_type, Some(3));
        assert_eq!(
            paragraph_section.margins_twips,
            [1_296, 1_152, 1_296, 1_152, 576, 648, 0]
        );
        assert!(paragraph_section.title_page);
        let body_section = source.section.as_ref().expect("body sectPr should remain");
        assert_eq!(body_section.width_twips, 15_840);
        assert_eq!(body_section.height_twips, 12_240);
    }

    #[test]
    fn document_prepare_writes_header_footer_table_and_content() {
        let fixture = office_fixture("document/sections-headers-footers.docx");
        let source = fs::read(fixture).unwrap();
        let editor = transcode_document_to_editor_payload(&source).unwrap();
        assert!(docy_table_types(&editor).contains(&4));
        assert!(contains_utf16le(&editor, "HEADER_SECTION1_DEFAULT"));
        assert!(contains_utf16le(&editor, "HEADER_SECTION1_FIRST"));
        assert!(contains_utf16le(&editor, "FOOTER_SECTION1_DEFAULT"));
    }

    #[test]
    fn document_section_rewrite_updates_layout_and_preserves_header_footer_refs() {
        let paragraph = r#"<w:p><w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rId9"/><w:headerReference w:type="first" r:id="rId10"/><w:footerReference w:type="default" r:id="rId11"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1152" w:right="1296" w:bottom="1152" w:left="1296" w:header="576" w:footer="648" w:gutter="0"/><w:cols w:space="720"/><w:docGrid w:linePitch="360"/></w:sectPr></w:pPr></w:p>"#;
        let decoded = DecodedDocumentParagraph {
            section: Some(DecodedDocumentSection {
                width_twips: Some(15_840),
                height_twips: Some(12_240),
                orientation: Some(1),
                margins_twips: [
                    Some(1_152),
                    Some(1_296),
                    Some(1_152),
                    Some(1_296),
                    Some(850),
                    Some(648),
                    Some(0),
                ],
                title_page: Some(true),
                break_type: Some(3),
                ..DecodedDocumentSection::default()
            }),
            ..DecodedDocumentParagraph::default()
        };
        let rewritten = rewrite_document_paragraph_xml(
            paragraph,
            &decoded,
            &BTreeMap::new(),
            &DecodedDocumentHeaderFooterRelationshipIds::default(),
        )
        .expect("section paragraph should rewrite");
        assert!(rewritten.contains(r#"<w:headerReference w:type="default" r:id="rId9"/>"#));
        assert!(rewritten.contains(r#"<w:headerReference w:type="first" r:id="rId10"/>"#));
        assert!(rewritten.contains(r#"<w:footerReference w:type="default" r:id="rId11"/>"#));
        assert!(rewritten.contains(r#"<w:cols w:space="720"/>"#));
        assert!(rewritten.contains(r#"<w:docGrid w:linePitch="360"/>"#));
        assert!(rewritten.contains(r#"<w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/>"#));
        assert!(rewritten.contains(r#"w:header="850""#));
        assert!(rewritten.contains(r#"<w:type w:val="nextPage"/>"#));
        assert!(rewritten.contains("<w:titlePg/>"));
    }

    #[test]
    fn document_header_footer_package_plan_materializes_unlinked_section_header() {
        let relationships_xml = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header2.xml"/><Relationship Id="rId11" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#;
        let content_types_xml = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#;
        let relationship_parts =
            parse_document_header_footer_relationship_parts(relationships_xml).unwrap();
        let header_footer_parts = DecodedDocumentHeaderFooterParts {
            headers: vec![
                vec![decoded_text_paragraph("HEADER_SECTION1_DEFAULT")],
                vec![decoded_text_paragraph("HEADER_SECTION1_FIRST")],
                vec![decoded_text_paragraph("HEADER_SECTION2_DEFAULT")],
            ],
            footers: vec![vec![decoded_text_paragraph("FOOTER_SECTION1_DEFAULT")]],
        };
        let mut ids = DecodedDocumentHeaderFooterRelationshipIds {
            headers: relationship_parts
                .headers
                .iter()
                .map(|part| part.id.clone())
                .collect(),
            footers: relationship_parts
                .footers
                .iter()
                .map(|part| part.id.clone())
                .collect(),
        };
        let replacements = prepare_document_binary_header_footer_package_replacements(
            Some(relationships_xml),
            content_types_xml,
            &relationship_parts,
            &header_footer_parts,
            &BTreeMap::new(),
            &mut ids,
        )
        .unwrap();
        assert_eq!(ids.headers, vec!["rId9", "rId10", "rId12"]);
        assert_eq!(ids.footers, vec!["rId11"]);
        let header3 = std::str::from_utf8(replacements.get("word/header3.xml").unwrap()).unwrap();
        assert!(header3.contains("HEADER_SECTION2_DEFAULT"));
        let relationships =
            std::str::from_utf8(replacements.get("word/_rels/document.xml.rels").unwrap()).unwrap();
        assert!(relationships.contains(r#"Id="rId12""#));
        assert!(relationships.contains(r#"Target="header3.xml""#));
        let content_types =
            std::str::from_utf8(replacements.get("[Content_Types].xml").unwrap()).unwrap();
        assert!(content_types.contains(r#"PartName="/word/header3.xml""#));
    }

    fn decoded_text_paragraph(text: &str) -> DecodedDocumentBlock {
        DecodedDocumentBlock::Paragraph(DecodedDocumentParagraph {
            text: text.to_string(),
            ..DecodedDocumentParagraph::default()
        })
    }

    fn docy_table_types(payload: &[u8]) -> Vec<u8> {
        assert!(payload.starts_with(b"DOCY;v10;0;"));
        let directory_start = b"DOCY;v10;0;".len();
        let count = payload[directory_start] as usize;
        (0..count)
            .map(|index| payload[directory_start + 1 + index * 5])
            .collect()
    }

    fn contains_utf16le(haystack: &[u8], needle: &str) -> bool {
        let needle = needle
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        haystack
            .windows(needle.len())
            .any(|candidate| candidate == needle)
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
}
