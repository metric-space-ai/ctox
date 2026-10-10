//! `shellquote.Split`: how a job's `options:` string becomes an argv.
//!
//! This is `github.com/kballard/go-shellquote`, which act calls on
//! `NewContainerInput.Options` before the result is handed to the flag parser.
//! It is **not** `str::split_whitespace`: an `options:` value is written by a
//! human in a YAML file, so it quotes, and `--env 'A=a b'` is one argument, not
//! two. Splitting on whitespace would silently turn that into `--env A=a b`,
//! and pflag would then reject it as a missing value.
//!
//! The upstream implementation is a `goto` state machine. This is the same
//! machine with the labels replaced by an enum, and the buffer is a `String`
//! rather than a `bytes.Buffer` — it accumulates decoded UTF-8 either way.
//!
//! Not supported, exactly as upstream: `$'…'` quoting, parameter expansion,
//! brace expansion, and pathname expansion. This is a *splitter*, not a shell.

/// The three ways an input can fail to split.
///
/// Upstream exports these as sentinel errors and compares them by identity;
/// the messages are what act would print inside its `Cannot split container
/// options: '%s': '%w'` wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitError {
    /// A `'` was never closed.
    UnterminatedSingleQuote,
    /// A `"` was never closed.
    UnterminatedDoubleQuote,
    /// The input ends in a `\`.
    UnterminatedEscape,
}

impl SplitError {
    /// The upstream error text, used verbatim in the wrapper act prints.
    pub fn message(self) -> &'static str {
        match self {
            SplitError::UnterminatedSingleQuote => "Unterminated single-quoted string",
            SplitError::UnterminatedDoubleQuote => "Unterminated double-quoted string",
            SplitError::UnterminatedEscape => "Unterminated backslash-escape",
        }
    }
}

impl std::fmt::Display for SplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for SplitError {}

const SPLIT_CHARS: [char; 3] = [' ', '\n', '\t'];
/// The characters a backslash escapes inside a double-quoted string. bash
/// accepts only these; a backslash before anything else is literal.
const DOUBLE_ESCAPE_CHARS: [char; 5] = ['$', '`', '"', '\n', '\\'];

fn is_split(c: char) -> bool {
    SPLIT_CHARS.contains(&c)
}

/// Which quoting context a word is being read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Outside any quotes.
    Raw,
    /// Right after a `\`, or the continuation of a double-quoted run.
    Escape,
    /// Inside `'…'`.
    Single,
    /// Inside `"…"`.
    Double,
}

/// `Split`: split a string according to `/bin/sh`'s word-splitting rules.
///
/// A word is terminated by unquoted whitespace, an unterminated quote or a
/// trailing backslash is an error, and `\<newline>` is elided entirely rather
/// than becoming a separator.
pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut rest = input;

    while !rest.is_empty() {
        let mut chars = rest.chars();
        let first = chars.next().expect("rest is not empty");

        // Leading whitespace is skipped, and a backslash-escaped newline is
        // skipped with it, so `\<newline>` never starts a word.
        if is_split(first) {
            rest = chars.as_str();
            continue;
        }
        if first == '\\' {
            match chars.next() {
                None => return Err(SplitError::UnterminatedEscape),
                Some('\n') => {
                    rest = chars.as_str();
                    continue;
                }
                Some(_) => {}
            }
        }

        let (word, remainder) = split_word(rest)?;
        words.push(word);
        rest = remainder;
    }
    Ok(words)
}

/// Read one word, returning it and whatever follows it.
fn split_word(input: &str) -> Result<(String, &str), SplitError> {
    let mut buf = String::new();
    let mut state = State::Raw;
    let mut cur = input;

    loop {
        match state {
            // A backslash escapes the *next* character, and the backslash
            // itself is dropped — including before a quote, which is why
            // `don\'t` is one word and not an unterminated string. A
            // backslash-escaped **newline** is dropped whole, so `with\<nl>a`
            // is the single word `witha`.
            State::Escape => {
                let mut chars = cur.chars();
                let Some(c) = chars.next() else {
                    return Err(SplitError::UnterminatedEscape);
                };
                if c != '\n' {
                    buf.push(c);
                }
                cur = chars.as_str();
                state = State::Raw;
            }

            State::Raw => {
                let mut chars = cur.chars();
                let Some(c) = chars.next() else {
                    // End of input: whatever is buffered is the word.
                    return Ok((buf, ""));
                };
                let after = chars.as_str();

                state = match c {
                    _ if is_split(c) => return Ok((buf, after)),
                    '\'' => State::Single,
                    '"' => State::Double,
                    '\\' => State::Escape,
                    _ => {
                        buf.push(c);
                        State::Raw
                    }
                };
                cur = after;
            }

            State::Single => {
                // Everything up to the closing quote is literal.
                match cur.find('\'') {
                    Some(index) => {
                        buf.push_str(&cur[..index]);
                        cur = &cur[index + 1..];
                        state = State::Raw;
                    }
                    None => return Err(SplitError::UnterminatedSingleQuote),
                }
            }

            State::Double => {
                let mut chars = cur.chars();
                let Some(c) = chars.next() else {
                    return Err(SplitError::UnterminatedDoubleQuote);
                };
                let after = chars.as_str();
                match c {
                    '"' => {
                        cur = after;
                        state = State::Raw;
                    }
                    '\\' => {
                        // Inside double quotes only `$`, `` ` ``, `"`, newline
                        // and `\` are escapable; before anything else the
                        // backslash is a literal character.
                        match chars.clone().next() {
                            Some(next) if DOUBLE_ESCAPE_CHARS.contains(&next) => {
                                let mut escaped = chars.clone();
                                escaped.next();
                                if next != '\n' {
                                    buf.push(next);
                                }
                                cur = escaped.as_str();
                            }
                            _ => {
                                buf.push('\\');
                                cur = after;
                            }
                        }
                    }
                    _ => {
                        buf.push(c);
                        cur = after;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TestSimpleSplit`, verbatim from `unquote_test.go`.
    #[test]
    fn simple_split_matches_upstream() {
        let cases: &[(&str, &[&str])] = &[
            ("hello", &["hello"]),
            ("hello goodbye", &["hello", "goodbye"]),
            ("hello   goodbye", &["hello", "goodbye"]),
            ("glob* test?", &["glob*", "test?"]),
            (
                "don\\'t you know the dewey decimal system\\?",
                &["don't", "you", "know", "the", "dewey", "decimal", "system?"],
            ),
            (
                "'don'\\''t you know the dewey decimal system?'",
                &["don't you know the dewey decimal system?"],
            ),
            ("one '' two", &["one", "", "two"]),
            (
                "text with\\\na backslash-escaped newline",
                &["text", "witha", "backslash-escaped", "newline"],
            ),
            (
                "text \"with\na\" quoted newline",
                &["text", "with\na", "quoted", "newline"],
            ),
            (
                "\"quoted\\d\\\\\\\" text with\\\na backslash-escaped newline\"",
                &["quoted\\d\\\" text witha backslash-escaped newline"],
            ),
            (
                "text with an escaped \\\n newline in the middle",
                &[
                    "text", "with", "an", "escaped", "newline", "in", "the", "middle",
                ],
            ),
            ("foo\"bar\"baz", &["foobarbaz"]),
        ];
        for (input, expected) in cases {
            let actual = split(input).expect("upstream splits this without error");
            assert_eq!(actual, *expected, "input {input:?}");
        }
    }

    /// `TestErrorSplit`, verbatim from `unquote_test.go`.
    #[test]
    fn error_split_matches_upstream() {
        let cases: &[(&str, SplitError)] = &[
            ("don't worry", SplitError::UnterminatedSingleQuote),
            ("'test'\\''ing", SplitError::UnterminatedSingleQuote),
            ("\"foo'bar", SplitError::UnterminatedDoubleQuote),
            ("foo\\", SplitError::UnterminatedEscape),
            ("   \\", SplitError::UnterminatedEscape),
        ];
        for (input, expected) in cases {
            assert_eq!(split(input).err(), Some(*expected), "input {input:?}");
        }
    }

    /// What `options:` actually looks like. The whole reason this module
    /// exists: `--env 'A=a b'` must stay one argument, because
    /// `split_whitespace` would turn it into two and pflag would then report a
    /// missing value for `--env`.
    #[test]
    fn a_quoted_env_value_stays_one_argument() {
        assert_eq!(
            split("--cpus 2 --env 'A=a b'").unwrap(),
            vec!["--cpus", "2", "--env", "A=a b"]
        );
    }

    /// A backslash escapes the next character **unconditionally outside
    /// quotes**, and only a small set of characters **inside** double quotes.
    ///
    /// Measured against `shellquote.Split` directly: `\a` is `a`, `"\a"` is
    /// `\a`, and `"\$"` is `$`.
    #[test]
    fn what_a_backslash_escapes_depends_on_where_it_is() {
        assert_eq!(split("\\a").unwrap(), vec!["a"], "outside quotes, anything");
        assert_eq!(split("\"\\a\"").unwrap(), vec!["\\a"], "inside double, not $`\"\\");
        // `\$` *is* escapable inside double quotes.
        assert_eq!(split("\"\\$\"").unwrap(), vec!["$"]);
        // A backslash before a single quote escapes it outside quotes, and that
        // is what keeps `don't` one word rather than an unterminated string.
        assert_eq!(split("don\\'t").unwrap(), vec!["don't"]);
    }

    /// An empty input is zero words, not one empty word — which is what makes
    /// `input.Options == ""` mean "no options at all" upstream.
    #[test]
    fn an_empty_input_yields_no_words() {
        assert_eq!(split("").unwrap(), Vec::<String>::new());
        assert_eq!(split("   \t\n ").unwrap(), Vec::<String>::new());
    }
}
