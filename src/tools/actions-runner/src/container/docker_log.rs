//! `logDockerResponse`, as act uses it: the Docker log stream, turned into log
//! lines.
//!
//! When act attaches to a container, or waits on a `docker pull`, the daemon
//! answers with a stream of newline-delimited JSON — one object per line, each
//! carrying a `stream`, a `status`, a `progress`, or an `error`. This module is
//! what turns that wire format into something a person reads.
//!
//! Upstream is a single function over a live `io.ReadCloser`:
//!
//! ```go
//! func logDockerResponse(logger logrus.FieldLogger, dockerResponse io.ReadCloser, isError bool) error
//! ```
//!
//! # The one real deviation: it is split in two
//!
//! This crate has no logrus and is blocking where `bollard` is async, so the
//! single upstream function is split at the only seam that matters:
//!
//! * [`handle_line`] is the whole of the upstream *logic* — one JSON line in,
//!   zero or one log line out, and a `Result` that is the failure act reports.
//!   It is pure, and it is what the tests below pin.
//! * [`log_docker_response`] is the loop that was left over: scan lines off a
//!   buffer, hand each to [`handle_line`], stop at the first error.
//!
//! That split is not a preference. Upstream's loop is unobservable apart from
//! the fact that it *stops early*, and the one thing worth testing — which
//! branch a given line takes, and what exactly is logged — is entirely inside
//! the line. Splitting it is what makes the behaviour testable without a
//! daemon, a socket, or a thread.
//!
//! # The reset is structural here
//!
//! Upstream allocates **one** `dockerMessage` before the loop and blanks all
//! six fields at the top of every iteration:
//!
//! ```go
//! msg.ID = ""
//! msg.Stream = ""
//! msg.Error = ""
//! msg.ErrorDetail.Message = ""
//! msg.Status = ""
//! msg.Progress = ""
//! ```
//!
//! That is not tidiness, it is load-bearing: `json.Unmarshal` only writes the
//! keys that are present, so without the blanking a `progress` from line *N*
//! would still be sitting in the struct when line *N+1* was unmarshalled, and a
//! bare `{"status":"Pulling"}` would be logged as a progress line. Here every
//! line is parsed into a **fresh** [`DockerMessage`] whose fields all default
//! to `""`, so a field absent from this line cannot be anything but empty. The
//! test `a_field_absent_from_one_line_does_not_leak_into_the_next` is the
//! reason this is worth saying out loud.
//!
//! # Three upstream quirks reproduced on purpose
//!
//! **1. The `errorDetail` branch returns the wrong field.** It logs
//! `msg.ErrorDetail.Message` and then returns `errors.New(msg.Error)` — the
//! *other* field, which is usually empty. So a pull that fails with only an
//! `errorDetail` logs a real message and then returns an **empty error**.
//! This looks like an upstream typo, and it is kept: a caller that switches on
//! the returned string sees exactly what act's caller saw. See
//! [`handle_line`].
//!
//! **2. A line that is not JSON is skipped, not fatal.** It is logged at
//! debug and the loop continues. The daemon's stream is not guaranteed to be
//! well-formed JSON at every line, and upstream treats a bad line as noise.
//!
//! **3. The status formats end in `\n`.** `"%s :: %s\n"` is logged *with* the
//! newline, and logrus's formatter adds another, so every progress line is
//! followed by a blank line upstream. The trailing `\n` is kept, because the
//! blank line is what act's output actually looks like.
//!
//! # Smaller things, all in the code
//!
//! * [`ErrorDetail::message`] has **no** json tag upstream, so `encoding/json`
//!   matches the key `Message` case-insensitively. See the field's doc comment.
//! * Go's `json.Unmarshal` accepts a `null` line as a no-op; serde would reject
//!   it. [`handle_line`] deserializes into an `Option` so `null` is the empty
//!   message, as upstream.
//! * `writeLog`'s `isError` chooses between `Errorf` and `Debugf`. That is
//!   [`Level::Error`] versus [`Level::Debug`] on a [`LogSink`]. Note that
//!   upstream passes `false` unconditionally on the two "I could not make sense
//!   of this" branches, so an unparseable line is **never** logged at error
//!   level, even when the stream is the error stream.

use serde::Deserialize;

use crate::common::{Level, LogSink};

/// `const logPrefix = "  \U0001F433  "` — two spaces, 🐳, two spaces.
///
/// It is declared in `docker_logger.go` but not used *in* it; the rest of
/// `pkg/container` prefixes its own messages with it (`docker run.go`,
/// `docker_pull.go`, `docker_build.go`, `docker_volume.go`). It is part of
/// this module's port because it is defined here and because the padding is
/// user-visible on every line those files emit.
///
/// Go writes the escape as `\U0001F433`; Rust spells the same code point
/// `\u{1F433}`.
pub const LOG_PREFIX: &str = "  \u{1F433}  ";

/// One line of the Docker log stream, decoded.
///
/// Every field below carries the name its json tag has upstream, plus the
/// alternative casings as aliases: `encoding/json` falls back to a
/// case-insensitive match for *every* field, tag or no tag, while serde matches
/// the name exactly. Go would also accept `iD` or `Id`; the alias lists do not,
/// and no practical Docker line uses them.
///
/// The struct is deliberately **not** reused across lines — see the module note
/// on the reset. Every field defaults to `""`, which is what makes an absent
/// key indistinguishable from an empty one, exactly as the blanked struct was.
#[derive(Debug, Default, Deserialize)]
struct DockerMessage {
    /// The layer or step this line is about.
    #[serde(default, rename = "id", alias = "ID")]
    id: String,
    /// Raw output from the container's stdout/stderr.
    #[serde(default, rename = "stream", alias = "Stream")]
    stream: String,
    /// A hard failure reported by the daemon.
    #[serde(default, rename = "error", alias = "Error")]
    error: String,
    /// The structured form of the same failure.
    #[serde(default, rename = "errorDetail", alias = "ErrorDetail")]
    error_detail: ErrorDetail,
    /// A human-readable step description, e.g. `Pulling fs layer`.
    #[serde(default, rename = "status", alias = "Status")]
    status: String,
    /// How far along that step is, e.g. `[==>  ] 1.2MB/8MB`.
    #[serde(default, rename = "progress", alias = "Progress")]
    progress: String,
}

/// `ErrorDetail struct { Message string }` — no json tag upstream.
///
/// That omission is the interesting part. `encoding/json` matches a struct
/// field with no tag by **field name**, and does so case-insensitively, so
/// `{"message": "…"}`, `{"Message": "…"}` and `{"MESSAGE": "…"}` all land in
/// this field — and the daemon emits the lowercase spelling while act's own
/// reasoning is about `Message`. serde would match `Message` alone, so the
/// daemon's actual spelling is added as an alias here.
#[derive(Debug, Default, Deserialize)]
struct ErrorDetail {
    /// `Message`.
    #[serde(default, rename = "Message", alias = "message", alias = "MESSAGE")]
    message: String,
}

/// `writeLog`: pick the level, then log.
///
/// Upstream branches between `logger.Errorf` and `logger.Debugf`; the level is
/// the only thing the two differ by, so that is the whole body.
fn write_log(sink: &dyn LogSink, is_error: bool, message: &str) {
    sink.log(if is_error { Level::Error } else { Level::Debug }, message);
}

/// One line of the Docker stream: log it, and say whether the stream failed.
///
/// This is the body of upstream's loop, with the loop taken off. `is_error`
/// selects the level for the branches that carry stream content, and is
/// upstream's own `isError` parameter — an `error` stream logs its lines at
/// error level.
///
/// The `Err` is the error act propagates, so it is a plain `String` and nothing
/// else: it is the decoded field, not a wrapper.
pub fn handle_line(line: &[u8], is_error: bool, sink: &dyn LogSink) -> Result<(), String> {
    // A `null` line is a successful no-op for `json.Unmarshal` — the struct
    // simply stays blank — so it is `Ok(None)` here rather than a parse error.
    // Everything else that fails to decode takes the debug-and-continue branch.
    let msg = match serde_json::from_slice::<Option<DockerMessage>>(line) {
        Ok(Some(msg)) => msg,
        Ok(None) => DockerMessage::default(),
        Err(err) => {
            // Go's `%v` here is `*json.SyntaxError` or `*json.UnmarshalTypeError`,
            // whose text comes from Go's own scanner. serde's text is used
            // instead: the sentence around it is the same, the error itself is
            // not, and reproducing `encoding/json`'s error strings would mean
            // porting its parser. The line is the user-visible part and it is
            // byte-exact.
            write_log(
                sink,
                false,
                &format!(
                    "Unable to unmarshal line [{}] ==> {}",
                    String::from_utf8_lossy(line),
                    err
                ),
            );
            return Ok(());
        }
    };

    if !msg.error.is_empty() {
        write_log(sink, is_error, &msg.error);
        return Err(msg.error);
    }

    if !msg.error_detail.message.is_empty() {
        write_log(sink, is_error, &msg.error_detail.message);
        // Upstream returns `msg.Error` here and not the message it has just
        // logged. When a line carries an `errorDetail` but no `error` — which
        // is the usual shape — this returns an **empty** string. That is an
        // upstream inconsistency, reproduced deliberately: the logged text and
        // the returned error are meant to be the same thing, and a port that
        // "fixed" it would stop matching act. See
        // `an_error_detail_line_returns_an_empty_error_even_though_it_logged_a_message`.
        return Err(msg.error);
    }

    if !msg.status.is_empty() {
        // The trailing `\n` is in the format string upstream, so it is in the
        // logged message here. logrus adds its own newline, which is why act's
        // progress output is double-spaced.
        if !msg.progress.is_empty() {
            write_log(
                sink,
                is_error,
                &format!("{} :: {} :: {}\n", msg.status, msg.id, msg.progress),
            );
        } else {
            write_log(sink, is_error, &format!("{} :: {}\n", msg.status, msg.id));
        }
    } else if !msg.stream.is_empty() {
        write_log(sink, is_error, &msg.stream);
    } else {
        // Upstream passes `false` here regardless of `isError`: a line it
        // cannot classify is never an error, even on the error stream.
        write_log(
            sink,
            false,
            &format!("Unable to handle line: {}", String::from_utf8_lossy(line)),
        );
    }

    Ok(())
}

/// `bufio.Scanner`'s `ScanLines`, for a buffer that is already whole.
///
/// bollard delivers a `Stream` of byte chunks and this crate blocks, so the
/// bytes are collected before they are scanned. That is what makes the loop
/// in [`log_docker_response`] testable with a literal.
///
/// The three behaviours that matter, all inherited from `ScanLines`:
///
/// * a trailing newline does **not** produce a final empty line
/// * a blank line in the middle **is** produced
/// * a trailing `\r` is dropped, so `\r\n` splits the same way `\n` does
///
/// Upstream's `bufio.Scanner` also has a 64KiB token limit and act ignores
/// `scanner.Err()`, so a longer line is silently dropped upstream. This
/// iterator has no limit; no Docker progress line is anywhere near 64KiB.
fn scan_lines(body: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = body;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let (line, remainder) = match rest.iter().position(|byte| *byte == b'\n') {
            Some(index) => (&rest[..index], &rest[index + 1..]),
            // The last line needs no terminator, and the buffer is now spent.
            None => (rest, &rest[rest.len()..]),
        };
        rest = remainder;
        Some(line.strip_suffix(b"\r").unwrap_or(line))
    })
}

/// `logDockerResponse` over a whole buffer: log every line, stop at the first
/// failure.
///
/// Upstream takes an `io.ReadCloser` — a live stream — and this takes the bytes
/// that stream produced. The body is the loop that was split off, and its two
/// observable behaviours are preserved: lines are processed in order, and the
/// first line carrying an `error` or an `errorDetail` ends the call with that
/// error, leaving the rest of the stream unread. Reaching the end without a
/// failure is `Ok(())`.
///
/// The error type is `anyhow::Error` rather than the `String` [`handle_line`]
/// returns, because the one caller — the Docker back-end's `ImagePull` — is
/// already an `anyhow` executor. The `String` is the decoded field verbatim and
/// nothing wraps it, so the message that propagates is the field's own text.
///
/// The empty-error quirk above survives that conversion: a line with only an
/// `errorDetail` yields `anyhow!("")`, which is still a non-`nil` error, so the
/// loop still stops. Treating an empty message as success would be a silent
/// behaviour change and is deliberately not done here.
pub fn log_docker_response(body: &[u8], is_error: bool, sink: &dyn LogSink) -> anyhow::Result<()> {
    for line in scan_lines(body) {
        handle_line(line, is_error, sink).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::CollectingSink;

    /// Feed one line that must log and must **not** fail.
    ///
    /// `handle_line`'s `Result` is part of the behaviour being pinned, so it is
    /// asserted rather than dropped — a line that unexpectedly reports an error
    /// would otherwise fail the test silently.
    fn ok_line(line: &[u8], is_error: bool, sink: &CollectingSink) {
        assert_eq!(
            handle_line(line, is_error, sink),
            Ok(()),
            "line: {}",
            String::from_utf8_lossy(line)
        );
    }

    /// The whole level scheme, in one call: `writeLog`'s `isError` is the only
    /// difference between the two branches.
    #[test]
    fn write_log_picks_the_level_from_is_error() {
        let sink = CollectingSink::new();
        write_log(&sink, true, "at error");
        write_log(&sink, false, "at debug");
        assert_eq!(sink.messages_at(Level::Error), ["at error"]);
        assert_eq!(sink.messages_at(Level::Debug), ["at debug"]);
    }

    /// The padding is user-visible, and it is two spaces either side.
    #[test]
    fn the_log_prefix_is_two_spaces_a_whale_and_two_spaces() {
        assert_eq!(LOG_PREFIX, "  \u{1F433}  ");
        assert_eq!(LOG_PREFIX.chars().count(), 5);
    }

    /// `"%s :: %s\n"` — a status with no progress, and the trailing newline
    /// that makes act's output double-spaced.
    #[test]
    fn a_status_line_logs_status_and_id() {
        let sink = CollectingSink::new();
        let outcome = handle_line(br#"{"id":"abc123","status":"Downloading"}"#, false, &sink);
        assert_eq!(outcome, Ok(()));
        assert_eq!(sink.messages_at(Level::Debug), ["Downloading :: abc123\n"]);
        assert!(sink.messages_at(Level::Error).is_empty());
    }

    /// `"%s :: %s :: %s\n"` — status, id and progress.
    #[test]
    fn a_status_and_progress_line_logs_all_three() {
        let sink = CollectingSink::new();
        ok_line(
            br#"{"id":"abc123","status":"Downloading","progress":"[==>  ] 1.2MB/8MB"}"#,
            false,
            &sink,
        );
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["Downloading :: abc123 :: [==>  ] 1.2MB/8MB\n"]
        );
    }

    /// `"%s"` on the stream field, verbatim — the container's own output, with
    /// nothing added to it.
    #[test]
    fn a_stream_line_is_logged_verbatim() {
        let sink = CollectingSink::new();
        ok_line(br#"{"stream":"hello from the container\n"}"#, false, &sink);
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["hello from the container\n"]
        );
    }

    /// `isError` is the level selector for stream content, so the same line
    /// lands at error level on the error stream.
    #[test]
    fn an_error_stream_logs_its_content_at_error_level() {
        let sink = CollectingSink::new();
        ok_line(br#"{"stream":"on stderr\n"}"#, true, &sink);
        assert_eq!(sink.messages_at(Level::Error), ["on stderr\n"]);
        assert!(sink.messages_at(Level::Debug).is_empty());
    }

    /// The `error` branch: log it, then return it. This is the error act
    /// propagates, so the returned string is the field itself.
    #[test]
    fn an_error_line_is_logged_and_returned() {
        let sink = CollectingSink::new();
        let outcome = handle_line(
            br#"{"error":"pull access denied for ghcr.io","id":"x"}"#,
            true,
            &sink,
        );
        assert_eq!(outcome, Err("pull access denied for ghcr.io".to_string()));
        assert_eq!(
            sink.messages_at(Level::Error),
            ["pull access denied for ghcr.io"]
        );
    }

    /// **The quirk, stated exactly.** The `errorDetail` branch logs
    /// `msg.ErrorDetail.Message` and returns `msg.Error` — a different field.
    /// But the `error` branch is checked *first* and returns, so this branch is
    /// only reachable when `msg.Error` is already `""`. The consequence is that
    /// an `errorDetail` line **always** yields an empty error, however
    /// informative the message it just logged was.
    #[test]
    fn an_error_detail_line_returns_an_empty_error_even_though_it_logged_a_message() {
        let sink = CollectingSink::new();
        let outcome = handle_line(
            br#"{"errorDetail":{"message":"no matching manifest for ubuntu-latest"}}"#,
            true,
            &sink,
        );
        assert_eq!(outcome, Err(String::new()));
        assert_eq!(
            sink.messages_at(Level::Error),
            ["no matching manifest for ubuntu-latest"]
        );
    }

    /// The ordering that makes the above the only reachable case: a line
    /// carrying both fields takes the `error` branch, so the detail message is
    /// logged nowhere and never returned.
    #[test]
    fn the_error_field_wins_when_a_line_carries_both() {
        let sink = CollectingSink::new();
        let outcome = handle_line(
            br#"{"error":"the error field","errorDetail":{"message":"the detail message"}}"#,
            true,
            &sink,
        );
        assert_eq!(outcome, Err("the error field".to_string()));
        assert_eq!(sink.messages_at(Level::Error), ["the error field"]);
    }

    /// Upstream blanks all six fields at the top of every iteration. A field
    /// absent from this line must not survive from the last one, so the second
    /// line is a plain status line and not a progress line.
    #[test]
    fn a_field_absent_from_one_line_does_not_leak_into_the_next() {
        let sink = CollectingSink::new();
        ok_line(
            br#"{"id":"first","status":"Pulling","progress":"[==>]  50%","stream":"","error":""}"#,
            false,
            &sink,
        );
        // No `progress` this time, and a different id.
        ok_line(br#"{"id":"second","status":"Pulling"}"#, false, &sink);
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["Pulling :: first :: [==>]  50%\n", "Pulling :: second\n"]
        );
    }

    /// The same reset, seen from the other end: a bare `{"stream": …}` after a
    /// status line must take the stream branch, not the status branch.
    #[test]
    fn a_status_does_not_leak_into_a_later_stream_line() {
        let sink = CollectingSink::new();
        ok_line(
            br#"{"id":"a","status":"Extracting","progress":"[====]"}"#,
            false,
            &sink,
        );
        ok_line(br#"{"stream":"just output\n"}"#, false, &sink);
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["Extracting :: a :: [====]\n", "just output\n"]
        );
    }

    /// A line that is not JSON is logged at debug and skipped — never fatal,
    /// and never at error level even on the error stream, because upstream
    /// passes `false` on this branch. The next line still parses, which is what
    /// proves the reset: a broken line cannot leave half-decoded state behind.
    ///
    /// The stream line is the control: on the same error stream it *does* reach
    /// error level, so the two debug lines below are there because upstream
    /// forced the level, not because nothing was logged.
    #[test]
    fn a_malformed_line_is_skipped_and_the_next_line_still_parses() {
        let sink = CollectingSink::new();
        assert_eq!(handle_line(b"{not json", true, &sink), Ok(()));
        // An empty line is a decode failure too, not a silent success.
        assert_eq!(handle_line(b"", true, &sink), Ok(()));
        ok_line(br#"{"stream":"still here\n"}"#, true, &sink);

        // The sentence and the line inside the brackets are upstream's and are
        // byte-exact. The text after `==>` is serde's, not `encoding/json`'s —
        // so it is deliberately not asserted on, only required to be there.
        for (line, logged) in [("{not json", 0), ("", 1)] {
            let message = &sink.messages_at(Level::Debug)[logged];
            let head = format!("Unable to unmarshal line [{line}] ==> ");
            assert!(
                message.starts_with(&head),
                "got {message:?}, wanted a prefix of {head:?}"
            );
            assert!(
                message.len() > head.len(),
                "the error text after `==>` is missing"
            );
        }
        assert_eq!(sink.messages_at(Level::Error), ["still here\n"]);
    }

    /// A well-formed JSON object with nothing act recognises takes the last
    /// branch, and it is logged at **debug** whatever `is_error` says.
    #[test]
    fn a_line_with_no_known_field_is_logged_at_debug_as_unhandled() {
        let sink = CollectingSink::new();
        ok_line(br#"{"unknown":"field"}"#, true, &sink);
        assert_eq!(
            sink.messages_at(Level::Debug),
            [r#"Unable to handle line: {"unknown":"field"}"#]
        );
        assert!(sink.messages_at(Level::Error).is_empty());
    }

    /// `json.Unmarshal` treats a `null` line as a successful no-op, so the
    /// blanked struct is what gets handled — here, the "unable to handle"
    /// branch. serde rejects `null` for a struct outright, so this is the one
    /// place the port has to reach for `Option` to stay faithful.
    #[test]
    fn a_null_line_is_a_no_op_rather_than_a_parse_failure() {
        let sink = CollectingSink::new();
        assert_eq!(handle_line(b"null", false, &sink), Ok(()));
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["Unable to handle line: null"]
        );
    }

    /// `ErrorDetail.Message` has no json tag upstream, so `encoding/json`
    /// matches the key case-insensitively. The daemon sends `message`; act's
    /// own field is spelled `Message`. Both must land in the same place.
    #[test]
    fn the_error_detail_message_key_is_matched_case_insensitively() {
        for line in [
            &br#"{"errorDetail":{"Message":"upper"}}"#[..],
            br#"{"errorDetail":{"message":"lower"}}"#,
            br#"{"errorDetail":{"MESSAGE":"shouty"}}"#,
        ] {
            let sink = CollectingSink::new();
            // The message must be found under any casing, and the line must
            // reach the `errorDetail` branch — which is only proof that the
            // field was matched, because an unmatched `errorDetail` would leave
            // an empty message and fall through to "unable to handle".
            let outcome = handle_line(line, true, &sink);
            assert_eq!(
                outcome,
                Err(String::new()),
                "the branch was not reached: {:?}",
                String::from_utf8_lossy(line)
            );
            assert_eq!(sink.lines().len(), 1, "{:?}", String::from_utf8_lossy(line));
            assert!(
                matches!(sink.lines()[0], (Level::Error, _)),
                "{:?}",
                String::from_utf8_lossy(line)
            );
        }
    }

    /// A plain string or number is a *type* error for `encoding/json`, and
    /// lands on the same debug-and-skip branch as a syntax error.
    #[test]
    fn a_json_value_that_is_not_an_object_is_skipped() {
        let sink = CollectingSink::new();
        assert_eq!(handle_line(b"42", false, &sink), Ok(()));
        assert_eq!(handle_line(br#""just a string""#, false, &sink), Ok(()));
        assert_eq!(sink.lines().len(), 2);
        for (level, message) in sink.lines() {
            assert_eq!(level, Level::Debug);
            assert!(
                message.starts_with("Unable to unmarshal line ["),
                "got {message:?}"
            );
        }
    }

    /// `bufio.ScanLines`: split on `\n`, no final empty line for a trailing
    /// newline, but a blank line in the middle is a line, and `\r` is dropped.
    #[test]
    fn lines_are_split_the_way_bufio_scans_them() {
        let as_strings = |body: &[u8]| -> Vec<String> {
            scan_lines(body)
                .map(|line| String::from_utf8_lossy(line).into_owned())
                .collect()
        };
        assert_eq!(as_strings(b""), Vec::<String>::new());
        assert_eq!(as_strings(b"a"), ["a"]);
        assert_eq!(as_strings(b"a\n"), ["a"], "no final empty line");
        assert_eq!(as_strings(b"a\nb"), ["a", "b"]);
        assert_eq!(as_strings(b"a\nb\n"), ["a", "b"]);
        assert_eq!(
            as_strings(b"a\n\n"),
            ["a", ""],
            "a blank line in the middle"
        );
        assert_eq!(as_strings(b"\n"), [""]);
        assert_eq!(as_strings(b"a\r\nb\r\n"), ["a", "b"], "CR is dropped");
    }

    /// The loop stops at the first failure and returns it, leaving the rest of
    /// the stream unlogged — upstream returns from inside the loop for the same
    /// reason.
    #[test]
    fn the_loop_stops_at_the_first_error_and_returns_it() {
        let sink = CollectingSink::new();
        let outcome = log_docker_response(
            b"{\"stream\":\"one\\n\"}\n{\"error\":\"boom\"}\n{\"stream\":\"three\\n\"}\n",
            true,
            &sink,
        );
        assert_eq!(outcome.unwrap_err().to_string(), "boom");
        assert_eq!(sink.messages_at(Level::Error), ["one\n", "boom"]);
    }

    /// A clean stream is logged in full and reports success.
    #[test]
    fn a_clean_stream_is_logged_in_order() {
        let sink = CollectingSink::new();
        let outcome = log_docker_response(
            b"{\"id\":\"a\",\"status\":\"Pulling\",\"progress\":\"[==>] 1MB\"}\n\
              {\"id\":\"a\",\"status\":\"Done\"}\n\
              {\"stream\":\"ready\\n\"}\n",
            false,
            &sink,
        );
        assert!(outcome.is_ok(), "got {:?}", outcome.err());
        assert_eq!(
            sink.messages_at(Level::Debug),
            ["Pulling :: a :: [==>] 1MB\n", "Done :: a\n", "ready\n"]
        );
    }

    /// The upstream signature's `nil` reader — a stream that was never opened
    /// logs nothing and succeeds. With a buffer that is the empty body.
    #[test]
    fn an_empty_body_logs_nothing_and_succeeds() {
        let sink = CollectingSink::new();
        assert!(log_docker_response(b"", true, &sink).is_ok());
        assert!(sink.lines().is_empty());
    }
}
