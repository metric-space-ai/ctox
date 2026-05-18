use crate::core::types::{ParsedPage, PdfDocumentHandle, ScreenshotResult};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfEngineError {
    #[error("pdf engine is not available in this environment: {0}")]
    Unavailable(String),

    #[error("pdf engine error: {0}")]
    Backend(String),
}

pub trait PdfEngine {
    fn load_document_bytes(
        &self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<PdfDocumentHandle, PdfEngineError>;

    fn load_document_path(
        &self,
        path: &str,
        password: Option<&str>,
    ) -> Result<PdfDocumentHandle, PdfEngineError>;

    fn extract_page(
        &self,
        doc: &PdfDocumentHandle,
        page_num: usize,
    ) -> Result<ParsedPage, PdfEngineError>;

    fn extract_all_pages(
        &self,
        doc: &PdfDocumentHandle,
        max_pages: Option<usize>,
        target_pages: Option<&str>,
    ) -> Result<Vec<ParsedPage>, PdfEngineError>;

    fn render_page_image(
        &self,
        doc: &PdfDocumentHandle,
        page_num: usize,
        dpi: u16,
    ) -> Result<ScreenshotResult, PdfEngineError>;

    fn close(&self, _doc: PdfDocumentHandle) -> Result<(), PdfEngineError> {
        Ok(())
    }
}
