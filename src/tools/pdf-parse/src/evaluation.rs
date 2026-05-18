#[cfg(feature = "pdfium")]
use crate::core::config::{LiteParseConfigOverrides, OutputFormat};
#[cfg(feature = "pdfium")]
use crate::{parse_pdf_path, LiteParseError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageFixture {
    pub id: String,
    pub pdf: String,
    pub page: usize,
    pub description: Option<String>,
    #[serde(default)]
    pub expected_lines: Vec<String>,
    #[serde(default)]
    pub required_patterns: Vec<String>,
    #[serde(default)]
    pub ordered_phrases: Vec<String>,
    #[serde(default)]
    pub same_line_groups: Vec<Vec<String>>,
    #[serde(default)]
    pub separate_line_groups: Vec<Vec<String>>,
    #[serde(default)]
    pub forbidden_patterns: Vec<String>,
    #[serde(default)]
    pub allowed_missing_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageFixtureCorpus {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub pdf_root: Option<String>,
    #[serde(default)]
    pub fixtures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PdfSampleAsset {
    pub id: String,
    pub path: String,
    pub download_url: String,
    pub source_name: String,
    pub source_url: String,
    pub description: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PdfSampleManifest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub samples: Vec<PdfSampleAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LineCheck {
    pub expected: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PhraseCheck {
    pub phrase: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GroupCheck {
    pub phrases: Vec<String>,
    pub matched: bool,
    pub line_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageFixtureEvaluation {
    pub fixture_id: String,
    pub pdf_path: String,
    pub page: usize,
    pub passed: bool,
    pub actual_line_count: usize,
    pub expected_line_hits: usize,
    pub expected_line_total: usize,
    pub required_pattern_hits: usize,
    pub required_pattern_total: usize,
    pub ordered_phrase_hits: usize,
    pub ordered_phrase_total: usize,
    pub same_line_hits: usize,
    pub same_line_total: usize,
    pub separate_line_hits: usize,
    pub separate_line_total: usize,
    pub forbidden_violations: usize,
    pub line_checks: Vec<LineCheck>,
    pub required_pattern_checks: Vec<PhraseCheck>,
    pub ordered_phrase_checks: Vec<PhraseCheck>,
    pub same_line_checks: Vec<GroupCheck>,
    pub separate_line_checks: Vec<GroupCheck>,
    pub forbidden_pattern_checks: Vec<PhraseCheck>,
}

pub fn load_page_fixture(path: impl AsRef<Path>) -> Result<PageFixture, FixtureError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_fixture_corpus(path: impl AsRef<Path>) -> Result<PageFixtureCorpus, FixtureError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_sample_manifest(path: impl AsRef<Path>) -> Result<PdfSampleManifest, FixtureError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn resolve_corpus_pdf_root(
    corpus_path: impl AsRef<Path>,
    corpus: &PageFixtureCorpus,
    cli_pdf_root: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(root) = cli_pdf_root {
        return Some(root.to_path_buf());
    }

    let pdf_root = corpus.pdf_root.as_deref()?;
    let pdf_root = Path::new(pdf_root);
    if pdf_root.is_absolute() {
        return Some(pdf_root.to_path_buf());
    }

    Some(
        corpus_path
            .as_ref()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(pdf_root),
    )
}

pub fn resolve_fixture_pdf_path(
    fixture_path: impl AsRef<Path>,
    fixture: &PageFixture,
    pdf_root: Option<&Path>,
) -> PathBuf {
    let pdf_path = Path::new(&fixture.pdf);
    if pdf_path.is_absolute() {
        return pdf_path.to_path_buf();
    }

    if let Some(root) = pdf_root {
        return root.join(pdf_path);
    }

    fixture_path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(pdf_path)
}

pub fn resolve_sample_root(
    manifest_path: impl AsRef<Path>,
    manifest: &PdfSampleManifest,
    cli_root: Option<&Path>,
) -> PathBuf {
    if let Some(root) = cli_root {
        return root.to_path_buf();
    }

    if let Some(root) = manifest.root.as_deref() {
        let root = Path::new(root);
        if root.is_absolute() {
            return root.to_path_buf();
        }

        return manifest_path
            .as_ref()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(root);
    }

    manifest_path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub fn resolve_sample_pdf_path(
    manifest_path: impl AsRef<Path>,
    manifest: &PdfSampleManifest,
    sample: &PdfSampleAsset,
    cli_root: Option<&Path>,
) -> PathBuf {
    let sample_path = Path::new(&sample.path);
    if sample_path.is_absolute() {
        return sample_path.to_path_buf();
    }

    resolve_sample_root(manifest_path, manifest, cli_root).join(sample_path)
}

pub fn evaluate_page_fixture_text(fixture: &PageFixture, text: &str) -> PageFixtureEvaluation {
    let normalized_lines: Vec<String> = text
        .lines()
        .map(normalize_line_for_match)
        .filter(|line| !line.is_empty())
        .collect();
    let joined_text = normalized_lines.join("\n");
    let flattened_text = normalized_lines.join(" ");

    let line_checks: Vec<LineCheck> = fixture
        .expected_lines
        .iter()
        .map(|expected| {
            let expected_normalized = normalize_line_for_match(expected);
            let matched = normalized_lines
                .iter()
                .any(|line| line == &expected_normalized);
            LineCheck {
                expected: expected.clone(),
                matched,
            }
        })
        .collect();

    let required_pattern_checks: Vec<PhraseCheck> = fixture
        .required_patterns
        .iter()
        .map(|pattern| PhraseCheck {
            phrase: pattern.clone(),
            matched: flattened_text.contains(&normalize_inline_text(pattern)),
        })
        .collect();

    let ordered_phrase_checks = evaluate_ordered_phrases(&fixture.ordered_phrases, &flattened_text);

    let same_line_checks: Vec<GroupCheck> = fixture
        .same_line_groups
        .iter()
        .map(|group| evaluate_same_line_group(group, &normalized_lines))
        .collect();

    let separate_line_checks: Vec<GroupCheck> = fixture
        .separate_line_groups
        .iter()
        .map(|group| evaluate_separate_line_group(group, &normalized_lines))
        .collect();

    let forbidden_pattern_checks: Vec<PhraseCheck> = fixture
        .forbidden_patterns
        .iter()
        .map(|pattern| PhraseCheck {
            phrase: pattern.clone(),
            matched: joined_text.contains(&normalize_inline_text(pattern)),
        })
        .collect();

    let expected_line_hits = line_checks.iter().filter(|check| check.matched).count();
    let required_pattern_hits = required_pattern_checks
        .iter()
        .filter(|check| check.matched)
        .count();
    let ordered_phrase_hits = ordered_phrase_checks
        .iter()
        .filter(|check| check.matched)
        .count();
    let same_line_hits = same_line_checks
        .iter()
        .filter(|check| check.matched)
        .count();
    let separate_line_hits = separate_line_checks
        .iter()
        .filter(|check| check.matched)
        .count();
    let forbidden_violations = forbidden_pattern_checks
        .iter()
        .filter(|check| check.matched)
        .count();

    let missing_lines = fixture
        .expected_lines
        .len()
        .saturating_sub(expected_line_hits);
    let passed = missing_lines <= fixture.allowed_missing_lines
        && required_pattern_hits == fixture.required_patterns.len()
        && ordered_phrase_hits == fixture.ordered_phrases.len()
        && same_line_hits == fixture.same_line_groups.len()
        && separate_line_hits == fixture.separate_line_groups.len()
        && forbidden_violations == 0;

    PageFixtureEvaluation {
        fixture_id: fixture.id.clone(),
        pdf_path: fixture.pdf.clone(),
        page: fixture.page,
        passed,
        actual_line_count: normalized_lines.len(),
        expected_line_hits,
        expected_line_total: fixture.expected_lines.len(),
        required_pattern_hits,
        required_pattern_total: fixture.required_patterns.len(),
        ordered_phrase_hits,
        ordered_phrase_total: fixture.ordered_phrases.len(),
        same_line_hits,
        same_line_total: fixture.same_line_groups.len(),
        separate_line_hits,
        separate_line_total: fixture.separate_line_groups.len(),
        forbidden_violations,
        line_checks,
        required_pattern_checks,
        ordered_phrase_checks,
        same_line_checks,
        separate_line_checks,
        forbidden_pattern_checks,
    }
}

#[cfg(feature = "pdfium")]
pub fn run_page_fixture(
    fixture_path: impl AsRef<Path>,
    pdf_root: Option<&Path>,
) -> Result<PageFixtureEvaluation, FixtureRunError> {
    let fixture = load_page_fixture(&fixture_path)?;
    let pdf_path = resolve_fixture_pdf_path(&fixture_path, &fixture, pdf_root);
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
        .ok_or_else(|| FixtureRunError::MissingPage {
            fixture_id: fixture.id.clone(),
            page: fixture.page,
        })?;

    let mut evaluation = evaluate_page_fixture_text(&fixture, &page.text);
    evaluation.pdf_path = pdf_path.to_string_lossy().into_owned();
    Ok(evaluation)
}

fn evaluate_ordered_phrases(phrases: &[String], flattened_text: &str) -> Vec<PhraseCheck> {
    let mut checks = Vec::with_capacity(phrases.len());
    let mut cursor = 0usize;

    for phrase in phrases {
        let phrase_normalized = normalize_inline_text(phrase);
        let haystack = &flattened_text[cursor..];
        if let Some(relative) = haystack.find(&phrase_normalized) {
            cursor += relative + phrase_normalized.len();
            checks.push(PhraseCheck {
                phrase: phrase.clone(),
                matched: true,
            });
        } else {
            checks.push(PhraseCheck {
                phrase: phrase.clone(),
                matched: false,
            });
        }
    }

    checks
}

fn evaluate_same_line_group(group: &[String], lines: &[String]) -> GroupCheck {
    let normalized_group: Vec<String> = group
        .iter()
        .map(|phrase| normalize_inline_text(phrase))
        .collect();
    for (index, line) in lines.iter().enumerate() {
        if normalized_group.iter().all(|phrase| line.contains(phrase)) {
            return GroupCheck {
                phrases: group.to_vec(),
                matched: true,
                line_index: Some(index),
            };
        }
    }

    GroupCheck {
        phrases: group.to_vec(),
        matched: false,
        line_index: None,
    }
}

fn evaluate_separate_line_group(group: &[String], lines: &[String]) -> GroupCheck {
    let normalized_group: Vec<String> = group
        .iter()
        .map(|phrase| normalize_inline_text(phrase))
        .collect();
    for (index, line) in lines.iter().enumerate() {
        if normalized_group.iter().all(|phrase| line.contains(phrase)) {
            return GroupCheck {
                phrases: group.to_vec(),
                matched: false,
                line_index: Some(index),
            };
        }
    }

    GroupCheck {
        phrases: group.to_vec(),
        matched: true,
        line_index: None,
    }
}

fn normalize_line_for_match(line: &str) -> String {
    normalize_inline_text(line)
}

fn normalize_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(feature = "pdfium")]
#[derive(Debug, thiserror::Error)]
pub enum FixtureRunError {
    #[error(transparent)]
    Fixture(#[from] FixtureError),
    #[error(transparent)]
    Parse(#[from] LiteParseError),
    #[error("fixture `{fixture_id}` did not produce page {page}")]
    MissingPage { fixture_id: String, page: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_phrases_must_match_in_sequence() {
        let checks = evaluate_ordered_phrases(
            &[
                "alpha beta".to_string(),
                "gamma".to_string(),
                "omega".to_string(),
            ],
            "alpha beta gamma delta omega",
        );

        assert!(checks.iter().all(|check| check.matched));
    }

    #[test]
    fn ordered_phrases_fail_when_order_breaks() {
        let checks = evaluate_ordered_phrases(
            &["gamma".to_string(), "alpha".to_string()],
            "alpha beta gamma",
        );

        assert_eq!(
            checks,
            vec![
                PhraseCheck {
                    phrase: "gamma".to_string(),
                    matched: true,
                },
                PhraseCheck {
                    phrase: "alpha".to_string(),
                    matched: false,
                },
            ]
        );
    }

    #[test]
    fn fixture_evaluation_checks_lines_groups_and_forbidden_patterns() {
        let fixture = PageFixture {
            id: "sample".to_string(),
            pdf: "sample.pdf".to_string(),
            page: 1,
            expected_lines: vec![
                "Wohnraummiete MK".to_string(),
                "VORGETÄUSCHTER EIGENBEDARF".to_string(),
            ],
            required_patterns: vec!["Revision hat Erfolg".to_string()],
            ordered_phrases: vec![
                "Der Vermieter kündigte".to_string(),
                "Revision hat Erfolg".to_string(),
            ],
            same_line_groups: vec![vec!["Wohnraummiete".to_string(), "MK".to_string()]],
            separate_line_groups: vec![vec![
                "VORGETÄUSCHTER EIGENBEDARF".to_string(),
                "1. Der Fall des BGH".to_string(),
            ]],
            forbidden_patterns: vec!["Wohnung ihr PLuS im netZ".to_string()],
            ..Default::default()
        };

        let text = "\
Wohnraummiete MK
VORGETÄUSCHTER EIGENBEDARF
1. Der Fall des BGH
Der Vermieter kündigte das Mietverhältnis.
Revision hat Erfolg.";

        let evaluation = evaluate_page_fixture_text(&fixture, text);

        assert!(evaluation.passed);
        assert_eq!(evaluation.expected_line_hits, 2);
        assert_eq!(evaluation.required_pattern_hits, 1);
        assert_eq!(evaluation.ordered_phrase_hits, 2);
        assert_eq!(evaluation.same_line_hits, 1);
        assert_eq!(evaluation.separate_line_hits, 1);
        assert_eq!(evaluation.forbidden_violations, 0);
    }

    #[test]
    fn corpus_pdf_root_resolves_relative_to_manifest() {
        let corpus = PageFixtureCorpus {
            name: "public".to_string(),
            description: None,
            pdf_root: Some("samples/public/opendataloader".to_string()),
            fixtures: Vec::new(),
        };

        let resolved = resolve_corpus_pdf_root(
            "tools/pdf-parse/tests/fixtures/corpus.public-opendataloader.json",
            &corpus,
            None,
        )
        .unwrap();

        assert_eq!(
            resolved,
            PathBuf::from("tools/pdf-parse/tests/fixtures/samples/public/opendataloader")
        );
    }

    #[test]
    fn sample_root_uses_manifest_directory_by_default() {
        let manifest = PdfSampleManifest {
            name: "public-samples".to_string(),
            description: None,
            root: None,
            samples: Vec::new(),
        };

        let resolved = resolve_sample_root(
            "tools/pdf-parse/tests/fixtures/samples/public/opendataloader/samples.json",
            &manifest,
            None,
        );

        assert_eq!(
            resolved,
            PathBuf::from("tools/pdf-parse/tests/fixtures/samples/public/opendataloader")
        );
    }
}
