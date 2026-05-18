use ctox_pdf_parse::{
    load_fixture_corpus, load_page_fixture, parse_pdf_path, resolve_corpus_pdf_root,
    resolve_fixture_pdf_path, run_page_fixture, LiteParseConfigOverrides, OutputFormat,
    PageFixtureEvaluation,
};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "usage: eval_fixture <fixture.json|corpus.json> [--pdf-root <dir>] [--fixture <id>] [--show-text]"
        )
    })?;

    let mut pdf_root: Option<PathBuf> = None;
    let mut fixture_filter: Option<String> = None;
    let mut show_text = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pdf-root" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("missing value for --pdf-root"))?;
                pdf_root = Some(PathBuf::from(value));
            }
            "--fixture" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("missing value for --fixture"))?;
                fixture_filter = Some(value);
            }
            "--show-text" => show_text = true,
            _ => return Err(anyhow::anyhow!("unknown argument: {arg}")),
        }
    }

    let input_path = PathBuf::from(first);
    let evaluations = if is_corpus_manifest(&input_path)? {
        run_corpus(&input_path, pdf_root.as_deref(), fixture_filter.as_deref())?
    } else {
        vec![run_page_fixture(&input_path, pdf_root.as_deref())?]
    };

    if evaluations.is_empty() {
        return Err(anyhow::anyhow!("no fixtures matched the requested scope"));
    }

    println!("{}", serde_json::to_string_pretty(&evaluations)?);

    if show_text {
        print_fixture_texts(&input_path, pdf_root.as_deref(), fixture_filter.as_deref())?;
    }

    if evaluations.iter().all(|evaluation| evaluation.passed) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("one or more fixtures failed"))
    }
}

fn run_corpus(
    corpus_path: &Path,
    pdf_root: Option<&Path>,
    fixture_filter: Option<&str>,
) -> anyhow::Result<Vec<PageFixtureEvaluation>> {
    let corpus = load_fixture_corpus(corpus_path)?;
    let corpus_dir = corpus_path.parent().unwrap_or_else(|| Path::new("."));
    let effective_pdf_root = resolve_corpus_pdf_root(corpus_path, &corpus, pdf_root);
    let mut evaluations = Vec::new();

    for fixture_rel_path in corpus.fixtures {
        let fixture_path = corpus_dir.join(&fixture_rel_path);
        if let Some(filter) = fixture_filter {
            let content = std::fs::read_to_string(&fixture_path)?;
            let parsed: serde_json::Value = serde_json::from_str(&content)?;
            let fixture_id = parsed
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if fixture_id != filter {
                continue;
            }
        }

        evaluations.push(run_page_fixture(
            &fixture_path,
            effective_pdf_root.as_deref(),
        )?);
    }

    Ok(evaluations)
}

fn is_corpus_manifest(path: &Path) -> anyhow::Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let parsed: serde_json::Value = serde_json::from_str(&content)?;
    Ok(parsed.get("fixtures").is_some_and(|value| value.is_array()))
}

fn print_fixture_texts(
    input_path: &Path,
    pdf_root: Option<&Path>,
    fixture_filter: Option<&str>,
) -> anyhow::Result<()> {
    if is_corpus_manifest(input_path)? {
        let corpus = load_fixture_corpus(input_path)?;
        let corpus_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
        let effective_pdf_root = resolve_corpus_pdf_root(input_path, &corpus, pdf_root);

        for fixture_rel_path in corpus.fixtures {
            let fixture_path = corpus_dir.join(&fixture_rel_path);
            if let Some(filter) = fixture_filter {
                let fixture = load_page_fixture(&fixture_path)?;
                if fixture.id != filter {
                    continue;
                }
                print_fixture_text(&fixture_path, &fixture, effective_pdf_root.as_deref())?;
            } else {
                let fixture = load_page_fixture(&fixture_path)?;
                print_fixture_text(&fixture_path, &fixture, effective_pdf_root.as_deref())?;
            }
        }
    } else {
        let fixture = load_page_fixture(input_path)?;
        print_fixture_text(input_path, &fixture, pdf_root)?;
    }

    Ok(())
}

fn print_fixture_text(
    fixture_path: &Path,
    fixture: &ctox_pdf_parse::PageFixture,
    pdf_root: Option<&Path>,
) -> anyhow::Result<()> {
    let pdf_path = resolve_fixture_pdf_path(fixture_path, fixture, pdf_root);
    let parse_result = parse_pdf_path(
        &pdf_path.to_string_lossy(),
        LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            precise_bounding_box: Some(false),
            output_format: Some(OutputFormat::Text),
            target_pages: Some(Some(fixture.page.to_string())),
            max_pages: Some(fixture.page),
            ..Default::default()
        },
    )?;

    let page = parse_result
        .pages
        .into_iter()
        .find(|page| page.page_num == fixture.page)
        .ok_or_else(|| {
            anyhow::anyhow!("missing page {} for fixture {}", fixture.page, fixture.id)
        })?;

    println!(
        "\n--- fixture={} page={} pdf={} ---\n{}",
        fixture.id,
        fixture.page,
        pdf_path.display(),
        page.text
    );

    Ok(())
}
