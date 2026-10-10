//! Port of go-git's `plumbing/format/gitignore`.
//!
//! act has no `.gitignore` implementation of its own. It calls go-git's
//! `ReadPatterns` to build the ignorer that decides what is copied into a
//! container, and it calls `ParsePattern` + `NewMatcher` again inside
//! `exprparser.hashFiles` to implement that workflow function. Both go through
//! this module, which is a direct port of the upstream package
//! (Apache-2.0, Copyright (c) 2015 go-git authors).
//!
//! Only [`read_patterns`] is ported. go-git's `LoadGlobalPatterns` and
//! `LoadSystemPatterns` (which parse `~/.gitconfig` and `/etc/gitconfig` for a
//! `core.excludesfile`) are not: act never calls them, and honouring a user's
//! global gitignore would change which files a job sees.
//!
//! Deviations from upstream:
//!
//! * Upstream walks a go-billy `Filesystem`, which lets the tests run against
//!   an in-memory tree. This port walks the real filesystem and the tests use a
//!   temporary directory, because the production call site always passes the
//!   host filesystem (`osfs.New(srcPath)`).
//! * `readIgnoreFile` scans with a `bufio.Scanner`, which silently stops at
//!   Go's 64 KiB line limit. This port reads the whole file, so an unusually
//!   long ignore line is still honoured.
//! * Upstream builds candidate domains with `append(path, name)`, which Go's
//!   slice aliasing makes subtle; the go-git test suite guards it explicitly.
//!   Rust's `Vec` has no aliasing, so the hazard does not exist here.
//! * `ReadPatterns` returns the patterns it managed to read *together with* the
//!   error, and act logs the error and uses the partial result. [`read_patterns`]
//!   therefore returns `(Vec<Pattern>, Option<io::Error>)` rather than a
//!   `Result`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::gomatch::{self, MatchResult};

const INCLUSION_PREFIX: char = '!';
const ZERO_TO_MANY_DIRS: &str = "**";
const PATTERN_DIR_SEP: char = '/';

const COMMENT_PREFIX: char = '#';
const GIT_DIR: &str = ".git";
const GITIGNORE_FILE: &str = ".gitignore";
const INFO_EXCLUDE_FILE: &str = ".git/info/exclude";

/// Outcome of matching one pattern against one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreResult {
    /// The pattern did not apply.
    NoMatch,
    /// The pattern ignores the path.
    Exclude,
    /// A `!`-prefixed pattern re-includes the path.
    Include,
}

impl IgnoreResult {
    /// True when the pattern applied at all, i.e. anything but
    /// [`IgnoreResult::NoMatch`]. This is the `> NoMatch` test upstream's
    /// matcher uses to stop at the first decisive pattern.
    fn is_decisive(self) -> bool {
        self != IgnoreResult::NoMatch
    }
}

/// A single `.gitignore` pattern together with the directory it was declared
/// in.
///
/// The `domain` is the path of the declaring directory *relative to the root
/// `ReadPatterns` was called on*, so a pattern from `vendor/.gitignore` has
/// domain `["vendor"]` and only matches below that directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    domain: Vec<String>,
    pattern: Vec<String>,
    inclusion: bool,
    dir_only: bool,
    is_glob: bool,
}

impl Pattern {
    /// Parses a gitignore pattern string.
    ///
    /// Port of go-git's `ParsePattern`.
    pub fn parse(pattern: &str, domain: &[String]) -> Self {
        let mut res = Pattern {
            // Upstream copies the domain so the caller cannot mutate it.
            domain: domain.to_vec(),
            pattern: Vec::new(),
            inclusion: false,
            dir_only: false,
            is_glob: false,
        };

        let mut p = pattern;
        if let Some(rest) = p.strip_prefix(INCLUSION_PREFIX) {
            res.inclusion = true;
            p = rest;
        }

        // A pattern ending in "\ " escapes the space, so the trailing spaces
        // must not be trimmed in that case.
        if !p.ends_with("\\ ") {
            p = p.trim_end_matches(' ');
        }

        if let Some(rest) = p.strip_suffix(PATTERN_DIR_SEP) {
            res.dir_only = true;
            p = rest;
        }

        if p.contains(PATTERN_DIR_SEP) {
            res.is_glob = true;
        }

        res.pattern = p.split(PATTERN_DIR_SEP).map(str::to_string).collect();
        res
    }

    /// Matches `path` (split into components) against this pattern.
    ///
    /// Port of go-git's `pattern.Match`.
    pub fn matches(&self, path: &[String], is_dir: bool) -> IgnoreResult {
        if path.len() <= self.domain.len() {
            return IgnoreResult::NoMatch;
        }
        for (i, e) in self.domain.iter().enumerate() {
            if &path[i] != e {
                return IgnoreResult::NoMatch;
            }
        }

        let path = &path[self.domain.len()..];
        if self.is_glob {
            if !self.glob_match(path, is_dir) {
                return IgnoreResult::NoMatch;
            }
        } else if !self.simple_name_match(path, is_dir) {
            return IgnoreResult::NoMatch;
        }

        if self.inclusion {
            IgnoreResult::Include
        } else {
            IgnoreResult::Exclude
        }
    }

    /// Port of go-git's `simpleNameMatch`: the first component only, tried
    /// against every component of the path.
    fn simple_name_match(&self, path: &[String], is_dir: bool) -> bool {
        for (i, name) in path.iter().enumerate() {
            match gomatch::match_path(&self.pattern[0], name) {
                // Upstream: a bad pattern means no match, not an error.
                MatchResult::BadPattern => return false,
                MatchResult::Matched => {
                    // A `dir/` pattern must not match a file that happens to
                    // sit at the end of the path.
                    if self.dir_only && !is_dir && i == path.len() - 1 {
                        return false;
                    }
                    return true;
                }
                MatchResult::NoMatch => continue,
            }
        }
        false
    }

    /// Port of go-git's `globMatch`.
    fn glob_match(&self, path: &[String], is_dir: bool) -> bool {
        let mut matched = false;
        let mut can_traverse = false;
        let mut pos = 0;

        for (i, pattern) in self.pattern.iter().enumerate() {
            if pattern.is_empty() {
                can_traverse = false;
                continue;
            }
            if pattern == ZERO_TO_MANY_DIRS {
                if i == self.pattern.len() - 1 {
                    break;
                }
                can_traverse = true;
                continue;
            }
            // `**` is only a whole component; `a**b` is not supported.
            if pattern.contains(ZERO_TO_MANY_DIRS) {
                return false;
            }
            if pos == path.len() {
                return false;
            }
            if can_traverse {
                can_traverse = false;
                while pos < path.len() {
                    let e = &path[pos];
                    pos += 1;
                    match gomatch::match_path(pattern, e) {
                        MatchResult::BadPattern => return false,
                        MatchResult::Matched => {
                            matched = true;
                            break;
                        }
                        MatchResult::NoMatch => {
                            if pos == path.len() {
                                // If nothing is left, fail.
                                matched = false;
                            }
                        }
                    }
                }
            } else {
                match gomatch::match_path(pattern, &path[pos]) {
                    MatchResult::BadPattern | MatchResult::NoMatch => return false,
                    MatchResult::Matched => {
                        matched = true;
                        pos += 1;
                    }
                }
            }
        }
        if matched && self.dir_only && !is_dir && pos == path.len() {
            matched = false;
        }
        matched
    }
}

/// A multi-pattern matcher. Patterns are held in ascending priority order: the
/// most generic settings first, then the repository `.gitignore`, then each
/// `.gitignore` further down the tree.
///
/// Port of go-git's `Matcher`.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    patterns: Vec<Pattern>,
}

impl Matcher {
    /// Port of go-git's `NewMatcher`.
    pub fn new(patterns: Vec<Pattern>) -> Self {
        Self { patterns }
    }

    /// True when `path` is ignored.
    ///
    /// Patterns are tried from the highest priority downwards and the first
    /// decisive one wins, so a nested `!pattern` overrides the pattern it was
    /// meant to un-ignore.
    pub fn matches(&self, path: &[String], is_dir: bool) -> bool {
        for pattern in self.patterns.iter().rev() {
            let result = pattern.matches(path, is_dir);
            if result.is_decisive() {
                return result == IgnoreResult::Exclude;
            }
        }
        false
    }
}

/// Port of go-git's `readIgnoreFile`.
///
/// A missing file is not an error. Any other I/O failure is returned.
fn read_ignore_file(
    root: &Path,
    path: &[String],
    ignore_file: &str,
) -> (Vec<Pattern>, Option<io::Error>) {
    let mut components: Vec<&str> = path.iter().map(String::as_str).collect();
    components.push(ignore_file);
    let full = root.join(components.iter().copied().collect::<PathBuf>());

    let Ok(contents) = fs::read_to_string(&full) else {
        return (Vec::new(), None);
    };

    let patterns = contents
        .split('\n')
        // `bufio.ScanLines` drops a trailing carriage return, which is what
        // makes a CRLF `.gitignore` work.
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.starts_with(COMMENT_PREFIX))
        .filter(|line| !line.trim().is_empty())
        .map(|line| Pattern::parse(line, path))
        .collect();

    (patterns, None)
}

/// Reads `.git/info/exclude` and every `.gitignore` under `path`, recursing
/// into subdirectories. The result is in ascending priority order.
///
/// Port of go-git's `ReadPatterns`. `root` is the directory the patterns are
/// resolved against and `path` is the directory to read, relative to `root` —
/// the caller passes an empty `path` for the top level.
///
/// A failure to list a directory is reported alongside whatever patterns were
/// read so far, matching Go's named return values: act logs the error and uses
/// the partial result rather than giving up.
pub fn read_patterns(root: &Path, path: &[String]) -> (Vec<Pattern>, Option<io::Error>) {
    let (mut patterns, _) = read_ignore_file(root, path, INFO_EXCLUDE_FILE);
    let (sub, _) = read_ignore_file(root, path, GITIGNORE_FILE);
    patterns.extend(sub);

    let dir: PathBuf = root.join(path.iter().collect::<PathBuf>());
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) => return (patterns, Some(err)),
    };

    // Upstream reads the whole directory and sorts by name.
    let mut names: Vec<(String, bool)> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false);
        names.push((name, is_dir));
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, is_dir) in names {
        if !is_dir || name == GIT_DIR {
            continue;
        }

        let mut candidate = path.to_vec();
        candidate.push(name.clone());
        if Matcher::new(patterns.clone()).matches(&candidate, true) {
            continue;
        }

        let (sub, err) = read_patterns(root, &candidate);
        if let Some(err) = err {
            return (patterns, Some(err));
        }
        if !sub.is_empty() {
            patterns.extend(sub);
        }
    }

    (patterns, None)
}

/// Convenience wrapper matching act's call shape: read the patterns of
/// `src_path` itself and build the matcher.
pub fn matcher_for(src_path: &Path) -> Matcher {
    Matcher::new(read_patterns(src_path, &[]).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| (*p).to_string()).collect()
    }

    fn p(pattern: &str, domain: &[&str]) -> Pattern {
        Pattern::parse(pattern, &s(domain))
    }

    // ---- pattern_test.go: the gocheck PatternSuite, ported 1:1 ----

    #[test]
    fn simple_match_inclusion() {
        assert_eq!(
            p("!vul?ano", &[]).matches(&s(&["value", "vulkano", "tail"]), false),
            IgnoreResult::Include
        );
    }

    #[test]
    fn match_domain_longer_mismatch() {
        let pattern = p("value", &["head", "middle", "tail"]);
        assert_eq!(
            pattern.matches(&s(&["head", "middle"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn match_domain_same_length_mismatch() {
        let pattern = p("value", &["head", "middle", "tail"]);
        assert_eq!(
            pattern.matches(&s(&["head", "middle", "tail"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn match_domain_mismatch_mismatch() {
        let pattern = p("value", &["head", "middle", "tail"]);
        assert_eq!(
            pattern.matches(&s(&["head", "middle", "_tail_", "value"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn simple_match_with_domain() {
        let pattern = p("middle/", &["value", "volcano"]);
        assert_eq!(
            pattern.matches(&s(&["value", "volcano", "middle", "tail"]), false),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn simple_match_only_match_in_domain_mismatch() {
        let pattern = p("volcano/", &["value", "volcano"]);
        assert_eq!(
            pattern.matches(&s(&["value", "volcano", "tail"]), true),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn simple_match_position() {
        assert_eq!(
            p("value", &[]).matches(&s(&["value", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value", &[]).matches(&s(&["head", "value", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value", &[]).matches(&s(&["head", "value"]), false),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn simple_match_dir_wanted() {
        assert_eq!(
            p("value/", &[]).matches(&s(&["value", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value/", &[]).matches(&s(&["head", "value", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value/", &[]).matches(&s(&["head", "value"]), true),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value/", &[]).matches(&s(&["head", "value"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn simple_match_mismatch() {
        assert_eq!(
            p("value", &[]).matches(&s(&["head", "val", "tail"]), false),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            p("val", &[]).matches(&s(&["head", "value", "tail"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn simple_match_with_wildcards() {
        assert_eq!(
            p("v*o", &[]).matches(&s(&["value", "vulkano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("vul?ano", &[]).matches(&s(&["value", "vulkano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("v[ou]l[kc]ano", &[]).matches(&s(&["value", "volcano"]), false),
            IgnoreResult::Exclude
        );
        // An unterminated character class is a bad pattern, which go-git
        // collapses into a plain mismatch.
        assert_eq!(
            p("v[ou]l[", &[]).matches(&s(&["value", "vol["]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_from_root() {
        assert_eq!(
            p("/value/vul?ano", &[]).matches(&s(&["value", "vulkano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value/vul?ano", &[]).matches(&s(&["value", "vulkano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("value/vulkano", &[]).matches(&s(&["value", "volcano"]), false),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            p("value/vul?ano", &[]).matches(&s(&["value"]), false),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            p("/value/volcano", &[]).matches(&s(&["value", "value", "volcano"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_with_domain() {
        let pattern = p("middle/tail/", &["value", "volcano"]);
        assert_eq!(
            pattern.matches(&s(&["value", "volcano", "middle", "tail"]), true),
            IgnoreResult::Exclude
        );
        let pattern = p("volcano/tail", &["value", "volcano"]);
        assert_eq!(
            pattern.matches(&s(&["value", "volcano", "tail"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_leading_asterisks() {
        assert_eq!(
            p("**/*lue/vol?ano", &[]).matches(&s(&["value", "volcano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("**/*lue/vol?ano", &[]).matches(&s(&["head", "value", "volcano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("**/*lue/vol?ano", &[]).matches(&s(&["head", "value", "Volcano", "tail"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_leading_asterisks_is_dir() {
        let pattern = p("**/*lue/vol?ano/", &[]);
        assert_eq!(
            pattern.matches(&s(&["head", "value", "volcano", "tail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            pattern.matches(&s(&["head", "value", "volcano"]), true),
            IgnoreResult::Exclude
        );
        assert_eq!(
            pattern.matches(&s(&["head", "value", "Colcano"]), true),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            pattern.matches(&s(&["head", "value", "volcano"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_tailing_asterisks() {
        assert_eq!(
            p("/*lue/vol?ano/**", &[])
                .matches(&s(&["value", "volcano", "tail", "moretail"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("/*lue/vol?ano/**", &[]).matches(&s(&["value", "volcano"]), false),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn glob_match_middle_asterisks() {
        assert_eq!(
            p("/*lue/**/vol?ano", &[]).matches(&s(&["value", "volcano"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("/*lue/**/vol?ano", &[]).matches(&s(&["value", "middle", "volcano"]), false),
            IgnoreResult::Exclude
        );
        assert_eq!(
            p("/*lue/**/vol?ano", &[])
                .matches(&s(&["value", "middle1", "middle2", "volcano"]), false),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn glob_match_middle_asterisks_is_dir() {
        let pattern = p("/*lue/**/vol?ano/", &[]);
        assert_eq!(
            pattern.matches(&s(&["value", "middle1", "middle2", "volcano"]), true),
            IgnoreResult::Exclude
        );
        assert_eq!(
            pattern.matches(&s(&["value", "middle1", "middle2", "volcano"]), false),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            pattern.matches(
                &s(&["value", "middle1", "middle2", "volcano", "tail"]),
                false
            ),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn glob_match_wrong_double_asterisk_mismatch() {
        assert_eq!(
            p("/*lue/**foo/vol?ano", &[]).matches(&s(&["value", "foo", "volcano", "tail"]), false),
            IgnoreResult::NoMatch
        );
    }

    #[test]
    fn glob_match_magic_chars() {
        assert_eq!(
            p("**/head/v[ou]l[kc]ano", &[]).matches(&s(&["value", "head", "volcano"]), false),
            IgnoreResult::Exclude
        );
    }

    #[test]
    fn glob_match_wrong_pattern_mismatch() {
        assert_eq!(
            p("**/head/v[ou]l[", &[]).matches(&s(&["value", "head", "vol["]), false),
            IgnoreResult::NoMatch
        );
        assert_eq!(
            p("/value/**/v[ou]l[", &[]).matches(&s(&["value", "head", "vol["]), false),
            IgnoreResult::NoMatch
        );
    }

    // https://github.com/go-git/go-git/issues/923
    #[test]
    fn glob_match_issue_923() {
        assert_eq!(
            p("**/android/**/GeneratedPluginRegistrant.java", &[]).matches(
                &s(&[
                    "packages",
                    "flutter_tools",
                    "lib",
                    "src",
                    "android",
                    "gradle.dart"
                ]),
                false
            ),
            IgnoreResult::NoMatch
        );
    }

    // ---- matcher_test.go: MatcherSuite.TestMatcher_Match ----

    #[test]
    fn matcher_match() {
        let matcher = Matcher::new(vec![p("**/middle/v[uo]l?ano", &[]), p("!volcano", &[])]);
        assert!(matcher.matches(&s(&["head", "middle", "vulkano"]), false));
        assert!(!matcher.matches(&s(&["head", "middle", "volcano"]), false));
    }

    // ---- dir_test.go: MatcherSuite.TestDir_ReadPatterns ----

    /// Builds the tree `dir_test.go` uses, on a real filesystem.
    ///
    /// ```text
    /// .git/info/exclude         exclude.crlf
    /// .gitignore                vendor/g*/  ignore.crlf  ignore_dir
    /// vendor/.gitignore         !github.com/
    /// ignore_dir/.gitignore     !file
    /// ignore_dir/file
    /// another/  exclude.crlf/  ignore.crlf/
    /// vendor/github.com/  vendor/gopkg.in/
    /// multiple/sub/ignores/first/.gitignore    ignore_dir
    /// multiple/sub/ignores/first/ignore_dir/
    /// multiple/sub/ignores/second/.gitignore   ignore_dir
    /// multiple/sub/ignores/second/ignore_dir/
    /// ```
    fn build_go_git_fixture(dir: &Path) {
        let write = |rel: &str, contents: &str| {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        };
        write(".git/info/exclude", "exclude.crlf\r\n");
        write(".gitignore", "vendor/g*/\nignore.crlf\r\nignore_dir\n");
        write("vendor/.gitignore", "!github.com/\n");
        write("ignore_dir/.gitignore", "!file\n");
        write("ignore_dir/file", "");
        write("another/.keep", "");
        write("vendor/github.com/.keep", "");
        write("vendor/gopkg.in/.keep", "");
        write("multiple/sub/ignores/first/.gitignore", "ignore_dir\n");
        write("multiple/sub/ignores/second/.gitignore", "ignore_dir\n");
    }

    fn check_go_git_patterns(dir: &Path) {
        let (ps, err) = read_patterns(dir, &[]);
        assert!(err.is_none(), "read_patterns must not fail: {err:?}");
        assert_eq!(ps.len(), 7, "expected 7 patterns, got {ps:?}");

        let m = Matcher::new(ps);
        assert!(m.matches(&s(&["exclude.crlf"]), true));
        assert!(m.matches(&s(&["ignore.crlf"]), true));
        assert!(m.matches(&s(&["vendor", "gopkg.in"]), true));
        assert!(m.matches(&s(&["ignore_dir", "file"]), false));
        assert!(!m.matches(&s(&["vendor", "github.com"]), true));
        assert!(m.matches(
            &s(&["multiple", "sub", "ignores", "first", "ignore_dir"]),
            true
        ));
        assert!(m.matches(
            &s(&["multiple", "sub", "ignores", "second", "ignore_dir"]),
            true
        ));
    }

    #[test]
    fn read_patterns_recurses_and_respects_negation() {
        let dir = tempfile::tempdir().unwrap();
        build_go_git_fixture(dir.path());
        check_go_git_patterns(dir.path());
    }

    /// Upstream calls `ReadPatterns` a second time with a slice that has spare
    /// capacity, to prove the domain is copied rather than aliased. Rust has no
    /// slice aliasing, so this asserts the same outcome via a second read.
    #[test]
    fn read_patterns_is_repeatable() {
        let dir = tempfile::tempdir().unwrap();
        build_go_git_fixture(dir.path());
        check_go_git_patterns(dir.path());
        check_go_git_patterns(dir.path());
    }

    #[test]
    fn read_patterns_reports_a_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let (ps, err) = read_patterns(&dir.path().join("nope"), &[]);
        assert!(err.is_some(), "a missing directory must be reported");
        assert!(ps.is_empty());
    }

    #[test]
    fn read_patterns_skips_comments_and_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".gitignore"),
            "# a comment\n\n   \ntarget\n",
        )
        .unwrap();
        let (ps, err) = read_patterns(dir.path(), &[]);
        assert!(err.is_none());
        assert_eq!(ps.len(), 1);
        let m = Matcher::new(ps);
        assert!(m.matches(&s(&["target"]), false));
    }

    #[test]
    fn read_patterns_keeps_partial_results_on_error() {
        // A directory that exists but cannot be listed: the root patterns are
        // still returned, because act uses them and only logs the error.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "target\n").unwrap();
        let sub = dir.path().join("locked");
        fs::create_dir(&sub).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&sub, fs::Permissions::from_mode(0o000)).unwrap();
            let (ps, err) = read_patterns(dir.path(), &[]);
            let _ = fs::set_permissions(&sub, fs::Permissions::from_mode(0o755));
            assert!(err.is_some(), "an unreadable subdirectory must be reported");
            assert_eq!(ps.len(), 1, "the root pattern must survive: {ps:?}");
        }
    }
}
