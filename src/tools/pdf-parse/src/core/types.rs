use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Coordinates {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BoundingBox {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Markup {
    pub highlight: Option<String>,
    pub underline: bool,
    pub squiggly: bool,
    pub strikeout: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TextItem {
    pub str: String,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub r: Option<i32>,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub markup: Option<Markup>,
    pub is_placeholder: Option<bool>,
    pub vgap: Option<f64>,
    pub from_ocr: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageImage {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub image_type: Option<String>,
    pub scale_factor: Option<f64>,
    pub original_orientation_angle: Option<i32>,
    pub coords: Option<Coordinates>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ParsedPage {
    pub page_num: usize,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub text_items: Vec<TextItem>,
    pub images: Vec<PageImage>,
    pub bounding_boxes: Option<Vec<BoundingBox>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ParseResult {
    pub total_pages: usize,
    pub pages: Vec<ParsedPage>,
    pub text: String,
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectionTextBox {
    pub str: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub r: Option<i32>,
    pub str_length: usize,
    pub markup: Option<Markup>,
    pub page_bbox: Option<Coordinates>,
    pub vgap: Option<f64>,
    pub is_placeholder: Option<bool>,
    pub from_ocr: Option<bool>,
    pub snap: Option<String>,
    pub left_anchor: Option<i32>,
    pub right_anchor: Option<i32>,
    pub center_anchor: Option<i32>,
    pub is_dup: Option<bool>,
    pub rendered: Option<bool>,
    pub is_margin_line_number: Option<bool>,
    pub should_space: Option<bool>,
    pub force_unsnapped: Option<bool>,
    pub rotated: Option<bool>,
    pub d: Option<f64>,
}

impl ProjectionTextBox {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForwardAnchors {
    pub forward_anchor_left: BTreeMap<String, i32>,
    pub forward_anchor_right: BTreeMap<String, i32>,
    pub forward_anchor_center: BTreeMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectToGridResult {
    pub text: String,
    pub prev_anchors: ForwardAnchors,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PdfDocumentHandle {
    pub source: String,
    pub num_pages: usize,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScreenshotResult {
    pub page_num: usize,
    pub width: f64,
    pub height: f64,
    pub image_buffer: Vec<u8>,
}
