use crate::core::config::LiteParseConfig;
use crate::core::types::{
    Coordinates, ForwardAnchors, ParsedPage, ProjectToGridResult, ProjectionTextBox,
};
use crate::processing::bbox::build_projection_boxes;
use crate::processing::clean_text::clean_raw_text;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;

static MARGIN_LINE_NUMBER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{1,2}[O]?$").unwrap());
static NUMERIC_PATTERN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[$]?-?[\d,]+\.?\d*%?$").unwrap());
static LOWER_UPPER_SPLIT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\p{Ll})(\p{Lu})\s+(\p{Ll}{2,})").unwrap());
static LONG_CAMEL_BOUNDARY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\p{Lu}?\p{Ll}{3,})(\p{Lu}\p{Ll}{3,})\b").unwrap());
static WORD_TO_ACRONYM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\p{Ll}{3,})(\p{Lu}{2,})($|[^\p{Ll}])").unwrap());
static PUNCT_TO_LETTER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([,.;:!?])(\p{L})").unwrap());
static DOUBLE_BAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\|\|+").unwrap());
static UPPER_SINGLE_LETTER_SPLIT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\p{Lu})\s+(\p{Ll}{2,})\b").unwrap());
static SHORT_ACRONYM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Z]{2,4}[a-z])\b").unwrap());
static LOWERCASE_COMMA_NOISE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s+[a-z],\s+").unwrap());
static URL_WITH_SCHEME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\bhttps?://[A-Za-z0-9-]+(?:\.\s*[A-Za-z0-9-]+)+(?:/[A-Za-z0-9._~:/?#\[\]@!$&()*+,;=%-]*)?",
    )
    .unwrap()
});
static WWW_DOMAIN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bwww\.\s*[A-Za-z0-9-]+(?:\.\s*[A-Za-z0-9-]+)+\b").unwrap());
static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[A-Za-z0-9._%+-]+\s*@\s*[A-Za-z0-9-]+(?:\.\s*[A-Za-z0-9-]+)+\b").unwrap()
});

fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn char_width_estimate(bbox: &ProjectionTextBox) -> f64 {
    let len = bbox.str.trim().chars().count().max(1) as f64;
    (bbox.w / len).max(1.0)
}

fn rendered_spaces_between(previous: &ProjectionTextBox, current: &ProjectionTextBox) -> usize {
    let gap = (current.x - previous.x - previous.w).max(0.0);
    let unit = char_width_estimate(previous).max(char_width_estimate(current));

    if gap <= unit * 0.55 {
        0
    } else if gap <= unit * 1.35 {
        1
    } else {
        ((gap / unit).round() as usize).clamp(1, 8)
    }
}

fn suppress_geometry_space(
    previous: &ProjectionTextBox,
    current: &ProjectionTextBox,
    previous_last: Option<char>,
    current_first: Option<char>,
) -> bool {
    let gap = (current.x - previous.x - previous.w).max(0.0);
    let unit = char_width_estimate(previous).max(char_width_estimate(current));

    gap <= unit * 0.65
        && previous_last.is_some_and(|ch| ch.is_alphabetic())
        && current_first.is_some_and(|ch| ch.is_alphabetic())
}

fn normalize_render_fragment(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect();
    let whitespace_count = stripped.chars().filter(|ch| ch.is_whitespace()).count();

    if whitespace_count > 6 {
        return DOUBLE_BAR_RE.replace_all(&stripped, "|").into_owned();
    }

    let normalized = collapse_duplicate_word_starts(&stripped);
    let normalized = LONG_CAMEL_BOUNDARY_RE.replace_all(&normalized, "$1 $2");
    let normalized = LOWER_UPPER_SPLIT_RE.replace_all(&normalized, "$1 $2$3");
    let normalized = WORD_TO_ACRONYM_RE.replace_all(&normalized, "$1 $2$3");
    let normalized = UPPER_SINGLE_LETTER_SPLIT_RE.replace_all(&normalized, "$1$2");
    let normalized = PUNCT_TO_LETTER_RE.replace_all(&normalized, "$1 $2");
    DOUBLE_BAR_RE.replace_all(&normalized, "|").into_owned()
}

fn normalize_rendered_line(text: &str) -> String {
    let collapsed = collapse_duplicate_word_starts(text);
    let mut normalized = SHORT_ACRONYM_RE
        .replace_all(&collapsed, |captures: &regex::Captures<'_>| {
            captures[1].to_uppercase()
        })
        .into_owned();

    if normalized.chars().filter(|ch| ch.is_whitespace()).count() <= 8 {
        for _ in 0..2 {
            normalized = LONG_CAMEL_BOUNDARY_RE
                .replace_all(&normalized, "$1 $2")
                .into_owned();
            normalized = LOWER_UPPER_SPLIT_RE
                .replace_all(&normalized, "$1 $2$3")
                .into_owned();
            normalized = UPPER_SINGLE_LETTER_SPLIT_RE
                .replace_all(&normalized, "$1$2")
                .into_owned();
        }
    }

    let normalized = LOWERCASE_COMMA_NOISE_RE
        .replace_all(&normalized, ", ")
        .into_owned();
    stitch_urlish_fragments(&normalized)
}

fn collapse_duplicate_word_starts(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        let at_word_start = index == 0 || !chars[index - 1].is_alphabetic();
        let previous_boundary = index
            .checked_sub(1)
            .and_then(|previous| chars.get(previous))
            .copied();
        let previous_alpha = chars[..index]
            .iter()
            .rev()
            .copied()
            .find(|candidate| candidate.is_alphabetic());
        let should_collapse_lowercase = chars[index].is_lowercase()
            && previous_boundary.is_none_or(char::is_whitespace)
            && previous_alpha.is_none_or(char::is_lowercase);
        if at_word_start
            && index + 3 < chars.len()
            && chars[index].is_alphabetic()
            && (chars[index].is_uppercase() || should_collapse_lowercase)
            && chars[index] == chars[index + 1]
            && chars[index + 2].is_lowercase()
        {
            output.push(chars[index]);
            index += 2;
            continue;
        }

        output.push(chars[index]);
        index += 1;
    }

    output
}

fn stitch_urlish_fragments(text: &str) -> String {
    let normalized = EMAIL_RE
        .replace_all(text, |captures: &regex::Captures<'_>| {
            collapse_urlish_match(captures.get(0).map_or("", |m| m.as_str()))
        })
        .into_owned();
    let normalized = URL_WITH_SCHEME_RE
        .replace_all(&normalized, |captures: &regex::Captures<'_>| {
            collapse_urlish_match(captures.get(0).map_or("", |m| m.as_str()))
        })
        .into_owned();

    WWW_DOMAIN_RE
        .replace_all(&normalized, |captures: &regex::Captures<'_>| {
            collapse_urlish_match(captures.get(0).map_or("", |m| m.as_str()))
        })
        .into_owned()
}

fn collapse_urlish_match(text: &str) -> String {
    text.replace(". ", ".")
        .replace(" .", ".")
        .replace("@ ", "@")
        .replace(" @", "@")
}

fn can_merge_markup(
    a: Option<&crate::core::types::Markup>,
    b: Option<&crate::core::types::Markup>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn merge_page_bbox(a: Option<Coordinates>, b: Option<Coordinates>) -> Option<Coordinates> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let x = a.x.min(b.x);
            let y = a.y.min(b.y);
            let x2 = (a.x + a.w).max(b.x + b.w);
            let y2 = (a.y + a.h).max(b.y + b.h);
            Some(Coordinates {
                x,
                y,
                w: x2 - x,
                h: y2 - y,
            })
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn effective_page_bbox(bbox: &ProjectionTextBox) -> Coordinates {
    bbox.page_bbox.clone().unwrap_or(Coordinates {
        x: bbox.x,
        y: bbox.y,
        w: bbox.w,
        h: bbox.h,
    })
}

#[derive(Debug, Clone, Copy)]
struct RightRailZone {
    start_x: f64,
    max_width: f64,
    min_gap: f64,
    x_tolerance: f64,
}

#[derive(Debug, Clone, Copy)]
struct TwoColumnZone {
    left_start: f64,
    right_start: f64,
    left_width: f64,
    right_width: f64,
    min_gap: f64,
    x_tolerance: f64,
    width_tolerance: f64,
}

#[derive(Debug, Clone, Copy)]
struct TwoColumnMetrics {
    left_start: f64,
    right_start: f64,
    left_width: f64,
    right_width: f64,
    gap: f64,
}

pub fn handle_rotation_reading_order(boxes: &[ProjectionTextBox]) -> Vec<ProjectionTextBox> {
    let mut groups: BTreeMap<i32, Vec<ProjectionTextBox>> = BTreeMap::new();

    for bbox in boxes {
        groups
            .entry(bbox.r.unwrap_or(0))
            .or_default()
            .push(bbox.clone());
    }

    let mut transformed: Vec<ProjectionTextBox> = Vec::new();

    for (rotation, mut group) in groups {
        group.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

        match rotation.rem_euclid(360) {
            90 => {
                for mut bbox in group {
                    let delta_y = 0.0;
                    let old_x = bbox.x;
                    bbox.x = bbox.y.round();
                    bbox.y = old_x + delta_y;
                    std::mem::swap(&mut bbox.w, &mut bbox.h);
                    bbox.r = Some(0);
                    bbox.rotated = Some(true);
                    transformed.push(bbox);
                }
            }
            180 => {
                for mut bbox in group {
                    let new_x = bbox.ry.unwrap_or(bbox.y).round();
                    let new_y = bbox.rx.unwrap_or(bbox.x);
                    bbox.x = new_x;
                    bbox.y = new_y;
                    bbox.r = Some(0);
                    bbox.rotated = Some(true);
                    transformed.push(bbox);
                }
            }
            270 => {
                let max_y = group
                    .iter()
                    .map(|bbox| bbox.y + bbox.h)
                    .fold(0.0_f64, f64::max);

                for mut bbox in group {
                    let delta_y = 0.0;
                    let old_x = bbox.x;
                    bbox.x = (max_y - bbox.y - bbox.h).round();
                    bbox.y = old_x + delta_y;
                    std::mem::swap(&mut bbox.w, &mut bbox.h);
                    bbox.r = Some(0);
                    bbox.rotated = Some(true);
                    transformed.push(bbox);
                }
            }
            _ => transformed.extend(group),
        }
    }

    transformed.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap()
            .then(a.x.partial_cmp(&b.x).unwrap())
    });

    transformed
}

pub fn bbox_to_lines(
    text_bbox: &[ProjectionTextBox],
    median_width: f64,
    median_height: f64,
    page_width: Option<f64>,
) -> Vec<Vec<ProjectionTextBox>> {
    if text_bbox.is_empty() {
        return vec![];
    }

    let y_sort_tolerance = (median_height * 0.5).max(5.0);

    let mut sorted = text_bbox.to_vec();

    if let Some(page_width) = page_width {
        let midpoint = page_width / 2.0;
        let left_zone = midpoint - 25.0;
        let right_zone = midpoint + 25.0;

        for bbox in &mut sorted {
            let trimmed = bbox.str.trim();
            let is_margin_line_number = MARGIN_LINE_NUMBER_RE.is_match(trimmed)
                && bbox.w < 15.0
                && bbox.x >= left_zone
                && bbox.x <= right_zone;

            if is_margin_line_number {
                bbox.is_margin_line_number = Some(true);
            }
        }
    }

    sorted.sort_by(|a, b| {
        let ay = (a.y / y_sort_tolerance).round();
        let by = (b.y / y_sort_tolerance).round();
        ay.partial_cmp(&by)
            .unwrap()
            .then(a.y.partial_cmp(&b.y).unwrap())
            .then(a.x.partial_cmp(&b.x).unwrap())
    });

    let mut merged: Vec<ProjectionTextBox> = Vec::new();

    for bbox in sorted {
        if let Some(prev) = merged.last_mut() {
            let x_delta = bbox.x - prev.x - prev.w;
            let same_y = approx_eq(prev.y, bbox.y, y_sort_tolerance);
            let same_h = approx_eq(prev.h, bbox.h, median_height.max(2.0));
            let can_merge = same_y
                && same_h
                && ((x_delta > -0.5 && x_delta < 0.0) || (x_delta >= 0.0 && x_delta < 0.1))
                && can_merge_markup(prev.markup.as_ref(), bbox.markup.as_ref());

            if can_merge {
                prev.str.push_str(&bbox.str);
                prev.w = (bbox.x + bbox.w) - prev.x;
                prev.h = prev.h.max(bbox.h);
                prev.str_length += bbox.str_length;
                prev.page_bbox = merge_page_bbox(
                    Some(effective_page_bbox(prev)),
                    Some(effective_page_bbox(&bbox)),
                );
                continue;
            }
        }

        merged.push(bbox);
    }

    let x_overlap_tolerance = (median_width / 3.0).max(5.0);
    let mut lines: Vec<Vec<ProjectionTextBox>> = Vec::new();

    for bbox in merged {
        let mut placed = false;

        for line in &mut lines {
            let margin_mismatch = line
                .first()
                .and_then(|first| first.is_margin_line_number)
                .unwrap_or(false)
                != bbox.is_margin_line_number.unwrap_or(false);

            let y_tolerance = if bbox.rotated.unwrap_or(false) {
                (median_height * 2.0).max(20.0)
            } else {
                y_sort_tolerance
            };

            let line_min_y = line.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
            let line_max_y = line.iter().map(|b| b.y + b.h).fold(0.0_f64, f64::max);

            let line_collides = line.iter().any(|existing| {
                let overlap =
                    (existing.right().min(bbox.right()) - existing.x.max(bbox.x)).max(0.0);
                overlap > x_overlap_tolerance
            });

            let bbox_center_y = bbox.y + bbox.h / 2.0;
            let y_close = line
                .iter()
                .any(|existing| approx_eq(existing.y, bbox.y, y_tolerance))
                || (bbox_center_y >= line_min_y && bbox_center_y <= line_max_y)
                || (bbox.y >= line_min_y && bbox.y <= line_max_y);

            if !line_collides && !margin_mismatch && y_close {
                line.push(bbox.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            lines.push(vec![bbox]);
        }
    }

    for line in &mut lines {
        line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    }

    lines.sort_by(|a, b| {
        let ay = a.first().map(|b| b.y).unwrap_or(0.0);
        let by = b.first().map(|b| b.y).unwrap_or(0.0);
        ay.partial_cmp(&by).unwrap()
    });

    for line in &mut lines {
        if line.is_empty() {
            continue;
        }

        let mut compact: Vec<ProjectionTextBox> = Vec::new();

        for current in line.drain(..) {
            if let Some(previous) = compact.last_mut() {
                let markup_ok = can_merge_markup(previous.markup.as_ref(), current.markup.as_ref());
                let gap = current.x - previous.x - previous.w;

                let looks_like_table_number =
                    |s: &str| -> bool { s.len() >= 2 && NUMERIC_PATTERN_RE.is_match(s) };

                let both_numbers = looks_like_table_number(previous.str.trim())
                    && looks_like_table_number(current.str.trim());

                if markup_ok && !both_numbers && gap <= 1.0 {
                    previous.str.push_str(current.str.trim_start());
                    previous.w = (current.x + current.w) - previous.x;
                    previous.h = previous.h.max(current.h);
                    previous.str_length = previous.str.chars().count();
                    previous.page_bbox = merge_page_bbox(
                        Some(effective_page_bbox(previous)),
                        Some(effective_page_bbox(&current)),
                    );
                    continue;
                }
            }

            compact.push(current);
        }

        *line = compact;
    }

    let mut i = 1;
    while i + 1 < lines.len() {
        if !lines[i - 1].is_empty() && !lines[i].is_empty() {
            let prev_min_y = lines[i - 1]
                .iter()
                .map(|b| b.y)
                .fold(f64::INFINITY, f64::min);
            let prev_max_y = lines[i - 1]
                .iter()
                .map(|b| b.y + b.h)
                .fold(0.0_f64, f64::max);
            let curr_min_y = lines[i].iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
            let curr_max_y = lines[i].iter().map(|b| b.y + b.h).fold(0.0_f64, f64::max);

            let overlaps_in_y = curr_min_y <= prev_max_y && curr_max_y >= prev_min_y;
            let overlaps_in_x = lines[i - 1].iter().any(|left| {
                lines[i].iter().any(|right| {
                    (left.right().min(right.right()) - left.x.max(right.x)).max(0.0) > 0.0
                })
            });

            if overlaps_in_y && !overlaps_in_x {
                let mut merged_line = lines.remove(i);
                lines[i - 1].append(&mut merged_line);
                lines[i - 1].sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                continue;
            }
        }

        i += 1;
    }

    let mut with_blanks: Vec<Vec<ProjectionTextBox>> = Vec::new();
    for (idx, line) in lines.into_iter().enumerate() {
        if idx > 0 {
            let prev = with_blanks.last().cloned().unwrap_or_default();
            if !prev.is_empty() && !line.is_empty() {
                let prev_first = &prev[0];
                let curr_first = &line[0];
                let y_delta = curr_first.y - prev_first.y - prev_first.h;
                if y_delta > median_height {
                    let blanks = (((y_delta / median_height).round() as i32) - 1).clamp(1, 10);
                    for _ in 0..blanks {
                        with_blanks.push(Vec::new());
                    }
                }
            }
        }
        with_blanks.push(line);
    }

    with_blanks
}

fn split_lines_for_right_rail(
    lines: Vec<Vec<ProjectionTextBox>>,
    page_width: f64,
    median_width: f64,
) -> Vec<Vec<ProjectionTextBox>> {
    let Some(zone) = detect_right_rail_zone(&lines, page_width, median_width) else {
        return lines;
    };

    let mut split_lines = Vec::with_capacity(lines.len());
    for line in lines {
        if line.is_empty() {
            split_lines.push(line);
            continue;
        }

        if let Some(split_index) = find_right_rail_split(&line, &zone, page_width) {
            let mut sorted = line;
            sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
            split_lines.push(sorted[..split_index].to_vec());
            split_lines.push(sorted[split_index..].to_vec());
        } else {
            split_lines.push(line);
        }
    }

    split_lines
}

#[derive(Debug, Clone, Copy)]
enum ColumnSide {
    Left,
    Right,
}

fn reflow_two_column_regions(
    lines: Vec<Vec<ProjectionTextBox>>,
    page_width: f64,
    median_width: f64,
) -> Vec<Vec<ProjectionTextBox>> {
    let Some(zone) = detect_two_column_zone(&lines, page_width, median_width) else {
        return lines;
    };

    let supporting_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| find_two_column_split(line, &zone, page_width).map(|_| index))
        .collect();

    let Some(&first_index) = supporting_indices.first() else {
        return lines;
    };
    let Some(&last_index) = supporting_indices.last() else {
        return lines;
    };

    let mut output = Vec::new();
    output.extend(lines[..first_index].iter().cloned());

    let mut left_column = Vec::new();
    let mut right_column = Vec::new();

    for line in &lines[first_index..=last_index] {
        if line.is_empty() {
            continue;
        }

        if let Some((split_index, _metrics)) = find_two_column_split(line, &zone, page_width) {
            let mut sorted = line.clone();
            sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
            left_column.push(sorted[..split_index].to_vec());
            right_column.push(sorted[split_index..].to_vec());
            continue;
        }

        match classify_two_column_line(line, &zone) {
            Some(ColumnSide::Left) => left_column.push(line.clone()),
            Some(ColumnSide::Right) => right_column.push(line.clone()),
            None => output.push(line.clone()),
        }
    }

    output.extend(left_column);
    if !right_column.is_empty() && !output.is_empty() {
        output.push(Vec::new());
    }
    output.extend(right_column);
    output.extend(lines[last_index + 1..].iter().cloned());
    output
}

fn detect_two_column_zone(
    lines: &[Vec<ProjectionTextBox>],
    page_width: f64,
    _median_width: f64,
) -> Option<TwoColumnZone> {
    if page_width <= 0.0 {
        return None;
    }

    let provisional = TwoColumnZone {
        left_start: page_width * 0.08,
        right_start: page_width * 0.50,
        left_width: page_width * 0.36,
        right_width: page_width * 0.36,
        min_gap: (page_width * 0.03).max(18.0).min(page_width * 0.12),
        x_tolerance: (page_width * 0.04).max(18.0),
        width_tolerance: page_width * 0.12,
    };

    let mut metrics = Vec::new();
    for line in lines {
        if let Some((_, candidate)) = find_two_column_split(line, &provisional, page_width) {
            metrics.push(candidate);
        }
    }

    if metrics.len() < 4 {
        return None;
    }

    let left_starts: Vec<f64> = metrics.iter().map(|metric| metric.left_start).collect();
    let right_starts: Vec<f64> = metrics.iter().map(|metric| metric.right_start).collect();
    let left_widths: Vec<f64> = metrics.iter().map(|metric| metric.left_width).collect();
    let right_widths: Vec<f64> = metrics.iter().map(|metric| metric.right_width).collect();
    let gaps: Vec<f64> = metrics.iter().map(|metric| metric.gap).collect();

    let zone = TwoColumnZone {
        left_start: median_value(&left_starts).unwrap_or(provisional.left_start),
        right_start: median_value(&right_starts).unwrap_or(provisional.right_start),
        left_width: median_value(&left_widths).unwrap_or(provisional.left_width),
        right_width: median_value(&right_widths).unwrap_or(provisional.right_width),
        min_gap: median_value(&gaps).unwrap_or(provisional.min_gap),
        x_tolerance: provisional.x_tolerance,
        width_tolerance: provisional.width_tolerance,
    };

    let support = lines
        .iter()
        .filter(|line| find_two_column_split(line, &zone, page_width).is_some())
        .count();
    if support < 4 {
        return None;
    }

    Some(zone)
}

fn find_two_column_split(
    line: &[ProjectionTextBox],
    zone: &TwoColumnZone,
    page_width: f64,
) -> Option<(usize, TwoColumnMetrics)> {
    if line.len() < 2 {
        return None;
    }

    let mut sorted = line.to_vec();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    for split_index in 1..sorted.len() {
        let left = &sorted[..split_index];
        let right = &sorted[split_index..];
        let left_start = left.iter().map(|bbox| bbox.x).fold(f64::INFINITY, f64::min);
        let left_right = left
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let right_start = right
            .iter()
            .map(|bbox| bbox.x)
            .fold(f64::INFINITY, f64::min);
        let right_right = right
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let left_width = (left_right - left_start).max(0.0);
        let right_width = (right_right - right_start).max(0.0);
        let gap = (right_start - left_right).max(0.0);

        if gap < zone.min_gap * 0.8 {
            continue;
        }
        if left_start > page_width * 0.20 || right_start < page_width * 0.42 {
            continue;
        }
        if left_width < page_width * 0.20 || right_width < page_width * 0.20 {
            continue;
        }
        if left_width > page_width * 0.48 || right_width > page_width * 0.48 {
            continue;
        }
        if !column_side_is_prose_like(left) || !column_side_is_prose_like(right) {
            continue;
        }

        let within_cluster = (left_start - zone.left_start).abs() <= zone.x_tolerance
            && (right_start - zone.right_start).abs() <= zone.x_tolerance
            && (left_width - zone.left_width).abs() <= zone.width_tolerance
            && (right_width - zone.right_width).abs() <= zone.width_tolerance;

        if within_cluster || (zone.left_start - page_width * 0.08).abs() < f64::EPSILON {
            return Some((
                split_index,
                TwoColumnMetrics {
                    left_start,
                    right_start,
                    left_width,
                    right_width,
                    gap,
                },
            ));
        }
    }

    None
}

fn classify_two_column_line(
    line: &[ProjectionTextBox],
    zone: &TwoColumnZone,
) -> Option<ColumnSide> {
    if line.is_empty() {
        return None;
    }

    let min_x = line.iter().map(|bbox| bbox.x).fold(f64::INFINITY, f64::min);
    let max_right = line
        .iter()
        .map(ProjectionTextBox::right)
        .fold(0.0_f64, f64::max);
    let width = (max_right - min_x).max(0.0);

    if min_x >= zone.right_start - zone.x_tolerance {
        return Some(ColumnSide::Right);
    }

    if max_right <= zone.right_start - zone.min_gap * 0.35 {
        return Some(ColumnSide::Left);
    }

    if min_x <= zone.left_start + zone.x_tolerance
        && width <= zone.left_width + zone.width_tolerance
    {
        return Some(ColumnSide::Left);
    }

    None
}

fn column_side_is_prose_like(side: &[ProjectionTextBox]) -> bool {
    let mut alpha = 0usize;
    let mut digits = 0usize;

    for bbox in side {
        for ch in bbox.str.chars() {
            if ch.is_alphabetic() {
                alpha += 1;
            } else if ch.is_ascii_digit() {
                digits += 1;
            }
        }
    }

    alpha >= 8 && alpha >= digits
}

fn detect_right_rail_zone(
    lines: &[Vec<ProjectionTextBox>],
    page_width: f64,
    median_width: f64,
) -> Option<RightRailZone> {
    if page_width <= 0.0 {
        return None;
    }

    let min_start_x = page_width * 0.72;
    let max_rail_width = page_width * 0.18;
    let min_gap = ((median_width * 1.5).max(18.0))
        .min(page_width * 0.10)
        .max(page_width * 0.04);

    let mut starts = Vec::new();
    let mut widths = Vec::new();
    let mut gaps = Vec::new();

    for line in lines {
        if let Some(metrics) =
            right_rail_metrics_for_line(line, min_start_x, max_rail_width, min_gap, page_width)
        {
            starts.push(metrics.start_x);
            widths.push(metrics.width);
            gaps.push(metrics.gap);
        }
    }

    if starts.len() < 2 || starts.len() > 10 {
        return None;
    }

    Some(RightRailZone {
        start_x: median_value(&starts).unwrap_or(min_start_x),
        max_width: median_value(&widths).unwrap_or(max_rail_width),
        min_gap: median_value(&gaps).unwrap_or(min_gap),
        x_tolerance: (page_width * 0.04).max(median_width * 2.0).max(18.0),
    })
}

#[derive(Debug, Clone, Copy)]
struct RightRailMetrics {
    start_x: f64,
    width: f64,
    gap: f64,
}

fn right_rail_metrics_for_line(
    line: &[ProjectionTextBox],
    min_start_x: f64,
    max_rail_width: f64,
    min_gap: f64,
    page_width: f64,
) -> Option<RightRailMetrics> {
    if line.len() < 2 {
        return None;
    }

    let mut sorted = line.to_vec();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    for split_index in 1..sorted.len() {
        let left = &sorted[..split_index];
        let right = &sorted[split_index..];
        let left_min_x = left.iter().map(|bbox| bbox.x).fold(f64::INFINITY, f64::min);
        let left_right = left
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let right_left = right
            .iter()
            .map(|bbox| bbox.x)
            .fold(f64::INFINITY, f64::min);
        let right_right = right
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let left_width = (left_right - left_min_x).max(0.0);
        let right_width = (right_right - right_left).max(0.0);
        let gap = (right_left - left_right).max(0.0);
        let right_chars: usize = right
            .iter()
            .map(|bbox| bbox.str.trim().chars().count())
            .sum();
        let right_has_letters = right
            .iter()
            .any(|bbox| bbox.str.chars().any(|ch| ch.is_alphabetic()));

        if gap >= min_gap
            && right_left >= min_start_x
            && right_width <= max_rail_width
            && left_width >= page_width * 0.30
            && right_chars >= 4
            && right_has_letters
        {
            return Some(RightRailMetrics {
                start_x: right_left,
                width: right_width,
                gap,
            });
        }
    }

    None
}

fn find_right_rail_split(
    line: &[ProjectionTextBox],
    zone: &RightRailZone,
    page_width: f64,
) -> Option<usize> {
    if line.len() < 2 {
        return None;
    }

    let mut sorted = line.to_vec();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    for split_index in 1..sorted.len() {
        let left = &sorted[..split_index];
        let right = &sorted[split_index..];
        let left_min_x = left.iter().map(|bbox| bbox.x).fold(f64::INFINITY, f64::min);
        let left_right = left
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let right_left = right
            .iter()
            .map(|bbox| bbox.x)
            .fold(f64::INFINITY, f64::min);
        let right_right = right
            .iter()
            .map(ProjectionTextBox::right)
            .fold(0.0_f64, f64::max);
        let left_width = (left_right - left_min_x).max(0.0);
        let right_width = (right_right - right_left).max(0.0);
        let gap = (right_left - left_right).max(0.0);
        let right_has_letters = right
            .iter()
            .any(|bbox| bbox.str.chars().any(|ch| ch.is_alphabetic()));

        if gap >= zone.min_gap * 0.75
            && right_left >= zone.start_x - zone.x_tolerance
            && right_width <= zone.max_width * 1.35
            && left_width >= page_width * 0.25
            && right_has_letters
        {
            return Some(split_index);
        }
    }

    None
}

fn median_value(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(sorted[sorted.len() / 2])
}

fn filter_dot_garbage(boxes: Vec<ProjectionTextBox>) -> Vec<ProjectionTextBox> {
    let dot_count = boxes
        .iter()
        .filter(|bbox| bbox.str.trim_matches('.').is_empty() && bbox.str.contains('.'))
        .count();

    if dot_count >= 100 {
        boxes
            .into_iter()
            .filter(|bbox| !(bbox.str.trim_matches('.').is_empty() && bbox.str.contains('.')))
            .collect()
    } else {
        boxes
    }
}

fn infer_left_anchors(
    boxes: &[ProjectionTextBox],
    existing: &BTreeMap<String, i32>,
) -> BTreeMap<String, i32> {
    if !existing.is_empty() {
        return existing.clone();
    }

    let mut xs: Vec<i32> = boxes.iter().map(|bbox| bbox.x.round() as i32).collect();

    xs.sort_unstable();
    xs.dedup();

    let mut anchors = BTreeMap::new();
    for (idx, x) in xs.into_iter().enumerate() {
        let column = if idx == 0 { 1 } else { 1 + (idx as i32 * 10) };
        anchors.insert(x.to_string(), column);
    }
    anchors
}

fn render_lines_minimal(lines: &[Vec<ProjectionTextBox>]) -> String {
    let mut rendered: Vec<String> = Vec::new();

    for line in lines {
        if line.is_empty() {
            rendered.push(String::new());
            continue;
        }

        let mut out = String::new();
        let mut previous_box: Option<&ProjectionTextBox> = None;

        for bbox in line {
            let normalized = normalize_render_fragment(&bbox.str);
            let visible = normalized.trim();
            if !visible.is_empty() {
                let fragment = if previous_box.is_none() {
                    normalized.trim_start_matches(char::is_whitespace)
                } else {
                    normalized.as_str()
                };

                if let Some(previous) = previous_box {
                    let previous_fragment = normalize_render_fragment(&previous.str);
                    let explicit_boundary_space = previous_fragment
                        .chars()
                        .last()
                        .is_some_and(char::is_whitespace)
                        || fragment.chars().next().is_some_and(char::is_whitespace);

                    let previous_last = previous_fragment.trim_end().chars().last();
                    let current_first = fragment.trim_start().chars().next();
                    let mut spaces = rendered_spaces_between(previous, bbox);
                    if explicit_boundary_space {
                        spaces = 0;
                    } else if spaces == 1
                        && suppress_geometry_space(previous, bbox, previous_last, current_first)
                    {
                        spaces = 0;
                    }
                    if !explicit_boundary_space
                        && spaces == 0
                        && previous_last.is_some_and(|ch| ch.is_lowercase())
                        && current_first.is_some_and(|ch| ch.is_uppercase())
                    {
                        spaces = 1;
                    }
                    if spaces > 0 {
                        out.push_str(&" ".repeat(spaces));
                    }
                }
                out.push_str(fragment);
                previous_box = Some(bbox);
            }
        }

        if out.trim().is_empty() {
            rendered.push(String::new());
        } else {
            rendered.push(normalize_rendered_line(out.trim_end()));
        }
    }

    rendered.join("\n")
}

pub fn project_to_grid(
    _config: &LiteParseConfig,
    page: &ParsedPage,
    projection_boxes: Vec<ProjectionTextBox>,
    prev_anchors: ForwardAnchors,
    _total_pages: usize,
) -> ProjectToGridResult {
    let mut projection_boxes = filter_dot_garbage(projection_boxes);
    projection_boxes = handle_rotation_reading_order(&projection_boxes);

    let widths: Vec<f64> = projection_boxes
        .iter()
        .map(|bbox| bbox.w)
        .filter(|w| *w > 0.0)
        .collect();

    let heights: Vec<f64> = projection_boxes
        .iter()
        .map(|bbox| bbox.h)
        .filter(|h| *h > 0.0)
        .collect();

    let median_width = if widths.is_empty() {
        10.0
    } else {
        let mut ws = widths;
        ws.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ws[ws.len() / 2]
    };

    let median_height = if heights.is_empty() {
        12.0
    } else {
        let mut hs = heights;
        hs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        hs[hs.len() / 2]
    };

    let lines = bbox_to_lines(
        &projection_boxes,
        median_width,
        median_height,
        Some(page.width),
    );
    let lines = split_lines_for_right_rail(lines, page.width, median_width);
    let lines = reflow_two_column_regions(lines, page.width, median_width);

    let forward_anchor_left =
        infer_left_anchors(&projection_boxes, &prev_anchors.forward_anchor_left);

    ProjectToGridResult {
        text: render_lines_minimal(&lines),
        prev_anchors: ForwardAnchors {
            forward_anchor_left,
            forward_anchor_right: prev_anchors.forward_anchor_right,
            forward_anchor_center: prev_anchors.forward_anchor_center,
        },
    }
}

pub fn project_pages_to_grid(pages: &[ParsedPage], config: &LiteParseConfig) -> Vec<ParsedPage> {
    let total_pages = pages.len();
    let mut prev_anchors = ForwardAnchors::default();
    let mut output = Vec::new();

    for page in pages {
        let projection_boxes = build_projection_boxes(&page.text_items);
        let result = project_to_grid(config, page, projection_boxes, prev_anchors, total_pages);

        let mut next_page = page.clone();
        next_page.text = result.text.clone();

        output.push(next_page);
        prev_anchors = result.prev_anchors;
    }

    let mut cleaned = output;
    clean_raw_text(&mut cleaned, config);
    cleaned
}
