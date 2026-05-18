#![cfg(feature = "pdfium")]

use crate::core::types::{ParsedPage, PdfDocumentHandle, ScreenshotResult, TextItem};
use crate::engines::pdf::interface::{PdfEngine, PdfEngineError};
use pdfium_auto::bind_pdfium_silent;
use pdfium_render::prelude::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering as AtomicOrdering},
    Arc, Mutex,
};

/// Pdfium-backed PDF engine.
///
/// Design notes:
/// - `pdfium-render` is used as the single Rust-side backend for both text extraction
///   and page rendering.
/// - the LiteParse layout logic remains separate in `processing::grid_projection`.
/// - production extraction should map `PdfPageTextChar` geometry to `TextItem`s
///   and let the Rust grid logic recover visual reading order.
///
/// This implementation intentionally keeps the lifetime-heavy Pdfium objects out of the
/// public parser core by reopening documents from a stable handle when necessary.
pub struct PdfiumBackend {
    pdfium: Pdfium,
    in_memory_docs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    next_id: AtomicU64,
}

impl PdfiumBackend {
    pub fn new() -> Result<Self, PdfEngineError> {
        let pdfium =
            bind_pdfium_silent().map_err(|err| PdfEngineError::Backend(err.to_string()))?;
        Ok(Self {
            pdfium,
            in_memory_docs: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        })
    }

    fn next_handle_key(&self) -> String {
        format!(
            "buffer:{}",
            self.next_id.fetch_add(1, AtomicOrdering::Relaxed)
        )
    }

    fn reopen_document<'a>(
        &'a self,
        handle: &'a PdfDocumentHandle,
    ) -> Result<PdfDocument<'a>, PdfEngineError> {
        let password = handle.password.as_deref();
        if handle.source.starts_with("buffer:") {
            let bytes = self
                .in_memory_docs
                .lock()
                .map_err(|_| PdfEngineError::Backend("buffer lock poisoned".into()))?
                .get(&handle.source)
                .cloned()
                .ok_or_else(|| PdfEngineError::Backend("missing in-memory document".into()))?;

            self.pdfium
                .load_pdf_from_byte_vec(bytes, password)
                .map_err(|err| PdfEngineError::Backend(err.to_string()))
        } else {
            self.pdfium
                .load_pdf_from_file(&handle.source, password)
                .map_err(|err| PdfEngineError::Backend(err.to_string()))
        }
    }

    fn page_to_parsed_page(
        &self,
        page: &PdfPage<'_>,
        page_num: usize,
    ) -> Result<ParsedPage, PdfEngineError> {
        let width = page.width().value;
        let height = page.height().value;
        let page_text = page
            .text()
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;

        let raw_text = page_text.all();
        let page_chars = collect_page_chars(&page_text, height);
        let char_lines = cluster_page_char_lines(&page_chars);
        let segment_items = inject_inter_segment_spaces(
            text_items_from_segments(&page_text, height, &page_chars),
            &page_chars,
        );
        let segment_items = refine_problematic_lines(segment_items, &char_lines);
        let char_run_items = text_items_from_char_lines(&char_lines);
        let mut text_items = segment_items;

        if text_items.is_empty() {
            text_items = if !char_run_items.is_empty() {
                char_run_items
            } else {
                page_text
                    .chars()
                    .iter()
                    .filter_map(|ch| text_item_from_char(&ch, height))
                    .collect()
            };
        }

        Ok(ParsedPage {
            page_num,
            width: width as f64,
            height: height as f64,
            text: raw_text,
            text_items,
            images: Vec::new(),
            bounding_boxes: None,
        })
    }
}

impl PdfEngine for PdfiumBackend {
    fn load_document_bytes(
        &self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<PdfDocumentHandle, PdfEngineError> {
        let doc = self
            .pdfium
            .load_pdf_from_byte_slice(bytes, password)
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;

        let key = self.next_handle_key();

        self.in_memory_docs
            .lock()
            .map_err(|_| PdfEngineError::Backend("buffer lock poisoned".into()))?
            .insert(key.clone(), bytes.to_vec());

        Ok(PdfDocumentHandle {
            source: key,
            num_pages: doc.pages().len() as usize,
            password: password.map(str::to_owned),
        })
    }

    fn load_document_path(
        &self,
        path: &str,
        password: Option<&str>,
    ) -> Result<PdfDocumentHandle, PdfEngineError> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;

        Ok(PdfDocumentHandle {
            source: path.to_string(),
            num_pages: doc.pages().len() as usize,
            password: password.map(str::to_owned),
        })
    }

    fn extract_page(
        &self,
        doc: &PdfDocumentHandle,
        page_num: usize,
    ) -> Result<ParsedPage, PdfEngineError> {
        let document = self.reopen_document(doc)?;
        let page = document
            .pages()
            .get(page_num.saturating_sub(1).try_into().unwrap_or(u16::MAX))
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;
        self.page_to_parsed_page(&page, page_num)
    }

    fn extract_all_pages(
        &self,
        doc: &PdfDocumentHandle,
        max_pages: Option<usize>,
        target_pages: Option<&str>,
    ) -> Result<Vec<ParsedPage>, PdfEngineError> {
        let limit = max_pages.unwrap_or(doc.num_pages).min(doc.num_pages);

        let page_numbers: Vec<usize> = if let Some(target_pages) = target_pages {
            target_pages
                .split(',')
                .filter_map(|piece| {
                    let piece = piece.trim();
                    if let Some((start, end)) = piece.split_once('-') {
                        let start = start.trim().parse::<usize>().ok()?;
                        let end = end.trim().parse::<usize>().ok()?;
                        Some((start..=end).collect::<Vec<_>>())
                    } else {
                        piece.parse::<usize>().ok().map(|n| vec![n])
                    }
                })
                .flatten()
                .filter(|page| *page >= 1 && *page <= limit)
                .collect()
        } else {
            (1..=limit).collect()
        };

        let mut pages = Vec::new();
        for page_num in page_numbers {
            pages.push(self.extract_page(doc, page_num)?);
        }
        Ok(pages)
    }

    fn render_page_image(
        &self,
        doc: &PdfDocumentHandle,
        page_num: usize,
        dpi: u16,
    ) -> Result<ScreenshotResult, PdfEngineError> {
        let document = self.reopen_document(doc)?;
        let page = document
            .pages()
            .get(page_num.saturating_sub(1).try_into().unwrap_or(u16::MAX))
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;
        let scale_factor = (dpi as f32 / 72.0).max(1.0);
        let render = page
            .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale_factor))
            .map_err(|err| PdfEngineError::Backend(err.to_string()))?;
        let image = render.as_image().to_rgba8();
        let width = image.width() as f64;
        let height = image.height() as f64;

        Ok(ScreenshotResult {
            page_num,
            width,
            height,
            image_buffer: image.into_raw(),
        })
    }

    fn close(&self, doc: PdfDocumentHandle) -> Result<(), PdfEngineError> {
        if doc.source.starts_with("buffer:") {
            if let Ok(mut guard) = self.in_memory_docs.lock() {
                guard.remove(&doc.source);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct PageChar {
    text: String,
    x: f64,
    y: f64,
    right: f64,
    bottom: f64,
    width: f64,
    height: f64,
    rotation: i32,
    font_name: Option<String>,
    font_size: Option<f64>,
}

impl PageChar {
    fn is_whitespace(&self) -> bool {
        self.text.chars().all(char::is_whitespace)
    }

    fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

#[derive(Debug, Clone)]
struct CharLine {
    chars: Vec<PageChar>,
    min_y: f64,
    max_y: f64,
    rotation: i32,
}

impl CharLine {
    fn new(ch: PageChar) -> Self {
        let min_y = ch.y;
        let max_y = ch.bottom;
        let rotation = ch.rotation;
        Self {
            chars: vec![ch],
            min_y,
            max_y,
            rotation,
        }
    }

    fn accepts(&self, ch: &PageChar, tolerance: f64) -> bool {
        if self.rotation != ch.rotation {
            return false;
        }

        let overlap = (self.max_y.min(ch.bottom) - self.min_y.max(ch.y)).max(0.0);
        if overlap > ch.height.min(self.max_y - self.min_y) * 0.2 {
            return true;
        }

        let center_y = ch.center_y();
        center_y >= self.min_y - tolerance && center_y <= self.max_y + tolerance
    }

    fn push(&mut self, ch: PageChar) {
        self.min_y = self.min_y.min(ch.y);
        self.max_y = self.max_y.max(ch.bottom);
        self.chars.push(ch);
    }
}

#[derive(Debug, Default)]
struct CharRun {
    text: String,
    min_x: f64,
    max_right: f64,
    min_y: f64,
    max_bottom: f64,
    rotation: i32,
    font_name: Option<String>,
    font_size: Option<f64>,
}

impl CharRun {
    fn new(ch: &PageChar) -> Self {
        Self {
            text: ch.text.clone(),
            min_x: ch.x,
            max_right: ch.right,
            min_y: ch.y,
            max_bottom: ch.bottom,
            rotation: ch.rotation,
            font_name: ch.font_name.clone(),
            font_size: ch.font_size,
        }
    }

    fn push(&mut self, ch: &PageChar) {
        self.text.push_str(&ch.text);
        self.min_x = self.min_x.min(ch.x);
        self.max_right = self.max_right.max(ch.right);
        self.min_y = self.min_y.min(ch.y);
        self.max_bottom = self.max_bottom.max(ch.bottom);
    }

    fn into_text_item(self) -> TextItem {
        TextItem {
            str: self.text,
            x: self.min_x,
            y: self.min_y,
            width: Some((self.max_right - self.min_x).max(0.0)),
            height: Some((self.max_bottom - self.min_y).max(0.0)),
            w: Some((self.max_right - self.min_x).max(0.0)),
            h: Some((self.max_bottom - self.min_y).max(0.0)),
            rx: None,
            ry: None,
            r: Some(self.rotation),
            font_name: self.font_name,
            font_size: self.font_size,
            markup: None,
            is_placeholder: None,
            vgap: None,
            from_ocr: Some(false),
        }
    }
}

fn collect_page_chars(page_text: &PdfPageText<'_>, page_height: f32) -> Vec<PageChar> {
    page_text
        .chars()
        .iter()
        .filter_map(|ch| {
            let mut text = ch.unicode_string()?;
            if matches!(text.as_str(), "\r" | "\n") {
                return None;
            }
            if text == "\u{00a0}" {
                text = " ".to_string();
            }
            if text.chars().all(char::is_control) {
                return None;
            }

            let bounds = ch.loose_bounds().or_else(|_| ch.tight_bounds()).ok()?;
            let left = bounds.left().value;
            let right = bounds.right().value;
            let top = bounds.top().value;
            let bottom = bounds.bottom().value;
            let width = (right - left).max(0.0);
            let height = (top - bottom).max(0.0);
            let y_from_top = (page_height - top).max(0.0);
            let rotation = ch.get_rotation_clockwise_degrees().round() as i32;
            let font_size = Some(ch.scaled_font_size().value as f64);
            let font_name = {
                let name = ch.font_name();
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            };

            Some(PageChar {
                text,
                x: left as f64,
                y: y_from_top as f64,
                right: right as f64,
                bottom: y_from_top as f64 + height as f64,
                width: width as f64,
                height: height as f64,
                rotation,
                font_name,
                font_size,
            })
        })
        .collect()
}

fn text_items_from_segments(
    page_text: &PdfPageText<'_>,
    page_height: f32,
    page_chars: &[PageChar],
) -> Vec<TextItem> {
    let mut items = Vec::new();

    for segment in page_text.segments().iter() {
        let raw_segment_text = segment.text();
        if raw_segment_text.trim().is_empty() {
            continue;
        }

        let bounds = segment.bounds();
        let left = bounds.left().value;
        let right = bounds.right().value;
        let top = bounds.top().value;
        let bottom = bounds.bottom().value;
        let width = (right - left).max(0.0);
        let height = (top - bottom).max(0.0);
        let y_from_top = (page_height - top).max(0.0);

        let (rotation, font_name, font_size) = segment
            .chars()
            .ok()
            .and_then(|chars| {
                let ch = chars.iter().next()?;
                let rotation = ch.get_rotation_clockwise_degrees().round() as i32;
                let font_name = {
                    let name = ch.font_name();
                    if name.is_empty() {
                        None
                    } else {
                        Some(name)
                    }
                };
                let font_size = Some(ch.scaled_font_size().value as f64);
                Some((rotation, font_name, font_size))
            })
            .unwrap_or((0, None, None));

        let reconstructed = reconstruct_segment_text(
            page_chars,
            left as f64,
            right as f64,
            y_from_top as f64,
            height as f64,
            rotation,
        );
        let resolved_text = choose_segment_text(&raw_segment_text, reconstructed.as_deref());

        items.push(TextItem {
            str: resolved_text,
            x: left as f64,
            y: y_from_top as f64,
            width: Some(width as f64),
            height: Some(height as f64),
            w: Some(width as f64),
            h: Some(height as f64),
            rx: None,
            ry: None,
            r: Some(rotation),
            font_name,
            font_size,
            markup: None,
            is_placeholder: None,
            vgap: None,
            from_ocr: Some(false),
        });
    }

    items
}

fn reconstruct_segment_text(
    page_chars: &[PageChar],
    left: f64,
    right: f64,
    top: f64,
    height: f64,
    rotation: i32,
) -> Option<String> {
    let bottom = top + height;
    let y_padding = (height * 0.4).max(1.5);
    let x_padding = 1.0_f64;

    let mut chars: Vec<&PageChar> = page_chars
        .iter()
        .filter(|ch| {
            if ch.rotation != rotation {
                return false;
            }

            let center_x = ch.center_x();
            let center_y = ch.center_y();
            center_x >= left - x_padding
                && center_x <= right + x_padding
                && center_y >= top - y_padding
                && center_y <= bottom + y_padding
        })
        .collect();

    if chars.is_empty() {
        return None;
    }

    chars.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(Ordering::Equal))
    });

    let median_width = median(chars.iter().map(|ch| ch.width).filter(|width| *width > 0.0))
        .unwrap_or(4.0)
        .max(1.0);
    let mut output = String::new();
    let mut previous: Option<&PageChar> = None;

    for ch in chars {
        if ch.is_whitespace() {
            if !output.ends_with(' ') && !output.is_empty() {
                output.push(' ');
            }
            previous = None;
            continue;
        }

        if let Some(prev) = previous {
            let gap = (ch.x - prev.right).max(0.0);
            if gap > median_width * 0.55 && !output.ends_with(' ') {
                output.push(' ');
            }
        }

        output.push_str(&ch.text);
        previous = Some(ch);
    }

    if output.trim().is_empty() {
        None
    } else {
        Some(output)
    }
}

fn choose_segment_text(segment_text: &str, reconstructed: Option<&str>) -> String {
    let Some(reconstructed) = reconstructed else {
        return segment_text.to_string();
    };

    if reconstructed.trim().is_empty() {
        return segment_text.to_string();
    }

    let segment_compact = compact_text(segment_text);
    let reconstructed_compact = compact_text(reconstructed);
    let segment_alnum = compact_alphanumeric_text(segment_text);
    let reconstructed_alnum = compact_alphanumeric_text(reconstructed);
    let segment_score = text_quality_score(segment_text);
    let reconstructed_score = text_quality_score(reconstructed);
    let compact_len_ratio = similarity_len_ratio(&segment_compact, &reconstructed_compact);
    let are_variants = segment_compact == reconstructed_compact
        || segment_alnum == reconstructed_alnum
        || is_duplicate_letter_variation(&segment_compact, &reconstructed_compact);

    if are_variants {
        if reconstructed_score + 4 <= segment_score
            || (is_duplicate_letter_variation(&segment_compact, &reconstructed_compact)
                && reconstructed_score + 3 <= segment_score)
        {
            reconstructed.to_string()
        } else {
            segment_text.to_string()
        }
    } else if compact_len_ratio >= 0.93 && reconstructed_score + 6 <= segment_score {
        reconstructed.to_string()
    } else {
        segment_text.to_string()
    }
}

fn compact_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        .collect()
}

fn compact_alphanumeric_text(text: &str) -> String {
    text.chars().filter(|ch| ch.is_alphanumeric()).collect()
}

fn similarity_len_ratio(left: &str, right: &str) -> f64 {
    let left_len = left.chars().count();
    let right_len = right.chars().count();
    if left_len == 0 || right_len == 0 {
        return 0.0;
    }

    let min_len = left_len.min(right_len) as f64;
    let max_len = left_len.max(right_len) as f64;
    min_len / max_len
}

fn is_duplicate_letter_variation(left: &str, right: &str) -> bool {
    if left == right || left.is_empty() || right.is_empty() {
        return false;
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    if left_chars.len().abs_diff(right_chars.len()) != 1 {
        return false;
    }

    let (shorter, longer) = if left_chars.len() < right_chars.len() {
        (&left_chars, &right_chars)
    } else {
        (&right_chars, &left_chars)
    };

    let mut short_index = 0usize;
    let mut long_index = 0usize;
    let mut consumed_extra = false;

    while short_index < shorter.len() && long_index < longer.len() {
        if shorter[short_index] == longer[long_index] {
            short_index += 1;
            long_index += 1;
            continue;
        }

        if consumed_extra {
            return false;
        }

        let extra = longer[long_index];
        let previous = long_index
            .checked_sub(1)
            .and_then(|index| longer.get(index))
            .copied();
        let next = longer.get(long_index + 1).copied();
        let duplicates_neighbor = previous.is_some_and(|candidate| candidate == extra)
            || next.is_some_and(|candidate| candidate == extra);

        if extra.is_alphabetic() && duplicates_neighbor {
            consumed_extra = true;
            long_index += 1;
        } else {
            return false;
        }
    }

    if long_index < longer.len() {
        if consumed_extra {
            return false;
        }
        let extra = longer[long_index];
        let previous = long_index
            .checked_sub(1)
            .and_then(|index| longer.get(index))
            .copied();
        return extra.is_alphabetic() && previous.is_some_and(|candidate| candidate == extra);
    }

    true
}

fn median<I>(values: I) -> Option<f64>
where
    I: Iterator<Item = f64>,
{
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    Some(values[values.len() / 2])
}

fn inject_inter_segment_spaces(mut items: Vec<TextItem>, page_chars: &[PageChar]) -> Vec<TextItem> {
    if items.len() < 2 {
        return items;
    }

    items.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    for index in 1..items.len() {
        let previous = &items[index - 1];
        let current = &items[index];

        if !segments_share_line(previous, current) {
            continue;
        }

        let previous_right = previous.x + previous.w.or(previous.width).unwrap_or(0.0);
        let current_left = current.x;
        let current_height = current.h.or(current.height).unwrap_or(0.0);
        let previous_height = previous.h.or(previous.height).unwrap_or(0.0);
        let max_gap = previous_height
            .max(current_height)
            .max(previous.font_size.unwrap_or(0.0))
            .max(current.font_size.unwrap_or(0.0))
            * 0.9
            + 2.0;
        let gap = (current_left - previous_right).max(0.0);

        if gap > max_gap {
            continue;
        }

        let previous_hints = detect_item_boundary_hints(previous, page_chars);
        let current_hints = detect_item_boundary_hints(current, page_chars);

        if (previous_hints.trailing_space || current_hints.leading_space)
            && !items[index - 1]
                .str
                .chars()
                .last()
                .is_some_and(char::is_whitespace)
            && !items[index]
                .str
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            items[index - 1].str.push(' ');
        }
    }

    items
}

#[derive(Debug, Clone, Copy, Default)]
struct ItemBoundaryHints {
    leading_space: bool,
    trailing_space: bool,
}

fn detect_item_boundary_hints(item: &TextItem, page_chars: &[PageChar]) -> ItemBoundaryHints {
    let item_width = item.w.or(item.width).unwrap_or(0.0);
    let item_height = item.h.or(item.height).unwrap_or(0.0);
    let item_right = item.x + item_width;
    let item_bottom = item.y + item_height;
    let rotation = item.r.unwrap_or(0);

    let chars: Vec<&PageChar> = page_chars
        .iter()
        .filter(|ch| {
            ch.rotation == rotation
                && ch.center_x() >= item.x - 1.0
                && ch.center_x() <= item_right + 1.0
                && ch.center_y() >= item.y - 2.0
                && ch.center_y() <= item_bottom + 2.0
        })
        .collect();

    if chars.is_empty() {
        return ItemBoundaryHints::default();
    }

    let mut visible_chars: Vec<&PageChar> = chars
        .iter()
        .copied()
        .filter(|ch| !ch.is_whitespace())
        .collect();

    if visible_chars.is_empty() {
        return ItemBoundaryHints::default();
    }

    visible_chars.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
    let first_visible = visible_chars[0];
    let last_visible = visible_chars[visible_chars.len() - 1];
    let unit = median(
        visible_chars
            .iter()
            .map(|ch| ch.width)
            .filter(|width| *width > 0.0),
    )
    .unwrap_or(4.0)
    .max(1.0);

    let has_space_before_first = chars.iter().any(|ch| {
        ch.is_whitespace()
            && ch.center_x() >= item.x - 1.0
            && ch.center_x() <= first_visible.center_x() + 0.5
    });
    let has_space_after_last = chars.iter().any(|ch| {
        ch.is_whitespace()
            && ch.center_x() >= last_visible.center_x() - 0.5
            && ch.center_x() <= item_right + 1.0
    });

    ItemBoundaryHints {
        leading_space: has_space_before_first && first_visible.x - item.x > unit * 0.45,
        trailing_space: has_space_after_last && item_right - last_visible.right > unit * 0.45,
    }
}

fn segments_share_line(previous: &TextItem, current: &TextItem) -> bool {
    if previous.r != current.r {
        return false;
    }

    let previous_height = previous.h.or(previous.height).unwrap_or(0.0);
    let current_height = current.h.or(current.height).unwrap_or(0.0);
    let previous_bottom = previous.y + previous_height;
    let current_bottom = current.y + current_height;
    let overlap = (previous_bottom.min(current_bottom) - previous.y.max(current.y)).max(0.0);
    overlap > previous_height.min(current_height) * 0.2
        || (previous.y - current.y).abs() <= previous_height.max(current_height) * 0.6
}

fn refine_problematic_lines(items: Vec<TextItem>, char_lines: &[CharLine]) -> Vec<TextItem> {
    if items.is_empty() {
        return items;
    }

    let lines = cluster_item_lines(&items);
    let mut refined = Vec::new();

    for line in lines {
        let replacement = reconstruct_line_items(&line, char_lines);
        if should_use_line_reconstruction(&line, &replacement) {
            refined.extend(replacement);
        } else {
            refined.extend(line);
        }
    }

    refined
}

fn cluster_item_lines(items: &[TextItem]) -> Vec<Vec<TextItem>> {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    let mut lines: Vec<Vec<TextItem>> = Vec::new();
    for item in sorted {
        let mut placed = false;

        for line in &mut lines {
            if line
                .last()
                .is_some_and(|previous| segments_share_line(previous, &item))
            {
                line.push(item.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            lines.push(vec![item]);
        }
    }

    for line in &mut lines {
        line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
    }

    lines
}

fn reconstruct_line_items(line: &[TextItem], char_lines: &[CharLine]) -> Vec<TextItem> {
    if line.is_empty() {
        return Vec::new();
    }

    let min_x = line.iter().map(|item| item.x).fold(f64::INFINITY, f64::min);
    let max_right = line
        .iter()
        .map(|item| item.x + item.w.or(item.width).unwrap_or(0.0))
        .fold(0.0_f64, f64::max);
    let min_y = line.iter().map(|item| item.y).fold(f64::INFINITY, f64::min);
    let max_bottom = line
        .iter()
        .map(|item| item.y + item.h.or(item.height).unwrap_or(0.0))
        .fold(0.0_f64, f64::max);
    let rotation = line.first().and_then(|item| item.r).unwrap_or(0);
    let line_height = (max_bottom - min_y).max(1.0);
    let line_center_y = min_y + line_height / 2.0;
    let y_tolerance = (line_height * 0.22).max(1.0);

    let mut candidate_lines: Vec<&CharLine> = char_lines
        .iter()
        .filter(|line| {
            line.rotation == rotation
                && char_line_overlaps_x(line, min_x, max_right)
                && (char_line_center_y(line) - line_center_y).abs() <= y_tolerance
        })
        .collect();

    if candidate_lines.is_empty() {
        if let Some(nearest) = char_lines
            .iter()
            .filter(|line| {
                line.rotation == rotation && char_line_overlaps_x(line, min_x, max_right)
            })
            .min_by(|left, right| {
                (char_line_center_y(left) - line_center_y)
                    .abs()
                    .partial_cmp(&(char_line_center_y(right) - line_center_y).abs())
                    .unwrap_or(Ordering::Equal)
            })
        {
            candidate_lines.push(nearest);
        }
    }

    let mut chars: Vec<PageChar> = candidate_lines
        .into_iter()
        .flat_map(|line| line.chars.iter())
        .filter(|ch| ch.center_x() >= min_x - 1.0 && ch.center_x() <= max_right + 1.0)
        .cloned()
        .collect();

    chars.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
    build_line_runs(&chars)
}

fn should_use_line_reconstruction(original: &[TextItem], replacement: &[TextItem]) -> bool {
    if !line_replacement_is_usable(original, replacement) {
        return false;
    }

    let original_score = line_quality_score(original);
    let replacement_score = line_quality_score(replacement);

    replacement_score + 3 <= original_score
        || (original_score >= 8 && replacement_score + 1 < original_score)
        || (original.len() <= 2
            && replacement.len() > original.len()
            && original_score >= 5
            && replacement_score + 1 < original_score)
}

fn line_replacement_is_usable(original: &[TextItem], replacement: &[TextItem]) -> bool {
    if replacement.is_empty() {
        return false;
    }

    let original_compact_len: usize = original
        .iter()
        .map(|item| compact_text(&item.str).chars().count())
        .sum();
    let replacement_compact_len: usize = replacement
        .iter()
        .map(|item| compact_text(&item.str).chars().count())
        .sum();

    if original_compact_len == 0 {
        return false;
    }

    let coverage_ratio = replacement_compact_len as f64 / original_compact_len as f64;
    (0.92..=1.12).contains(&coverage_ratio)
}

fn line_quality_score(line: &[TextItem]) -> i32 {
    let text = render_line_for_scoring(line);
    let mut score = text_quality_score(&text);
    if line.len() <= 2 && alpha_tokens(&text).len() >= 3 {
        score += 1;
    }
    score
}

fn text_quality_score(text: &str) -> i32 {
    let tokens = alpha_tokens(&text);
    let raw_tokens = raw_alpha_tokens(text);
    let mut score = 0_i32;

    for token in &tokens {
        if token_has_mixed_case_weirdness(token) {
            score += 3;
        }
    }

    for window in tokens.windows(3) {
        let previous_len = window[0].chars().count();
        let current_len = window[1].chars().count();
        let next_len = window[2].chars().count();
        if current_len <= 2 && previous_len >= 4 && next_len >= 4 {
            score += 2;
        }
        if current_len == 1 && previous_len >= 3 && next_len >= 3 {
            score += 3;
        }
    }

    for window in raw_tokens.windows(3) {
        let previous_len = window[0].chars().count();
        let current_len = window[1].chars().count();
        let next_len = window[2].chars().count();
        if current_len == 1 && previous_len >= 3 && next_len >= 3 {
            score += 4;
        } else if current_len <= 2 && previous_len >= 4 && next_len >= 4 {
            score += 2;
        }
    }

    let isolated_letters = raw_tokens
        .iter()
        .filter(|token| token.chars().count() == 1)
        .count();
    if isolated_letters >= 2 {
        score += isolated_letters as i32;
    }

    score
}

fn normalize_text_for_scoring(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut normalized = String::with_capacity(text.len() + 8);

    for (index, ch) in chars.iter().enumerate() {
        if index > 0 {
            let previous = chars[index - 1];
            let next = chars.get(index + 1).copied();
            let upper_run_len = chars[index..]
                .iter()
                .take_while(|candidate| candidate.is_uppercase())
                .count();
            let split_before_current = previous.is_lowercase()
                && ch.is_uppercase()
                && (next.is_some_and(|candidate| candidate.is_lowercase()) || upper_run_len >= 2);

            if split_before_current && !normalized.ends_with(' ') {
                normalized.push(' ');
            }
        }

        normalized.push(*ch);
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_line_for_scoring(line: &[TextItem]) -> String {
    let mut sorted = line.to_vec();
    sorted.sort_by(|left, right| left.x.partial_cmp(&right.x).unwrap_or(Ordering::Equal));

    let mut rendered = String::new();
    let mut previous: Option<&TextItem> = None;

    for item in &sorted {
        let fragment = normalize_text_for_scoring(&item.str);
        if fragment.trim().is_empty() {
            continue;
        }

        if let Some(previous_item) = previous {
            let spaces = rendered_spaces_between_items(previous_item, item);
            if spaces > 0
                && !rendered.chars().last().is_some_and(char::is_whitespace)
                && !fragment.chars().next().is_some_and(char::is_whitespace)
            {
                rendered.push_str(&" ".repeat(spaces));
            }
        }

        if previous.is_none() {
            rendered.push_str(fragment.trim_start());
        } else {
            rendered.push_str(fragment.as_str());
        }

        previous = Some(item);
    }

    rendered.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn rendered_spaces_between_items(previous: &TextItem, current: &TextItem) -> usize {
    let previous_width = previous.w.or(previous.width).unwrap_or(0.0).max(1.0);
    let current_width = current.w.or(current.width).unwrap_or(0.0).max(1.0);
    let previous_chars = compact_text(&previous.str).chars().count().max(1) as f64;
    let current_chars = compact_text(&current.str).chars().count().max(1) as f64;
    let unit = (previous_width / previous_chars)
        .max(current_width / current_chars)
        .max(1.0);
    let gap = (current.x - previous.x - previous_width).max(0.0);

    if gap <= unit * 0.55 {
        0
    } else {
        1
    }
}

fn alpha_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_alphabetic())
                .to_string()
        })
        .filter(|token| token.chars().count() >= 2)
        .collect()
}

fn raw_alpha_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_alphabetic())
                .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn token_has_mixed_case_weirdness(token: &str) -> bool {
    let mut letters = token.chars().filter(|ch| ch.is_alphabetic());
    let Some(first) = letters.next() else {
        return false;
    };
    let rest: Vec<char> = letters.collect();
    if rest.is_empty() {
        return false;
    }

    let is_title_case = first.is_uppercase() && rest.iter().all(|ch| ch.is_lowercase());
    let is_all_upper = first.is_uppercase() && rest.iter().all(|ch| ch.is_uppercase());
    let is_all_lower = first.is_lowercase() && rest.iter().all(|ch| ch.is_lowercase());
    if is_title_case || is_all_upper || is_all_lower {
        return false;
    }

    token.chars().any(|ch| ch.is_uppercase()) && token.chars().any(|ch| ch.is_lowercase())
}

fn text_items_from_char_lines(lines: &[CharLine]) -> Vec<TextItem> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut items = Vec::new();
    for line in lines.iter().cloned() {
        items.extend(build_line_runs(&line.chars));
    }

    items
}

fn cluster_page_char_lines(page_chars: &[PageChar]) -> Vec<CharLine> {
    if page_chars.is_empty() {
        return Vec::new();
    }

    let median_height = median(
        page_chars
            .iter()
            .filter(|ch| !ch.is_whitespace())
            .map(|ch| ch.height)
            .filter(|height| *height > 0.0),
    )
    .unwrap_or(10.0);
    let y_tolerance = (median_height * 0.55).max(3.0);

    let mut chars = page_chars.to_vec();
    chars.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    let mut lines: Vec<CharLine> = Vec::new();

    for ch in chars {
        let mut placed = false;

        for line in &mut lines {
            if line.accepts(&ch, y_tolerance) {
                line.push(ch.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            lines.push(CharLine::new(ch));
        }
    }

    lines.sort_by(|a, b| a.min_y.partial_cmp(&b.min_y).unwrap_or(Ordering::Equal));

    for line in &mut lines {
        line.chars
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
    }

    lines
}

fn char_line_center_y(line: &CharLine) -> f64 {
    line.min_y + (line.max_y - line.min_y) / 2.0
}

fn char_line_overlaps_x(line: &CharLine, min_x: f64, max_right: f64) -> bool {
    line.chars
        .iter()
        .any(|ch| ch.center_x() >= min_x - 1.0 && ch.center_x() <= max_right + 1.0)
}

fn build_line_runs(chars: &[PageChar]) -> Vec<TextItem> {
    if chars.is_empty() {
        return Vec::new();
    }

    let median_width = median(
        chars
            .iter()
            .filter(|ch| !ch.is_whitespace())
            .map(|ch| ch.width)
            .filter(|width| *width > 0.0),
    )
    .unwrap_or(4.0)
    .max(1.0);

    let mut runs = Vec::new();
    let mut current: Option<CharRun> = None;
    let mut previous_non_space: Option<&PageChar> = None;

    for ch in chars {
        if ch.is_whitespace() {
            if let Some(run) = current.as_mut() {
                if !run.text.ends_with(' ') {
                    run.text.push(' ');
                }
            }
            if let Some(run) = current.take() {
                runs.push(run.into_text_item());
            }
            previous_non_space = None;
            continue;
        }

        let starts_new_run = previous_non_space.is_some_and(|prev| {
            let gap = (ch.x - prev.right).max(0.0);
            let unit = prev.width.max(ch.width).max(median_width);
            gap > unit * 1.35
        });

        if starts_new_run {
            if let Some(run) = current.take() {
                runs.push(run.into_text_item());
            }
        }

        if let Some(run) = current.as_mut() {
            if let Some(prev) = previous_non_space {
                let gap = (ch.x - prev.right).max(0.0);
                let unit = prev.width.max(ch.width).max(median_width);
                if gap > unit * 0.55 && !run.text.ends_with(' ') {
                    run.text.push(' ');
                }
            }
            run.push(ch);
        } else {
            current = Some(CharRun::new(ch));
        }

        previous_non_space = Some(ch);
    }

    if let Some(run) = current {
        runs.push(run.into_text_item());
    }

    runs
}

fn text_item_from_char(ch: &PdfPageTextChar<'_>, page_height: f32) -> Option<TextItem> {
    let text = ch.unicode_string()?;
    let bounds = ch.loose_bounds().or_else(|_| ch.tight_bounds()).ok()?;
    let left = bounds.left().value;
    let right = bounds.right().value;
    let top = bounds.top().value;
    let bottom = bounds.bottom().value;
    let width = (right - left).max(0.0);
    let height = (top - bottom).max(0.0);
    let y_from_top = (page_height - top).max(0.0);
    let rotation = ch.get_rotation_clockwise_degrees().round() as i32;
    let font_size = Some(ch.scaled_font_size().value as f64);
    let font_name = {
        let name = ch.font_name();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    };

    Some(TextItem {
        str: text,
        x: left as f64,
        y: y_from_top as f64,
        width: Some(width as f64),
        height: Some(height as f64),
        w: Some(width as f64),
        h: Some(height as f64),
        rx: None,
        ry: None,
        r: Some(rotation),
        font_name,
        font_size,
        markup: None,
        is_placeholder: None,
        vgap: None,
        from_ocr: Some(false),
    })
}
