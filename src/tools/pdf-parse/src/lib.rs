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

/// Render up to `max_pages` pages of a PDF to PNG image bytes at the given
/// `dpi`, returning one PNG byte vector per rendered page. Used to feed
/// scanned / image-only PDFs (which yield no extractable text) to a vision
/// model for OCR.
#[cfg(feature = "pdfium")]
pub fn render_pdf_pages_png(
    bytes: &[u8],
    max_pages: usize,
    dpi: u16,
    password: Option<&str>,
) -> Result<Vec<Vec<u8>>, LiteParseError> {
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;

    let backend = PdfiumBackend::new()?;
    let doc = backend.load_document_bytes(bytes, password)?;
    let page_count = doc.num_pages.min(max_pages.max(1));
    let mut pages = Vec::with_capacity(page_count);
    for page_num in 1..=page_count {
        let shot = backend.render_page_image(&doc, page_num, dpi)?;
        let rgba = RgbaImage::from_raw(shot.width as u32, shot.height as u32, shot.image_buffer)
            .ok_or_else(|| {
                PdfEngineError::Backend(format!("failed to build image buffer for page {page_num}"))
            })?;
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|err| {
                PdfEngineError::Backend(format!("failed to PNG-encode page {page_num}: {err}"))
            })?;
        pages.push(png);
    }
    Ok(pages)
}
