use crate::core::config::LiteParseConfig;
use crate::core::types::ParsedPage;
use once_cell::sync::Lazy;
use regex::Regex;

static MK_FOOTER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d{2}-\d{4}\s+MIETRECHT(?:SKOMPAKT|KOMPAKT)?\s+\d+\s*$").unwrap());
static INDESIGN_ARTIFACT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^document\d+.*\.?\s*indd\b").unwrap());
static LINE_ENUMERATOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^((?:\d+\.)|(?:[a-z]\)))(\p{L})").unwrap());
static DOUBLE_PERIOD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([\p{L}\p{N}])\.\.([\s)\]])").unwrap());
static DUPLICATE_COMMA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r",{2,}").unwrap());

pub fn detect_and_remove_margin_on_page(page: &mut ParsedPage) {
    let lines: Vec<&str> = page.text.lines().collect();

    if lines.is_empty() {
        page.text.clear();
        return;
    }

    let mut min_x: Option<usize> = None;
    let mut min_y: Option<usize> = None;
    let mut max_y: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if let Some(first_non_space) = line.chars().position(|c| !c.is_whitespace()) {
            min_x = Some(match min_x {
                Some(current) => current.min(first_non_space),
                None => first_non_space,
            });
            min_y = Some(min_y.map_or(idx, |current| current.min(idx)));
            max_y = Some(max_y.map_or(idx, |current| current.max(idx)));
        }
    }

    let (Some(min_x), Some(min_y), Some(max_y)) = (min_x, min_y, max_y) else {
        page.text.clear();
        return;
    };

    let kept = &lines[min_y..=max_y];
    let normalized: Vec<String> = kept
        .iter()
        .map(|line| {
            // `min_x` is a character index (computed via `chars().position`), so it
            // must be translated into a byte offset on a char boundary before
            // slicing. Indexing `&line[min_x..]` directly treats it as a byte
            // offset and panics on multibyte input (NBSP, accents, CJK), which is
            // ubiquitous in real PDFs. `char_indices().nth(min_x)` yields the byte
            // offset of the `min_x`-th character; lines with `<= min_x` characters
            // map to end-of-string and slice empty.
            let start = line
                .char_indices()
                .nth(min_x)
                .map_or(line.len(), |(byte_idx, _)| byte_idx);
            line[start..].trim_end().to_string()
        })
        .collect();

    page.text = normalized.join("\n");
}

pub fn clean_raw_text(pages: &mut [ParsedPage], _config: &LiteParseConfig) {
    for page in pages {
        detect_and_remove_margin_on_page(page);
        page.text = normalize_page_text(&page.text);
    }
}

fn normalize_page_text(text: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut carry = String::new();
    let mut join_next = false;

    for raw_line in text.lines() {
        let sanitized = raw_line.replace('\u{0000}', " ").replace('\u{0002}', "");
        let trimmed_end = sanitized.trim_end().to_string();
        let ends_with_soft_wrap =
            raw_line.contains('\u{0002}') || trimmed_end.ends_with('\u{00ad}');

        if carry.is_empty() {
            carry = trimmed_end;
            join_next = ends_with_soft_wrap;
            continue;
        }

        if join_next {
            carry.push_str(trimmed_end.trim_start());
        } else {
            output.push(std::mem::take(&mut carry));
            carry = trimmed_end;
        }
        join_next = ends_with_soft_wrap;
    }

    if !carry.is_empty() {
        output.push(carry);
    }

    filter_layout_artifacts(&output).join("\n")
}

fn filter_layout_artifacts(lines: &[String]) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut drop_following_kompakt = false;

    for line in lines {
        let normalized_line = normalize_line_artifacts(line);
        let trimmed = normalized_line.trim();
        if trimmed.is_empty() {
            if filtered.last().is_some_and(|prev: &String| prev.is_empty()) {
                continue;
            }
            filtered.push(String::new());
            continue;
        }

        if MK_FOOTER_RE.is_match(trimmed) {
            drop_following_kompakt = true;
            continue;
        }

        if INDESIGN_ARTIFACT_RE.is_match(trimmed) {
            continue;
        }

        if drop_following_kompakt && trimmed.eq_ignore_ascii_case("KOMPAKT") {
            drop_following_kompakt = false;
            continue;
        }

        drop_following_kompakt = false;
        filtered.push(normalized_line);
    }

    filtered
}

fn normalize_line_artifacts(line: &str) -> String {
    let line = line.replace('\u{00ad}', "");
    let line = LINE_ENUMERATOR_RE.replace(&line, "$1 $2");
    let line = DOUBLE_PERIOD_RE.replace_all(&line, "$1.$2");
    let line = DUPLICATE_COMMA_RE.replace_all(&line, ",");
    line.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> ParsedPage {
        ParsedPage {
            text: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn margin_removal_handles_multibyte_leading_whitespace() {
        // NBSP (U+00A0, 2 bytes) as leading whitespace gives a char-index `min_x`
        // that is not a valid byte boundary on the longer line. The pre-fix code
        // sliced `&line[min_x..]` and panicked with "byte index is not a char
        // boundary". This must now clean without panicking.
        let mut p = page(" z\n\u{00a0}z");
        detect_and_remove_margin_on_page(&mut p);
        // min_x == 1 char of leading whitespace; one char stripped from each line.
        assert_eq!(p.text, "z\nz");
    }

    #[test]
    fn margin_removal_strips_common_char_indent_with_accents() {
        // Two leading spaces are common to both lines; the accented content must
        // survive intact (no byte/char index confusion mid-grapheme).
        let mut p = page("  Müller\n  Größe");
        detect_and_remove_margin_on_page(&mut p);
        assert_eq!(p.text, "Müller\nGröße");
    }

    #[test]
    fn margin_removal_is_panic_free_on_cjk_and_short_lines() {
        // A line shorter than `min_x` characters and CJK content must not panic.
        let mut p = page("   一\n二");
        detect_and_remove_margin_on_page(&mut p);
        // min_x is 0 (line "二" starts at column 0), so nothing is stripped.
        assert_eq!(p.text, "   一\n二");
    }
}
