//! Manuscript builder + Markdown renderer + DOCX renderer.
//!
//! Pipeline: workspace state -> Manuscript struct -> JSON -> Markdown
//! and/or -> bundled Python helper -> DOCX.
//!
//! The Manuscript struct is the deterministic intermediate shape that
//! both the Markdown renderer (pure Rust) and the DOCX renderer (thin
//! subprocess wrapper around `scripts/render_manuscript.py`) consume.
//! Wave 4 ships the builder and the Markdown renderer; the DOCX side
//! delegates to the Python helper that already exists in the skill
//! directory (Wave 3 deliverable).

pub mod docx;
pub mod manuscript;
pub mod markdown;

pub use docx::{render_docx, DocxRenderError, DocxRenderOutcome};
pub use manuscript::{
    build_manuscript, AbbreviationRow, FigurePlaceholder, Manuscript, ManuscriptBlock,
    ManuscriptBlockKind, ManuscriptDoc, ManuscriptManifest, ManuscriptTable, ReferenceEntry,
};
pub use markdown::{render_markdown, CitationStyle, MarkdownRenderOptions};
