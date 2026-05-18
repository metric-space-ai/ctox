use ctox_pdf_parse::{parse_pdf_path, LiteParseConfigOverrides, OutputFormat};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect <pdf-path> [page-number]"))?;
    let page_filter = args.next().and_then(|value| value.parse::<usize>().ok());

    let result = parse_pdf_path(
        &path,
        LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            output_format: Some(OutputFormat::Text),
            precise_bounding_box: Some(false),
            ..Default::default()
        },
    )?;

    println!("total_pages={}", result.total_pages);
    for page in result.pages {
        if page_filter.is_some_and(|expected| expected != page.page_num) {
            continue;
        }
        println!("--- page {} ---", page.page_num);
        println!("{}", page.text);
    }

    Ok(())
}
