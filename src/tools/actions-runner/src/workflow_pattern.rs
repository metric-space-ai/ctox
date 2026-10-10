//! Port of `nektos/act` `pkg/workflowpattern`.
//!
//! Compiles GitHub Actions path/branch filter patterns (`paths`,
//! `paths-ignore`, `branches`, `branches-ignore`) into anchored regular
//! expressions and evaluates them.
//!
//! Deliberate differences from the Go original:
//!
//! * The pattern is iterated as `char`s instead of bytes, so a multi-byte
//!   character is escaped as a whole. For the ASCII patterns GitHub
//!   documents this is byte-for-byte identical, and it removes a latent
//!   split-UTF-8 case. Error positions remain byte offsets, as upstream.
//! * Upstream collects compilation errors in a `map[int]string` and joins
//!   them in Go's randomised map order. Here a `BTreeMap` yields
//!   position-sorted messages, so failures are deterministic.

use std::collections::BTreeMap;
use std::fmt;

use regex::Regex;

/// Sink for the trace lines act emits while evaluating a filter.
///
/// Upstream is an `Info(string, ...interface{})` variadic interface; the
/// port formats the line at the call site instead.
pub trait TraceWriter {
    fn info(&self, message: &str);
}

/// Discards trace output (upstream `EmptyTraceWriter`).
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyTraceWriter;

impl TraceWriter for EmptyTraceWriter {
    fn info(&self, _message: &str) {}
}

/// Writes trace output to stdout (upstream `StdOutTraceWriter`).
#[derive(Debug, Default, Clone, Copy)]
pub struct StdOutTraceWriter;

impl TraceWriter for StdOutTraceWriter {
    fn info(&self, message: &str) {
        println!("{message}");
    }
}

/// A single position-tagged pattern validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    /// Byte offset into the pattern, as reported by upstream.
    pub position: usize,
    /// Human-readable reason, verbatim from act.
    pub message: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Position: {} Error: {}", self.position, self.message)
    }
}

/// A pattern that could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPattern {
    /// The pattern with any leading `!` negation marker removed.
    pub pattern: String,
    /// Position-sorted validation failures.
    pub errors: Vec<PatternError>,
}

impl InvalidPattern {
    /// The failures joined the way act formats them.
    pub fn detail(&self) -> String {
        self.errors
            .iter()
            .map(PatternError::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for InvalidPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Pattern '{}': {}", self.pattern, self.detail())
    }
}

impl std::error::Error for InvalidPattern {}

/// A compiled GitHub Actions filter pattern.
#[derive(Debug, Clone)]
pub struct WorkflowPattern {
    /// The pattern text with the negation marker stripped.
    pub pattern: String,
    /// True when the source pattern was prefixed with `!`.
    pub negative: bool,
    regex: Regex,
}

impl WorkflowPattern {
    /// Compiles one raw pattern, honouring a leading `!` negation marker.
    pub fn compile(raw_pattern: &str) -> Result<Self, InvalidPattern> {
        let (negative, pattern) = match raw_pattern.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, raw_pattern),
        };
        let regex = pattern_to_regex(pattern)?;
        Ok(Self {
            pattern: pattern.to_string(),
            negative,
            regex,
        })
    }

    /// Tests one candidate path or branch name against the compiled regex.
    pub fn matches(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }
}

/// Compiles every pattern, failing on the first invalid one.
pub fn compile_patterns<'a, I>(patterns: I) -> Result<Vec<WorkflowPattern>, InvalidPattern>
where
    I: IntoIterator<Item = &'a str>,
{
    patterns
        .into_iter()
        .map(WorkflowPattern::compile)
        .collect()
}

/// Compiles a single raw pattern.
pub fn compile_pattern(raw_pattern: &str) -> Result<WorkflowPattern, InvalidPattern> {
    WorkflowPattern::compile(raw_pattern)
}

/// Translates a GitHub Actions glob pattern into an anchored regex.
///
/// Port of act's `PatternToRegex`.
fn pattern_to_regex(pattern: &str) -> Result<Regex, InvalidPattern> {
    let chars: Vec<char> = pattern.chars().collect();
    let len = chars.len();

    // Byte offset of every character, plus a trailing end offset, so error
    // positions keep upstream's byte semantics.
    let mut offsets: Vec<usize> = Vec::with_capacity(len + 1);
    let mut acc = 0usize;
    for c in &chars {
        offsets.push(acc);
        acc += c.len_utf8();
    }
    offsets.push(acc);

    let mut out = String::with_capacity(pattern.len() + 2);
    out.push('^');
    let mut errors: BTreeMap<usize, &'static str> = BTreeMap::new();
    let mut pos = 0usize;

    while pos < len {
        match chars[pos] {
            '*' => {
                if pos + 1 < len && chars[pos + 1] == '*' {
                    if pos + 2 < len && chars[pos + 2] == '/' {
                        // `**/` also matches zero path segments.
                        out.push_str("(.+/)?");
                        pos += 3;
                    } else {
                        out.push_str(".*");
                        pos += 2;
                    }
                } else {
                    // A single `*` never crosses a path separator.
                    out.push_str("[^/]*");
                    pos += 1;
                }
            }
            '+' | '?' => {
                // Leading quantifiers are literals, as in act.
                if pos > 0 {
                    out.push(chars[pos]);
                } else {
                    out.push_str(&quote_meta(chars[pos]));
                }
                pos += 1;
            }
            '[' => {
                out.push('[');
                pos += 1;
                if pos < len && chars[pos] == ']' {
                    // Upstream records the error and leaves the bracket
                    // unterminated; the non-empty error set rejects the
                    // pattern regardless.
                    errors.insert(offsets[pos], "Unexpected empty brackets '[]'");
                    pos += 1;
                    continue;
                }
                let start_pos = pos;
                while pos < len && chars[pos] != ']' {
                    if chars[pos] == '-' {
                        if pos <= start_pos || pos + 1 >= len {
                            errors.insert(offsets[pos], "Invalid range");
                            pos += 1;
                            continue;
                        }
                        let (lo, hi) = (chars[pos - 1], chars[pos + 1]);
                        let in_range = |a: char, b: char| valid_char(a, b, lo) && valid_char(a, b, hi) && lo <= hi;
                        if !in_range('A', 'z') && !in_range('0', '9') {
                            errors.insert(
                                offsets[pos],
                                "Ranges can only include a-z, A-Z, A-z, and 0-9",
                            );
                            pos += 1;
                            continue;
                        }
                        out.push('-');
                        out.push(hi);
                        pos += 2;
                    } else {
                        let c = chars[pos];
                        if !valid_char('A', 'z', c) && !valid_char('0', '9', c) {
                            errors.insert(offsets[pos], "Ranges can only include a-z, A-Z and 0-9");
                            pos += 1;
                            continue;
                        }
                        out.push_str(&quote_meta(c));
                        pos += 1;
                    }
                }
                if pos >= len || chars[pos] != ']' {
                    errors.insert(offsets[pos], "Missing closing bracket ']' after '['");
                    pos += 1;
                }
                out.push(']');
                pos += 1;
            }
            '\\' => {
                if pos + 1 >= len {
                    errors.insert(offsets[pos], "Missing symbol after \\");
                    pos += 1;
                    continue;
                }
                out.push_str(&quote_meta(chars[pos + 1]));
                pos += 2;
            }
            other => {
                out.push_str(&quote_meta(other));
                pos += 1;
            }
        }
    }

    if !errors.is_empty() {
        return Err(InvalidPattern {
            pattern: pattern.to_string(),
            errors: errors
                .into_iter()
                .map(|(position, message)| PatternError {
                    position,
                    message: message.to_string(),
                })
                .collect(),
        });
    }

    out.push('$');
    Regex::new(&out).map_err(|err| InvalidPattern {
        pattern: pattern.to_string(),
        errors: vec![PatternError {
            position: 0,
            message: format!("{err}"),
        }],
    })
}

/// `test` lies within the inclusive `a..=b` range.
fn valid_char(a: char, b: char, test: char) -> bool {
    test >= a && test <= b
}

/// Escapes regex metacharacters exactly as Go's `regexp.QuoteMeta` does.
///
/// Note that `-` is deliberately *not* escaped, matching upstream.
fn quote_meta(c: char) -> String {
    const SPECIAL: &[char] = &[
        '\\', '.', '+', '*', '?', '(', ')', '|', '[', ']', '{', '}', '^', '$',
    ];
    if SPECIAL.contains(&c) {
        let mut escaped = String::with_capacity(c.len_utf8() + 1);
        escaped.push('\\');
        escaped.push(c);
        escaped
    } else {
        c.to_string()
    }
}

/// Returns true when the workflow should be skipped (`paths` / `branches`).
///
/// Every matching pattern is applied, so a later negative pattern can undo an
/// earlier positive match. Any file that ends up matched keeps the workflow
/// running.
pub fn skip(sequence: &[WorkflowPattern], input: &[String], trace: &dyn TraceWriter) -> bool {
    if sequence.is_empty() {
        return false;
    }
    for file in input {
        let mut matched = false;
        for item in sequence {
            if item.matches(file) {
                if item.negative {
                    matched = false;
                    trace.info(&format!("{file} excluded by pattern {}", item.pattern));
                } else {
                    matched = true;
                    trace.info(&format!("{file} included by pattern {}", item.pattern));
                }
            }
        }
        if matched {
            return false;
        }
    }
    true
}

/// Returns true when the workflow should be skipped (`paths-ignore` / `branches-ignore`).
///
/// A file is ignored when some pattern matches it and that pattern is not
/// negated. Every file must be ignored for the workflow to be skipped.
pub fn filter(sequence: &[WorkflowPattern], input: &[String], trace: &dyn TraceWriter) -> bool {
    if sequence.is_empty() {
        return false;
    }
    for file in input {
        let mut matched = false;
        for item in sequence {
            if item.matches(file) != item.negative {
                trace.info(&format!("{file} ignored by pattern {}", item.pattern));
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Case {
        patterns: &'static [&'static str],
        inputs: &'static [&'static str],
        skip_result: bool,
        filter_result: bool,
    }

    fn owned(inputs: &[&str]) -> Vec<String> {
        inputs.iter().map(|s| (*s).to_string()).collect()
    }

    /// Upstream `TestMatchPattern`, case for case.
    #[test]
    fn match_pattern_matches_upstream_expectations() {
        let cases = vec![
            Case { patterns: &["*"], inputs: &["path/with/slash"], skip_result: true, filter_result: false },
            Case { patterns: &["path/a", "path/b", "path/c"], inputs: &["meta", "path/b", "otherfile"], skip_result: false, filter_result: false },
            Case { patterns: &["path/a", "path/b", "path/c"], inputs: &["path/b"], skip_result: false, filter_result: true },
            Case { patterns: &["path/a", "path/b", "path/c"], inputs: &["path/c", "path/b"], skip_result: false, filter_result: true },
            Case { patterns: &["path/a", "path/b", "path/c"], inputs: &["path/c", "path/b", "path/a"], skip_result: false, filter_result: true },
            Case { patterns: &["path/a", "path/b", "path/c"], inputs: &["path/c", "path/b", "path/d", "path/a"], skip_result: false, filter_result: false },
            Case { patterns: &[], inputs: &[], skip_result: false, filter_result: false },
            Case { patterns: &["\\!file"], inputs: &["!file"], skip_result: false, filter_result: true },
            Case { patterns: &["escape\\\\backslash"], inputs: &["escape\\backslash"], skip_result: false, filter_result: true },
            Case { patterns: &[".yml"], inputs: &["fyml"], skip_result: true, filter_result: false },
            // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-branches-and-tags
            Case { patterns: &["feature/*"], inputs: &["feature/my-branch"], skip_result: false, filter_result: true },
            Case { patterns: &["feature/*"], inputs: &["feature/your-branch"], skip_result: false, filter_result: true },
            Case { patterns: &["feature/**"], inputs: &["feature/beta-a/my-branch"], skip_result: false, filter_result: true },
            Case { patterns: &["feature/**"], inputs: &["feature/mona/the/octocat"], skip_result: false, filter_result: true },
            Case { patterns: &["main", "releases/mona-the-octocat"], inputs: &["main"], skip_result: false, filter_result: true },
            Case { patterns: &["main", "releases/mona-the-octocat"], inputs: &["releases/mona-the-octocat"], skip_result: false, filter_result: true },
            Case { patterns: &["*"], inputs: &["main"], skip_result: false, filter_result: true },
            Case { patterns: &["*"], inputs: &["releases"], skip_result: false, filter_result: true },
            Case { patterns: &["**"], inputs: &["all/the/branches"], skip_result: false, filter_result: true },
            Case { patterns: &["**"], inputs: &["every/tag"], skip_result: false, filter_result: true },
            Case { patterns: &["*feature"], inputs: &["mona-feature"], skip_result: false, filter_result: true },
            Case { patterns: &["*feature"], inputs: &["feature"], skip_result: false, filter_result: true },
            Case { patterns: &["*feature"], inputs: &["ver-10-feature"], skip_result: false, filter_result: true },
            Case { patterns: &["v2*"], inputs: &["v2"], skip_result: false, filter_result: true },
            Case { patterns: &["v2*"], inputs: &["v2.0"], skip_result: false, filter_result: true },
            Case { patterns: &["v2*"], inputs: &["v2.9"], skip_result: false, filter_result: true },
            Case { patterns: &["v[12].[0-9]+.[0-9]+"], inputs: &["v1.10.1"], skip_result: false, filter_result: true },
            Case { patterns: &["v[12].[0-9]+.[0-9]+"], inputs: &["v2.0.0"], skip_result: false, filter_result: true },
            // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-file-paths
            Case { patterns: &["*"], inputs: &["README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["*"], inputs: &["server.rb"], skip_result: false, filter_result: true },
            Case { patterns: &["*.jsx?"], inputs: &["page.js"], skip_result: false, filter_result: true },
            Case { patterns: &["*.jsx?"], inputs: &["page.jsx"], skip_result: false, filter_result: true },
            Case { patterns: &["**"], inputs: &["all/the/files.md"], skip_result: false, filter_result: true },
            Case { patterns: &["*.js"], inputs: &["app.js"], skip_result: false, filter_result: true },
            Case { patterns: &["*.js"], inputs: &["index.js"], skip_result: false, filter_result: true },
            Case { patterns: &["**.js"], inputs: &["index.js"], skip_result: false, filter_result: true },
            Case { patterns: &["**.js"], inputs: &["js/index.js"], skip_result: false, filter_result: true },
            Case { patterns: &["**.js"], inputs: &["src/js/app.js"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/*"], inputs: &["docs/README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/*"], inputs: &["docs/file.txt"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/**"], inputs: &["docs/README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/**"], inputs: &["docs/mona/octocat.txt"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/**/*.md"], inputs: &["docs/README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/**/*.md"], inputs: &["docs/mona/hello-world.md"], skip_result: false, filter_result: true },
            Case { patterns: &["docs/**/*.md"], inputs: &["docs/a/markdown/file.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/docs/**"], inputs: &["docs/hello.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/docs/**"], inputs: &["dir/docs/my-file.txt"], skip_result: false, filter_result: true },
            Case { patterns: &["**/docs/**"], inputs: &["space/docs/plan/space.doc"], skip_result: false, filter_result: true },
            Case { patterns: &["**/README.md"], inputs: &["README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/README.md"], inputs: &["js/README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/*src/**"], inputs: &["a/src/app.js"], skip_result: false, filter_result: true },
            Case { patterns: &["**/*src/**"], inputs: &["my-src/code/js/app.js"], skip_result: false, filter_result: true },
            Case { patterns: &["**/*-post.md"], inputs: &["my-post.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/*-post.md"], inputs: &["path/their-post.md"], skip_result: false, filter_result: true },
            Case { patterns: &["**/migrate-*.sql"], inputs: &["migrate-10909.sql"], skip_result: false, filter_result: true },
            Case { patterns: &["**/migrate-*.sql"], inputs: &["db/migrate-v1.0.sql"], skip_result: false, filter_result: true },
            Case { patterns: &["**/migrate-*.sql"], inputs: &["db/sept/migrate-v1.sql"], skip_result: false, filter_result: true },
            Case { patterns: &["*.md", "!README.md"], inputs: &["hello.md"], skip_result: false, filter_result: true },
            Case { patterns: &["*.md", "!README.md"], inputs: &["README.md"], skip_result: true, filter_result: true },
            Case { patterns: &["*.md", "!README.md"], inputs: &["docs/hello.md"], skip_result: true, filter_result: true },
            Case { patterns: &["*.md", "!README.md", "README*"], inputs: &["hello.md"], skip_result: false, filter_result: true },
            Case { patterns: &["*.md", "!README.md", "README*"], inputs: &["README.md"], skip_result: false, filter_result: true },
            Case { patterns: &["*.md", "!README.md", "README*"], inputs: &["README.doc"], skip_result: false, filter_result: true },
        ];

        let trace = EmptyTraceWriter;
        for case in cases {
            let label = case.patterns.join(",");
            let patterns = compile_patterns(case.patterns.iter().copied())
                .unwrap_or_else(|err| panic!("{label}: compile failed: {err}"));
            let inputs = owned(case.inputs);

            assert_eq!(
                case.skip_result,
                skip(&patterns, &inputs, &trace),
                "{label}: skipResult"
            );
            assert_eq!(
                case.filter_result,
                filter(&patterns, &inputs, &trace),
                "{label}: filterResult"
            );
        }
    }

    #[test]
    fn empty_brackets_are_rejected() {
        let err = compile_pattern("[]").expect_err("empty brackets must fail");
        assert_eq!(err.pattern, "[]");
        assert_eq!(err.errors.len(), 1);
        assert_eq!(err.errors[0].position, 1);
        assert_eq!(err.errors[0].message, "Unexpected empty brackets '[]'");
    }

    #[test]
    fn missing_closing_bracket_is_rejected() {
        let err = compile_pattern("[abc").expect_err("unterminated bracket must fail");
        assert_eq!(err.errors[0].message, "Missing closing bracket ']' after '['");
        assert_eq!(err.errors[0].position, 4);
    }

    #[test]
    fn dangling_escape_is_rejected() {
        let err = compile_pattern("abc\\").expect_err("dangling escape must fail");
        assert_eq!(err.errors[0].message, "Missing symbol after \\");
    }

    #[test]
    fn invalid_range_is_rejected() {
        let err = compile_pattern("[-a]").expect_err("leading dash must fail");
        assert_eq!(err.errors[0].message, "Invalid range");
    }

    #[test]
    fn reversed_range_is_rejected() {
        let err = compile_pattern("[9-0]").expect_err("reversed range must fail");
        assert_eq!(
            err.errors[0].message,
            "Ranges can only include a-z, A-Z, A-z, and 0-9"
        );
    }

    #[test]
    fn non_alphanumeric_class_member_is_rejected() {
        // Note act validates class members with a raw `A..=z` byte range, which
        // also admits `[\]^_` and backtick. Only characters outside that span
        // are rejected, so the fixture uses `!`.
        let err = compile_pattern("[a!]").expect_err("punctuation must fail");
        assert_eq!(
            err.errors[0].message,
            "Ranges can only include a-z, A-Z and 0-9"
        );
    }

    #[test]
    fn ascii_range_check_admits_punctuation_between_z_and_a() {
        // Upstream quirk preserved deliberately: `_` sits between `Z` and `a`
        // in ASCII and therefore passes act's range check.
        let pattern = compile_pattern("[a_]").expect("underscore is inside A..z");
        assert!(pattern.matches("_"));
        assert!(pattern.matches("a"));
    }

    #[test]
    fn error_positions_are_byte_offsets() {
        // A multi-byte character before the failure shifts the byte offset but
        // not the character offset.
        let err = compile_pattern("ä\\").expect_err("dangling escape must fail");
        assert_eq!(err.errors[0].position, 2);
    }

    #[test]
    fn multi_byte_patterns_are_escaped_whole() {
        let pattern = compile_pattern("ä.md").expect("multi-byte pattern must compile");
        assert!(pattern.matches("ä.md"));
        assert!(!pattern.matches("a.md"));
    }

    #[test]
    fn negation_marker_is_stripped_and_recorded() {
        let pattern = compile_pattern("!README.md").expect("negated pattern must compile");
        assert!(pattern.negative);
        assert_eq!(pattern.pattern, "README.md");
    }

    #[test]
    fn single_star_does_not_cross_separators() {
        let pattern = compile_pattern("*.js").expect("pattern must compile");
        assert!(pattern.matches("index.js"));
        assert!(!pattern.matches("src/index.js"));
    }

    #[test]
    fn double_star_crosses_separators() {
        let pattern = compile_pattern("**.js").expect("pattern must compile");
        assert!(pattern.matches("index.js"));
        assert!(pattern.matches("src/js/app.js"));
    }

    #[test]
    fn dash_is_not_escaped_like_upstream() {
        let pattern = compile_pattern("**/migrate-*.sql").expect("pattern must compile");
        assert!(pattern.matches("db/sept/migrate-v1.sql"));
    }
}
