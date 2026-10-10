//! Recursive-descent parser for the GitHub Actions expression language.
//!
//! Port of `actionlint`'s `expr_parser.go`. The precedence chain is
//! `||` → `&&` → comparison → prefix `!` → postfix `.`/`[]` → primary, and
//! every binary level is **right**-associative, matching upstream. That
//! associativity is observable: `a || b || c` groups as `a || (b || c)`.

use crate::expr::ast::{CompareOpKind, ExprNode, LogicalOpKind};
use crate::expr::lexer::{ExprError, Lexer, Token, TokenKind};

/// Parses one expression. `source` must still contain the closing `}}`.
pub fn parse(source: &str) -> Result<ExprNode, ExprError> {
    let mut parser = Parser::new(source);
    parser.run()
}

struct Parser {
    lexer: Lexer,
    cur: Token,
    error: Option<ExprError>,
}

impl Parser {
    fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source);
        let cur = lexer.next_token();
        Self {
            lexer,
            cur,
            error: None,
        }
    }

    fn run(&mut self) -> Result<ExprNode, ExprError> {
        let root = self.parse_logical_or();
        if let Some(err) = self.error() {
            return Err(err);
        }
        let root = root.ok_or_else(|| ExprError {
            message: "empty expression".to_string(),
            offset: self.cur.offset,
            line: self.cur.line,
            column: self.cur.column,
        })?;

        if self.cur.kind != TokenKind::End {
            // Mirror upstream: drain the rest so the message can list what was
            // left over.
            let mut remaining = vec![self.cur.kind.to_string()];
            let mut count = 1;
            loop {
                let next = self.lexer.next_token();
                if next.kind == TokenKind::End {
                    break;
                }
                remaining.push(next.kind.to_string());
                count += 1;
            }
            let message = format!(
                "parser did not reach end of input after parsing the expression. \
                 {count} remaining token(s) in the input: {}",
                quotes_builder(&remaining).build()
            );
            return Err(ExprError {
                message,
                offset: self.cur.offset,
                line: self.cur.line,
                column: self.cur.column,
            });
        }

        Ok(root)
    }

    fn error(&self) -> Option<ExprError> {
        if let Some(err) = self.lexer.error() {
            return Some(err.clone());
        }
        self.error.clone()
    }

    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(ExprError {
                message,
                offset: self.cur.offset,
                line: self.cur.line,
                column: self.cur.column,
            });
        }
    }

    fn unexpected(&mut self, where_: &str, expected: &[TokenKind]) {
        if self.error.is_some() {
            return;
        }
        let expected: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
        let what = if self.cur.kind == TokenKind::End {
            "end of input".to_string()
        } else {
            format!("token \"{}\"", self.cur.kind)
        };
        self.fail(format!(
            "unexpected {what} while parsing {where_}. expecting {}",
            quotes_builder(&expected).build()
        ));
    }

    fn next(&mut self) -> Token {
        let current = self.cur.clone();
        self.cur = self.lexer.next_token();
        current
    }

    fn peek_kind(&self) -> TokenKind {
        self.cur.kind
    }

    fn parse_ident(&mut self) -> Option<ExprNode> {
        let ident = self.next();

        if self.peek_kind() == TokenKind::LeftParen {
            // Function calls are parsed at primary level, not postfix, because
            // the callee is always a built-in name in workflow expressions.
            self.next();
            let mut args = Vec::new();
            if self.peek_kind() == TokenKind::RightParen {
                self.next();
            } else {
                loop {
                    args.push(self.parse_logical_or()?);
                    match self.peek_kind() {
                        TokenKind::Comma => {
                            self.next();
                        }
                        TokenKind::RightParen => {
                            self.next();
                            break;
                        }
                        _ => {
                            self.unexpected(
                                "arguments of function call",
                                &[TokenKind::Comma, TokenKind::RightParen],
                            );
                            return None;
                        }
                    }
                }
            }
            // Upstream keeps the callee's original casing here; only variable
            // access and property names are folded.
            return Some(ExprNode::FuncCall {
                callee: ident.value.clone(),
                args,
                token: ident,
            });
        }

        match ident.value.as_str() {
            // Keywords are case sensitive: TRUE and FALSE are variable names.
            "null" => Some(ExprNode::Null(ident)),
            "true" => Some(ExprNode::Bool {
                value: true,
                token: ident,
            }),
            "false" => Some(ExprNode::Bool {
                value: false,
                token: ident,
            }),
            // Variable access is case insensitive.
            other => Some(ExprNode::Variable {
                name: other.to_lowercase(),
                token: ident,
            }),
        }
    }

    fn parse_nested_expr(&mut self) -> Option<ExprNode> {
        self.next();
        let nested = self.parse_logical_or()?;
        if self.peek_kind() == TokenKind::RightParen {
            self.next();
            Some(nested)
        } else {
            self.unexpected(
                "closing ')' of nested expression (...)",
                &[TokenKind::RightParen],
            );
            None
        }
    }

    fn parse_int(&mut self) -> Option<ExprNode> {
        let text = self.cur.value.clone();
        let token = self.next();
        let value = match parse_go_int(&text) {
            Some(value) => value,
            None => {
                self.fail(format!(
                    "parsing invalid integer literal {text:?}: value out of range or not an integer"
                ));
                return None;
            }
        };
        Some(ExprNode::Int { value, token })
    }

    fn parse_float(&mut self) -> Option<ExprNode> {
        let text = self.cur.value.clone();
        let token = self.next();
        let value: f64 = match text.parse() {
            Ok(value) => value,
            Err(_) => {
                self.fail(format!("parsing invalid float literal {text:?}"));
                return None;
            }
        };
        Some(ExprNode::Float { value, token })
    }

    fn parse_string(&mut self) -> Option<ExprNode> {
        let token = self.next();
        let inner = token
            .value
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(&token.value)
            .to_string();
        Some(ExprNode::String {
            value: inner.replace("''", "'"),
            token,
        })
    }

    fn parse_primary_expr(&mut self) -> Option<ExprNode> {
        match self.peek_kind() {
            TokenKind::Ident => self.parse_ident(),
            TokenKind::LeftParen => self.parse_nested_expr(),
            TokenKind::Int => self.parse_int(),
            TokenKind::Float => self.parse_float(),
            TokenKind::String => self.parse_string(),
            _ => {
                self.unexpected(
                    "variable access, function call, null, bool, int, float or string",
                    &[
                        TokenKind::Ident,
                        TokenKind::LeftParen,
                        TokenKind::Int,
                        TokenKind::Float,
                        TokenKind::String,
                    ],
                );
                None
            }
        }
    }

    fn parse_postfix_op(&mut self) -> Option<ExprNode> {
        let mut node = self.parse_primary_expr()?;

        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.next();
                    match self.peek_kind() {
                        TokenKind::Star => {
                            self.next();
                            node = ExprNode::ArrayDeref {
                                receiver: Box::new(node),
                                token: self.cur.clone(),
                            };
                        }
                        TokenKind::Ident => {
                            let name = self.next();
                            // Property names are case insensitive.
                            node = ExprNode::ObjectDeref {
                                receiver: Box::new(node),
                                property: name.value.to_lowercase(),
                                token: name,
                            };
                        }
                        _ => {
                            self.unexpected(
                                "object property dereference like 'a.b' or array element dereference like 'a.*'",
                                &[TokenKind::Ident, TokenKind::Star],
                            );
                            return None;
                        }
                    }
                }
                TokenKind::LeftBracket => {
                    self.next();
                    let index = self.parse_logical_or()?;
                    node = ExprNode::IndexAccess {
                        operand: Box::new(node),
                        index: Box::new(index),
                        token: self.cur.clone(),
                    };
                    if self.peek_kind() != TokenKind::RightBracket {
                        self.unexpected(
                            "closing bracket ']' for index access",
                            &[TokenKind::RightBracket],
                        );
                        return None;
                    }
                    self.next();
                }
                _ => return Some(node),
            }
        }
    }

    fn parse_prefix_op(&mut self) -> Option<ExprNode> {
        if self.peek_kind() != TokenKind::Not {
            return self.parse_postfix_op();
        }
        let token = self.next();
        let operand = self.parse_prefix_op()?;
        Some(ExprNode::NotOp {
            operand: Box::new(operand),
            token,
        })
    }

    fn parse_compare_bin_op(&mut self) -> Option<ExprNode> {
        let left = self.parse_prefix_op()?;

        let kind = match self.peek_kind() {
            TokenKind::Less => CompareOpKind::Less,
            TokenKind::LessEq => CompareOpKind::LessEq,
            TokenKind::Greater => CompareOpKind::Greater,
            TokenKind::GreaterEq => CompareOpKind::GreaterEq,
            TokenKind::Eq => CompareOpKind::Eq,
            TokenKind::NotEq => CompareOpKind::NotEq,
            _ => return Some(left),
        };
        let token = self.next();

        let right = self.parse_compare_bin_op()?;
        Some(ExprNode::CompareOp {
            kind,
            left: Box::new(left),
            right: Box::new(right),
            token,
        })
    }

    fn parse_logical_and(&mut self) -> Option<ExprNode> {
        let left = self.parse_compare_bin_op()?;
        if self.peek_kind() != TokenKind::And {
            return Some(left);
        }
        let token = self.next();
        let right = self.parse_logical_and()?;
        Some(ExprNode::LogicalOp {
            kind: LogicalOpKind::And,
            left: Box::new(left),
            right: Box::new(right),
            token,
        })
    }

    fn parse_logical_or(&mut self) -> Option<ExprNode> {
        let left = self.parse_logical_and()?;
        if self.peek_kind() != TokenKind::Or {
            return Some(left);
        }
        let token = self.next();
        let right = self.parse_logical_or()?;
        Some(ExprNode::LogicalOp {
            kind: LogicalOpKind::Or,
            left: Box::new(left),
            right: Box::new(right),
            token,
        })
    }
}

/// Parses an integer the way Go's `strconv.ParseInt(s, 0, 32)` does.
///
/// Base 0 means the prefix decides: `0x` hexadecimal, `0` octal, otherwise
/// decimal, and a leading sign is allowed. Values beyond 32 bits are rejected,
/// as upstream.
fn parse_go_int(text: &str) -> Option<i64> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = if let Some(hex) = rest.strip_prefix("0x").or(rest.strip_prefix("0X")) {
        (16, hex)
    } else if let Some(oct) = rest.strip_prefix("0o").or(rest.strip_prefix("0O")) {
        (8, oct)
    } else if let Some(bin) = rest.strip_prefix("0b").or(rest.strip_prefix("0B")) {
        (2, bin)
    } else if rest.len() > 1 && rest.starts_with('0') {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    if digits.is_empty() {
        return None;
    }
    let magnitude = i64::from_str_radix(digits, radix).ok()?;
    let signed = if negative { -magnitude } else { magnitude };
    // Upstream parses into a 32-bit int.
    (i32::try_from(signed).is_ok()).then_some(signed)
}

/// Joins token names the way upstream's `quotesBuilder` does.
fn quotes_builder(parts: &[String]) -> QuotesBuilder {
    QuotesBuilder {
        parts: parts.to_vec(),
    }
}

struct QuotesBuilder {
    parts: Vec<String>,
}

impl QuotesBuilder {
    fn build(&self) -> String {
        self.parts
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(" or ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(source: &str) -> ExprNode {
        parse(source).unwrap_or_else(|err| panic!("{source:?} must parse: {err}"))
    }

    fn err(source: &str) -> ExprError {
        parse(source).unwrap_err()
    }

    #[test]
    fn parses_a_bare_variable() {
        assert!(matches!(ok("github }}"), ExprNode::Variable { .. }));
    }

    #[test]
    fn variable_names_are_folded() {
        match ok("GitHub }}") {
            ExprNode::Variable { name, .. } => assert_eq!(name, "github"),
            other => panic!("expected a variable, got {other:?}"),
        }
    }

    #[test]
    fn keywords_are_case_sensitive() {
        assert!(matches!(ok("true }}"), ExprNode::Bool { value: true, .. }));
        assert!(matches!(ok("null }}"), ExprNode::Null(_)));
        // Uppercase is a variable name, not the keyword.
        match ok("TRUE }}") {
            ExprNode::Variable { name, .. } => assert_eq!(name, "true"),
            other => panic!("expected a variable, got {other:?}"),
        }
    }

    #[test]
    fn parses_property_access() {
        match ok("github.event_name }}") {
            ExprNode::ObjectDeref {
                receiver, property, ..
            } => {
                assert_eq!(property, "event_name");
                assert!(matches!(*receiver, ExprNode::Variable { .. }));
            }
            other => panic!("expected an object deref, got {other:?}"),
        }
    }

    #[test]
    fn property_names_are_folded() {
        match ok("github.Event_Name }}") {
            ExprNode::ObjectDeref { property, .. } => assert_eq!(property, "event_name"),
            other => panic!("expected an object deref, got {other:?}"),
        }
    }

    #[test]
    fn parses_wildcard_dereference() {
        assert!(matches!(ok("matrix.* }}"), ExprNode::ArrayDeref { .. }));
    }

    #[test]
    fn parses_index_access() {
        assert!(matches!(
            ok("matrix.os[0] }}"),
            ExprNode::IndexAccess { .. }
        ));
    }

    #[test]
    fn parses_function_calls() {
        match ok("format('{0}', 'x') }}") {
            ExprNode::FuncCall { callee, args, .. } => {
                assert_eq!(callee, "format");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected a call, got {other:?}"),
        }
    }

    #[test]
    fn parses_zero_argument_calls() {
        match ok("success() }}") {
            ExprNode::FuncCall { callee, args, .. } => {
                assert_eq!(callee, "success");
                assert!(args.is_empty());
            }
            other => panic!("expected a call, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_expressions() {
        let node = ok("(1 == 1) }}");
        assert!(matches!(node, ExprNode::CompareOp { .. }));
    }

    #[test]
    fn parses_number_literals() {
        match ok("42 }}") {
            ExprNode::Int { value, .. } => assert_eq!(value, 42),
            other => panic!("expected an int, got {other:?}"),
        }
        match ok("-7 }}") {
            ExprNode::Int { value, .. } => assert_eq!(value, -7),
            other => panic!("expected an int, got {other:?}"),
        }
        match ok("0x1f }}") {
            ExprNode::Int { value, .. } => assert_eq!(value, 31),
            other => panic!("expected an int, got {other:?}"),
        }
        match ok("1.5 }}") {
            ExprNode::Float { value, .. } => assert!((value - 1.5).abs() < f64::EPSILON),
            other => panic!("expected a float, got {other:?}"),
        }
    }

    #[test]
    fn integers_outside_32_bits_are_rejected() {
        // Upstream parses with `strconv.ParseInt(s, 0, 32)`.
        assert!(parse("99999999999 }}").is_err());
    }

    #[test]
    fn string_literals_unescape_doubled_quotes() {
        match ok("'it''s' }}") {
            ExprNode::String { value, .. } => assert_eq!(value, "it's"),
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    fn logical_operators_are_right_associative() {
        match ok("a || b || c }}") {
            ExprNode::LogicalOp { right, .. } => {
                assert!(
                    matches!(*right, ExprNode::LogicalOp { .. }),
                    "expected right-nested logical op"
                );
            }
            other => panic!("expected a logical op, got {other:?}"),
        }
    }

    #[test]
    fn and_binds_tighter_than_or() {
        match ok("a || b && c }}") {
            ExprNode::LogicalOp {
                kind,
                right,
                ..
            } => {
                assert_eq!(kind, LogicalOpKind::Or);
                assert!(
                    matches!(*right, ExprNode::LogicalOp { kind: LogicalOpKind::And, .. }),
                    "`&&` must bind tighter than `||`"
                );
            }
            other => panic!("expected a logical op, got {other:?}"),
        }
    }

    #[test]
    fn comparisons_bind_tighter_than_logic() {
        match ok("a && b == c }}") {
            ExprNode::LogicalOp { right, .. } => {
                assert!(matches!(*right, ExprNode::CompareOp { .. }));
            }
            other => panic!("expected a logical op, got {other:?}"),
        }
    }

    #[test]
    fn prefix_not_binds_tighter_than_comparison() {
        match ok("!a == b }}") {
            ExprNode::CompareOp { left, .. } => assert!(matches!(*left, ExprNode::NotOp { .. })),
            other => panic!("expected a comparison, got {other:?}"),
        }
    }

    #[test]
    fn postfix_binds_tighter_than_prefix() {
        match ok("!github.sha }}") {
            ExprNode::NotOp { operand, .. } => {
                assert!(matches!(*operand, ExprNode::ObjectDeref { .. }));
            }
            other => panic!("expected a not, got {other:?}"),
        }
    }

    #[test]
    fn trailing_input_is_rejected() {
        let error = err("a b }}");
        assert!(
            error.message.contains("did not reach end of input"),
            "got: {}",
            error.message
        );
    }

    #[test]
    fn missing_operand_is_rejected() {
        assert!(err("github. }}").message.contains("unexpected"));
    }

    #[test]
    fn unclosed_bracket_is_rejected() {
        assert!(err("a[0 }}").message.contains("closing bracket"));
    }

    #[test]
    fn unclosed_nested_expression_is_rejected() {
        assert!(err("(a }}").message.contains("closing ')'"));
    }

    #[test]
    fn unclosed_call_is_rejected() {
        assert!(err("format('x' }}").message.contains("arguments of function call"));
    }

    #[test]
    fn go_style_integer_parsing() {
        assert_eq!(parse_go_int("0"), Some(0));
        assert_eq!(parse_go_int("10"), Some(10));
        assert_eq!(parse_go_int("0x1f"), Some(31));
        assert_eq!(parse_go_int("0X1F"), Some(31));
        assert_eq!(parse_go_int("0o17"), Some(15));
        assert_eq!(parse_go_int("0b101"), Some(5));
        assert_eq!(parse_go_int("017"), Some(15));
        assert_eq!(parse_go_int("-5"), Some(-5));
        assert_eq!(parse_go_int("+5"), Some(5));
        assert_eq!(parse_go_int("2147483647"), Some(2147483647));
        assert_eq!(parse_go_int("2147483648"), None);
        assert_eq!(parse_go_int("abc"), None);
        assert_eq!(parse_go_int("0x"), None);
        assert_eq!(parse_go_int(""), None);
    }

    #[test]
    fn quotes_builder_formats_alternatives() {
        let built = quotes_builder(&[",".to_string(), ")".to_string()]).build();
        assert_eq!(built, "\",\" or \")\"");
    }

    #[test]
    fn real_world_expressions_parse() {
        for source in [
            "github.event_name == 'push' }}",
            "github.ref == 'refs/heads/main' }}",
            "always() }}",
            "success() && !cancelled() }}",
            "contains(github.event.commits.*.message, 'fix') }}",
            "format('{0}-{1}', matrix.os, matrix.arch) }}",
            "needs.build.outputs.result == 'success' }}",
            "startsWith(github.ref, 'refs/tags/v') }}",
            "fromJson(toJSON(matrix)) }}",
        ] {
            assert!(parse(source).is_ok(), "{source:?} must parse");
        }
    }
}
