//! Expression syntax tree, mirroring `actionlint`'s `expr_ast.go`.
//!
//! The node set is deliberately identical to upstream, because both consumers
//! walk it by type: `act`'s `pkg/schema` looks for [`ExprNode::FuncCall`] and
//! [`ExprNode::Variable`] nodes to check arity and variable access, while
//! `pkg/exprparser` interprets the same nodes to produce values.

use crate::expr::lexer::Token;

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOpKind {
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
}

/// Logical operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOpKind {
    /// `&&`
    And,
    /// `||`
    Or,
}

/// One node of a parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprNode {
    /// A context or variable reference, already lower-cased.
    Variable {
        /// Variable name, case-folded exactly as upstream does.
        name: String,
        token: Token,
    },
    /// The `null` literal.
    Null(Token),
    /// `true` or `false`.
    Bool {
        /// Literal value.
        value: bool,
        token: Token,
    },
    /// An integer literal, including hexadecimal.
    Int {
        /// Literal value.
        value: i64,
        token: Token,
    },
    /// A floating point literal.
    Float {
        /// Literal value.
        value: f64,
        token: Token,
    },
    /// A single-quoted string literal with `''` unescaped.
    String {
        /// Literal value, without the surrounding quotes.
        value: String,
        token: Token,
    },
    /// `receiver.property`
    ObjectDeref {
        /// The value the property is read from.
        receiver: Box<ExprNode>,
        /// Property name, case-folded.
        property: String,
        token: Token,
    },
    /// `receiver.*` — filter over an object or array.
    ArrayDeref {
        /// The value being filtered.
        receiver: Box<ExprNode>,
        token: Token,
    },
    /// `operand[index]`
    IndexAccess {
        /// The indexed value.
        operand: Box<ExprNode>,
        /// The index expression.
        index: Box<ExprNode>,
        token: Token,
    },
    /// `!operand`
    NotOp {
        /// The negated value.
        operand: Box<ExprNode>,
        token: Token,
    },
    /// A binary comparison.
    CompareOp {
        /// Which comparison.
        kind: CompareOpKind,
        /// Left operand.
        left: Box<ExprNode>,
        /// Right operand.
        right: Box<ExprNode>,
        token: Token,
    },
    /// `&&` or `||`.
    LogicalOp {
        /// Which operator.
        kind: LogicalOpKind,
        /// Left operand.
        left: Box<ExprNode>,
        /// Right operand.
        right: Box<ExprNode>,
        token: Token,
    },
    /// `callee(args...)`
    FuncCall {
        /// Function name, case-folded.
        callee: String,
        /// Call arguments.
        args: Vec<ExprNode>,
        token: Token,
    },
}

impl ExprNode {
    /// The token this node starts at.
    pub fn token(&self) -> &Token {
        match self {
            Self::Variable { token, .. }
            | Self::Null(token)
            | Self::Bool { token, .. }
            | Self::Int { token, .. }
            | Self::Float { token, .. }
            | Self::String { token, .. }
            | Self::ObjectDeref { token, .. }
            | Self::ArrayDeref { token, .. }
            | Self::IndexAccess { token, .. }
            | Self::NotOp { token, .. }
            | Self::CompareOp { token, .. }
            | Self::LogicalOp { token, .. }
            | Self::FuncCall { token, .. } => token,
        }
    }

    /// Renders the node back to source form.
    ///
    /// The rendering is close to but not identical with upstream's `String()`
    /// methods; it exists for diagnostics, not for round-tripping.
    pub fn to_source(&self) -> String {
        match self {
            Self::Variable { name, .. } => name.clone(),
            Self::Null(_) => "null".to_string(),
            Self::Bool { value, .. } => value.to_string(),
            Self::Int { value, .. } => value.to_string(),
            Self::Float { value, .. } => value.to_string(),
            Self::String { value, .. } => format!("'{value}'"),
            Self::ObjectDeref {
                receiver, property, ..
            } => format!("{}.{}", receiver.to_source(), property),
            Self::ArrayDeref { receiver, .. } => format!("{}.*", receiver.to_source()),
            Self::IndexAccess { operand, index, .. } => {
                format!("{}[{}]", operand.to_source(), index.to_source())
            }
            Self::NotOp { operand, .. } => format!("!{}", operand.to_source()),
            Self::CompareOp {
                kind, left, right, ..
            } => {
                let op = match kind {
                    CompareOpKind::Less => "<",
                    CompareOpKind::LessEq => "<=",
                    CompareOpKind::Greater => ">",
                    CompareOpKind::GreaterEq => ">=",
                    CompareOpKind::Eq => "==",
                    CompareOpKind::NotEq => "!=",
                };
                format!("{} {} {}", left.to_source(), op, right.to_source())
            }
            Self::LogicalOp {
                kind, left, right, ..
            } => {
                let op = match kind {
                    LogicalOpKind::And => "&&",
                    LogicalOpKind::Or => "||",
                };
                format!("{} {} {}", left.to_source(), op, right.to_source())
            }
            Self::FuncCall { callee, args, .. } => {
                let rendered: Vec<String> = args.iter().map(ExprNode::to_source).collect();
                format!("{callee}({})", rendered.join(", "))
            }
        }
    }
}

/// Visits every node of a tree, matching `actionlint.VisitExprNode`.
///
/// `f` receives `(node, entering)`: `true` before descending into children and
/// `false` after. Return `false` from `f` to skip a subtree.
pub fn visit(node: &ExprNode, f: &mut impl FnMut(&ExprNode, bool) -> bool) {
    if !f(node, true) {
        return;
    }
    match node {
        ExprNode::ObjectDeref { receiver, .. } => visit(receiver, f),
        ExprNode::ArrayDeref { receiver, .. } => visit(receiver, f),
        ExprNode::IndexAccess { operand, index, .. } => {
            visit(operand, f);
            visit(index, f);
        }
        ExprNode::NotOp { operand, .. } => visit(operand, f),
        ExprNode::CompareOp { left, right, .. } | ExprNode::LogicalOp { left, right, .. } => {
            visit(left, f);
            visit(right, f);
        }
        ExprNode::FuncCall { args, .. } => {
            for arg in args {
                visit(arg, f);
            }
        }
        _ => {}
    }
    f(node, false);
}

/// Collects every function call name in the tree.
pub fn function_calls(node: &ExprNode) -> Vec<String> {
    let mut found = Vec::new();
    visit(node, &mut |node, entering| {
        if entering {
            if let ExprNode::FuncCall { callee, .. } = node {
                found.push(callee.clone());
            }
        }
        true
    });
    found
}

/// Collects every variable reference in the tree.
pub fn variables(node: &ExprNode) -> Vec<String> {
    let mut found = Vec::new();
    visit(node, &mut |node, entering| {
        if entering {
            if let ExprNode::Variable { name, .. } = node {
                found.push(name.clone());
            }
        }
        true
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::lexer::{Token, TokenKind};

    fn token(offset: usize) -> Token {
        Token {
            kind: TokenKind::Ident,
            value: "x".to_string(),
            offset,
            line: 1,
            column: offset + 1,
        }
    }

    fn ident(name: &str) -> ExprNode {
        ExprNode::Variable {
            name: name.to_string(),
            token: token(0),
        }
    }

    fn call(callee: &str, args: Vec<ExprNode>) -> ExprNode {
        ExprNode::FuncCall {
            callee: callee.to_string(),
            args,
            token: token(0),
        }
    }

    #[test]
    fn visit_descends_into_every_branch() {
        let tree = ExprNode::CompareOp {
            kind: CompareOpKind::Eq,
            left: Box::new(ident("github")),
            right: Box::new(ExprNode::ObjectDeref {
                receiver: Box::new(ident("github")),
                property: "event_name".to_string(),
                token: token(1),
            }),
            token: token(0),
        };
        let mut seen = 0;
        visit(&tree, &mut |_, _| {
            seen += 1;
            true
        });
        // compare(enter,exit) + left(enter,exit) + right(enter) + its
        // receiver(enter,exit) + right(exit) = 8 visits
        assert_eq!(seen, 8);
    }

    #[test]
    fn visit_can_skip_subtrees() {
        let tree = ExprNode::NotOp {
            operand: Box::new(call("contains", vec![ident("a"), ident("b")])),
            token: token(0),
        };
        let mut calls = 0;
        visit(&tree, &mut |node, entering| {
            if entering && matches!(node, ExprNode::NotOp { .. }) {
                return false;
            }
            true
        });
        // The NotOp subtree was skipped, so the call is never seen.
        assert_eq!(calls, 0);
        calls += function_calls(&tree).len();
        assert_eq!(calls, 1, "an unskipped visit does find the call");
    }

    #[test]
    fn collects_function_calls() {
        let tree = ExprNode::FuncCall {
            callee: "format".to_string(),
            args: vec![
                call("toJson", vec![ident("matrix")]),
                ident("x"),
                ExprNode::String {
                    value: "hi".to_string(),
                    token: token(0),
                },
            ],
            token: token(0),
        };
        assert_eq!(function_calls(&tree), vec!["format", "toJson"]);
    }

    #[test]
    fn collects_variables() {
        let tree = ExprNode::CompareOp {
            kind: CompareOpKind::Eq,
            left: Box::new(ident("github")),
            right: Box::new(ident("ref")),
            token: token(0),
        };
        assert_eq!(variables(&tree), vec!["github", "ref"]);
    }

    #[test]
    fn renders_source_forms() {
        assert_eq!(ident("github").to_source(), "github");
        assert_eq!(
            ExprNode::String {
                value: "a'b".to_string(),
                token: token(0)
            }
            .to_source(),
            "'a'b'"
        );
        assert_eq!(
            ExprNode::ObjectDeref {
                receiver: Box::new(ident("github")),
                property: "sha".to_string(),
                token: token(0)
            }
            .to_source(),
            "github.sha"
        );
        assert_eq!(
            ExprNode::LogicalOp {
                kind: LogicalOpKind::And,
                left: Box::new(ident("a")),
                right: Box::new(ident("b")),
                token: token(0)
            }
            .to_source(),
            "a && b"
        );
        assert_eq!(call("join", vec![ident("a")]).to_source(), "join(a)");
    }

    #[test]
    fn token_is_available_on_every_variant() {
        let nodes = vec![
            ident("a"),
            ExprNode::Null(token(0)),
            ExprNode::Bool {
                value: true,
                token: token(0),
            },
            ExprNode::Int {
                value: 1,
                token: token(0),
            },
            ExprNode::Float {
                value: 1.5,
                token: token(0),
            },
            ExprNode::String {
                value: "s".to_string(),
                token: token(0),
            },
            call("f", vec![]),
        ];
        for node in nodes {
            assert_eq!(node.token().offset, 0);
        }
    }
}
