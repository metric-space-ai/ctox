// Origin: CTOX
// License: AGPL-3.0-only
// ref: internal/translator/common/apply_patch_input.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial

//! Streams the `input` string out of one `apply_patch` arguments object.
//!
//! Fragments are bytes, matching Go strings, so a chunk may split a UTF-8
//! character. Invalid sequences are rejected and are not replaced with U+FFFD.

use crate::internal::client::codex::apply_patch::{unmarshal_json_string, unwrap_input};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplyPatchInputError {
    message: String,
}

impl ApplyPatchInputError {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ApplyPatchInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ApplyPatchInputError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    BeforeObject,
    BeforeKey,
    InKey,
    BeforeColon,
    BeforeValue,
    InValue,
    AfterValue,
    Complete,
}

/// Decodes the input string from streamed function arguments.
///
/// A decoder belongs to one call and must not be copied after use.
#[derive(Debug, Default)]
pub(crate) struct ApplyPatchInputDecoder {
    phase: Phase,
    key_raw: Vec<u8>,
    escape_raw: Vec<u8>,
    utf8_pending: Vec<u8>,
    high_surrogate: u16,
    input: String,
    finished: bool,
    error: Option<ApplyPatchInputError>,
}

impl ApplyPatchInputDecoder {
    /// Scans one fragment and returns only the newly validated characters.
    pub(crate) fn push(&mut self, fragment: &[u8]) -> Result<String, ApplyPatchInputError> {
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        if self.finished {
            if fragment.is_empty() {
                return Ok(String::new());
            }
            return self.fail("apply_patch arguments received after completion");
        }
        let start = self.input.len();
        for &byte in fragment {
            match self.phase {
                Phase::BeforeObject => {
                    if json_space(byte) {
                        continue;
                    }
                    if byte != b'{' {
                        return self.fail("apply_patch arguments must be a JSON object");
                    }
                    self.phase = Phase::BeforeKey;
                }
                Phase::BeforeKey => {
                    if json_space(byte) {
                        continue;
                    }
                    if byte != b'"' {
                        return self.fail("apply_patch arguments must contain the input field");
                    }
                    self.key_raw.push(byte);
                    self.phase = Phase::InKey;
                }
                Phase::InKey => {
                    self.key_raw.push(byte);
                    if !self.escape_raw.is_empty() {
                        self.escape_raw.clear();
                        continue;
                    }
                    if byte == b'\\' {
                        self.escape_raw.push(byte);
                        continue;
                    }
                    if byte < 0x20 {
                        return self.fail("invalid control character in apply_patch input key");
                    }
                    if byte == b'"' {
                        match unmarshal_json_string(&self.key_raw) {
                            Ok(key) if key == "input" => {
                                self.key_raw.clear();
                                self.phase = Phase::BeforeColon;
                            }
                            Ok(_) => {
                                return self
                                    .fail("apply_patch arguments must contain the input field");
                            }
                            Err(_) => return self.fail("decode apply_patch input key"),
                        }
                    }
                }
                Phase::BeforeColon => {
                    if json_space(byte) {
                        continue;
                    }
                    if byte != b':' {
                        return self.fail("apply_patch input key must be followed by a colon");
                    }
                    self.phase = Phase::BeforeValue;
                }
                Phase::BeforeValue => {
                    if json_space(byte) {
                        continue;
                    }
                    if byte != b'"' {
                        return self.fail("apply_patch input must be a string");
                    }
                    self.phase = Phase::InValue;
                }
                Phase::InValue => {
                    if let Err(message) = self.consume_value(byte) {
                        return self.fail(message);
                    }
                }
                Phase::AfterValue => {
                    if json_space(byte) {
                        continue;
                    }
                    if byte != b'}' {
                        return self
                            .fail("apply_patch arguments must contain only one input field");
                    }
                    self.phase = Phase::Complete;
                }
                Phase::Complete => {
                    if !json_space(byte) {
                        return self.fail("apply_patch arguments must not contain trailing JSON");
                    }
                }
            }
        }
        Ok(self.input[start..].to_owned())
    }

    /// Validates the final wrapper and returns only the previously unsent suffix.
    pub(crate) fn finish(&mut self, arguments: &[u8]) -> Result<String, ApplyPatchInputError> {
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        let text = match std::str::from_utf8(arguments) {
            Ok(text) => text,
            Err(_) => return self.fail("invalid UTF-8 in apply_patch input"),
        };
        let input = match unwrap_input(text) {
            Ok(input) => input,
            Err(message) => return self.fail(message),
        };
        let mut final_decoder = Self::default();
        if let Err(error) = final_decoder.push(arguments) {
            return self.fail(error.message());
        }
        if self.finished {
            if input != self.input {
                return self.fail("conflicting apply_patch arguments completion");
            }
            return Ok(String::new());
        }
        if !input.starts_with(&self.input) {
            return self.fail("final apply_patch input conflicts with streamed input");
        }
        let tail = input[self.input.len()..].to_owned();
        self.input.push_str(&tail);
        self.finished = true;
        self.phase = Phase::Complete;
        self.key_raw.clear();
        self.escape_raw.clear();
        self.utf8_pending.clear();
        self.high_surrogate = 0;
        Ok(tail)
    }

    /// Returns the decoded input, preserving its original whitespace.
    pub(crate) fn input(&self) -> &str {
        &self.input
    }

    fn fail(&mut self, message: impl Into<String>) -> Result<String, ApplyPatchInputError> {
        let error = ApplyPatchInputError {
            message: message.into(),
        };
        self.error = Some(error.clone());
        Err(error)
    }

    fn consume_value(&mut self, byte: u8) -> Result<(), &'static str> {
        if !self.utf8_pending.is_empty() || byte >= 0x80 {
            if !self.escape_raw.is_empty() || self.high_surrogate != 0 {
                return Err("invalid Unicode escape in apply_patch input");
            }
            self.utf8_pending.push(byte);
            if !full_rune(&self.utf8_pending) {
                return Ok(());
            }
            let text = std::str::from_utf8(&self.utf8_pending)
                .map_err(|_| "invalid UTF-8 in apply_patch input")?;
            self.input.push_str(text);
            self.utf8_pending.clear();
            return Ok(());
        }
        if !self.escape_raw.is_empty() {
            self.escape_raw.push(byte);
            if self.escape_raw.len() == 2 {
                if self.high_surrogate != 0 && byte != b'u' {
                    return Err("apply_patch input high surrogate requires a low surrogate");
                }
                let decoded = match byte {
                    b'u' => return Ok(()),
                    b'"' | b'\\' | b'/' => byte,
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    _ => return Err("invalid escape in apply_patch input"),
                };
                self.input.push(decoded as char);
                self.escape_raw.clear();
                return Ok(());
            }
            if hex_nibble(byte).is_none() {
                return Err("invalid Unicode escape in apply_patch input");
            }
            if self.escape_raw.len() < 6 {
                return Ok(());
            }
            let mut code = 0_u16;
            for &digit in &self.escape_raw[2..] {
                let Some(nibble) = hex_nibble(digit) else {
                    return Err("invalid Unicode escape in apply_patch input");
                };
                code = (code << 4) | nibble;
            }

            self.escape_raw.clear();
            if self.high_surrogate != 0 {
                if !(0xDC00..=0xDFFF).contains(&code) {
                    return Err("apply_patch input high surrogate requires a low surrogate");
                }
                self.input
                    .push(decode_surrogate_pair(self.high_surrogate, code));
                self.high_surrogate = 0;
            } else if (0xD800..=0xDBFF).contains(&code) {
                self.high_surrogate = code;
            } else if (0xDC00..=0xDFFF).contains(&code) {
                return Err("unpaired low surrogate in apply_patch input");
            } else {
                self.input.push(
                    char::from_u32(u32::from(code))
                        .ok_or("invalid Unicode escape in apply_patch input")?,
                );
            }
            return Ok(());
        }
        if self.high_surrogate != 0 && byte != b'\\' {
            return Err("apply_patch input high surrogate requires a low surrogate");
        }
        match byte {
            b'\\' => self.escape_raw.push(byte),
            b'"' => self.phase = Phase::AfterValue,
            value if value < 0x20 => return Err("invalid control character in apply_patch input"),
            value => self.input.push(value as char),
        }
        Ok(())
    }
}

fn json_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn hex_nibble(byte: u8) -> Option<u16> {
    match byte {
        b'0'..=b'9' => Some(u16::from(byte - b'0')),
        b'a'..=b'f' => Some(u16::from(byte - b'a') + 10),
        b'A'..=b'F' => Some(u16::from(byte - b'A') + 10),
        _ => None,
    }
}

fn decode_surrogate_pair(high: u16, low: u16) -> char {
    let code = 0x1_0000 + (u32::from(high - 0xD800) << 10) + u32::from(low - 0xDC00);
    char::from_u32(code).expect("surrogate pair is a scalar")
}

fn full_rune(pending: &[u8]) -> bool {
    if pending.is_empty() {
        return false;
    }
    let (size, second_lo, second_hi) = utf8_lead(pending[0]);
    let len = pending.len();
    if len >= size {
        return true;
    }
    if len > 1 && !(second_lo..=second_hi).contains(&pending[1]) {
        return true;
    }
    if len > 2 && !(0x80..=0xBF).contains(&pending[2]) {
        return true;
    }
    if len > 3 && !(0x80..=0xBF).contains(&pending[3]) {
        return true;
    }
    false
}

fn utf8_lead(byte: u8) -> (usize, u8, u8) {
    match byte {
        0xC2..=0xDF => (2, 0x80, 0xBF),
        0xE0 => (3, 0xA0, 0xBF),
        0xE1..=0xEC | 0xEE..=0xEF => (3, 0x80, 0xBF),
        0xED => (3, 0x80, 0x9F),
        0xF0 => (4, 0x90, 0xBF),
        0xF1..=0xF3 => (4, 0x80, 0xBF),
        0xF4 => (4, 0x80, 0x8F),
        _ => (1, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::ApplyPatchInputDecoder;
    use crate::internal::client::codex::apply_patch::wrap_input;

    fn push_ok(decoder: &mut ApplyPatchInputDecoder, fragment: &[u8]) -> String {
        decoder
            .push(fragment)
            .unwrap_or_else(|error| panic!("push {}: {error}", String::from_utf8_lossy(fragment)))
    }

    fn check_fragments(fragments: &[&[u8]], arguments: &[u8], want: &str) {
        let mut decoder = ApplyPatchInputDecoder::default();
        let mut output = String::new();
        for (index, fragment) in fragments.iter().enumerate() {
            let delta = push_ok(&mut decoder, fragment);
            assert!(
                std::str::from_utf8(delta.as_bytes()).is_ok(),
                "fragment {index} returned invalid UTF-8"
            );
            output.push_str(&delta);
            assert_eq!(decoder.input(), output, "fragment {index}");
        }
        let tail = decoder.finish(arguments).expect("finish");
        output.push_str(&tail);
        assert_eq!(output, want);
        assert_eq!(decoder.input(), want);
    }

    #[test]
    fn every_split_reassembles_the_patch() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+中文 \\\"\n*** End Patch\n";
        let arguments = wrap_input(patch);
        let bytes = arguments.as_bytes();
        for split in 0..=bytes.len() {
            let mut decoder = ApplyPatchInputDecoder::default();
            let mut output = String::new();
            for fragment in [&bytes[..split], &bytes[split..]] {
                output.push_str(&push_ok(&mut decoder, fragment));
            }
            output.push_str(&decoder.finish(bytes).unwrap_or_else(|error| {
                panic!("split {split}: {error}");
            }));
            assert_eq!(output, patch, "split {split}");
            assert_eq!(decoder.input(), patch, "split {split}");
        }
    }

    #[test]
    fn string_fragments_survive_every_split_and_finish() {
        let cases = [
            ("empty", r#"{"input":""}"#.to_owned(), String::new()),
            (
                "whitespace",
                " \t\r\n{ \n\"input\" \t: \"  line  \\n\\t next\\r\\n\" \r}\n\t".to_owned(),
                "  line  \n\t next\r\n".to_owned(),
            ),
            (
                "escapes",
                r#"{"input":"\"\\\/\b\f\n\r\t\u0000\u0041\u4e2d\u6587"}"#.to_owned(),
                "\"\\/\u{0008}\u{000c}\n\r\t\u{0000}A中文".to_owned(),
            ),
            (
                "escaped key",
                r#"{"in\u0070ut":"patch"}"#.to_owned(),
                "patch".to_owned(),
            ),
            (
                "surrogate pair",
                r#"{"input":"before\uD83D\uDE00after"}"#.to_owned(),
                "before😀after".to_owned(),
            ),
            (
                "surrogate boundaries",
                r#"{"input":"\ud800\udc00\uDBFF\uDFFF\uD7FF\uE000"}"#.to_owned(),
                "\u{10000}\u{10FFFF}\u{D7FF}\u{E000}".to_owned(),
            ),
            (
                "utf8",
                "{\"input\":\"¢中文😀\u{FFFD}\"}".to_owned(),
                "¢中文😀\u{FFFD}".to_owned(),
            ),
        ];
        for (name, arguments, want) in cases {
            let bytes = arguments.as_bytes();
            for split in 0..=bytes.len() {
                check_fragments(&[&bytes[..split], &bytes[split..]], bytes, &want);
                check_fragments(&[&bytes[..split]], bytes, &want);
            }
            let fragments = (0..bytes.len())
                .map(|index| &bytes[index..index + 1])
                .collect::<Vec<_>>();
            check_fragments(&fragments, bytes, &want);
            let _ = name;
        }
    }

    #[test]
    fn preview_emits_characters_before_the_json_closes() {
        let mut decoder = ApplyPatchInputDecoder::default();
        let first = push_ok(
            &mut decoder,
            br#"{"input":"*** Begin Patch\n*** Add File: a.txt\n+  "#,
        );
        assert_eq!(first, "*** Begin Patch\n*** Add File: a.txt\n+  ");
        assert_eq!(push_ok(&mut decoder, &[0xe4]), "");
        assert_eq!(push_ok(&mut decoder, &[0xb8]), "");
        assert_eq!(push_ok(&mut decoder, &[0xad, b'\\']), "中");
        assert_eq!(push_ok(&mut decoder, b"uD8"), "");
        assert_eq!(push_ok(&mut decoder, b"3D"), "");
        assert_eq!(push_ok(&mut decoder, br"\uDE"), "");
        assert_eq!(
            push_ok(&mut decoder, br"00\n*** End Patch\n"),
            "😀\n*** End Patch\n"
        );
        let want = "*** Begin Patch\n*** Add File: a.txt\n+  中😀\n*** End Patch\n";
        assert_eq!(decoder.input(), want);
        assert_eq!(decoder.finish(wrap_input(want).as_bytes()).unwrap(), "");
    }

    #[test]
    fn rejects_invalid_arguments_at_every_split() {
        let mut cases = vec![
            ("empty", Vec::new()),
            ("array", br"[]".to_vec()),
            ("empty object", br"{}".to_vec()),
            ("wrong key", br#"{"patch":"x"}"#.to_vec()),
            ("unquoted key", br#"{input:"x"}"#.to_vec()),
            ("invalid key escape", br#"{"in\qput":"x"}"#.to_vec()),
            ("missing colon", br#"{"input" "x"}"#.to_vec()),
            ("number value", br#"{"input":42}"#.to_vec()),
            ("null value", br#"{"input":null}"#.to_vec()),
            ("boolean value", br#"{"input":true}"#.to_vec()),
            ("object value", br#"{"input":{}}"#.to_vec()),
            ("array value", br#"{"input":[]}"#.to_vec()),
            ("duplicate key", br#"{"input":"x","input":"y"}"#.to_vec()),
            ("extra key", br#"{"input":"x","extra":"y"}"#.to_vec()),
            ("trailing comma", br#"{"input":"x",}"#.to_vec()),
            ("trailing json", br#"{"input":"x"}{}"#.to_vec()),
            ("non json whitespace", b"\x0b{\"input\":\"x\"}".to_vec()),
            ("invalid escape", br#"{"input":"\q"}"#.to_vec()),
            ("invalid hex", br#"{"input":"\u12G4"}"#.to_vec()),
            ("short unicode", br#"{"input":"\u123"}"#.to_vec()),
            ("dangling escape", br#"{"input":"x\"#.to_vec()),
            ("raw newline", b"{\"input\":\"x\ny\"}".to_vec()),
            ("raw nul", b"{\"input\":\"x\x00y\"}".to_vec()),
            ("low surrogate alone", br#"{"input":"\uDE00"}"#.to_vec()),
            ("high surrogate alone", br#"{"input":"\uD83D"}"#.to_vec()),
            ("high followed by text", br#"{"input":"\uD83Dx"}"#.to_vec()),
            (
                "high followed by newline escape",
                br#"{"input":"\uD83D\n"}"#.to_vec(),
            ),
            (
                "high followed by bmp",
                br#"{"input":"\uD83D\u0041"}"#.to_vec(),
            ),
            (
                "two high surrogates",
                br#"{"input":"\uD83D\uD83D"}"#.to_vec(),
            ),
            (
                "invalid utf8 continuation",
                b"{\"input\":\"\xe4A\"}".to_vec(),
            ),
            (
                "utf8 truncated by quote",
                b"{\"input\":\"\xe4\xb8\"}".to_vec(),
            ),
            ("overlong utf8", b"{\"input\":\"\xc0\xaf\"}".to_vec()),
            (
                "utf8 encoded surrogate",
                b"{\"input\":\"\xed\xa0\x80\"}".to_vec(),
            ),
            (
                "utf8 beyond maximum",
                b"{\"input\":\"\xf4\x90\x80\x80\"}".to_vec(),
            ),
            ("unclosed value", br#"{"input":"x"#.to_vec()),
            ("unclosed object", br#"{"input":"x""#.to_vec()),
        ];
        for (name, arguments) in &mut cases {
            for split in 0..=arguments.len() {
                let mut decoder = ApplyPatchInputDecoder::default();
                for fragment in [&arguments[..split], &arguments[split..]] {
                    if decoder.push(fragment).is_err() {
                        break;
                    }
                }
                assert!(
                    decoder.finish(arguments).is_err(),
                    "{name} split {split} accepted {}",
                    String::from_utf8_lossy(arguments)
                );
                assert!(std::str::from_utf8(decoder.input().as_bytes()).is_ok());
            }
            let mut byte_decoder = ApplyPatchInputDecoder::default();
            for index in 0..arguments.len() {
                if byte_decoder.push(&arguments[index..index + 1]).is_err() {
                    break;
                }
            }
            assert!(
                byte_decoder.finish(arguments).is_err(),
                "{name} single-byte fragments accepted"
            );
            assert!(
                ApplyPatchInputDecoder::default().finish(arguments).is_err(),
                "{name} finish without deltas accepted"
            );
        }
    }

    #[test]
    fn invalid_value_fails_immediately() {
        for fragment in [
            br#"{"input":4"#.as_slice(),
            br#"{"input":n"#,
            br#"{"input":t"#,
            br#"{"input":["#,
            br#"{"input":{"#,
            br#"{"input":"\q"#,
            br#"{"input":"\u12G"#,
            br#"{"input":"\uDE00"#,
            br#"{"input":"\uD83Dx"#,
        ] {
            let mut decoder = ApplyPatchInputDecoder::default();
            assert!(
                decoder.push(fragment).is_err(),
                "accepted {}",
                String::from_utf8_lossy(fragment)
            );
        }
    }

    #[test]
    fn invalid_pending_characters_do_not_emit_replacement() {
        let cases: &[(&str, &[u8], &[u8])] = &[
            ("surrogate closed", br"\uD83D", br#""}"#),
            ("surrogate text", br"\uD83D", b"x"),
            ("surrogate escape", br"\uD83D\", b"n"),
            ("invalid low surrogate", br"\uD83D\u00", b"41"),
            ("invalid escape", br"\", b"q"),
            ("invalid hex", br"\u12", b"G"),
            ("invalid utf8", &[0xe4], b"x"),
            ("truncated utf8", &[0xe4, 0xb8], br#""}"#),
        ];
        for (name, pending, continuation) in cases {
            let mut decoder = ApplyPatchInputDecoder::default();
            let mut first = br#"{"input":"safe"#.to_vec();
            first.extend_from_slice(pending);
            assert_eq!(push_ok(&mut decoder, &first), "safe", "{name}");
            assert!(decoder.push(continuation).is_err(), "{name}");
            assert_eq!(decoder.input(), "safe", "{name}");
        }
    }

    #[test]
    fn finish_and_repeated_termination() {
        let patch = "  first\nsecond  \n";
        let arguments = wrap_input(patch);
        let cases = [
            ("no deltas", &b""[..], patch),
            (
                "partial deltas",
                br#"{"input":"  first\n"#.as_slice(),
                "second  \n",
            ),
            ("complete deltas", arguments.as_bytes(), ""),
        ];
        for (name, pushed, want_tail) in cases {
            let mut decoder = ApplyPatchInputDecoder::default();
            push_ok(&mut decoder, pushed);
            let tail = decoder.finish(arguments.as_bytes()).expect(name);
            assert_eq!(tail, want_tail, "{name}");
            assert_eq!(decoder.input(), patch, "{name}");
            for repeat in 0..3 {
                assert_eq!(
                    decoder.finish(arguments.as_bytes()).unwrap(),
                    "",
                    "{name} repeat {repeat}"
                );
            }
            assert!(decoder
                .finish(wrap_input(&format!("{patch}more")).as_bytes())
                .is_err());
        }
    }

    #[test]
    fn rejects_final_prefix_conflict() {
        for final_input in ["other", "pre", "prefix changed"] {
            let mut decoder = ApplyPatchInputDecoder::default();
            assert_eq!(
                push_ok(&mut decoder, br#"{"input":"prefix original"#),
                "prefix original"
            );
            assert!(decoder.finish(wrap_input(final_input).as_bytes()).is_err());
            assert_eq!(decoder.input(), "prefix original");
        }
    }

    #[test]
    fn error_is_terminal() {
        let mut decoder = ApplyPatchInputDecoder::default();
        let initial = decoder
            .push(br#"{"input":"safe\uD83D\u0041"#)
            .expect_err("invalid surrogate");
        assert_eq!(decoder.input(), "safe");
        let again = decoder.push(br#"suffix"}"#).expect_err("stored error");
        assert_eq!(again, initial);
        let finished = decoder
            .finish(wrap_input("safe").as_bytes())
            .expect_err("finish keeps the error");
        assert_eq!(finished, initial);
    }

    #[test]
    fn finish_error_is_terminal() {
        for arguments in [
            br#"{"input":null}"#.as_slice(),
            br#"{"input":"wrong prefix"}"#,
            br#"{"input":"safe\uD83D"}"#,
        ] {
            let mut decoder = ApplyPatchInputDecoder::default();
            assert_eq!(push_ok(&mut decoder, br#"{"input":"safe"#), "safe");
            let initial = decoder.finish(arguments).expect_err("invalid final");
            let again = decoder
                .finish(br#"{"input":"safe"}"#)
                .expect_err("stored finish error");
            assert_eq!(again, initial);
            assert_eq!(decoder.input(), "safe");
        }
    }

    #[test]
    fn equivalent_final_encoding() {
        let mut decoder = ApplyPatchInputDecoder::default();
        assert_eq!(
            push_ok(&mut decoder, r#"{"input":"中\n"#.as_bytes()),
            "中\n"
        );
        assert_eq!(
            decoder
                .finish(r#"{"in\u0070ut":"\u4e2d\n😀"}"#.as_bytes())
                .unwrap(),
            "😀"
        );
        let repeat = " \n{\"input\":\"中\\n\\uD83D\\uDE00\"}\t";
        assert_eq!(decoder.finish(repeat.as_bytes()).unwrap(), "");
    }

    #[test]
    fn push_after_finish_rejects_more_bytes() {
        let mut decoder = ApplyPatchInputDecoder::default();
        decoder.finish(br#"{"input":"patch"}"#).unwrap();
        assert!(decoder.push(b"more").is_err());
        assert_eq!(decoder.input(), "patch");
    }
}
