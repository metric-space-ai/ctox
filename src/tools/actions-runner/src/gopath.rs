//! Port of Go's **`path`** package — `path.Clean`, `path.Join`, `path.Dir`.
//!
//! # This is not `filepath`, and the difference is load-bearing
//!
//! Go ships **two** path packages and they are not the same function:
//!
//! * **`path`** is the *slash* package. Always `/`, never a volume, never a
//!   drive letter, no platform branch anywhere.
//! * **`filepath`** is the *OS* package. `\` on Windows, `C:` volumes, and it
//!   rewrites separators while it cleans.
//!
//! The crate already has the second one as [`crate::artifacts::clean`]. This
//! module is the first, and adding it is not tidying: `runner::step`'s
//! `symlinkJoin` reaches for `path.Join`/`path.Clean`/`path.Dir`, and a port
//! that quietly reached for `filepath` instead would **pass every test on
//! macOS and be wrong on Windows** — because on a Unix host the two agree, so
//! the mistake is invisible exactly where it is easiest to write.
//!
//! The two disagree concretely (measured with go1.26.2):
//!
//! | input | `path.Clean` | `filepath.Clean` on Windows |
//! |---|---|---|
//! | `a\b` | `a\b` — `\` is an ordinary character | `a\b`, but it *is* a separator |
//! | `a\..\b` | `a\..\b` — unchanged | `b` — the `..` is consumed |
//! | `C:/a/b` | `C:/a/b` — slashes kept, no volume | `C:\a\b` — volume recognised, rewritten |
//!
//! Every function here is therefore written on `/` and `str`, never on
//! [`std::path`], which resolves at compile time for the host.
//!
//! # Why `symlinkJoin` needs the slash package at all
//!
//! It is a path-traversal guard, and the paths it guards are *container* paths:
//! an action directory, a `GITHUB_WORKSPACE` inside a Linux image. Those are
//! slash paths on every host, including a Windows one — which is why upstream
//! uses `path` and not `filepath`. Getting this wrong would make the guard
//! compare a backslash path against a forward-slash prefix, reject everything,
//! and fail safe for the wrong reason.
//!
//! Upstream is a fork of the Go standard library (BSD-style licence, see the
//! `LICENSE` file in the act tree).

/// `path.Clean`: the shortest path name equivalent to `p`, purely lexically.
///
/// No filesystem is touched, which is what lets the result be computed for a
/// path that does not exist yet — the whole point for `symlinkJoin`.
///
/// A leading `/` is preserved, `.` elements are dropped, `..` pops the previous
/// element and is itself dropped at the root, and empty input becomes `.`.
/// Empty is the one input with no path in it, so `.` is the honest answer.
pub fn clean(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }

    let rooted = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();

    for element in p.split('/') {
        match element {
            // "." contributes nothing, and an empty element is a doubled
            // separator, which contributes nothing either.
            "" | "." => {}
            ".." => match out.last() {
                // At the root, or already leading with `..`, a `..` cannot be
                // resolved — Go drops it at the root and keeps it in a
                // relative path, where it is still meaningful.
                Some(&"..") | None if !rooted => out.push(".."),
                Some(_) => {
                    out.pop();
                }
                None => {}
            },
            other => out.push(other),
        }
    }

    let joined = out.join("/");
    if rooted {
        return format!("/{joined}");
    }
    if joined.is_empty() {
        return ".".to_string();
    }
    joined
}

/// `path.Join`: the elements joined with `/`, then [`clean`].
///
/// # The two shapes that surprise people, both measured
///
/// * **An empty result stays empty.** `Join("", "")` is `""`, *not* `"."`.
///   `Clean` is applied to the concatenation, and the concatenation of nothing
///   is nothing. `Join` is therefore not `Clean(join_with_slash)` — the
///   difference is visible in exactly one case and it is the empty one.
/// * **A later element is never absolute.** `Join("/a/b", "/c")` is
///   `"/a/b/c"`, not `"/c"`. Go's `path` has no rule that a leading `/` in a
///   later element resets the path, and that is what makes a symlink whose
///   target looks absolute get appended rather than obeyed.
pub fn join(elems: &[&str]) -> String {
    let joined = elems
        .iter()
        .copied()
        .filter(|element| !element.is_empty())
        .collect::<Vec<&str>>()
        .join("/");
    if joined.is_empty() {
        return String::new();
    }
    clean(&joined)
}

/// `path.Dir`: everything before the final `/`, cleaned.
///
/// A path with no `/` has directory `.`, and `/` is its own parent. The result
/// is cleaned, so a trailing `..` in the input does not survive.
pub fn dir(p: &str) -> String {
    match p.rfind('/') {
        Some(index) => clean(&p[..index + 1]),
        None => clean(""),
    }
}

#[cfg(test)]
mod tests {
    use super::{clean, dir, join};

    /// Every row measured against go1.26.2. A disagreement names its row.
    #[test]
    fn clean_reproduces_every_measured_row() {
        for (input, want) in [
            ("", "."),
            (".", "."),
            ("..", ".."),
            ("/", "/"),
            ("//", "/"),
            ("a", "a"),
            ("a/b", "a/b"),
            ("/a/b", "/a/b"),
            ("a/../b", "b"),
            ("/a/../..", "/"),
            ("../../a/b/target", "../../a/b/target"),
            ("/a/b/", "/a/b"),
            ("./", "."),
            ("a/./b", "a/b"),
            ("a//b", "a/b"),
            ("/../a", "/a"),
            ("a/..", "."),
            ("C:/a/b", "C:/a/b"),
            ("a\\b", "a\\b"),
            ("/a/b/c/../../d", "/a/d"),
        ] {
            assert_eq!(clean(input), want, "Clean({input:?})");
        }
    }

    /// The rows that separate this from `filepath.Clean`. On macOS the two
    /// agree, so without them this module could quietly be the wrong one.
    #[test]
    fn a_backslash_is_an_ordinary_character_and_a_drive_is_not_a_volume() {
        assert_eq!(clean("a\\b"), "a\\b");
        // `..` between backslashes is *not* consumed: there is no separator
        // there, so the three names are one component.
        assert_eq!(clean("a\\..\\b"), "a\\..\\b");
        // A drive letter keeps its slashes instead of being rewritten to `\`:
        // this package has no notion of a volume.
        assert_eq!(clean("C:/a/b"), "C:/a/b");
    }

    #[test]
    fn join_reproduces_every_measured_row() {
        for (a, b, want) in [
            ("", "", ""),
            ("", "b", "b"),
            ("a", "", "a"),
            ("a", "b", "a/b"),
            ("/a/b", "../x", "/a/x"),
            ("/a/b", "/a/b/abs", "/a/b/a/b/abs"),
            ("/a/b", "../../etc/passwd", "/etc/passwd"),
            ("/a/b", "sub/target", "/a/b/sub/target"),
            ("/a/b", "../../../a/b/target", "/a/b/target"),
            (".", "target", "target"),
            ("", "/abs", "/abs"),
            ("/a/b", ".", "/a/b"),
            ("/a/b", "..", "/a"),
            ("a", "../..", ".."),
            ("/a/b", "x/../y", "/a/b/y"),
        ] {
            assert_eq!(join(&[a, b]), want, "Join({a:?}, {b:?})");
        }
    }

    /// The empty case, which is the one place `join` is *not* `clean` of the
    /// concatenation. Easy to "fix" into `"."` and thereby change a caller
    /// that tests for emptiness.
    #[test]
    fn joining_nothing_is_empty_and_not_a_dot() {
        assert_eq!(join(&[]), "");
        assert_eq!(join(&["", ""]), "");
        assert_eq!(join(&[""]), "");
        // …while a non-empty result that cleans to nothing is a dot.
        assert_eq!(join(&[".", "."]), ".");
        assert_eq!(join(&["a", ".."]), ".");
    }

    /// A later element never resets the path, even when it looks absolute.
    /// This is the row `symlinkJoin` depends on: a symlink target written as an
    /// absolute path is appended to the link's directory, not obeyed.
    #[test]
    fn a_later_element_is_never_treated_as_absolute() {
        assert_eq!(join(&["/a/b", "/c"]), "/a/b/c");
        assert_eq!(join(&["/a/b", "/a/b/abs"]), "/a/b/a/b/abs");
    }

    #[test]
    fn dir_reproduces_every_measured_row() {
        for (input, want) in [
            ("", "."),
            (".", "."),
            ("a", "."),
            ("/a", "/"),
            ("/a/b", "/a"),
            ("a/b/c", "a/b"),
            ("/", "/"),
            ("a/", "a"),
            ("/a/", "/a"),
            ("..", "."),
            ("../a", ".."),
            ("/a/b/../c", "/a"),
        ] {
            assert_eq!(dir(input), want, "Dir({input:?})");
        }
    }
}
