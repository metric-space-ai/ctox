use ctox_pdf_parse::engines::pdf::interface::PdfEngine;
use ctox_pdf_parse::processing::bbox::build_projection_boxes;
use ctox_pdf_parse::processing::grid_projection::project_to_grid;
use ctox_pdf_parse::{ForwardAnchors, LiteParseConfig, PdfiumBackend};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_grid <pdf-path> <page-number>"))?;
    let page_num = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_grid <pdf-path> <page-number>"))?
        .parse::<usize>()?;

    let backend = PdfiumBackend::new()?;
    let doc = backend.load_document_path(&path, None)?;
    let page = backend.extract_page(&doc, page_num)?;
    let boxes = build_projection_boxes(&page.text_items);
    let result = project_to_grid(
        &LiteParseConfig::default(),
        &page,
        boxes,
        ForwardAnchors::default(),
        1,
    );

    println!("page={}", page_num);
    for (index, line) in result.text.lines().enumerate() {
        println!("{index:>3}: {line}");
    }

    Ok(())
}
