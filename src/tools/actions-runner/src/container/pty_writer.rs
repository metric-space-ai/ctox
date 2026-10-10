//! The writer that ends a command's output when the process finishes.
//!
//! act runs a step on a pseudo-terminal, and a process that has exited still
//! leaves the PTY master readable. Without an explicit end, `io.Copy` from the
//! master blocks until something closes it — which on a shared runner means the
//! job's log never finishes. So after the command returns, act writes `EOT`
//! (0x04, the terminal's end-of-transmission character) into the TTY, and this
//! writer notices it coming back out and stops.
//!
//! # The one interesting rule
//!
//! A terminal's `EOT` only means end-of-line when the cursor is at the start
//! of a line. If the command's last output had no trailing newline — the shell
//! printed a prompt and stopped, say — `EOT` would cut the line in half. So the
//! writer adds the missing newline first, and reports end-of-file either way.
//!
//! "Did the previous write end mid-line?" is a single flag, and it is *not*
//! recomputed for the current write: the state being answered is where the
//! cursor was before the `EOT` arrived.
use std::io::{self, Write};

/// Wraps a sink and stops at `EOT`.
pub struct PtyWriter<W: Write> {
    out: W,
    /// Whether to look for `EOT` at all. Off until the command has exited, so
    /// a `0x04` that a command legitimately outputs mid-run is passed through.
    auto_stop: bool,
    /// Whether the previous write ended without a newline.
    dirty_line: bool,
}

impl<W: Write> PtyWriter<W> {
    /// A writer that passes everything through.
    pub fn new(out: W) -> Self {
        PtyWriter {
            out,
            auto_stop: false,
            dirty_line: false,
        }
    }

    /// Starts looking for `EOT`.
    pub fn set_auto_stop(&mut self, auto_stop: bool) {
        self.auto_stop = auto_stop;
    }

    /// Whether `EOT` is being looked for.
    pub fn auto_stop(&self) -> bool {
        self.auto_stop
    }

    /// Whether the last write ended mid-line.
    pub fn dirty_line(&self) -> bool {
        self.dirty_line
    }

    /// The wrapped sink, for a caller that has finished with the writer and
    /// wants what it wrote.
    pub fn into_inner(self) -> W {
        self.out
    }
}

/// The end-of-transmission character.
const EOT: u8 = 4;

impl<W: Write> Write for PtyWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.auto_stop && buf.last() == Some(&EOT) {
            let without_eot = &buf[..buf.len() - 1];
            let written = self.out.write(without_eot)?;
            if written < without_eot.len() {
                // A short write has to be reported as such; the `EOT` has
                // not been consumed and the caller will offer it again.
                return Ok(written);
            }
            // The cursor has to be at the start of a line for `EOT` to mean
            // end of output, so a partial line is finished first.
            let cursor_mid_line = self.dirty_line
                || (buf.len() > 1 && buf[buf.len() - 2] != b'\n');
            if cursor_mid_line {
                let _ = self.out.write(b"\n");
            }
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of output"));
        }

        self.dirty_line = buf.last() != Some(&b'\n');
        self.out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer() -> (PtyWriter<Vec<u8>>, ()) {
        (PtyWriter::new(Vec::new()), ())
    }

    #[test]
    fn before_auto_stop_an_eot_is_ordinary_output() {
        let (mut w, _) = writer();
        w.write_all(b"a\x04b").expect("written");
        assert_eq!(w.into_inner(), b"a\x04b");
    }

    /// The plain case: the command ended on a line boundary, so `EOT` just
    /// ends the output and no newline is invented.
    #[test]
    fn an_eot_after_a_newline_ends_the_output() {
        let (mut w, _) = writer();
        w.write_all(b"line one\n").expect("written");
        w.set_auto_stop(true);

        let outcome = w.write(b"line two\n\x04");
        assert!(outcome.is_err(), "end of output");
        assert_eq!(
            outcome.expect_err("eof").kind(),
            io::ErrorKind::UnexpectedEof,
        );
        assert_eq!(w.into_inner(), b"line one\nline two\n");
    }

    /// The rule the writer exists for: the command left the cursor mid-line,
    /// so the line is finished before the stream ends.
    #[test]
    fn an_eot_mid_line_finishes_the_line_first() {
        let (mut w, _) = writer();
        w.write_all(b"prompt $ ").expect("written");
        w.set_auto_stop(true);
        assert!(w.dirty_line(), "no trailing newline");

        assert!(w.write(b"\x04").is_err());
        assert_eq!(
            w.into_inner(),
            b"prompt $ \n",
            "a newline is added so the last line is not cut in half",
        );
    }

    /// The same rule, decided from the current write rather than the previous
    /// one: a partial line followed by `EOT` in the *same* write still gets a
    /// newline.
    #[test]
    fn a_partial_line_and_an_eot_in_one_write() {
        let (mut w, _) = writer();
        w.write_all(b"clean\n").expect("written");
        w.set_auto_stop(true);

        assert!(w.write(b"partial\x04").is_err());
        assert_eq!(w.into_inner(), b"clean\npartial\n");
    }

    /// A single `EOT` with nothing before it: there is no previous line to
    /// dirty and no second-to-last byte, so nothing is appended.
    #[test]
    fn a_lone_eot_appends_nothing() {
        let (mut w, _) = writer();
        w.set_auto_stop(true);
        assert!(w.write(b"\x04").is_err());
        assert_eq!(w.into_inner(), b"");
    }

    /// The `EOT` is not passed through, and what came before it is.
    #[test]
    fn the_eot_itself_is_dropped() {
        let (mut w, _) = writer();
        w.write_all(b"x\n").expect("written");
        w.set_auto_stop(true);
        let _ = w.write(b"y\x04");
        assert!(!w.into_inner().contains(&EOT));
    }

    /// A short write is reported, not swallowed, so the caller retries and the
    /// `EOT` is not consumed twice.
    #[test]
    fn a_short_write_is_reported() {
        /// A sink that accepts one byte at a time.
        struct Dribble(Vec<u8>);
        impl Write for Dribble {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                match buf.split_first() {
                    Some((first, _)) => {
                        self.0.push(*first);
                        Ok(1)
                    }
                    None => Ok(0),
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut w = PtyWriter::new(Dribble(Vec::new()));
        w.write_all(b"ok\n").expect("written");
        w.set_auto_stop(true);
        assert_eq!(
            w.write(b"more\x04").expect("a short write is not an error"),
            1,
            "short write, EOT not consumed",
        );
        assert!(!w.dirty_line());
    }
}
