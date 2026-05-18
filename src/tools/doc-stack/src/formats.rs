use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExistingWriteSurface {
    TextTools,
    SpecializedEmail,
    RawFsOnly,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreferredSemanticWriteMode {
    None,
    TextNative,
    StructuredEmail,
    ContainerRegenerate,
}

#[derive(Debug, Clone, Copy)]
pub struct DocumentFormatSpec {
    pub parser_kind: &'static str,
    pub extensions: &'static [&'static str],
    pub existing_write_surface: ExistingWriteSurface,
    pub preferred_semantic_write_mode: PreferredSemanticWriteMode,
    pub notes: &'static str,
}

const FORMAT_SPECS: &[DocumentFormatSpec] = &[
    DocumentFormatSpec {
        parser_kind: "text",
        extensions: &["txt", "log", "yaml", "yml"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "Plain text files already fit generic Codex text editing and should use direct text serialization first.",
    },
    DocumentFormatSpec {
        parser_kind: "markdown",
        extensions: &["md", "markdown", "rst", "org"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "Structured text with stable round-tripping through direct text edits.",
    },
    DocumentFormatSpec {
        parser_kind: "html",
        extensions: &["html", "htm"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "HTML stays text-native; semantic edits should still serialize back to source HTML, not derived plain text.",
    },
    DocumentFormatSpec {
        parser_kind: "json",
        extensions: &["json"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "JSON can use generic text editing now and later a structured serializer.",
    },
    DocumentFormatSpec {
        parser_kind: "xml",
        extensions: &["xml"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "XML already has in-repo traversal helpers and should stay text-native for a first write path.",
    },
    DocumentFormatSpec {
        parser_kind: "table",
        extensions: &["csv", "tsv"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::TextNative,
        notes: "Delimited tables are text files; later semantic edits can still emit CSV or TSV directly.",
    },
    DocumentFormatSpec {
        parser_kind: "rtf",
        extensions: &["rtf"],
        existing_write_surface: ExistingWriteSurface::TextTools,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::None,
        notes: "RTF is text-backed, but the current reader is lossy and does not justify semantic write-back yet.",
    },
    DocumentFormatSpec {
        parser_kind: "pdf",
        extensions: &["pdf"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::None,
        notes: "PDF should remain read-first; generic overwrite exists underneath, but not a trustworthy semantic edit story.",
    },
    DocumentFormatSpec {
        parser_kind: "email",
        extensions: &["eml"],
        existing_write_surface: ExistingWriteSurface::SpecializedEmail,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::StructuredEmail,
        notes: "CTOX already has RFC822 parsing and raw-message construction, so email is the best first structured write target.",
    },
    DocumentFormatSpec {
        parser_kind: "docx",
        extensions: &["docx"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "OOXML containers need regenerate-or-replace semantics, not patch-in-place text edits.",
    },
    DocumentFormatSpec {
        parser_kind: "pptx",
        extensions: &["pptx"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "Presentation containers should be rebuilt from normalized slide content if write support is added.",
    },
    DocumentFormatSpec {
        parser_kind: "xlsx",
        extensions: &["xlsx"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "Workbook containers need cell-aware regeneration rather than binary patching.",
    },
    DocumentFormatSpec {
        parser_kind: "odt",
        extensions: &["odt"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "OpenDocument text files are zip containers and should use regenerate-or-replace semantics.",
    },
    DocumentFormatSpec {
        parser_kind: "ods",
        extensions: &["ods"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "OpenDocument spreadsheets need table-aware regeneration, not line patches.",
    },
    DocumentFormatSpec {
        parser_kind: "odp",
        extensions: &["odp"],
        existing_write_surface: ExistingWriteSurface::RawFsOnly,
        preferred_semantic_write_mode: PreferredSemanticWriteMode::ContainerRegenerate,
        notes: "OpenDocument presentations should follow the same normalized-slide regeneration pattern as PPTX.",
    },
];

pub fn format_specs() -> &'static [DocumentFormatSpec] {
    FORMAT_SPECS
}

pub fn parser_kind_for_path(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    FORMAT_SPECS
        .iter()
        .find(|spec| {
            spec.extensions
                .iter()
                .any(|candidate| *candidate == extension)
        })
        .map(|spec| spec.parser_kind)
        .unwrap_or("text")
}

pub fn supports_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    FORMAT_SPECS.iter().any(|spec| {
        spec.extensions
            .iter()
            .any(|candidate| *candidate == extension)
    })
}
