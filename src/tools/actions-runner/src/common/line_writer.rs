//! Line-buffered output, which is how a job's stdout becomes log lines.
//!
//! A step's output arrives in whatever chunks the pipe hands over — a line can
//! span three writes, and one write can hold four lines. act's `lineWriter`
//! accumulates until it sees a newline, hands the line **including** the
//! newline to each handler in turn, and resets. A trailing fragment stays in
//! the buffer for the next write, and is only flushed when something arrives
//! after it, so the final line of a command without a newline is never emitted.
//!
//! That last detail is upstream behaviour, and it is the reason the runner
//! flushes the buffer itself when a step ends.

use std::io::Write;
use std::sync::{Arc, Mutex};

/// A handler for one complete line.
///
/// Returning `false` stops the remaining handlers for that line — the line is
/// still consumed and the buffer still reset, so the next line starts clean.
pub type LineHandler = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// The buffered writer.
#[derive(Clone, Default)]
pub struct LineWriter {
    buffer: Arc<Mutex<String>>,
    handlers: Vec<LineHandler>,
}

impl LineWriter {
    /// A writer feeding `handlers` in order.
    pub fn new(handlers: Vec<LineHandler>) -> Self {
        LineWriter {
            buffer: Arc::new(Mutex::new(String::new())),
            handlers,
        }
    }

    /// Emits whatever is buffered, as a final line, and clears the buffer.
    ///
    /// Upstream has no such method: a command whose output does not end in a
    /// newline loses its last line. The runner cannot have that, so this is
    /// the one addition, and it is what the trailing fragment is for.
    pub fn flush_line(&self) {
        let line = {
            let mut buffer = self.buffer.lock().expect("line writer poisoned");
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };
        self.dispatch(&line);
    }

    fn dispatch(&self, line: &str) {
        for handler in &self.handlers {
            if !handler(line) {
                break;
            }
        }
    }
}

impl Write for LineWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let text = String::from_utf8_lossy(buf);
        for line in text.split_inclusive('\n') {
            let complete = line.ends_with('\n');
            self.buffer
                .lock()
                .expect("line writer poisoned")
                .push_str(line);
            if complete {
                let ready = std::mem::take(&mut *self.buffer.lock().expect("line writer poisoned"));
                self.dispatch(&ready);
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // A `Write` flush must not lose the partial line, so the buffer stays
        // put; `flush_line` is what ends a stream.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn recorder() -> (LineHandler, Arc<StdMutex<Vec<String>>>) {
        let lines = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&lines);
        let handler: LineHandler = Arc::new(move |line| {
            sink.lock().expect("poisoned").push(line.to_string());
            true
        });
        (handler, lines)
    }

    // line_writer_test.go: TestLineWriter
    #[test]
    fn lines_are_buffered_across_writes() {
        let (handler, lines) = recorder();
        let mut writer = LineWriter::new(vec![handler]);

        for chunk in [
            "hello",
            " ",
            "world!!\nextra",
            " line\n and another\nlast",
            " line\n",
            "no newline here...",
        ] {
            writer.write_all(chunk.as_bytes()).expect("written");
        }

        assert_eq!(
            *lines.lock().expect("poisoned"),
            [
                "hello world!!\n",
                "extra line\n",
                " and another\n",
                "last line\n",
            ],
            "the trailing fragment is not emitted until something follows it",
        );
    }

    /// The write count is the byte count, not the number of lines: a
    /// `Write` that reports fewer bytes written than it was given looks like
    /// a short write to the caller.
    #[test]
    fn the_write_reports_every_byte() {
        let (handler, _) = recorder();
        let mut writer = LineWriter::new(vec![handler]);
        for chunk in ["a", "bb", "ccc\ndddd"] {
            assert_eq!(
                writer.write(chunk.as_bytes()).expect("written"),
                chunk.len(),
                "{chunk:?}",
            );
        }
    }

    /// The fragment is not lost: `flush_line` is the port's addition, and
    /// without it a command whose output has no trailing newline reports
    /// nothing.
    #[test]
    fn the_trailing_fragment_is_emitted_on_request() {
        let (handler, lines) = recorder();
        let mut writer = LineWriter::new(vec![handler]);
        writer.write_all(b"no newline here").expect("written");
        assert!(lines.lock().expect("poisoned").is_empty());

        writer.flush_line();
        assert_eq!(*lines.lock().expect("poisoned"), ["no newline here"]);

        // And flushing an empty buffer emits nothing.
        writer.flush_line();
        assert_eq!(lines.lock().expect("poisoned").len(), 1);
    }

    /// A handler returning false stops the chain for that line, and the line
    /// is still consumed.
    #[test]
    fn a_handler_can_stop_the_chain() {
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let first_sink = Arc::clone(&seen);
        let first: LineHandler = Arc::new(move |line| {
            first_sink
                .lock()
                .expect("poisoned")
                .push(format!("a:{line}"));
            false
        });
        let second_sink = Arc::clone(&seen);
        let second: LineHandler = Arc::new(move |line| {
            second_sink
                .lock()
                .expect("poisoned")
                .push(format!("b:{line}"));
            true
        });

        let mut writer = LineWriter::new(vec![first, second]);
        writer.write_all(b"one\ntwo\n").expect("written");

        assert_eq!(
            *seen.lock().expect("poisoned"),
            ["a:one\n", "a:two\n"],
            "the second handler never runs",
        );
    }

    /// Several handlers all see the line when they all return true.
    #[test]
    fn every_handler_sees_the_line() {
        let count = Arc::new(StdMutex::new(0usize));
        let handlers: Vec<LineHandler> = (0..3)
            .map(|_| {
                let count = Arc::clone(&count);
                Arc::new(move |_: &str| {
                    *count.lock().expect("poisoned") += 1;
                    true
                }) as LineHandler
            })
            .collect();

        let mut writer = LineWriter::new(handlers);
        writer.write_all(b"x\n").expect("written");
        assert_eq!(*count.lock().expect("poisoned"), 3);
    }

    /// A write that is not valid UTF-8 must not fail: Go works on bytes and
    /// hands whatever it has to the handler.
    #[test]
    fn invalid_utf8_does_not_fail_the_write() {
        let (handler, lines) = recorder();
        let mut writer = LineWriter::new(vec![handler]);
        writer.write_all(&[0xff, 0xfe, b'\n']).expect("written");
        assert_eq!(lines.lock().expect("poisoned").len(), 1);
    }
}
