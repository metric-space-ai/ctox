use ctox_pdf_parse::{parse_pdf_path, LiteParseConfigOverrides, OutputFormat, PageFixture};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let pdf_path = args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "usage: suggest_fixture <pdf-path> <page> [--id <fixture-id>] [--pdf-root <dir>] [--description <text>] [--expected-lines <n>]"
        )
    })?;
    let page = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing page number"))?
        .parse::<usize>()?;

    let mut fixture_id: Option<String> = None;
    let mut pdf_root: Option<PathBuf> = None;
    let mut description: Option<String> = None;
    let mut expected_line_count = 4usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--id" => {
                fixture_id = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --id"))?,
                );
            }
            "--pdf-root" => {
                pdf_root =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("missing value for --pdf-root")
                    })?));
            }
            "--description" => {
                description = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --description"))?,
                );
            }
            "--expected-lines" => {
                expected_line_count = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("missing value for --expected-lines"))?
                    .parse::<usize>()?;
            }
            _ => return Err(anyhow::anyhow!("unknown argument: {arg}")),
        }
    }

    let parse_result = parse_pdf_path(
        &pdf_path,
        LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            precise_bounding_box: Some(false),
            output_format: Some(OutputFormat::Text),
            target_pages: Some(Some(page.to_string())),
            max_pages: Some(page),
            ..Default::default()
        },
    )?;

    let parsed_page = parse_result
        .pages
        .into_iter()
        .find(|candidate| candidate.page_num == page)
        .ok_or_else(|| anyhow::anyhow!("page {page} not found in parse result"))?;

    let expected_lines: Vec<String> = parsed_page
        .text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .take(expected_line_count)
        .map(str::to_string)
        .collect();

    let pdf_field = normalize_pdf_path(&pdf_path, pdf_root.as_deref());
    let fixture = PageFixture {
        id: fixture_id.unwrap_or_else(|| default_fixture_id(&pdf_field, page)),
        pdf: pdf_field,
        page,
        description,
        expected_lines,
        required_patterns: Vec::new(),
        ordered_phrases: Vec::new(),
        same_line_groups: Vec::new(),
        separate_line_groups: Vec::new(),
        forbidden_patterns: Vec::new(),
        allowed_missing_lines: 0,
    };

    println!("{}", serde_json::to_string_pretty(&fixture)?);
    Ok(())
}

fn normalize_pdf_path(pdf_path: &str, pdf_root: Option<&Path>) -> String {
    let pdf_path = Path::new(pdf_path);
    if let Some(root) = pdf_root {
        if let Ok(relative) = pdf_path.strip_prefix(root) {
            return relative.to_string_lossy().into_owned();
        }
    }

    pdf_path.to_string_lossy().into_owned()
}

fn default_fixture_id(pdf: &str, page: usize) -> String {
    let stem = Path::new(pdf)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("fixture");
    let slug = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    format!("{slug}_p{page}")
}
