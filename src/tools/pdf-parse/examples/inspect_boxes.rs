use ctox_pdf_parse::engines::pdf::interface::PdfEngine;
use ctox_pdf_parse::processing::bbox::build_projection_boxes;
use ctox_pdf_parse::processing::grid_projection::{bbox_to_lines, handle_rotation_reading_order};
use ctox_pdf_parse::PdfiumBackend;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_boxes <pdf-path> <page-number>"))?;
    let page_num = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_boxes <pdf-path> <page-number>"))?
        .parse::<usize>()?;

    let backend = PdfiumBackend::new()?;
    let doc = backend.load_document_path(&path, None)?;
    let page = backend.extract_page(&doc, page_num)?;
    let mut boxes = build_projection_boxes(&page.text_items);
    boxes = handle_rotation_reading_order(&boxes);

    let widths: Vec<f64> = boxes
        .iter()
        .map(|bbox| bbox.w)
        .filter(|w| *w > 0.0)
        .collect();
    let heights: Vec<f64> = boxes
        .iter()
        .map(|bbox| bbox.h)
        .filter(|h| *h > 0.0)
        .collect();
    let median_width = median_or(&widths, 10.0);
    let median_height = median_or(&heights, 12.0);
    let lines = bbox_to_lines(&boxes, median_width, median_height, Some(page.width));

    println!(
        "page={} width={} height={}",
        page.page_num, page.width, page.height
    );
    for (line_index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            println!("line {}: <blank>", line_index);
            continue;
        }
        println!("line {}:", line_index);
        for bbox in line {
            println!(
                "  x={:>6.1} w={:>6.1} h={:>5.1} text={:?}",
                bbox.x, bbox.w, bbox.h, bbox.str
            );
        }
    }

    Ok(())
}

fn median_or(values: &[f64], fallback: f64) -> f64 {
    if values.is_empty() {
        return fallback;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[sorted.len() / 2]
}
