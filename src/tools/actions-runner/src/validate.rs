//! Schema validation of a workflow against the embedded definitions.
//!
//! Port of `nektos/act` `pkg/schema`'s `Node` walker. It walks a parsed YAML
//! tree alongside the custom schema language in `schemas/*.json` and reports
//! every mismatch it finds, rather than stopping at the first.
//!
//! Three behaviours are worth naming because they are not obvious:
//!
//! * **A value containing `${{ … }}` is accepted without further checking.**
//!   An interpolated value is not statically knowable, so act skips it. This
//!   is why `runs-on: ${{ matrix.os }}` validates.
//! * **Outside any context, expressions are almost forbidden.** Only integers,
//!   floats and string literals may appear; a variable or function call is an
//!   error. Contexts are pushed by the schema definitions themselves.
//! * **The `insert` directive is a template splice.** A mapping key of
//!   `${{ insert }}` is not a property, it is a marker meaning "expand this
//!   workflow here". It is only allowed inside a context.

use std::fmt;

use regex::Regex;

use crate::expr::ast::ExprNode;
use crate::expr::lexer::TokenKind;
use crate::expr::{function_calls, variables};
use crate::schema::{functions_for, Definition, Schema};
use crate::yaml_node::{Document, NodeId, NodeKind};

/// One validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIssue {
    /// Where the problem is, rendered like upstream's `formatLocation`.
    pub location: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for SchemaIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{}{}", self.location, self.message))
    }
}

impl std::error::Error for SchemaIssue {}

/// Accumulates issues, mirroring Go's `errors.Join`.
#[derive(Debug, Default)]
struct Issues(Vec<SchemaIssue>);

impl Issues {
    fn push(&mut self, location: impl Into<String>, message: impl Into<String>) {
        self.0.push(SchemaIssue {
            location: location.into(),
            message: message.into(),
        });
    }

    fn merged(&mut self, other: Vec<SchemaIssue>) {
        self.0.extend(other);
    }

    fn into_result(self) -> Result<(), Vec<SchemaIssue>> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0)
        }
    }
}

/// Validates nodes of a parsed document against a schema.
pub struct Validator<'a> {
    schema: &'a Schema,
    insert_directive: Regex,
}

impl<'a> Validator<'a> {
    /// Creates a validator for one schema document.
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            // Upstream writes `\$\{{\s*insert\s*}}` and relies on Go's RE2
            // tolerating the braces. The Rust regex crate reads a bare `{` or
            // `}` as syntax, so the literals are spelled as one-character
            // classes, which mean exactly one literal character in both.
            insert_directive: Regex::new(r"[$][{][{]\s*insert\s*[}][}]")
                .expect("insert directive regex must compile"),
        }
    }

    /// Creates a validator for the workflow schema.
    pub fn workflow() -> Self {
        Self::new(crate::schema::workflow_schema())
    }

    /// Creates a validator for the action schema.
    pub fn action() -> Self {
        Self::new(crate::schema::action_schema())
    }

    /// Checks a node against a named definition, starting with an empty
    /// context.
    pub fn check(&self, doc: &Document, node: NodeId, definition: &str) -> Result<(), Vec<SchemaIssue>> {
        self.check_with_context(doc, node, definition, &[])
    }

    /// Checks a node against a named definition inside a context.
    pub fn check_with_context(
        &self,
        doc: &Document,
        node: NodeId,
        definition: &str,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let def = self.schema.definition(definition);
        // A node with no inherited context takes it from its own definition.
        let owned_context: Vec<String>;
        let context = if context.is_empty() {
            owned_context = def.context.clone();
            &owned_context
        } else {
            context
        };

        let Some(node_meta) = doc.node(node) else {
            return Ok(());
        };

        let (is_expression, expression_issues) = self.check_expression(doc, node, context);
        expression_issues?;
        if is_expression {
            return Ok(());
        }

        if def.mapping.is_some() {
            return self.check_mapping(doc, node, &def, context);
        }
        if def.sequence.is_some() {
            return self.check_sequence(doc, node, &def, context);
        }
        if def.one_of.is_some() {
            return self.check_one_of(doc, node, &def, context);
        }

        if node_meta.kind != NodeKind::Scalar {
            return Err(vec![SchemaIssue {
                location: format_location(doc, node),
                message: format!("Expected a scalar got {}", kind_name(node_meta.kind)),
            }]);
        }

        if let Some(string_def) = &def.string {
            return self.check_string(doc, node, string_def, context);
        }
        if def.number.is_some() {
            return self.check_number(doc, node);
        }
        if def.boolean.is_some() {
            return self.check_boolean(doc, node);
        }
        if let Some(allowed) = &def.allowed_values {
            let value = node_meta.value.clone();
            if allowed.contains(&value) {
                return Ok(());
            }
            return Err(vec![SchemaIssue {
                location: format_location(doc, node),
                message: format!("Expected one of {} got {value}", allowed.join(",")),
            }]);
        }
        if def.null.is_some() {
            return Ok(());
        }

        Err(vec![SchemaIssue {
            location: format_location(doc, node),
            message: format!("unsupported definition {definition}"),
        }])
    }

    /// Scans a scalar for `${{ … }}` and validates each expression found.
    ///
    /// Returns whether the value was an expression, in which case the caller
    /// skips structural checks.
    fn check_expression(
        &self,
        doc: &Document,
        node: NodeId,
        context: &[String],
    ) -> (bool, Result<(), Vec<SchemaIssue>>) {
        let Some(meta) = doc.node(node) else {
            return (false, Ok(()));
        };
        let mut value = meta.value.clone();
        let mut had_expression = false;
        let mut issues = Issues::default();
        let location = format_location(doc, node);

        loop {
            let Some(index) = value.find("${{") else {
                return (had_expression, issues.into_result());
            };
            value = value[index + 3..].to_string();
            had_expression = true;

            // The lexer needs the `}}` terminator, which the raw text carries.
            match crate::expr::parse(&value) {
                Ok(parsed) => {
                    if let Err(found) = self.check_single_expression(&parsed, context) {
                        issues.merged(found);
                    }
                    let offset = consumed_offset(&value);
                    if offset == 0 || offset > value.len() {
                        return (had_expression, issues.into_result());
                    }
                    value = value[offset..].to_string();
                }
                Err(err) => {
                    issues.push(location.clone(), format!("Failed to parse: {}", err.message));
                    // Upstream continues the loop here. The remaining text no
                    // longer starts at this `${{`, so the next `find` either
                    // finds a later expression or terminates the scan.
                    if !value.contains("${{") {
                        return (had_expression, issues.into_result());
                    }
                }
            }
        }
    }

    /// Checks one parsed expression against the current context.
    fn check_single_expression(
        &self,
        node: &ExprNode,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let mut issues = Issues::default();

        if context.is_empty() {
            // Without a context, only literal values may be interpolated.
            let kind = node.token().kind;
            if !matches!(kind, TokenKind::Int | TokenKind::Float | TokenKind::String) {
                issues.push(
                    String::new(),
                    "expressions are not allowed here".to_string(),
                );
            }
            return issues.into_result();
        }

        let funcs = functions_for(context);
        for callee in function_calls(node) {
            match funcs.iter().find(|f| f.name.eq_ignore_ascii_case(&callee)) {
                Some(func) => {
                    let argc = function_argument_count(node, &callee);
                    if func.min > argc {
                        issues.push(
                            String::new(),
                            format!(
                                "Missing parameters for {callee} expected >= {} got {argc}",
                                func.min
                            ),
                        );
                    }
                    if func.max < argc {
                        issues.push(
                            String::new(),
                            format!(
                                "Too many parameters for {callee} expected <= {} got {argc}",
                                func.max
                            ),
                        );
                    }
                }
                None => issues.push(String::new(), format!("Unknown Function Call {callee}")),
            }
        }

        for name in variables(node) {
            if !context.iter().any(|allowed| allowed.eq_ignore_ascii_case(&name)) {
                issues.push(String::new(), format!("Unknown Variable Access {name}"));
            }
        }

        issues.into_result()
    }

    fn check_string(
        &self,
        doc: &Document,
        node: NodeId,
        def: &crate::schema::StringDefinition,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let value = doc.node(node).map(|n| n.value.clone()).unwrap_or_default();
        let location = format_location(doc, node);

        if !def.constant.is_empty() && def.constant != value {
            return Err(vec![SchemaIssue {
                location,
                message: format!("Expected {} got {value}", def.constant),
            }]);
        }

        if def.is_expression {
            let mut source = value.clone();
            source.push_str("}}");
            match crate::expr::parse(&source) {
                Ok(parsed) => self
                    .check_single_expression(&parsed, context)
                    .map_err(|issues| prefix_all(issues, &location)),
                Err(err) => Err(vec![SchemaIssue {
                    location,
                    message: format!("Failed to parse: {}", err.message),
                }]),
            }
        } else {
            Ok(())
        }
    }

    fn check_number(&self, doc: &Document, node: NodeId) -> Result<(), Vec<SchemaIssue>> {
        let value = doc.node(node).map(|n| n.value.clone()).unwrap_or_default();
        if value.parse::<f64>().is_ok() {
            return Ok(());
        }
        Err(vec![SchemaIssue {
            location: format_location(doc, node),
            message: format!("Expected a number got {value}"),
        }])
    }

    fn check_boolean(&self, doc: &Document, node: NodeId) -> Result<(), Vec<SchemaIssue>> {
        if doc.boolean(node).is_some() {
            return Ok(());
        }
        let value = doc.node(node).map(|n| n.value.clone()).unwrap_or_default();
        Err(vec![SchemaIssue {
            location: format_location(doc, node),
            message: format!("Expected a boolean got {value}"),
        }])
    }

    fn check_sequence(
        &self,
        doc: &Document,
        node: NodeId,
        def: &Definition,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let Some(meta) = doc.node(node) else {
            return Ok(());
        };
        if meta.kind != NodeKind::Sequence {
            return Err(vec![SchemaIssue {
                location: format_location(doc, node),
                message: format!("Expected a sequence got {}", kind_name(meta.kind)),
            }]);
        }
        let Some(sequence) = &def.sequence else {
            return Ok(());
        };

        let mut issues = Issues::default();
        for child in &meta.content {
            let sub = self.child_context(sequence.item_type.clone(), context);
            if let Err(found) = self.check_with_context(doc, *child, &sequence.item_type, &sub) {
                issues.merged(found);
            }
        }
        issues.into_result()
    }

    fn check_one_of(
        &self,
        doc: &Document,
        node: NodeId,
        def: &Definition,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let Some(alternatives) = &def.one_of else {
            return Ok(());
        };
        let location = format_location(doc, node);
        let mut all = Issues::default();

        for alternative in alternatives {
            let sub = self.child_context(alternative.clone(), context);
            match self.check_with_context(doc, node, alternative, &sub) {
                Ok(()) => return Ok(()),
                Err(found) => {
                    for issue in found {
                        all.push(
                            location.clone(),
                            format!("Failed to match {alternative}: {}", issue.message),
                        );
                    }
                }
            }
        }

        all.into_result()
    }

    fn check_mapping(
        &self,
        doc: &Document,
        node: NodeId,
        def: &Definition,
        context: &[String],
    ) -> Result<(), Vec<SchemaIssue>> {
        let Some(meta) = doc.node(node) else {
            return Ok(());
        };
        if meta.kind != NodeKind::Mapping {
            return Err(vec![SchemaIssue {
                location: format_location(doc, node),
                message: format!("Expected a mapping got {}", kind_name(meta.kind)),
            }]);
        }
        let Some(mapping) = &def.mapping else {
            return Ok(());
        };

        let mut issues = Issues::default();
        for (key_id, value_id) in doc.map_entries(node) {
            let Some(key) = doc.scalar(key_id) else {
                continue;
            };

            if self.insert_directive.is_match(&key) {
                if context.is_empty() {
                    issues.push(
                        format_location(doc, key_id),
                        "insert is not allowed here".to_string(),
                    );
                }
                continue;
            }

            let (is_expression, expression_issues) = self.check_expression(doc, key_id, context);
            if let Err(found) = expression_issues {
                issues.merged(found);
                continue;
            }
            if is_expression {
                continue;
            }

            let property = mapping.properties.get(&key);
            let value_definition = match property {
                Some(property) => property.type_name.clone(),
                None => {
                    if mapping.loose_value_type.is_empty() {
                        issues.push(
                            format_location(doc, key_id),
                            format!("Unknown Property {key}"),
                        );
                        continue;
                    }
                    mapping.loose_value_type.clone()
                }
            };

            let sub = self.child_context(value_definition.clone(), context);
            if let Err(found) = self.check_with_context(doc, value_id, &value_definition, &sub) {
                issues.merged(found);
            }
        }

        issues.into_result()
    }

    /// The context a child definition inherits: the parent's, plus whatever
    /// the child's own definition declares.
    fn child_context(&self, definition: String, parent: &[String]) -> Vec<String> {
        let mut out = parent.to_vec();
        out.extend(self.schema.definition(&definition).context.clone());
        out
    }
}

/// How many arguments a specific call in the tree was given.
fn function_argument_count(node: &ExprNode, callee: &str) -> usize {
    let mut found = 0;
    crate::expr::ast::visit(node, &mut |node, entering| {
        if entering {
            if let ExprNode::FuncCall {
                callee: name,
                args,
                ..
            } = node
            {
                if name.eq_ignore_ascii_case(callee) {
                    found = args.len();
                }
            }
        }
        true
    });
    found
}

/// Byte offset just past the `}}` of the first expression in `value`.
fn consumed_offset(value: &str) -> usize {
    match value.find("}}") {
        Some(index) => index + 2,
        None => 0,
    }
}

fn prefix_all(issues: Vec<SchemaIssue>, prefix: &str) -> Vec<SchemaIssue> {
    issues
        .into_iter()
        .map(|issue| SchemaIssue {
            location: format!("{prefix}{}", issue.location),
            message: issue.message,
        })
        .collect()
}

fn format_location(doc: &Document, node: NodeId) -> String {
    doc.node(node)
        .map(|n| format!("Line: {} Column {}: ", n.line, n.column))
        .unwrap_or_default()
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Document => "document",
        NodeKind::Sequence => "sequence",
        NodeKind::Mapping => "mapping",
        NodeKind::Scalar => "scalar",
        NodeKind::Alias => "alias",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(source: &str, definition: &str) -> Result<(), Vec<SchemaIssue>> {
        let doc = parsed(source);
        let root = doc.root().expect("root");
        Validator::workflow().check(&doc, root, definition)
    }

    /// Checks the value stored under `key` in a single-entry mapping.
    fn check_value(source: &str, key: &str, definition: &str) -> Result<(), Vec<SchemaIssue>> {
        let doc = parsed(source);
        let root = doc.root().expect("root");
        let value = doc.map_get(root, key).expect("key must exist");
        Validator::workflow().check(&doc, value, definition)
    }

    fn parsed(source: &str) -> Document {
        let mut doc = Document::parse(source).expect("parses");
        doc.resolve_aliases().expect("resolves");
        doc
    }

    fn issues(source: &str, definition: &str) -> Vec<String> {
        check(source, definition)
            .expect_err("expected validation to fail")
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_valid_scalar_passes() {
        assert!(check("name: CI\njobs: {}\n", "workflow-root").is_ok());
    }

    #[test]
    fn unknown_property_is_rejected() {
        let found = issues("nonsense: 1\n", "workflow-root");
        assert!(
            found.iter().any(|i| i.contains("Unknown Property")),
            "got {found:?}"
        );
    }

    #[test]
    fn wrong_shape_is_rejected() {
        let found = issues("- a\n- b\n", "workflow-root");
        assert!(
            found.iter().any(|i| i.contains("Expected a mapping")),
            "got {found:?}"
        );
    }

    #[test]
    fn interpolated_values_are_accepted() {
        // `runs-on: ${{ matrix.os }}` must validate: an interpolated value is
        // not statically knowable, so act skips the structural check.
        assert!(check_value("runs-on: ${{ matrix.os }}\n", "runs-on", "runs-on").is_ok());
        assert!(check_value("if: ${{ success() }}\n", "if", "job-if").is_ok());
    }

    #[test]
    fn expressions_outside_a_context_are_rejected() {
        // No context means only literals may be interpolated.
        let found = issues("value: ${{ some.variable }}\n", "any");
        assert!(
            found.iter().any(|i| i.contains("expressions are not allowed")),
            "got {found:?}"
        );
    }

    #[test]
    fn literal_expressions_outside_a_context_are_allowed() {
        assert!(check("value: ${{ 42 }}\n", "any").is_ok());
        assert!(check("value: ${{ 'text' }}\n", "any").is_ok());
    }

    #[test]
    fn insert_directive_is_rejected_outside_a_context() {
        // The directive is a mapping *key*, so the document must be a mapping.
        let found = issues("${{ insert }}: value\n", "any");
        assert!(
            found.iter().any(|i| i.contains("insert is not allowed")),
            "got {found:?}"
        );
    }

    #[test]
    fn a_real_workflow_validates() {
        let source = concat!(
            "name: CI\n",
            "on:\n  push:\n    branches: [main]\n",
            "jobs:\n  build:\n",
            "    runs-on: ubuntu-latest\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4\n",
            "      - run: echo hi\n",
            "        shell: bash\n",
        );
        assert!(check(source, "workflow-root").is_ok());
    }

    #[test]
    fn a_real_workflow_with_matrix_validates() {
        let source = concat!(
            "name: CI\n",
            "on: [push, pull_request]\n",
            "jobs:\n  build:\n",
            "    strategy:\n      matrix:\n        os: [ubuntu-latest]\n        include:\n          - experimental: true\n",
            "    runs-on: ${{ matrix.os }}\n",
            "    steps:\n      - run: echo ${{ matrix.os }}\n",
        );
        assert!(check(source, "workflow-root").is_ok());
    }

    #[test]
    fn a_real_workflow_with_a_bad_step_fails() {
        let source = concat!(
            "name: CI\n",
            "on: [push]\n",
            "jobs:\n  build:\n",
            "    runs-on: ubuntu-latest\n",
            "    steps:\n      - uses: actions/checkout@v4\n        bogus: yes\n",
        );
        assert!(check(source, "workflow-root").is_err());
    }

    #[test]
    fn sequences_check_every_item() {
        let found = issues("- 1\n- nonsense\n", "sequence-of-string");
        assert!(!found.is_empty(), "got {found:?}");
    }

    #[test]
    fn allowed_values_are_enforced() {
        let schema = Validator::workflow();
        let mut doc = Document::parse("push\n").expect("parses");
        doc.resolve_aliases().expect("resolves");
        let root = doc.root().expect("root");
        // `on` values are constrained by the schema.
        let result = schema.check(&doc, root, "on");
        assert!(result.is_ok(), "push is a valid event");
    }

    #[test]
    fn issue_display_includes_location() {
        let issue = SchemaIssue {
            location: "Line: 1 Column 1: ".to_string(),
            message: "boom".to_string(),
        };
        assert_eq!(issue.to_string(), "Line: 1 Column 1: boom");
    }

    #[test]
    fn kind_names_match_upstream() {
        assert_eq!(kind_name(NodeKind::Mapping), "mapping");
        assert_eq!(kind_name(NodeKind::Sequence), "sequence");
        assert_eq!(kind_name(NodeKind::Scalar), "scalar");
        assert_eq!(kind_name(NodeKind::Alias), "alias");
    }
}
