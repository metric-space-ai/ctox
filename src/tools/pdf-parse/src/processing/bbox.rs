use crate::core::types::{BoundingBox, Coordinates, ProjectionTextBox, TextItem};

const OCR_OVERLAP_THRESHOLD: f64 = 0.5;

pub fn get_overlap_area(a: &Coordinates, b: &Coordinates) -> f64 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);

    let w = (x2 - x1).max(0.0);
    let h = (y2 - y1).max(0.0);

    w * h
}

pub fn filter_ocr_blocks_overlapping_with_text(
    ocr_blocks: &[Coordinates],
    text_items: &[TextItem],
) -> Vec<Coordinates> {
    ocr_blocks
        .iter()
        .filter(|ocr| {
            let ocr_area = (ocr.w * ocr.h).max(1.0);

            for item in text_items {
                let text_box = Coordinates {
                    x: item.x,
                    y: item.y,
                    w: item.w.or(item.width).unwrap_or(0.0),
                    h: item.h.or(item.height).unwrap_or(0.0),
                };

                let overlap = get_overlap_area(ocr, &text_box);
                let text_area = (text_box.w * text_box.h).max(1.0);

                if overlap / ocr_area >= OCR_OVERLAP_THRESHOLD
                    || overlap / text_area >= OCR_OVERLAP_THRESHOLD
                {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}

pub fn build_projection_boxes(text_items: &[TextItem]) -> Vec<ProjectionTextBox> {
    text_items
        .iter()
        .map(|item| {
            let w = item.w.or(item.width).unwrap_or(0.0);
            let h = item.h.or(item.height).unwrap_or(0.0);

            ProjectionTextBox {
                str: item.str.clone(),
                x: item.x.round(),
                y: item.y.round(),
                w: w.round(),
                h: h.round(),
                rx: item.rx,
                ry: item.ry,
                r: item.r,
                str_length: item.str.chars().count(),
                markup: item.markup.clone(),
                page_bbox: Some(Coordinates {
                    x: item.x,
                    y: item.y,
                    w,
                    h,
                }),
                vgap: item.vgap,
                is_placeholder: item.is_placeholder,
                from_ocr: item.from_ocr,
                ..Default::default()
            }
        })
        .collect()
}

pub fn build_bounding_boxes(text_items: &[TextItem]) -> Vec<BoundingBox> {
    text_items
        .iter()
        .filter(|item| !item.str.trim().is_empty())
        .map(|item| BoundingBox {
            x1: item.x,
            y1: item.y,
            x2: item.x + item.w.or(item.width).unwrap_or(0.0),
            y2: item.y + item.h.or(item.height).unwrap_or(0.0),
        })
        .collect()
}
