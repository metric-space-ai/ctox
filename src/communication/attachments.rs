use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AttachmentFile {
    pub path: PathBuf,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AttachmentRef {
    pub name: String,
    pub path: String,
    pub content_type: String,
    pub size_bytes: u64,
}

pub(crate) fn load_outbound_attachments(paths: &[String]) -> Result<Vec<AttachmentFile>> {
    paths
        .iter()
        .map(|raw| {
            let path = PathBuf::from(raw);
            let bytes = std::fs::read(&path)
                .with_context(|| format!("failed to read attachment {}", path.display()))?;
            let file_name = file_name_for_path(&path);
            Ok(AttachmentFile {
                content_type: content_type_for_path(&path).to_string(),
                size_bytes: bytes.len() as u64,
                path,
                file_name,
                bytes,
            })
        })
        .collect()
}

pub(crate) fn refs_for_paths(paths: &[String]) -> Result<Vec<AttachmentRef>> {
    load_outbound_attachments(paths)
        .map(|items| items.into_iter().map(AttachmentRef::from).collect())
}

pub(crate) fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => "text/csv; charset=utf-8",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "gif" => "image/gif",
        "htm" | "html" => "text/html; charset=utf-8",
        "jpeg" | "jpg" => "image/jpeg",
        "json" => "application/json",
        "log" | "txt" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "svg" => "image/svg+xml",
        "tsv" => "text/tab-separated-values; charset=utf-8",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

pub(crate) fn safe_file_name(value: &str, fallback: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

pub(crate) fn file_name_for_path(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| safe_file_name(value, "attachment.bin"))
        .unwrap_or_else(|| "attachment.bin".to_string())
}

impl From<AttachmentFile> for AttachmentRef {
    fn from(value: AttachmentFile) -> Self {
        Self {
            name: value.file_name,
            path: value.path.display().to_string(),
            content_type: value.content_type,
            size_bytes: value.size_bytes,
        }
    }
}
