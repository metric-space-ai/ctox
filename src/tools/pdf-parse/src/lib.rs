pub mod core;
pub mod engines;
pub mod evaluation;
pub mod parser;
pub mod processing;

pub use core::config::{LiteParseConfig, LiteParseConfigOverrides, OutputFormat};
pub use core::types::*;
pub use evaluation::*;
pub use parser::{LiteParse, LiteParseError};

#[cfg(feature = "pdfium")]
pub use engines::pdf::interface::{PdfEngine, PdfEngineError};
#[cfg(feature = "pdfium")]
pub use engines::pdf::pdfium_backend::PdfiumBackend;

#[cfg(feature = "pdfium")]
pub fn parse_pdf_bytes(
    bytes: &[u8],
    overrides: LiteParseConfigOverrides,
) -> Result<ParseResult, LiteParseError> {
    let parser = LiteParse::new(PdfiumBackend::new()?, overrides);
    parser.parse_pdf_bytes(bytes)
}

#[cfg(feature = "pdfium")]
pub fn parse_pdf_path(
    path: &str,
    overrides: LiteParseConfigOverrides,
) -> Result<ParseResult, LiteParseError> {
    let parser = LiteParse::new(PdfiumBackend::new()?, overrides);
    parser.parse_pdf_path(path)
}

#[cfg(feature = "pdfium")]
pub fn page_count_for_pdf_bytes(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<usize, PdfEngineError> {
    let backend = PdfiumBackend::new()?;
    let doc = backend.load_document_bytes(bytes, password)?;
    Ok(doc.num_pages)
}

#[cfg(feature = "pdfium")]
pub fn page_count_for_pdf_path(
    path: &str,
    password: Option<&str>,
) -> Result<usize, PdfEngineError> {
    let backend = PdfiumBackend::new()?;
    let doc = backend.load_document_path(path, password)?;
    Ok(doc.num_pages)
}
