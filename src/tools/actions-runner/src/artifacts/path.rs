//! The Go path helpers `safeResolve` is built from.
//!
//! `safeResolve` is the only thing standing between a request and the
//! filesystem, so it is ported exactly rather than approximated: Go's
//! `filepath.Clean` and `filepath.Join` are reimplemented here instead of
//! reaching for `std::path`, because `std::path` refuses to collapse `..` and
//! that is the whole job.
//!
//! The separator follows the host, as Go's does. On Unix `/` separates and a
//! leading `/` roots a path; on Windows both `/` and `\` separate, and a path
//! is rooted by a separator, a drive letter, or a UNC prefix.

/// Whether `c` separates path elements on this platform.
#[inline]
fn is_sep(c: char) -> bool {
    if cfg!(windows) {
        c == '/' || c == '\\'
    } else {
        c == '/'
    }
}

/// `volumeNameLen`: the prefix a `..` must not be allowed to escape.
///
/// Always zero on Unix. On Windows Go peels off a UNC share (`\\server\share`)
/// or a drive (`C:`) before cleaning, which is what stops `\\server\share\..`
/// from naming the parent of the share — a real thing to get right, because
/// Windows build computers are a target and an artifact directory may well be
/// a network share.
#[cfg(not(windows))]
fn volume_len(_path: &str) -> usize {
    0
}

#[cfg(windows)]
fn volume_len(path: &str) -> usize {
    let bytes = path.as_bytes();
    // `\\?\\VOL`, `\\.\\device` and `\\server\share`
    if bytes.len() >= 2 && is_sep(bytes[1] as char) && is_sep(bytes[0] as char) {
        // A single leading separator pair (`\\`) is a rooted path, not a UNC
        // share; Go requires two separators *and* a host and share name.
        let rest = &path[2..];
        let host_end = rest.find(|c| is_sep(c)).unwrap_or(rest.len());
        if host_end == 0 {
            return 0;
        }
        let after_host = &rest[host_end..];
        let share = after_host.trim_start_matches(is_sep);
        if share.is_empty() {
            return 0;
        }
        let share_end = share.find(|c| is_sep(c)).unwrap_or(share.len());
        if share_end == 0 {
            return 0;
        }
        let consumed = 2 + host_end + (after_host.len() - share.len()) + share_end;
        // Go keeps a trailing separator in the volume name.
        let consumed = if path[..consumed].ends_with(is_sep) {
            consumed
        } else if path[consumed..].starts_with(is_sep) {
            consumed + 1
        } else {
            consumed
        };
        return consumed;
    }
    // `C:` or `C:whatever`
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return 2;
    }
    0
}

/// `filepath.Clean`.
///
/// `std::path`'s `components` normalises `.` away but hands `..` back as a
/// `ParentDir` component for the caller to deal with, and `Path::join` then
/// keeps it. Go collapses it lexically, without touching the filesystem, which
/// is what makes `safeResolve` work on a path that does not exist yet.
pub fn clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }

    let volume = volume_len(path);
    let body = &path[volume..];
    if body.is_empty() {
        return path.to_string();
    }
    let rooted = is_sep(body.chars().next().expect("body is not empty"));
    let chars: Vec<char> = path.chars().collect();
    // The volume is a prefix `..` cannot cross, so cleaning starts after it
    // and it is put back at the end.
    let volume_chars = chars.len() - body.chars().count();
    let n = chars.len();
    let mut r = volume_chars;

    let mut out: Vec<char> = Vec::with_capacity(n + 1);
    let mut dotdot = 0usize;
    if rooted {
        out.push(MAIN_SEPARATOR);
        r = volume_chars + 1;
        dotdot = 1;
    }

    while r < n {
        if is_sep(chars[r]) {
            // Empty path element.
            r += 1;
        } else if chars[r] == '.' && (r + 1 == n || is_sep(chars[r + 1])) {
            // `.` element.
            r += 1;
        } else if chars[r] == '.' && chars[r + 1] == '.' && (r + 2 == n || is_sep(chars[r + 2])) {
            // `..` element: remove the last segment that can be removed.
            r += 2;
            if out.len() > dotdot {
                out.pop();
                while out.len() > dotdot && !is_sep(*out.last().expect("non-empty")) {
                    out.pop();
                }
                // Go does not truncate its buffer, it rewinds a write cursor,
                // so the separator it already put down is reused by the next
                // element instead of being doubled. Truncating to a Vec has
                // to drop it explicitly or `a/b/../c` cleans to `a//c`.
                if out.len() > dotdot && is_sep(*out.last().expect("non-empty")) {
                    out.pop();
                }
            } else if !rooted {
                if !out.is_empty() {
                    out.push(MAIN_SEPARATOR);
                }
                out.push('.');
                out.push('.');
                dotdot = out.len();
            }
        } else {
            if (rooted && out.len() != dotdot) || (!rooted && !out.is_empty()) {
                out.push(MAIN_SEPARATOR);
            }
            while r < n && !is_sep(chars[r]) {
                out.push(chars[r]);
                r += 1;
            }
        }
    }

    if out.is_empty() {
        if volume_chars == 0 {
            return ".".to_string();
        }
        // A bare volume with nothing after it, which is `C:` on its own.
        let mut bare: String = chars[..volume_chars].iter().collect();
        bare.push('.');
        return bare;
    }
    if volume_chars == 0 {
        return out.into_iter().collect();
    }
    // Put the volume back in front; nothing above can have removed it.
    let mut full: String = chars[..volume_chars].iter().collect();
    full.extend(out);
    full
}

/// `filepath.Join`.
///
/// Go concatenates every element from the first non-empty one onward with a
/// separator and cleans the result **once**. It does not trim separators off
/// the elements first, which is the part that is easy to get wrong: a leading
/// `"/"` is what roots the result, and an earlier version of this port dropped
/// that slash and let `..` climb out of the base directory.
///
/// Trailing separators do *not* survive, since `Clean` removes them.
pub fn join(parts: &[&str]) -> String {
    let Some(first) = parts.iter().position(|part| !part.is_empty()) else {
        return String::new();
    };
    let joined = parts[first..].join(&MAIN_SEPARATOR.to_string());
    clean(&joined)
}

/// The platform separator, as `os.PathSeparator`.
pub const MAIN_SEPARATOR: char = if cfg!(windows) { '\\' } else { '/' };

/// `filepath.ToSlash`: native separators become `/`.
pub fn to_slash(path: &str) -> String {
    if MAIN_SEPARATOR == '/' {
        return path.to_string();
    }
    path.replace(MAIN_SEPARATOR, "/")
}

/// `filepath.Rel`: one path expressed relative to another.
///
/// `None` is Go's error, which it raises in exactly one situation: the two
/// paths disagree about being rooted, so one is absolute and the other is not
/// (or one is empty). Two absolute paths never fail — an unrelated one comes
/// back as `../..` rather than as an error, and an earlier version of this port
/// returned `None` for it, which made [`crate::container::HostEnvironment`]
/// treat every path outside the workdir as if it could not be related at all.
pub fn rel(base: &str, target: &str) -> Option<String> {
    let base = clean(base);
    let target = clean(target);
    if base == target {
        return Some(".".to_string());
    }

    // `Clean("")` is `"."`, and a relative path is not rooted. Comparing
    // this rather than `starts_with` is what rejects a relative base against
    // an absolute target.
    let base_rooted = base.starts_with(MAIN_SEPARATOR);
    let target_rooted = target.starts_with(MAIN_SEPARATOR);
    if base_rooted != target_rooted {
        return None;
    }

    fn segments(path: &str) -> Vec<&str> {
        let parts: Vec<&str> = path
            .split(MAIN_SEPARATOR)
            .filter(|part| !part.is_empty())
            .collect();
        // `Clean` turns an empty path into `"."`, which means "here", not a
        // directory called `.`.
        if parts == ["."] {
            Vec::new()
        } else {
            parts
        }
    }
    let base_parts = segments(&base);
    let target_parts = segments(&target);

    let common = base_parts
        .iter()
        .zip(target_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut out: Vec<&str> = Vec::new();
    // Whatever is left of the base is walked back out of.
    out.extend(std::iter::repeat_n("..", base_parts.len() - common));
    out.extend_from_slice(&target_parts[common..]);

    if out.is_empty() {
        return Some(".".to_string());
    }
    Some(out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rewrites a Go-style expectation to the host's separator.
    ///
    /// The table was taken from Go 1.26 on Unix. Windows agrees on every
    /// input except the shape of the separator, and the port has to agree on
    /// Windows too, so the expectations are translated rather than branched.
    fn native(path: &str) -> String {
        if MAIN_SEPARATOR == '/' {
            return path.to_string();
        }
        path.replace('/', &MAIN_SEPARATOR.to_string())
    }

    /// `TestMkdirFsImplSafeResolve`, plus the rest of the traversal shapes the
    /// route handlers depend on. Every expectation is Go's output for
    /// `safeResolve("/foo/bar", input)`, captured by running it.
    #[test]
    fn safe_resolve_matches_go() {
        let table: &[(&str, &str)] = &[
            // server_test.go: TestMkdirFsImplSafeResolve
            ("baz", "/foo/bar/baz"),
            ("baz/blue", "/foo/bar/baz/blue"),
            ("baz/../../blue", "/foo/bar/blue"),
            ("../../parent", "/foo/bar/parent"),
            ("/root", "/foo/bar/root"),
            ("/", "/foo/bar"),
            ("", "/foo/bar"),
            // server_test.go: TestDownloadArtifactFileUnsafePath
            // server_test.go: TestArtifactUploadBlobUnsafePath
            ("../../some/file", "/foo/bar/some/file"),
            ("2/../../some/file", "/foo/bar/some/file"),
            // The rest of the table.
            ("some/file", "/foo/bar/some/file"),
            ("./x", "/foo/bar/x"),
            ("a//b", "/foo/bar/a/b"),
            ("a/./b", "/foo/bar/a/b"),
            ("../", "/foo/bar"),
            ("..", "/foo/bar"),
            ("a/b/../../..", "/foo/bar"),
            ("....//....//", "/foo/bar/..../...."),
            ("a/ b/c", "/foo/bar/a/ b/c"),
            ("x.zip", "/foo/bar/x.zip"),
        ];
        for (input, want) in table {
            assert_eq!(
                super::super::safe_resolve("/foo/bar", input),
                native(want),
                "safeResolve(/foo/bar, {input:?})",
            );
        }
    }

    #[test]
    fn clean_matches_go() {
        let table: &[(&str, &str)] = &[
            ("", "."),
            (".", "."),
            ("/", "/"),
            ("//", "/"),
            ("a/../b", "b"),
            ("a/./b", "a/b"),
            ("../a", "../a"),
            ("/../a", "/a"),
            ("a//b", "a/b"),
            ("a/b/../../..", ".."),
            ("...", "..."),
            ("a/..", "."),
            ("/a/..", "/"),
            ("a/b/../c", "a/c"),
        ];
        for (input, want) in table {
            assert_eq!(clean(input), native(want), "Clean({input:?})");
        }
    }

    #[test]
    fn join_matches_go() {
        let table: &[(&str, &str, &str)] = &[
            ("a", "b", "a/b"),
            ("a/", "/b", "a/b"),
            ("a", "", "a"),
            ("", "b", "b"),
            ("/foo/bar", "some/file", "/foo/bar/some/file"),
            ("1", "some/file", "1/some/file"),
            ("a", "../b", "b"),
            ("a", "b/", "a/b"),
            (".", "b", "b"),
        ];
        for (base, rest, want) in table {
            let base = native(base);
            let rest = native(rest);
            assert_eq!(
                join(&[&base, &rest]),
                native(want),
                "Join({base:?}, {rest:?})",
            );
        }
    }

    #[test]
    fn rel_matches_go() {
        // Every expectation is Go 1.26's `filepath.Rel` output.
        let table: &[(&str, &str, Option<&str>)] = &[
            ("/work", "/work", Some(".")),
            ("/work", "/work/", Some(".")),
            // Not a prefix, and still not an error: Go walks back up.
            ("/work", "/somewhere/else", Some("../somewhere/else")),
            ("/work", "/work/sub", Some("sub")),
            ("/a/b", "/a/b/c/d", Some("c/d")),
            ("/a/b", "/a/c", Some("../c")),
            // Rootedness is the only thing that can fail.
            ("/a/b", "relative", None),
            ("relative", "/a/b", None),
            ("", "/a", None),
            ("/a", "", None),
            ("/", "/a", Some("a")),
            ("/a", "/", Some("..")),
        ];
        for (base, target, want) in table {
            assert_eq!(
                rel(&native(base), &native(target)).as_deref(),
                *want,
                "Rel({base:?}, {target:?})",
            );
        }
    }

    #[test]
    fn a_dot_dot_can_never_escape_the_base_directory() {
        // Every way of trying, checked as a property rather than as a table:
        // whatever the request supplies, the result stays under the base.
        let base = "/foo/bar";
        for input in [
            "..",
            "../..",
            "../../../../../../etc/passwd",
            "/etc/passwd",
            "a/../../../..",
            "./.././../x",
        ] {
            let resolved = super::super::safe_resolve(base, input);
            assert!(resolved.starts_with(base), "{input:?} escaped: {resolved}",);
        }
    }

    #[test]
    fn to_slash_leaves_unix_paths_alone() {
        if MAIN_SEPARATOR == '/' {
            assert_eq!(to_slash("a/b/c"), "a/b/c");
        } else {
            assert_eq!(to_slash("a\\b\\c"), "a/b/c");
            assert_eq!(to_slash("a/b/c"), "a/b/c");
        }
    }
}
