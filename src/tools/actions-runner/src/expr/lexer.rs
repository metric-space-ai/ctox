//! Tokenizer for the GitHub Actions expression language.
//!
//! Port of `actionlint`'s `expr_lexer.go`. Upstream drives Go's `text/scanner`
//! and slices token text out of the source using scanner positions; this port
//! keeps a parallel byte-offset table so offsets stay byte-based, which
//! matters because act advances through the raw scalar with
//! `val[lexer.Offset():]` to find the next `${{`.
//!
//! Note the terminator: the expression is not lexed to end of input. A `}}`
//! closes it and yields [`TokenKind::End`], because that is how the language
//! embeds an expression inside a surrounding string.

use std::fmt;

/// Kind of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Default value, never produced.
    Unknown,
    /// The `}}` end marker.
    End,
    /// An identifier.
    Ident,
    /// A single-quoted string literal, quotes included.
    String,
    /// An integer literal, hexadecimal included.
    Int,
    /// A floating point literal.
    Float,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `.`
    Dot,
    /// `!`
    Not,
    /// `<`
    Less,
    /// `<=`
    LessEq,
    /// `>`
    Greater,
    /// `>=`
    GreaterEq,
    /// `==`
    Eq,
    /// `!=`
    NotEq,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `*`
    Star,
    /// `,`
    Comma,
}

impl TokenKind {
    /// The spelling used in upstream error messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::End => "END",
            Self::Ident => "IDENT",
            Self::String => "STRING",
            Self::Int => "INTEGER",
            Self::Float => "FLOAT",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::Dot => ".",
            Self::Not => "!",
            Self::Less => "<",
            Self::LessEq => "<=",
            Self::Greater => ">",
            Self::GreaterEq => ">=",
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::And => "&&",
            Self::Or => "||",
            Self::Star => "*",
            Self::Comma => ",",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A lexed token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What was lexed.
    pub kind: TokenKind,
    /// Source text of the token.
    pub value: String,
    /// Byte offset of the token's first character.
    pub offset: usize,
    /// 1-based line of the token's first character.
    pub line: usize,
    /// 1-based column of the token's first character.
    pub column: usize,
}

impl Token {
    /// A placeholder token for nodes the interpreter synthesises, such as the
    /// implicit `success()` wrapper. It carries no source position.
    pub fn synthetic() -> Self {
        Self {
            kind: TokenKind::Ident,
            value: String::new(),
            offset: 0,
            line: 0,
            column: 0,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.kind, self.line, self.column, self.offset
        )
    }
}

/// A lexing or parsing failure, positioned in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    /// What went wrong.
    pub message: String,
    /// Byte offset of the failure.
    pub offset: usize,
    /// 1-based line of the failure.
    pub line: usize,
    /// 1-based column of the failure.
    pub column: usize,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}:{}]", self.message, self.line, self.column)
    }
}

impl std::error::Error for ExprError {}

fn is_whitespace(r: char) -> bool {
    r == ' ' || r == '\n' || r == '\r' || r == '\t'
}

fn is_alpha(r: char) -> bool {
    r.is_ascii_alphabetic()
}

fn is_num(r: char) -> bool {
    r.is_ascii_digit()
}

fn is_hex_num(r: char) -> bool {
    r.is_ascii_hexdigit()
}

fn is_alnum(r: char) -> bool {
    is_alpha(r) || is_num(r)
}

const EXPECTED_PUNCT_CHARS: &str = "''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '";
const EXPECTED_DIGIT_CHARS: &str = "'0'..'9'";
const EXPECTED_ALPHA_CHARS: &str = "'a'..'z', 'A'..'Z', '_'";

fn expected_all_chars() -> String {
    format!("{EXPECTED_ALPHA_CHARS}, {EXPECTED_DIGIT_CHARS}, {EXPECTED_PUNCT_CHARS}")
}

/// Incremental tokenizer over an expression source.
pub struct Lexer {
    chars: Vec<char>,
    /// Byte offset of every character, plus the total length as a final entry.
    offsets: Vec<usize>,
    /// Index of the next character to consume.
    pos: usize,
    /// Byte offset where the current token starts.
    start_byte: usize,
    /// 1-based line where the current token starts.
    start_line: usize,
    /// 1-based column where the current token starts.
    start_column: usize,
    /// Line of `pos`, tracked while scanning.
    line: usize,
    /// Column of `pos`.
    column: usize,
    /// First error seen; the lexer keeps going so callers can report one.
    error: Option<ExprError>,
}

impl Lexer {
    /// Starts lexing `source`. The source is expected to still contain the
    /// closing `}}`.
    pub fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let mut offsets = Vec::with_capacity(chars.len() + 1);
        let mut acc = 0usize;
        for c in &chars {
            offsets.push(acc);
            acc += c.len_utf8();
        }
        offsets.push(acc);
        Self {
            chars,
            offsets: offsets.clone(),
            pos: 0,
            start_byte: 0,
            start_line: 1,
            start_column: 1,
            line: 1,
            column: 1,
            error: None,
        }
    }

    /// Byte offset just past the characters consumed so far.
    ///
    /// act uses this to resume scanning after an interpolated expression.
    pub fn offset(&self) -> usize {
        self.byte_at(self.pos)
    }

    /// 1-based line of the scan position.
    pub fn line(&self) -> usize {
        self.line
    }

    fn byte_at(&self, index: usize) -> usize {
        self.offsets[index.min(self.offsets.len() - 1)]
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Consumes the current character and returns the one after it, matching
    /// upstream's `scan.Next(); scan.Peek()`.
    fn eat(&mut self) -> Option<char> {
        if self.pos < self.chars.len() {
            if self.chars[self.pos] == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.pos += 1;
        self.peek()
    }

    /// Consumes exactly one character, for single-character tokens.
    fn bump(&mut self) {
        if self.pos < self.chars.len() {
            if self.chars[self.pos] == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.pos += 1;
    }

    fn record_error(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(ExprError {
                message,
                offset: self.byte_at(self.pos),
                line: self.line,
                column: self.column,
            });
        }
    }

    fn token(&mut self, kind: TokenKind) -> Token {
        let end = self.byte_at(self.pos);
        let value: String = self.chars[self.start_byte..end].iter().collect();
        let token = Token {
            kind,
            value,
            offset: self.start_byte,
            line: self.start_line,
            column: self.start_column,
        };
        self.start_byte = end;
        self.start_line = self.line;
        self.start_column = self.column;
        token
    }

    fn eof_token(&mut self) -> Token {
        Token {
            kind: TokenKind::End,
            value: String::new(),
            offset: self.start_byte,
            line: self.start_line,
            column: self.start_column,
        }
    }

    fn unexpected(&mut self, found: Option<char>, where_: &str, expected: &str) -> Token {
        let what = match found {
            None => "EOF".to_string(),
            Some(r) => format!("character {}", quote_rune(r)),
        };
        let note = if found == Some('"') {
            ". do you mean string literals? only single quotes are available for string delimiter"
        } else {
            ""
        };
        let message = format!("got unexpected {what} while lexing {where_}, expecting {expected}{note}");
        self.record_error(message);
        self.eof_token()
    }

    fn skip_whitespace(&mut self) {
        while let Some(r) = self.peek() {
            if !is_whitespace(r) {
                return;
            }
            self.bump();
            self.start_byte = self.byte_at(self.pos);
            self.start_line = self.line;
            self.start_column = self.column;
        }
    }

    fn lex_ident(&mut self) -> Token {
        // Context names may contain `-`, as in `job-status`.
        loop {
            match self.eat() {
                Some(r) if is_alnum(r) || r == '_' || r == '-' => continue,
                _ => return self.token(TokenKind::Ident),
            }
        }
    }

    fn lex_num(&mut self) -> Token {
        let mut r = self.peek();

        if r == Some('-') {
            r = self.eat();
        }

        if r == Some('0') {
            r = self.eat();
            if r == Some('x') {
                self.bump();
                return self.lex_hex_int();
            }
        } else {
            if !r.is_some_and(is_num) {
                return self.unexpected(r, "integer part of number", EXPECTED_DIGIT_CHARS);
            }
            loop {
                r = self.eat();
                if !r.is_some_and(is_num) {
                    break;
                }
            }
        }

        let mut kind = TokenKind::Int;

        if r == Some('.') {
            r = self.eat();
            if !r.is_some_and(is_num) {
                return self.unexpected(
                    r,
                    "fraction part of float number",
                    EXPECTED_DIGIT_CHARS,
                );
            }
            loop {
                r = self.eat();
                if !r.is_some_and(is_num) {
                    break;
                }
            }
            kind = TokenKind::Float;
        }

        if r == Some('e') || r == Some('E') {
            r = self.eat();
            if r == Some('-') {
                r = self.eat();
            }
            if r == Some('0') {
                r = self.eat();
            } else {
                if !r.is_some_and(is_num) {
                    return self.unexpected(
                        r,
                        "exponent part of float number",
                        EXPECTED_DIGIT_CHARS,
                    );
                }
                loop {
                    r = self.eat();
                    if !r.is_some_and(is_num) {
                        break;
                    }
                }
            }
            kind = TokenKind::Float;
        }

        if r.is_some_and(is_alnum) {
            let so_far = &self.chars[self.start_byte..self.pos];
            let text: String = so_far.iter().collect();
            return self.unexpected(
                r,
                &format!("character following number {text}"),
                EXPECTED_PUNCT_CHARS,
            );
        }

        self.token(kind)
    }

    fn lex_hex_int(&mut self) -> Token {
        let mut r = self.peek();
        if r == Some('0') {
            r = self.eat();
        } else {
            if !r.is_some_and(is_hex_num) {
                let expected = format!("{EXPECTED_DIGIT_CHARS}, 'a'..'f', 'A'..'F'");
                return self.unexpected(r, "hex integer", &expected);
            }
            loop {
                r = self.eat();
                if !r.is_some_and(is_hex_num) {
                    break;
                }
            }
        }

        if r.is_some_and(is_alnum) {
            let so_far = &self.chars[self.start_byte..self.pos];
            let text: String = so_far.iter().collect();
            return self.unexpected(
                r,
                &format!("character following hex integer {text}"),
                EXPECTED_PUNCT_CHARS,
            );
        }

        self.token(TokenKind::Int)
    }

    fn lex_string(&mut self) -> Token {
        loop {
            match self.eat() {
                Some('\'') => {
                    if self.eat() != Some('\'') {
                        return self.token(TokenKind::String);
                    }
                }
                None => return self.unexpected(None, "end of string literal", "'''"),
                Some(_) => continue,
            }
        }
    }

    fn lex_end_marker(&mut self) -> Token {
        let r = self.eat();
        if r != Some('}') {
            return self.unexpected(r, "end marker }}", "'}'");
        }
        self.bump();
        self.token(TokenKind::End)
    }

    fn lex_less(&mut self) -> Token {
        let mut kind = TokenKind::Less;
        if self.eat() == Some('=') {
            kind = TokenKind::LessEq;
            self.bump();
        }
        self.token(kind)
    }

    fn lex_greater(&mut self) -> Token {
        let mut kind = TokenKind::Greater;
        if self.eat() == Some('=') {
            kind = TokenKind::GreaterEq;
            self.bump();
        }
        self.token(kind)
    }

    fn lex_eq(&mut self) -> Token {
        if self.eat() != Some('=') {
            let r = self.peek();
            return self.unexpected(r, "== operator", "'='");
        }
        self.bump();
        self.token(TokenKind::Eq)
    }

    fn lex_bang(&mut self) -> Token {
        let mut kind = TokenKind::Not;
        if self.eat() == Some('=') {
            self.bump();
            kind = TokenKind::NotEq;
        }
        self.token(kind)
    }

    fn lex_and(&mut self) -> Token {
        if self.eat() != Some('&') {
            let r = self.peek();
            return self.unexpected(r, "&& operator", "'&'");
        }
        self.bump();
        self.token(TokenKind::And)
    }

    fn lex_or(&mut self) -> Token {
        if self.eat() != Some('|') {
            let r = self.peek();
            return self.unexpected(r, "|| operator", "'|'");
        }
        self.bump();
        self.token(TokenKind::Or)
    }

    fn lex_char(&mut self, kind: TokenKind) -> Token {
        self.bump();
        self.token(kind)
    }

    /// Lexes the next token.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let Some(r) = self.peek() else {
            self.record_error("unexpected EOF while lexing expression".to_string());
            return self.eof_token();
        };

        if is_alpha(r) || r == '_' {
            return self.lex_ident();
        }
        if is_num(r) || r == '-' {
            return self.lex_num();
        }

        match r {
            '\'' => self.lex_string(),
            '}' => self.lex_end_marker(),
            '!' => self.lex_bang(),
            '<' => self.lex_less(),
            '>' => self.lex_greater(),
            '=' => self.lex_eq(),
            '&' => self.lex_and(),
            '|' => self.lex_or(),
            '(' => self.lex_char(TokenKind::LeftParen),
            ')' => self.lex_char(TokenKind::RightParen),
            '[' => self.lex_char(TokenKind::LeftBracket),
            ']' => self.lex_char(TokenKind::RightBracket),
            '.' => self.lex_char(TokenKind::Dot),
            '*' => self.lex_char(TokenKind::Star),
            ',' => self.lex_char(TokenKind::Comma),
            other => self.unexpected(Some(other), "expression", &expected_all_chars()),
        }
    }

    /// The first error seen, if any.
    pub fn error(&self) -> Option<&ExprError> {
        self.error.as_ref()
    }
}

fn quote_rune(r: char) -> String {
    match r {
        '\'' => "'\\''".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\n' => "'\\n'".to_string(),
        '\t' => "'\\t'".to_string(),
        '\r' => "'\\r'".to_string(),
        c if c.is_control() => format!("'\\x{:02x}'", c as u32),
        c => format!("'{c}'"),
    }
}

/// Lexes `source` up to and including its `}}` terminator.
///
/// Returns the tokens, the byte offset just past the terminator, and any
/// error. This is `actionlint.LexExpression`.
pub fn lex_expression(source: &str) -> (Vec<Token>, usize, Option<ExprError>) {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        if let Some(err) = lexer.error() {
            return (Vec::new(), lexer.offset(), Some(err.clone()));
        }
        let is_end = token.kind == TokenKind::End;
        tokens.push(token);
        if is_end {
            return (tokens, lexer.offset(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        let (tokens, _, err) = lex_expression(source);
        assert!(err.is_none(), "unexpected error: {err:?}");
        tokens.iter().map(|t| t.kind).collect()
    }

    fn values(source: &str) -> Vec<String> {
        let (tokens, _, err) = lex_expression(source);
        assert!(err.is_none(), "unexpected error: {err:?}");
        tokens.iter().map(|t| t.value.clone()).collect()
    }

    #[test]
    fn identifier_and_end_marker() {
        assert_eq!(kinds("github }}"), vec![TokenKind::Ident, TokenKind::End]);
        assert_eq!(values("github }}"), vec!["github", "}}"]);
    }

    #[test]
    fn identifiers_may_contain_dashes_and_underscores() {
        assert_eq!(kinds("job-status_name }}"), vec![TokenKind::Ident, TokenKind::End]);
        assert_eq!(values("job-status_name }}")[0], "job-status_name");
    }

    #[test]
    fn string_literals_escape_by_doubling() {
        assert_eq!(kinds("'a' }}"), vec![TokenKind::String, TokenKind::End]);
        // A doubled quote stays inside the literal.
        assert_eq!(values("'it''s' }}")[0], "'it''s'");
    }

    #[test]
    fn numbers_lex_as_int_and_float() {
        assert_eq!(kinds("42 }}"), vec![TokenKind::Int, TokenKind::End]);
        assert_eq!(kinds("-7 }}"), vec![TokenKind::Int, TokenKind::End]);
        assert_eq!(kinds("1.5 }}"), vec![TokenKind::Float, TokenKind::End]);
        assert_eq!(kinds("1e3 }}"), vec![TokenKind::Float, TokenKind::End]);
        assert_eq!(kinds("1.5e-3 }}"), vec![TokenKind::Float, TokenKind::End]);
    }

    #[test]
    fn hexadecimal_integers_are_supported() {
        assert_eq!(kinds("0x1f }}"), vec![TokenKind::Int, TokenKind::End]);
        assert_eq!(values("0x1f }}")[0], "0x1f");
    }

    #[test]
    fn operators_lex_as_two_character_tokens() {
        assert_eq!(kinds("<= }}"), vec![TokenKind::LessEq, TokenKind::End]);
        assert_eq!(kinds(">= }}"), vec![TokenKind::GreaterEq, TokenKind::End]);
        assert_eq!(kinds("== }}"), vec![TokenKind::Eq, TokenKind::End]);
        assert_eq!(kinds("!= }}"), vec![TokenKind::NotEq, TokenKind::End]);
        assert_eq!(kinds("&& }}"), vec![TokenKind::And, TokenKind::End]);
        assert_eq!(kinds("|| }}"), vec![TokenKind::Or, TokenKind::End]);
    }

    #[test]
    fn single_character_operators() {
        // Ten single-character operators plus the terminator.
        assert_eq!(kinds("< > ! . * , ( ) [ ] }}").len(), 11);
        assert_eq!(
            kinds("< > ! . * , ( ) [ ] }}")[0..3],
            [TokenKind::Less, TokenKind::Greater, TokenKind::Not]
        );
    }

    #[test]
    fn whitespace_is_skipped() {
        // Two identifiers separated by a newline and a tab.
        assert_eq!(
            kinds("  a \n b \t}}"),
            vec![TokenKind::Ident, TokenKind::Ident, TokenKind::End]
        );
        assert_eq!(values("  a \n b \t}}")[0], "a");
        assert_eq!(values("  a \n b \t}}")[1], "b");
    }

    #[test]
    fn missing_terminator_is_an_error() {
        let (_, _, err) = lex_expression("github");
        assert!(err.is_some(), "must require the }} terminator");
    }

    #[test]
    fn double_quote_gets_a_hint() {
        let (_, _, err) = lex_expression("\"a\" }}");
        let err = err.expect("error");
        assert!(
            err.message.contains("only single quotes are available"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn unknown_character_is_rejected() {
        let (_, _, err) = lex_expression("# }}");
        assert!(err.is_some(), "unexpected character must fail");
    }

    #[test]
    fn unterminated_string_is_rejected() {
        let (_, _, err) = lex_expression("'abc }}");
        assert!(err.is_some(), "unterminated string must fail");
    }

    #[test]
    fn trailing_text_after_a_number_is_rejected() {
        let (_, _, err) = lex_expression("12abc }}");
        assert!(err.is_some(), "letters after a number must fail");
    }

    #[test]
    fn token_offsets_are_byte_offsets() {
        let (tokens, _, _) = lex_expression("abc == 'x' }}");
        assert_eq!(tokens[0].offset, 0);
        assert_eq!(tokens[1].offset, 4);
        assert_eq!(tokens[2].offset, 7);
    }

    #[test]
    fn offset_points_past_the_terminator() {
        let (_, offset, _) = lex_expression("abc }}");
        assert_eq!(offset, 6);
    }

    #[test]
    fn line_and_column_are_one_based() {
        let (tokens, _, _) = lex_expression("a\nb }}");
        assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
        assert_eq!((tokens[1].line, tokens[1].column), (2, 1));
    }

    #[test]
    fn token_kind_spellings_match_upstream() {
        assert_eq!(TokenKind::Ident.to_string(), "IDENT");
        assert_eq!(TokenKind::End.to_string(), "END");
        assert_eq!(TokenKind::NotEq.to_string(), "!=");
        assert_eq!(TokenKind::Unknown.to_string(), "UNKNOWN");
    }

    #[test]
    fn token_display_includes_position() {
        let (tokens, _, _) = lex_expression("abc }}");
        assert_eq!(tokens[0].to_string(), "IDENT:1:1:0");
    }
}
