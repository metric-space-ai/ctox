//! GitHub Actions expression language: lexer, AST and parser.
//!
//! This is the Rust replacement for `github.com/rhysd/actionlint`'s expression
//! subsystem, which `nektos/act` uses in two places:
//!
//! * `pkg/schema` *validates* `${{ ... }}` — function arity and variable
//!   access against the context path.
//! * `pkg/exprparser` *evaluates* `${{ ... }}`.
//!
//! In Go both share actionlint's parser. Here the parser is implemented once
//! and serves both consumers, which is one of the few places where the port is
//! structurally simpler than the original.
//!
//! Upstream: <https://github.com/rhysd/actionlint> (MIT, Copyright (c) Yasuhiro
//! Matsumoto), consumed by <https://github.com/nektos/act> (MIT, Copyright (c)
//! Christoph Schitt).
//!
//! Syntax: <https://docs.github.com/en/actions/learn-github-actions/expressions>

pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;

pub use ast::{function_calls, variables, visit, CompareOpKind, ExprNode, LogicalOpKind};
pub use interpreter::{
    format_float_g, from_json_value, is_truthy, DefaultStatus, DefaultStatusCheck, EvalError,
    EvaluationContext, EvaluationEnvironment, Interpreter, StatusProvider, Value,
};
pub use lexer::{lex_expression, ExprError, Lexer, Token, TokenKind};
pub use parser::parse;

/// Extracts every `${{ ... }}` expression from a raw YAML scalar value.
///
/// act calls this while walking schema definitions: it scans for `${{`,
/// parses the expression, resumes after the lexer offset, and repeats. The
/// returned offsets are byte offsets into `value`.
///
/// Returns the expressions in source order together with any error, so a
/// caller can keep scanning after a bad expression exactly as act does.
pub fn scan_expressions(value: &str) -> (Vec<ExprNode>, Option<ExprError>) {
    let mut nodes = Vec::new();
    let mut first_error = None;
    let mut rest = value;

    loop {
        let Some(index) = rest.find("${{") else {
            return (nodes, first_error);
        };
        let after_open = &rest[index + 3..];
        let (tokens, offset, lex_error) = lex_expression(after_open);
        if let Some(err) = lex_error {
            first_error.get_or_insert(err);
            return (nodes, first_error);
        }

        // The End token is the `}}` terminator. Re-parse just the body so the
        // parser never has to see the terminator twice.
        let body_end = tokens
            .iter()
            .position(|t| t.kind == TokenKind::End)
            .map(|i| tokens[i].offset)
            .unwrap_or(offset);
        let body = &after_open[..body_end];
        // Re-append the `}}` terminator: the lexer needs it, the body has none.
        let mut reparse = String::with_capacity(body.len() + 2);
        reparse.push_str(body);
        reparse.push_str("}}");
        match parser::parse(&reparse) {
            Ok(node) => {
                nodes.push(node);
            }
            Err(err) => {
                first_error.get_or_insert(err);
            }
        }

        if offset == 0 || offset > after_open.len() {
            return (nodes, first_error);
        }
        rest = &after_open[offset..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(value: &str) -> Vec<String> {
        let (nodes, err) = scan_expressions(value);
        assert!(err.is_none(), "unexpected error: {err:?}");
        nodes.iter().map(ExprNode::to_source).collect()
    }

    #[test]
    fn finds_a_single_expression() {
        assert_eq!(sources("prefix ${{ a }} suffix"), vec!["a"]);
    }

    #[test]
    fn finds_several_expressions() {
        assert_eq!(sources("${{ a }} and ${{ b }}"), vec!["a", "b"]);
    }

    #[test]
    fn finds_an_expression_embedded_in_text() {
        assert_eq!(sources("echo ${{ matrix.os }}!"), vec!["matrix.os"]);
    }

    #[test]
    fn finds_an_expression_inside_a_run_block() {
        let raw = "npm run build -- --target=${{ matrix.target }} --os ${{ matrix.os }}";
        assert_eq!(sources(raw), vec!["matrix.target", "matrix.os"]);
    }

    #[test]
    fn returns_nothing_without_expressions() {
        assert!(sources("plain text").is_empty());
    }

    #[test]
    fn returns_nothing_for_empty_text() {
        assert!(sources("").is_empty());
    }

    #[test]
    fn reports_a_broken_expression() {
        let (_, err) = scan_expressions("${{ a b }}");
        assert!(err.is_some(), "must surface a parse error");
    }

    #[test]
    fn reports_a_missing_terminator() {
        let (_, err) = scan_expressions("${{ a");
        assert!(err.is_some(), "must require the closing }}");
    }
}
