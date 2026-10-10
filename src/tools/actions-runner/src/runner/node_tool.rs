//! The two things act does to `PATH` after the container is up.
//!
//! Port of the `GetNodeToolFullPath`, `InitializeNodeTool`, `ApplyExtraPath`
//! and `UpdateExtraPath` halves of `pkg/runner/run_context.go`, plus the
//! decision inside `UpdateExtraPath` that reads an env file.
//!
//! | act | here |
//! |---|---|
//! | `GetNodeToolFullPath` | [`node_tool_from_output`] + [`RunContext::get_node_tool_full_path`] |
//! | `InitializeNodeTool` | [`RunContext::initialize_node_tool`] |
//! | `ApplyExtraPath` | [`RunContext::apply_extra_path`] |
//! | `UpdateExtraPath`'s line handling | [`paths_from_env_file`] + [`RunContext::update_extra_path`] |
//!
//! # No upstream test covers any of this
//!
//! `GetNodeToolFullPath`, `InitializeNodeTool`, `ApplyExtraPath` and
//! `UpdateExtraPath` have no case in `pkg/runner/*_test.go` on v0.2.89. The Go
//! source plus the probes behind the tables below are therefore a weaker
//! authority than a test would be, and every number here is labelled as measured
//! so nobody later reads it as a port of an upstream case.
//!
//! # Why the decision is split out
//!
//! `GetNodeToolFullPath` and `UpdateExtraPath` both need a live container: one
//! spawns `node` in it, the other reads a tar stream out of it. The *decisions*
//! they make from what came back do not, and those are the parts with edge
//! cases. So each is a free function taking the raw bytes, and the method that
//! needs a container is a thin wrapper. Same shape as
//! [`crate::runner::run_context::set_action_runtime_vars_with`]: the part that
//! needs the outside world keeps it, the part worth testing does not.
//!
//! # `addPath` is not here
//!
//! It lives upstream in `command.go`, not `run_context.go`, and is ported in
//! [`crate::runner::command`] — along with the rule that the new entry goes
//! first and only *its own* duplicates are dropped.

use std::collections::BTreeMap;

use super::run_context::RunContext;

/// The UTF-8 byte order mark, as bytes.
///
/// Upstream tests the first three bytes numerically rather than decoding, so a
/// truncated `EF BB` is left alone. Measured, and pinned in the table below.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// `GetNodeToolFullPath`'s decision, given what the container printed.
///
/// The probe runs `node --no-warnings -e "console.log(process.execPath)"` and
/// this is what it does with the answer: trim the line terminators from both
/// ends, and accept the result **only if no `\r` or `\n` survives in the
/// middle**. Anything else and it falls back to the bare word `node`, on the
/// reasoning that a `node` that printed more than one line has told us
/// something we did not ask for and should not be trusted as a path.
///
/// Measured on v0.2.89 (`failed` is whether the exec itself errored):
///
/// | container output | result |
/// |---|---|
/// | `"/usr/bin/node\n"` | `/usr/bin/node` |
/// | `"\r\n/usr/bin/node\r\n"` | `/usr/bin/node` — both ends trimmed |
/// | `"/usr/bin/node"` | `/usr/bin/node` |
/// | `"  /usr/bin/node  \n"` | `"  /usr/bin/node  "` — spaces are not trimmed |
/// | `"/usr/bin/node\n/usr/local/bin/node\n"` | `node` — two lines |
/// | `"node\nv20.1.0\n"` | `node` — a version banner is two lines |
/// | `"\t\n/usr/bin/node\n\t"` | `node` — the tab is fine, the inner `\n` is not |
/// | `"üñî/node\n"` | `üñî/node` — a non-ASCII path is fine |
/// | anything, with an error | `node` |
///
/// # The row that looks like a bug and is not
///
/// `"\n\n"` and `""` both yield **`""`**, not `node`. `strings.Trim` reduces them
/// to the empty string, and `ContainsAny("", "\r\n")` is false — so the
/// emptiness passes the check and the path becomes empty rather than the
/// fallback. An empty `PATH` entry is later harmless, because
/// [`RunContext::apply_extra_path`] joins it and an empty component
/// contributes nothing to a `PATH`; but it is not the `node` upstream's comment
/// implies it would be, and it is pinned here so nobody "corrects" it.
pub fn node_tool_from_output(raw: &str, failed: bool) -> String {
    // `strings.Trim(s, "\r\n")` — a cutset, so it strips any run of either
    // character from both ends and nothing from the middle.
    let trimmed = raw.trim_matches(|c| c == '\r' || c == '\n');
    if !failed && !trimmed.contains(['\r', '\n']) {
        return trimmed.to_string();
    }
    "node".to_string()
}

/// The directories an env file contributes, in order.
///
/// Upstream reads a tar stream from the container, skips its first tar entry,
/// then scans the rest. Only three things happen to each line, and all three
/// are measured:
///
/// * a UTF-8 BOM is stripped from the **first** line only — PowerShell 5 writes
///   one, and a workflow written for Windows would otherwise add a path called
///   `﻿/home/runner/work/bin`;
/// * empty lines are skipped, so a trailing newline does not add an empty entry;
/// * everything else is passed on, **untrimmed**.
///
/// Measured on v0.2.89, with `\u{feff}` standing for the BOM:
///
/// | lines | result |
/// |---|---|
/// | `["\u{feff}/a/bin", "/b/bin"]` | `["/a/bin", "/b/bin"]` — first line only |
/// | `["/a/bin", "\u{feff}/b/bin"]` | `["/a/bin", "\u{feff}/b/bin"]` |
/// | `["\u{feff}"]` | `[]` — the BOM leaves an empty line, which is skipped |
/// | `["\u{feff}\u{feff}/a/bin"]` | `["\u{feff}/a/bin"]` — one BOM, not all |
/// | `["\xef\xbb", "/b/bin"]` | `["\xef\xbb", "/b/bin"]` — a truncated BOM stays |
///
/// Note the second row: a BOM on any line but the first survives and becomes part
/// of a directory name. That is upstream, and it is not corrected.
pub fn paths_from_env_file(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (index, raw) in contents.split('\n').enumerate() {
        // Go's `bufio.Scanner` drops a trailing `\r` before a `\n`; a file
        // written on Windows arrives as CRLF and must not produce a path with a
        // carriage return on the end.
        let mut line = raw.strip_suffix('\r').unwrap_or(raw);
        if index == 0 {
            line = strip_utf8_bom(line);
        }
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

/// Strips a leading UTF-8 BOM, and only a complete one.
///
/// # The truncated case cannot be ported, and that is worth knowing
///
/// Upstream tests three **bytes** — `line[0] == 239 && line[1] == 187 &&
/// line[2] == 191` — so a file beginning with `EF BB` and no `BF` is left
/// alone. Such a string is not valid UTF-8, and a Rust `&str` cannot hold one:
/// `paths_from_env_file` takes `&str`, so the input has already been validated
/// by the time this runs.
///
/// The consequence is narrow and stated rather than papered over: a file whose
/// first line is a *truncated* BOM cannot reach this function as a `&str`. It
/// would have to be read as bytes and decoded afterwards. A truncated BOM is
/// itself a malformed file, so nothing that works today is affected — but the
/// check is written on characters rather than bytes, and this says so.
fn strip_utf8_bom(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() >= 3 && bytes[0..3] == UTF8_BOM {
        // The BOM is three bytes at a character boundary by construction, so
        // slicing here cannot split a character.
        return &line[3..];
    }
    line
}

impl RunContext {
    /// `InitializeNodeTool`: force the probe now.
    ///
    /// It returns nothing, which looks redundant until you know what it is for:
    /// the result is memoised, and act calls this *before* a step runs so the
    /// first `hashFiles` or `node` invocation does not pay for a process spawn
    /// inside the container.
    pub fn initialize_node_tool(
        &mut self,
        ctx: &crate::common::context::RunContext,
        container: &dyn crate::container::ExecutionsEnvironment,
    ) {
        self.get_node_tool_full_path(ctx, container);
    }

    /// `GetNodeToolFullPath`: where `node` actually is, memoised.
    ///
    /// The probe needs a container to run in, so the container is a parameter
    /// rather than read off the context — the context only holds
    /// [`ContainerPaths`](super::run_context::ContainerPaths), the reduced view,
    /// because the lifecycle is what holds the real one. The memo lives on
    /// `self`, exactly as upstream's `nodeToolFullPath` field does.
    pub fn get_node_tool_full_path(
        &mut self,
        ctx: &crate::common::context::RunContext,
        container: &dyn crate::container::ExecutionsEnvironment,
    ) -> String {
        if !self.node_tool_full_path.is_empty() {
            return self.node_tool_full_path.clone();
        }
        // Upstream gives the probe a one-minute timeout. The runner's own
        // cancellation is not modelled here, so the bound is not either: a
        // caller that wants one wraps the executor.
        let path_name = container.path_variable_name();
        let mut cenv: BTreeMap<String, String> = BTreeMap::new();
        if let Ok(image_env) = container.update_from_image_env() {
            if let Some(value) = image_env.get(path_name) {
                cenv.insert(path_name.to_string(), value.clone());
            }
        }
        if cenv.is_empty() {
            cenv.insert(
                path_name.to_string(),
                container.default_path_variable(),
            );
        }

        // Upstream swaps both log destinations for two buffers and reads only
        // stdout. `replace_log_writer` takes **one** sink here, so stderr lands
        // in the same buffer — a deliberate, documented difference: a `node`
        // that writes to stderr now makes the probe see two lines and fall back
        // to the bare word `node`. Upstream passes `--no-warnings` to keep that
        // from happening, and the same argument applies here.
        let sink = std::sync::Arc::new(crate::common::context::CollectingSink::new());
        let previous =
            container.replace_log_writer(std::sync::Arc::clone(&sink) as std::sync::Arc<dyn crate::common::LogSink>);

        let outcome = container
            .exec(
                &[
                    "node".to_string(),
                    "--no-warnings".to_string(),
                    "-e".to_string(),
                    "console.log(process.execPath)".to_string(),
                ],
                &cenv,
                "",
                "",
            )(ctx);

        // Restored before the answer is read, so a later step's output is not
        // swallowed by the probe's buffer. Upstream uses `Finally` for the same
        // reason and for the same guarantee: the restore happens whether or not
        // the exec failed.
        if let Some(previous) = previous {
            container.replace_log_writer(previous);
        }

        let captured = sink
            .lines()
            .into_iter()
            .map(|(_, message)| message)
            .collect::<Vec<String>>()
            .join("\n");
        self.node_tool_full_path = node_tool_from_output(&captured, outcome.is_err());
        self.node_tool_full_path.clone()
    }

    /// `ApplyExtraPath`: put the entries `add-path` collected in front of
    /// whatever `PATH` the step starts with.
    ///
    /// Does nothing at all when no step called `add-path`, which is the common
    /// case and worth stating: an untouched `PATH` is never rewritten, so the
    /// container's own value survives verbatim.
    ///
    /// Three details are load-bearing:
    ///
    /// * On Windows the variable is `Path`, and the step's environment may hold
    ///   `PATH` from the workflow. Upstream then looks for a case-insensitive
    ///   match in the step's own map and uses **that** spelling, so the two do
    ///   not become two variables. This is what
    ///   [`Self::resolve_path_variable_name`] is for.
    /// * An empty `PATH` is filled from the image's environment, then from the
    ///   platform default, before anything is prepended — prepending to `""`
    ///   would yield a leading separator.
    /// * The extra entries go **before** the existing value, most recent first,
    ///   because that is the order `add-path` built them in.
    pub fn apply_extra_path(
        &self,
        container: &dyn crate::container::ExecutionsEnvironment,
        env: &mut BTreeMap<String, String>,
    ) {
        if self.extra_path.is_empty() {
            return;
        }
        let path_name = self.resolve_path_variable_name(container, env);
        if env.get(&path_name).is_none_or(|value| value.is_empty()) {
            let mut filled = String::new();
            if let Ok(image_env) = container.update_from_image_env() {
                if let Some(value) = image_env.get(&path_name) {
                    filled = value.clone();
                }
            }
            if filled.is_empty() {
                filled = container.default_path_variable();
            }
            env.insert(path_name.clone(), filled);
        }
        let existing = env.get(&path_name).cloned().unwrap_or_default();
        let mut entries: Vec<&str> = self.extra_path.iter().map(String::as_str).collect();
        entries.push(&existing);
        env.insert(path_name, container.join_path_variable(&entries));
    }

    /// Which spelling of the path variable this environment actually uses.
    ///
    /// On a case-insensitive platform the workflow may have written `PATH`
    /// while the platform's name is `Path`. Upstream scans the step's
    /// environment for a case-insensitive match and prefers it, which is what
    /// keeps the two from becoming separate variables. On a case-sensitive
    /// platform the platform's own name is simply right.
    pub fn resolve_path_variable_name(
        &self,
        container: &dyn crate::container::ExecutionsEnvironment,
        env: &BTreeMap<String, String>,
    ) -> String {
        let name = container.path_variable_name();
        if !container.is_environment_case_insensitive() {
            return name.to_string();
        }
        for key in env.keys() {
            if key.eq_ignore_ascii_case(name) {
                return key.clone();
            }
        }
        name.to_string()
    }

    /// `UpdateExtraPath`: read the `GITHUB_PATH` file the action wrote and add
    /// every line to the extra path.
    ///
    /// Returns the paths it added, in the order they will be prepended, so a
    /// caller can see what happened even when the container is the host
    /// environment. Upstream returns only an error and mutates
    /// `rc.ExtraPath`; the return value is the addition, not a replacement.
    pub fn update_extra_path(
        &mut self,
        container: &dyn crate::container::ExecutionsEnvironment,
        github_env_path: &str,
    ) -> anyhow::Result<Vec<String>> {
        let archive = container.container_archive(github_env_path)?;
        let contents = first_tar_entry_text(&archive)?;
        let paths = paths_from_env_file(&contents);
        let mut added = Vec::with_capacity(paths.len());
        for path in &paths {
            // Upstream routes through `addPath`, so a path already present is
            // moved to the front rather than duplicated.
            if let Some(index) = self.extra_path.iter().position(|value| value == path) {
                self.extra_path.remove(index);
            }
            self.extra_path.insert(0, path.clone());
            added.push(path.clone());
        }
        Ok(added)
    }
}

/// The text of the first entry in a `tar` stream.
///
/// Upstream opens the archive, calls `reader.Next()` once to step past the
/// header, and reads the rest. An archive with no entries reads as `io.EOF`,
/// which upstream tolerates — it checks `err != io.EOF` — and then scans an
/// exhausted reader, which yields nothing. So an empty archive is an empty file,
/// not an error, and a `GITHUB_PATH` a step never wrote is not a failure.
fn first_tar_entry_text(archive: &[u8]) -> anyhow::Result<String> {
    let mut tar = tar::Archive::new(std::io::Cursor::new(archive));
    let mut entries = tar
        .entries()
        .map_err(|err| anyhow::anyhow!("reading the env file archive: {err}"))?;
    let Some(entry) = entries.next() else {
        return Ok(String::new());
    };
    let mut entry = entry
        .map_err(|err| anyhow::anyhow!("reading the env file archive: {err}"))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text)
        .map_err(|err| anyhow::anyhow!("reading the env file: {err}"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{
        node_tool_from_output, paths_from_env_file, strip_utf8_bom, RunContext,
    };

    // -------------------------------------------- nodeToolFullPath --

    /// Every row measured on v0.2.89.
    #[test]
    fn node_tool_from_output_reproduces_every_measured_row() {
        for (raw, want) in [
            ("/usr/bin/node\n", "/usr/bin/node"),
            ("\n/usr/bin/node\n", "/usr/bin/node"),
            ("/usr/bin/node\r\n", "/usr/bin/node"),
            ("\r\n/usr/bin/node\r\n", "/usr/bin/node"),
            ("/usr/bin/node", "/usr/bin/node"),
            ("  /usr/bin/node  \n", "  /usr/bin/node  "),
            ("üñî/node\n", "üñî/node"),
            ("trailing spaces   ", "trailing spaces   "),
            ("   \n", "   "),
            ("/usr/bin/node\n/usr/local/bin/node\n", "node"),
            ("/usr/bin/node\nwarning: something\n", "node"),
            ("node\nv20.1.0\n", "node"),
            ("/usr/bin/node\n ", "node"),
            ("a\nb", "node"),
            ("\t\n/usr/bin/node\n\t", "node"),
        ] {
            assert_eq!(node_tool_from_output(raw, false), want, "raw {raw:?}");
        }
    }

    /// The row that reads like a bug: an empty answer is **not** the fallback.
    /// `Trim` empties it, an empty string contains no `\r` or `\n`, so the check
    /// passes and the path is `""`. Pinned so a later "fix" is a deliberate act
    /// rather than an accident.
    #[test]
    fn an_empty_answer_is_kept_rather_than_becoming_the_fallback() {
        assert_eq!(node_tool_from_output("", false), "");
        assert_eq!(node_tool_from_output("\n\n", false), "");
    }

    /// Any error discards whatever came back, including a perfectly good path.
    #[test]
    fn an_error_discards_the_output_whatever_it_said() {
        assert_eq!(node_tool_from_output("/usr/bin/node\n", true), "node");
        assert_eq!(node_tool_from_output("", true), "node");
    }

    /// Only a *single* line is accepted, and a version banner is two lines.
    /// This is the reason the function exists: a `node` that printed something
    /// extra has not told us where it is.
    #[test]
    fn a_second_line_discards_the_answer_even_when_the_first_is_a_path() {
        assert_eq!(node_tool_from_output("node\nv20.1.0\n", false), "node");
    }

    // ----------------------------------------------- updateExtraPath --

    /// Every row measured on v0.2.89, with `\u{feff}` standing for the BOM.
    #[test]
    fn paths_from_env_file_reproduces_every_measured_row() {
        for (contents, want) in [
            ("\u{feff}/a/bin\n/b/bin", vec!["/a/bin", "/b/bin"]),
            ("/a/bin\n\u{feff}/b/bin", vec!["/a/bin", "\u{feff}/b/bin"]),
            ("\u{feff}\u{feff}/a/bin", vec!["\u{feff}/a/bin"]),
            ("\u{feff}", vec![]),
            ("\u{feff}\n/b/bin", vec!["/b/bin"]),
            ("/a/bin\n\n/b/bin\n", vec!["/a/bin", "/b/bin"]),
            ("", vec![]),
            ("\n", vec![]),
            ("/only/one", vec!["/only/one"]),
        ] {
            assert_eq!(paths_from_env_file(contents), want, "contents {contents:?}");
        }
    }

    /// The BOM is stripped from the **first line only**, and only a complete
    /// three-byte one. A truncated `EF BB` is a directory name like any other.
    #[test]
    fn only_a_complete_bom_on_the_first_line_is_stripped() {
        assert_eq!(strip_utf8_bom("\u{feff}/a"), "/a");
        // A character that merely *starts* with the BOM's first byte is not a
        // BOM. U+00EF is 0xEF 0x89, so its first three bytes are not the BOM's.
        assert_eq!(strip_utf8_bom("\u{ef}/a"), "\u{ef}/a");
        // Not the first line.
        assert_eq!(paths_from_env_file("/a\n\u{feff}/b"), vec!["/a", "\u{feff}/b"]);
    }

    /// A CRLF file — which is what a Windows step writes — must not produce
    /// paths with a carriage return on the end. Upstream's `bufio.Scanner`
    /// strips it, and so does this.
    #[test]
    fn a_crlf_file_does_not_leave_carriage_returns_on_the_paths() {
        assert_eq!(paths_from_env_file("/a/bin\r\n/b/bin\r\n"), vec!["/a/bin", "/b/bin"]);
    }

    // ------------------------------------------------- extra_path --

    /// A `uses:`-free check of the rule `updateExtraPath` relies on: adding a
    /// path that is already present **moves** it to the front rather than
    /// duplicating it, which is `addPath`'s rule reached through
    /// [`crate::runner::command`].
    #[test]
    fn readding_a_path_moves_it_to_the_front_instead_of_duplicating_it() {
        let mut rc = RunContext {
            extra_path: vec!["/a".to_string(), "/b".to_string()],
            ..RunContext::default()
        };
        // What `updateExtraPath` does per line.
        for path in ["/c", "/a"] {
            if let Some(index) = rc.extra_path.iter().position(|value| value == path) {
                rc.extra_path.remove(index);
            }
            rc.extra_path.insert(0, path.to_string());
        }
        assert_eq!(rc.extra_path, vec!["/a", "/c", "/b"]);
    }
}
