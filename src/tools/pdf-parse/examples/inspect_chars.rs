use pdfium_auto::bind_pdfium_silent;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_chars <pdf-path> <page-number>"))?;
    let page_num = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_chars <pdf-path> <page-number>"))?
        .parse::<usize>()?;

    let pdfium = bind_pdfium_silent()?;
    let document = pdfium.load_pdf_from_file(&path, None)?;
    let page = document.pages().get((page_num.saturating_sub(1)) as u16)?;
    let text = page.text()?;

    println!(
        "page={} width={} height={}",
        page_num,
        page.width().value,
        page.height().value
    );
    for ch in text.chars().iter() {
        let value = ch.unicode_string().unwrap_or_default();
        let bounds = ch.loose_bounds().or_else(|_| ch.tight_bounds());
        if let Ok(bounds) = bounds {
            println!(
                "idx={:>4} x={:>7.2} y={:>7.2} r={:>7.2} w={:>6.2} text={:?}",
                ch.index(),
                bounds.left().value,
                bounds.bottom().value,
                bounds.right().value,
                (bounds.right().value - bounds.left().value).max(0.0),
                value
            );
        } else {
            println!("idx={:>4} text={:?}", ch.index(), value);
        }
    }

    Ok(())
}
