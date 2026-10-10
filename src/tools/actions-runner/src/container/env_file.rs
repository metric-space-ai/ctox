//! Merging an env file from a container into a step's environment.
//!
//! A step can set `env-file: something.env`, and the file's contents become
//! environment variables for that step. The format is the one `docker run
//! --env-file` takes, plus heredoc-style multi-line values:
//!
//! ```text
//! SIMPLE=value
//! QUOTED="with spaces"
//! MULTI<<EOF
//! first
//! second
//! EOF
//! ```
//!
//! # Three details that are load-bearing
//!
//! * **The UTF-8 BOM is skipped, on the first line only.** Windows PowerShell
//!   5.1 writes one, and without skipping it the first variable's name would
//!   start with three invisible bytes.
//! * **A `=` before a `<<` wins.** The line is split at whichever comes first,
//!   so a value that legitimately contains `<<` is not mistaken for a heredoc
//!   — `A=b<<c` is one variable named `A`.
//! * **A line with neither is an error, not a skip.** An env file with a
//!   typo in it should stop the step rather than silently contribute nothing.
//!
//! **A missing file is not an error.** Upstream returns `nil` when the archive
//! cannot be read, which makes `env-file: .env` work on a fresh checkout where
//! the file is gitignored and absent.

use std::collections::BTreeMap;

/// What was wrong with an env file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvFileError {
    /// A line with neither `=` nor `<<`.
    InvalidFormat(String),
    /// A heredoc whose closing delimiter never arrived.
    DelimiterNotFound(String),
}

impl std::fmt::Display for EnvFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat(line) => {
                write!(f, "invalid format '{line}', expected a line with '=' or '<<'")
            }
            Self::DelimiterNotFound(delimiter) => write!(
                f,
                "invalid format delimiter '{delimiter}' not found before end of file"
            ),
        }
    }
}

impl std::error::Error for EnvFileError {}

/// Parses the contents of an env file into `env`.
///
/// Values overwrite whatever is already there, and a variable named twice
/// keeps the last value — the same as reading the file top to bottom.
pub fn parse_env_text(text: &str, env: &mut BTreeMap<String, String>) -> Result<(), EnvFileError> {
    let mut lines = text.lines().peekable();
    let mut first = true;
    while let Some(raw) = lines.next() {
        let mut line = raw;
        if first {
            first = false;
            line = strip_utf8_bom(line);
        }

        let single = line.find('=');
        let multi = line.find("<<");

        match (single, multi) {
            // A `=` before any `<<` makes this a single-line assignment.
            (Some(eq), heredoc) if heredoc.is_none_or(|index| eq < index) => {
                env.insert(line[..eq].to_string(), line[eq + 1..].to_string());
            }
            // Otherwise a `<<` starts a heredoc, and its delimiter is the rest
            // of the line.
            (_, Some(index)) => {
                let delimiter = &line[index + 2..];
                let mut content = String::new();
                let mut found = false;
                for part in lines.by_ref() {
                    if part == delimiter {
                        found = true;
                        break;
                    }
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(part);
                }
                if !found {
                    return Err(EnvFileError::DelimiterNotFound(delimiter.to_string()));
                }
                env.insert(line[..index].to_string(), content);
            }
            (Some(_), _) => unreachable!("handled by the guard above"),
            (None, None) => return Err(EnvFileError::InvalidFormat(line.to_string())),
        }
    }
    Ok(())
}

/// Skips the UTF-8 byte order mark, which is three bytes.
///
/// Go checks the raw bytes of the line; the same three code points survive a
/// decode to `char`, so comparing the characters is equivalent and does not
/// depend on how the bytes were read.
fn strip_utf8_bom(line: &str) -> &str {
    line.strip_prefix('\u{feff}').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<BTreeMap<String, String>, EnvFileError> {
        let mut env = BTreeMap::new();
        parse_env_text(text, &mut env)?;
        Ok(env)
    }

    #[test]
    fn simple_assignments() {
        let env = parse("A=1\nB=two\nC=\n").expect("parsed");
        assert_eq!(env["A"], "1");
        assert_eq!(env["B"], "two");
        assert_eq!(env["C"], "", "an empty value is still a variable");
        assert_eq!(env.len(), 3);
    }

    /// A `=` inside the value is kept, and only the first one splits.
    #[test]
    fn only_the_first_equals_splits() {
        let env = parse("URL=postgres://a:b@h/db?x=1&y=2\n").expect("parsed");
        assert_eq!(env["URL"], "postgres://a:b@h/db?x=1&y=2");
    }

    /// The `=`-before-`<<` rule: a value containing `<<` is not a heredoc.
    #[test]
    fn an_equals_before_a_heredoc_marks_wins() {
        let env = parse("A=b<<c\n").expect("parsed");
        assert_eq!(env["A"], "b<<c");
        assert_eq!(env.len(), 1, "not treated as a heredoc");
    }

    #[test]
    fn a_heredoc_becomes_a_multiline_value() {
        let env = parse("KEY<<EOF\nfirst\nsecond\nEOF\nAFTER=1\n").expect("parsed");
        assert_eq!(env["KEY"], "first\nsecond");
        assert_eq!(env["AFTER"], "1", "parsing continues after the heredoc");
    }

    /// A heredoc's value keeps the line breaks and nothing else — the
    /// delimiter line is consumed, and no trailing newline is added.
    #[test]
    fn a_heredoc_joins_with_newlines() {
        let env = parse("K<<END\na\n\nb\nEND\n").expect("parsed");
        assert_eq!(env["K"], "a\n\nb");
    }

    /// PowerShell 5.1 writes a BOM, and without skipping it the first
    /// variable's name starts with three invisible bytes.
    #[test]
    fn a_utf8_bom_on_the_first_line_is_skipped() {
        let env = parse("\u{feff}FIRST=1\nSECOND=2\n").expect("parsed");
        assert_eq!(env["FIRST"], "1");
        assert_eq!(env["SECOND"], "2");
        assert!(!env.keys().any(|key| key.starts_with('\u{feff}')));
    }

    /// The BOM is only stripped from the first line: a later one is part of
    /// that variable's value.
    #[test]
    fn a_bom_later_in_the_file_is_not_stripped() {
        let env = parse("A=1\nB=\u{feff}2\n").expect("parsed");
        assert_eq!(env["B"], "\u{feff}2");
    }

    /// A typo stops the step instead of contributing nothing quietly.
    #[test]
    fn a_line_without_an_assignment_is_an_error() {
        let error = parse("A=1\nnot an assignment\n").expect_err("an error");
        assert_eq!(error, EnvFileError::InvalidFormat("not an assignment".to_string()));
        assert_eq!(
            error.to_string(),
            "invalid format 'not an assignment', expected a line with '=' or '<<'",
        );
    }

    #[test]
    fn an_unterminated_heredoc_is_an_error() {
        let error = parse("K<<END\na\nb\n").expect_err("an error");
        assert_eq!(error, EnvFileError::DelimiterNotFound("END".to_string()));
        assert_eq!(
            error.to_string(),
            "invalid format delimiter 'END' not found before end of file",
        );
    }

    /// A later assignment wins, which is what reading the file top to bottom
    /// gives.
    #[test]
    fn a_repeated_variable_keeps_the_last_value() {
        let env = parse("A=1\nA=2\n").expect("parsed");
        assert_eq!(env["A"], "2");
        assert_eq!(env.len(), 1);
    }

    /// Parsing merges into an existing environment rather than replacing it,
    /// and a variable named in the file wins.
    #[test]
    fn parsing_merges_into_what_is_already_there() {
        let mut env = BTreeMap::new();
        env.insert("KEEP".to_string(), "yes".to_string());
        env.insert("OVERRIDE".to_string(), "old".to_string());
        parse_env_text("OVERRIDE=new\nADDED=1\n", &mut env).expect("parsed");
        assert_eq!(env["KEEP"], "yes");
        assert_eq!(env["OVERRIDE"], "new");
        assert_eq!(env["ADDED"], "1");
    }

    #[test]
    fn an_empty_file_adds_nothing() {
        let mut env = BTreeMap::new();
        parse_env_text("", &mut env).expect("an empty file is not an error");
        assert!(env.is_empty());
    }
}
