//! Port of Go's `path/filepath.Match` and go-git's `index.match`.
//!
//! act never uses a third-party glob library. Both of its glob call sites —
//! `pkg/filecollector` (via go-git's gitignore) and the git index `Glob` — are
//! built on Go's `path/filepath.Match`, and both depend on its exact,
//! backtracking behaviour. A regex, a `glob` crate or Rust's own
//! `Path::matches` all differ on `*` not crossing a path separator, on a
//! malformed `[` class, and on how a trailing `*` interacts with an exhausted
//! name, so the algorithm is ported rather than replaced.
//!
//! Two nearly identical matchers exist upstream and both are preserved:
//!
//! * [`Flavor::FileSystem`] is Go's `filepath.Match`. It short-circuits a
//!   trailing `*`, refuses to let `*` cross [`MAIN_SEPARATOR`], and re-scans
//!   the remaining pattern to decide whether a miss was caused by a malformed
//!   pattern or by a genuine mismatch.
//! * [`Flavor::FullPath`] is go-git's `plumbing/format/index.match`, the same
//!   matcher with the path-awareness removed. `index.Glob` is called with
//!   patterns like `vendor/**`, so `**` must be able to consume `/`.
//!
//! Upstream is a fork of the Go standard library (BSD-style licence, see the
//! `LICENSE` file in the act and go-git trees).
//!
//! Deviations from upstream:
//!
//! * Upstream works on `string` (a byte slice) and advances with
//!   `utf8.DecodeRuneInString`. This port keeps the byte-level scanning and
//!   decodes UTF-8 explicitly, so a multi-byte character is still consumed as
//!   one `?`.
//! * Go returns `ErrBadPattern` from `Match`. Callers in both trees collapse
//!   that to "no match" immediately, so [`MatchResult::is_matched`] is what the
//!   ports of those callers use. The error is still reported rather than
//!   silently dropped, so a future caller can distinguish the two.

/// The OS path separator Go uses in [`Flavor::FileSystem`].
///
/// On Windows Go's `filepath.Separator` is `\`, and — unlike `os.IsPathSeparator`
/// — the matcher compares against this single byte.
pub const MAIN_SEPARATOR: char = std::path::MAIN_SEPARATOR;

/// Which of the two upstream matchers to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// Go's `path/filepath.Match`: `*` never crosses a path separator.
    FileSystem,
    /// go-git's `plumbing/format/index.match`: matches whole paths, so `*`
    /// and `**` also cross `/`.
    FullPath,
}

/// The outcome of a match, mirroring Go's `(matched bool, err error)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// The name matched the pattern.
    Matched,
    /// The name did not match, and the pattern was well formed.
    NoMatch,
    /// The pattern was malformed (Go's `filepath.ErrBadPattern`).
    BadPattern,
}

impl MatchResult {
    /// True only for [`MatchResult::Matched`].
    ///
    /// This is what both ports need: go-git's gitignore and `index.Glob` turn
    /// a `BadPattern` into a plain mismatch, because the error is only ever
    /// produced by user-supplied ignore patterns and never reported further.
    pub fn is_matched(self) -> bool {
        matches!(self, MatchResult::Matched)
    }
}

/// Matches `name` against `pattern` using [`Flavor::FileSystem`], i.e. Go's
/// `filepath.Match`.
pub fn match_path(pattern: &str, name: &str) -> MatchResult {
    match_with(Flavor::FileSystem, pattern, name)
}

/// Matches `name` against `pattern` using [`Flavor::FullPath`], i.e. go-git's
/// `index.match`.
pub fn match_full_path(pattern: &str, name: &str) -> MatchResult {
    match_with(Flavor::FullPath, pattern, name)
}

fn match_with(flavor: Flavor, pattern: &str, name: &str) -> MatchResult {
    let separator = MAIN_SEPARATOR as u8;
    let separator_aware = matches!(flavor, Flavor::FileSystem);
    let mut pattern = pattern.as_bytes();
    let mut name = name.as_bytes();

    'pattern: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(pattern);
        pattern = rest;

        // `filepath.Match` alone short-circuits: a trailing `*` matches the
        // rest of the name as long as the name holds no separator. The index
        // matcher has no such shortcut, because there `*` may cross `/`.
        if separator_aware && star && chunk.is_empty() {
            return if name.contains(&separator) {
                MatchResult::NoMatch
            } else {
                MatchResult::Matched
            };
        }

        // Look for a match at the current position.
        let (t, ok, err) = match_chunk(chunk, name, separator_aware);
        // If this is the last chunk, make sure the name was exhausted too,
        // otherwise we would report a false negative even though the star
        // could still match.
        if ok && (t.is_empty() || !pattern.is_empty()) {
            name = t;
            continue;
        }
        if let Some(result) = err {
            return result;
        }
        if star {
            // Look for a match further along the name. `filepath.Match` stops
            // at a separator; the index matcher does not.
            let mut i = 0;
            while i < name.len() && (!separator_aware || name[i] != separator) {
                let (t, ok, err) = match_chunk(chunk, &name[i + 1..], separator_aware);
                if ok {
                    // If this is the last chunk, make sure the name was
                    // exhausted too.
                    if pattern.is_empty() && !t.is_empty() {
                        i += 1;
                        continue;
                    }
                    name = t;
                    continue 'pattern;
                }
                if let Some(result) = err {
                    return result;
                }
                i += 1;
            }
        }
        return MatchResult::NoMatch;
    }

    if name.is_empty() {
        MatchResult::Matched
    } else {
        MatchResult::NoMatch
    }
}

/// Splits off the next star-prefixed, non-star chunk of `pattern`.
///
/// Port of go-git's `scanChunk`, identical in `path/filepath/match.go`.
fn scan_chunk(pattern: &[u8]) -> (bool, &[u8], &[u8]) {
    let mut pattern = pattern;
    let mut star = false;
    while !pattern.is_empty() && pattern[0] == b'*' {
        pattern = &pattern[1..];
        star = true;
    }
    let mut in_range = false;
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            // On Windows a backslash is the separator, not an escape.
            b'\\' if !cfg!(windows) => {
                // The malformed-pattern check happens in `match_chunk`; here
                // the backslash only hides the next byte from the scan.
                if i + 1 < pattern.len() {
                    i += 1;
                }
            }
            b'[' => in_range = true,
            b']' => in_range = false,
            b'*' if !in_range => break,
            _ => {}
        }
        i += 1;
    }
    (star, &pattern[..i], &pattern[i..])
}

/// Matches `chunk` against the start of `s`, returning the remainder of `s`.
///
/// Port of Go's `matchChunk`, which is what go-git's gitignore uses through
/// `path/filepath.Match`; go-git's own `index.match` is the same function
/// without the separator awareness, selected here by `separator_aware`.
///
/// `chunk` is a run of single-character operators: literals, character classes
/// and `?`. Once the match has failed, the loop keeps walking `chunk` purely
/// to report a malformed pattern, and stops reading `s`.
fn match_chunk<'a>(
    chunk: &[u8],
    s: &'a [u8],
    separator_aware: bool,
) -> (&'a [u8], bool, Option<MatchResult>) {
    let separator = MAIN_SEPARATOR as u8;
    let mut chunk = chunk;
    let mut s = s;
    let mut failed = false;
    while !chunk.is_empty() {
        failed = failed || s.is_empty();
        match chunk[0] {
            b'[' => {
                // Character class.
                let mut r = char::REPLACEMENT_CHARACTER;
                if !failed {
                    let (decoded, n) = decode_rune(s);
                    r = decoded;
                    s = &s[n..];
                }
                chunk = &chunk[1..];
                // Possibly negated.
                let mut negated = false;
                if !chunk.is_empty() && chunk[0] == b'^' {
                    negated = true;
                    chunk = &chunk[1..];
                }
                // Parse all ranges.
                let mut matched = false;
                let mut n_range = 0;
                loop {
                    if !chunk.is_empty() && chunk[0] == b']' && n_range > 0 {
                        chunk = &chunk[1..];
                        break;
                    }
                    let (lo, rest) = match get_esc(chunk) {
                        Ok(value) => value,
                        Err(err) => return (&s[..0], false, Some(err)),
                    };
                    chunk = rest;
                    let mut hi = lo;
                    if chunk[0] == b'-' {
                        match get_esc(&chunk[1..]) {
                            Ok(value) => {
                                hi = value.0;
                                chunk = value.1;
                            }
                            Err(err) => return (&s[..0], false, Some(err)),
                        }
                    }
                    matched = matched || (lo <= r && r <= hi);
                    n_range += 1;
                }
                failed = failed || matched == negated;
            }
            b'?' => {
                if !failed {
                    // `?` stands for exactly one character, and in
                    // `filepath.Match` that character may not be a separator.
                    failed = separator_aware && s[0] == separator;
                    let (_, n) = decode_rune(s);
                    s = &s[n..];
                }
                chunk = &chunk[1..];
            }
            b'\\' if !cfg!(windows) => {
                chunk = &chunk[1..];
                if chunk.is_empty() {
                    return (&s[..0], false, Some(MatchResult::BadPattern));
                }
                // Go falls through into the literal comparison.
                if !failed {
                    failed = chunk[0] != s[0];
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
            literal => {
                if !failed {
                    failed = literal != s[0];
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
        }
    }
    if failed {
        return (&s[..0], false, None);
    }
    (s, true, None)
}

/// Reads a possibly escaped character out of a character class.
///
/// Port of go-git's `getEsc`. Returns the character and the remaining chunk.
fn get_esc(chunk: &[u8]) -> Result<(char, &[u8]), MatchResult> {
    if chunk.is_empty() || chunk[0] == b'-' || chunk[0] == b']' {
        return Err(MatchResult::BadPattern);
    }
    let mut chunk = chunk;
    if chunk[0] == b'\\' && !cfg!(windows) {
        chunk = &chunk[1..];
        if chunk.is_empty() {
            return Err(MatchResult::BadPattern);
        }
    }
    let (r, n) = decode_rune(chunk);
    if r == char::REPLACEMENT_CHARACTER && n == 1 {
        return Err(MatchResult::BadPattern);
    }
    let rest = &chunk[n..];
    if rest.is_empty() {
        return Err(MatchResult::BadPattern);
    }
    Ok((r, rest))
}

/// Decodes the first UTF-8 scalar of `bytes`, returning it and its width.
///
/// Mirrors `utf8.DecodeRuneInString`: invalid input yields
/// ([`char::REPLACEMENT_CHARACTER`], 1).
fn decode_rune(bytes: &[u8]) -> (char, usize) {
    match std::str::from_utf8(bytes) {
        Ok(text) => match text.chars().next() {
            Some(r) => (r, r.len_utf8()),
            None => (char::REPLACEMENT_CHARACTER, 1),
        },
        Err(err) => {
            let valid = err.valid_up_to();
            if valid > 0 {
                let text = unsafe { std::str::from_utf8_unchecked(&bytes[..valid]) };
                match text.chars().next() {
                    Some(r) => (r, r.len_utf8()),
                    None => (char::REPLACEMENT_CHARACTER, 1),
                }
            } else {
                (char::REPLACEMENT_CHARACTER, 1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row of Go's `matchTests` table
    /// (`src/path/filepath/match_test.go`). `bad_pattern` is Go's
    /// `ErrBadPattern`.
    struct Case {
        pattern: &'static str,
        name: &'static str,
        matched: bool,
        bad_pattern: bool,
    }

    const fn case(pattern: &'static str, name: &'static str, matched: bool) -> Case {
        Case { pattern, name, matched, bad_pattern: false }
    }

    const fn bad(pattern: &'static str, name: &'static str) -> Case {
        Case { pattern, name, matched: false, bad_pattern: true }
    }

    /// Go's `matchTests`, verbatim. This is the table go-git's `match` and
    /// therefore act's gitignore matching is derived from, so it is the
    /// acceptance corpus for this port.
    const GO_MATCH_TESTS: &[Case] = &[
        case("abc", "abc", true),
        case("*", "abc", true),
        case("*c", "abc", true),
        case("a*", "a", true),
        case("a*", "abc", true),
        case("a*", "ab/c", false),
        case("a*/b", "abc/b", true),
        case("a*/b", "a/c/b", false),
        case("a*b*c*d*e*/f", "axbxcxdxe/f", true),
        case("a*b*c*d*e*/f", "axbxcxdxexxx/f", true),
        case("a*b*c*d*e*/f", "axbxcxdxe/xxx/f", false),
        case("a*b*c*d*e*/f", "axbxcxdxexxx/fff", false),
        case("a*b?c*x", "abxbbxdbxebxczzx", true),
        case("a*b?c*x", "abxbbxdbxebxczzy", false),
        case("ab[c]", "abc", true),
        case("ab[b-d]", "abc", true),
        case("ab[e-g]", "abc", false),
        case("ab[^c]", "abc", false),
        case("ab[^b-d]", "abc", false),
        case("ab[^e-g]", "abc", true),
        // `a\*b` in a Go string literal is the 4-byte pattern `a\*b`.
        case("a\\*b", "a*b", true),
        case("a\\*b", "ab", false),
        case("a?b", "a\u{263a}b", true),
        case("a[^a]b", "a\u{263a}b", true),
        case("a???b", "a\u{263a}b", false),
        case("a[^a][^a][^a]b", "a\u{263a}b", false),
        case("[a-\u{3b6}]*", "\u{3b1}", true),
        case("*[a-\u{3b6}]", "A", false),
        case("a?b", "a/b", false),
        case("a*b", "a/b", false),
        // `[\]a]` in a Go string literal is the 5-byte pattern `[\]a]`.
        case("[\\]a]", "]", true),
        case("[\\-]", "-", true),
        case("[x\\-]", "x", true),
        case("[x\\-]", "-", true),
        case("[x\\-]", "z", false),
        case("[\\-x]", "x", true),
        case("[\\-x]", "-", true),
        case("[\\-x]", "a", false),
        bad("[]a]", "]"),
        bad("[-]", "-"),
        bad("[x-]", "x"),
        bad("[x-]", "-"),
        bad("[x-]", "z"),
        bad("[-x]", "x"),
        bad("[-x]", "-"),
        bad("[-x]", "a"),
        // `\` alone is a dangling escape.
        bad("\\", "a"),
        bad("[a-b-c]", "a"),
        bad("[", "a"),
        bad("[^", "a"),
        bad("[^bc", "a"),
        bad("a[", "a"),
        bad("a[", "ab"),
        bad("a[", "x"),
        bad("a/b[", "x"),
        case("*x", "xxx", true),
    ];

    /// Replicates Go's `TestMatch` platform handling: on Windows no escape is
    /// allowed, so escape-containing patterns are skipped and both sides are
    /// cleaned to backslashes first.
    fn windows_adjust(pattern: &str, name: &str) -> Option<(String, String)> {
        if cfg!(windows) {
            if pattern.contains('\\') {
                return None;
            }
            return Some((
                pattern.replace('/', "\\"),
                name.replace('/', "\\"),
            ));
        }
        Some((pattern.to_string(), name.to_string()))
    }

    #[test]
    fn go_match_table() {
        for tt in GO_MATCH_TESTS {
            let Some((pattern, name)) = windows_adjust(tt.pattern, tt.name) else {
                continue;
            };
            let result = match_path(&pattern, &name);
            let matched = result.is_matched();
            let got_bad = result == MatchResult::BadPattern;
            assert_eq!(
                matched, tt.matched,
                "Match({pattern:?}, {name:?}) matched: got {matched}, want {}",
                tt.matched
            );
            assert_eq!(
                got_bad, tt.bad_pattern,
                "Match({pattern:?}, {name:?}) error: got BadPattern={got_bad}, want {}",
                tt.bad_pattern
            );
        }
    }

    /// go-git's `plumbing/format/index.TestIndexGlob` fixtures, exercised
    /// through [`match_full_path`]. The index matcher has no path awareness,
    /// so unlike `filepath.Match` its `*` crosses separators.
    #[test]
    fn index_matcher_is_not_path_aware() {
        // `foo/b*` — would match nothing in the filesystem flavour.
        assert!(match_full_path("foo/b*", "foo/bar/bar").is_matched());
        assert!(match_full_path("foo/b*", "foo/baz/qux").is_matched());
        assert!(!match_full_path("foo/b*", "fux").is_matched());

        assert!(match_full_path("f*", "fux").is_matched());
        assert!(match_full_path("f*", "foo/bar/bar").is_matched());
        assert!(match_full_path("f*", "foo/baz/qux").is_matched());

        assert!(match_full_path("f*/baz/q*", "foo/baz/qux").is_matched());
        assert!(!match_full_path("f*/baz/q*", "foo/bar/bar").is_matched());

        // The filesystem flavour refuses to let `*` cross a separator.
        assert!(!match_path("foo/b*", "foo/bar/bar").is_matched());
    }
}
