use once_cell::sync::Lazy;
use regex::Regex;

static NUMERICISH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:[*+-]?[\d,.\s]+[%]?$|[*]?-?[\d,.\s]+$|[ZN]/A$|[Z-]$)").unwrap());

pub fn clean_ocr_table_artifacts(text: &str) -> String {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return String::new();
    }

    let stripped = trimmed
        .trim_start_matches(|c: char| matches!(c, '|' | '[' | ']' | '(' | ')' | '{' | '}'))
        .trim_end_matches(|c: char| matches!(c, '|' | '[' | ']' | '(' | ')' | '{' | '}'))
        .trim();

    if NUMERICISH_RE.is_match(stripped) {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn to_subscript_string(input: &str) -> String {
    input.chars().map(to_subscript_char).collect()
}

pub fn to_superscript_string(input: &str) -> String {
    input.chars().map(to_superscript_char).collect()
}

fn to_subscript_char(c: char) -> char {
    match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => c,
    }
}

fn to_superscript_char(c: char) -> char {
    match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        _ => c,
    }
}
