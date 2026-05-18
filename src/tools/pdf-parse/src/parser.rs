use crate::core::config::{LiteParseConfig, LiteParseConfigOverrides, OutputFormat};
use crate::core::types::ParseResult;
use crate::engines::pdf::interface::{PdfEngine, PdfEngineError};
use crate::processing::bbox::build_bounding_boxes;
use crate::processing::grid_projection::project_pages_to_grid;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LiteParseError {
    #[error(transparent)]
    Pdf(#[from] PdfEngineError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct LiteParse<E: PdfEngine> {
    config: LiteParseConfig,
    pdf_engine: E,
}

impl<E: PdfEngine> LiteParse<E> {
    pub fn new(pdf_engine: E, overrides: LiteParseConfigOverrides) -> Self {
        Self {
            config: LiteParseConfig::merge(overrides),
            pdf_engine,
        }
    }

    pub fn config(&self) -> &LiteParseConfig {
        &self.config
    }

    pub fn parse_pdf_path(&self, path: &str) -> Result<ParseResult, LiteParseError> {
        let doc = self
            .pdf_engine
            .load_document_path(path, self.config.password.as_deref())?;

        let mut pages = self.pdf_engine.extract_all_pages(
            &doc,
            Some(self.config.max_pages),
            self.config.target_pages.as_deref(),
        )?;

        pages = project_pages_to_grid(&pages, &self.config);

        if self.config.precise_bounding_box {
            for page in &mut pages {
                page.bounding_boxes = Some(build_bounding_boxes(&page.text_items));
            }
        }

        let text = pages
            .iter()
            .map(|page| page.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut result = ParseResult {
            total_pages: doc.num_pages,
            pages,
            text,
            json: None,
        };

        if matches!(self.config.output_format, OutputFormat::Json) {
            result.json = Some(serde_json::to_value(&result)?);
        }

        Ok(result)
    }

    pub fn parse_pdf_bytes(&self, bytes: &[u8]) -> Result<ParseResult, LiteParseError> {
        let doc = self
            .pdf_engine
            .load_document_bytes(bytes, self.config.password.as_deref())?;

        let mut pages = self.pdf_engine.extract_all_pages(
            &doc,
            Some(self.config.max_pages),
            self.config.target_pages.as_deref(),
        )?;

        pages = project_pages_to_grid(&pages, &self.config);

        if self.config.precise_bounding_box {
            for page in &mut pages {
                page.bounding_boxes = Some(build_bounding_boxes(&page.text_items));
            }
        }

        let text = pages
            .iter()
            .map(|page| page.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut result = ParseResult {
            total_pages: doc.num_pages,
            pages,
            text,
            json: None,
        };

        if matches!(self.config.output_format, OutputFormat::Json) {
            result.json = Some(serde_json::to_value(&result)?);
        }

        Ok(result)
    }
}
