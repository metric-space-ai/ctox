//! Interpreter for the GitHub Actions expression language.
//!
//! Port of `nektos/act` `pkg/exprparser`. Where act leans on Go reflection to
//! walk `interface{}` values, this port uses an explicit [`Value`] enum. That
//! is a simplification the port enables rather than a behaviour change: the
//! same kinds are distinguished, and the quirks that come with them are
//! preserved on purpose.
//!
//! The quirks worth naming:
//!
//! * A string operand in a comparison is **re-evaluated as an expression**
//!   before being treated as a number, so `'1' == 1` is true.
//! * A `Float64` of exactly `0.0` is returned from `&&`/`||` and `.*` as an
//!   integer `0`, because `getSafeValue` normalises it. This is observable in
//!   `format()` output.
//! * Property and map lookups are case-insensitive.
//! * `NaN` is falsy, and so is `0`; an empty object or array is truthy.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{json, Value as JsonValue};

use crate::expr::ast::{CompareOpKind, ExprNode, LogicalOpKind};
use crate::expr::lexer::Token;
use crate::expr::parser;

/// A runtime value produced by evaluating an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// No value. Go's `reflect.Invalid`.
    Null,
    /// Boolean.
    Bool(bool),
    /// String.
    String(String),
    /// Integer. Kept distinct from [`Value::Float`] because act's comparison
    /// logic branches on the Go kind.
    Int(i64),
    /// Floating point number.
    Float(f64),
    /// Ordered list.
    Array(Vec<Value>),
    /// String-keyed map.
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// Convenience constructor for an object literal.
    ///
    /// The key is `Into<String>` so a caller writing a literal context does not
    /// have to spell `String::from` on every member — and the context builders
    /// in `runner::expression` are exactly that: `github`, `steps`, `needs`,
    /// each of them a literal member list.
    pub fn object<I, K>(entries: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// The Go kind name, used in error messages.
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "invalid",
            Self::Bool(_) => "bool",
            Self::String(_) => "string",
            Self::Int(_) => "int",
            Self::Float(_) => "float64",
            Self::Array(_) => "slice",
            Self::Object(_) => "map",
        }
    }

    /// Looks a property up case-insensitively, as act does for maps.
    fn property(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Object(map) => map
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The string this value carries, or `None` for every other kind.
    ///
    /// Not a conversion on purpose. A context member that is a number is not a
    /// string, and act's contexts hold typed values — `steps.x.outputs.y` is a
    /// number when the workflow wrote one. Coercing it here would make a
    /// wrong-typed member indistinguishable from a right-typed one, which is
    /// exactly the distinction the evaluator's own comparisons rely on.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The map this value carries, or `None` for every other kind.
    ///
    /// Used to walk a context from the outside — `needs.<job>.outputs` and
    /// `steps.<id>.conclusion` are three levels down, and the callers are
    /// tests and the workflow-facing readers rather than the evaluator, which
    /// goes through [`Value::property`] so that lookup stays case-insensitive.
    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }
}

/// Truthiness, matching `exprparser.IsTruthy`.
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Int(i) => *i != 0,
        Value::Float(f) => !f.is_nan() && *f != 0.0,
        Value::Array(_) | Value::Object(_) => true,
        Value::Null => false,
    }
}

/// The implicit status check applied to `if:` conditions and step
/// continuations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultStatusCheck {
    /// No implicit check; the expression stands on its own.
    None,
    /// `success()`
    Success,
    /// `always()`
    Always,
    /// `cancelled()`
    Cancelled,
    /// `failure()`
    Failure,
}

impl DefaultStatusCheck {
    /// The function name this check maps to.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Success => "success",
            Self::Always => "always",
            Self::Cancelled => "cancelled",
            Self::Failure => "failure",
        }
    }
}

/// Where an expression is being evaluated. `success()` and `failure()` mean
/// different things for a job than for a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationContext {
    /// A `run:` step.
    Step,
    /// A job condition.
    Job,
    /// Any other context, which act rejects.
    Other(String),
}

/// Hook that computes `hashFiles()`. act uses it to hash inside a container.
pub type HashFilesFn = fn(&[Value]) -> Result<Value, EvalError>;

/// Supplies the job/step outcome the status functions report.
///
/// act reads this from `model.Run`; that type arrives with the `model` port, so
/// the dependency is inverted behind a trait.
pub trait StatusProvider {
    /// True when the job succeeded.
    fn job_success(&self) -> bool;
    /// True when the step succeeded.
    fn step_success(&self) -> bool;
    /// True when the job failed.
    fn job_failure(&self) -> bool;
    /// True when the step failed.
    fn step_failure(&self) -> bool;
    /// True when the run was cancelled.
    fn cancelled(&self) -> bool;
}

/// A status provider that reports everything as successful and not cancelled.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultStatus;

impl StatusProvider for DefaultStatus {
    fn job_success(&self) -> bool {
        true
    }
    fn step_success(&self) -> bool {
        true
    }
    fn job_failure(&self) -> bool {
        false
    }
    fn step_failure(&self) -> bool {
        false
    }
    fn cancelled(&self) -> bool {
        false
    }
}

/// Everything an expression can reference.
#[derive(Debug, Default, Clone)]
pub struct EvaluationEnvironment {
    /// The `github` context.
    pub github: Option<Value>,
    /// The `env` context.
    pub env: BTreeMap<String, Value>,
    /// The `job` context.
    pub job: Option<Value>,
    /// The `jobs` context of a reusable workflow call.
    pub jobs: Option<BTreeMap<String, Value>>,
    /// The `steps` context.
    pub steps: BTreeMap<String, Value>,
    /// The `runner` context.
    pub runner: BTreeMap<String, Value>,
    /// The `secrets` context.
    pub secrets: BTreeMap<String, Value>,
    /// The `vars` context.
    pub vars: BTreeMap<String, Value>,
    /// The `strategy` context.
    pub strategy: BTreeMap<String, Value>,
    /// The `matrix` context.
    pub matrix: BTreeMap<String, Value>,
    /// The `needs` context.
    pub needs: BTreeMap<String, Value>,
    /// The `inputs` context.
    pub inputs: BTreeMap<String, Value>,
    /// Overrides `hashFiles()`; act uses this to hash inside a container.
    pub hash_files: Option<HashFilesFn>,
}

impl EvaluationEnvironment {
    /// Looks a top-level context up by name.
    fn context(&self, name: &str) -> Option<Value> {
        Some(match name {
            "github" => self.github.clone()?,
            "env" => Value::Object(self.env.clone()),
            "job" => self.job.clone()?,
            "jobs" => Value::Object(self.jobs.clone()?),
            "steps" => Value::Object(self.steps.clone()),
            "runner" => Value::Object(self.runner.clone()),
            "secrets" => Value::Object(self.secrets.clone()),
            "vars" => Value::Object(self.vars.clone()),
            "strategy" => Value::Object(self.strategy.clone()),
            "matrix" => Value::Object(self.matrix.clone()),
            "needs" => Value::Object(self.needs.clone()),
            "inputs" => Value::Object(self.inputs.clone()),
            "infinity" => Value::Float(f64::INFINITY),
            "nan" => Value::Float(f64::NAN),
            _ => return None,
        })
    }
}

/// An evaluation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    /// What went wrong.
    pub message: String,
}

impl EvalError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EvalError {}

/// Evaluates expressions against an environment.
pub struct Interpreter<'a> {
    env: &'a EvaluationEnvironment,
    status: &'a dyn StatusProvider,
    context: EvaluationContext,
}

impl<'a> Interpreter<'a> {
    /// Creates an interpreter bound to an environment and a status provider.
    pub fn new(
        env: &'a EvaluationEnvironment,
        status: &'a dyn StatusProvider,
        context: EvaluationContext,
    ) -> Self {
        Self {
            env,
            status,
            context,
        }
    }

    /// Evaluates `input`, which may or may not carry the `${{` prefix.
    ///
    /// When `default_status_check` is not [`DefaultStatusCheck::None`] and the
    /// expression does not already contain a status function, the expression
    /// is wrapped in `status() && expr`, which is what makes a bare `if:`
    /// behave like `if: success() && …`.
    pub fn evaluate(
        &self,
        input: &str,
        default_status_check: DefaultStatusCheck,
    ) -> Result<Value, EvalError> {
        let mut body = input.strip_prefix("${{").unwrap_or(input).to_string();
        if default_status_check != DefaultStatusCheck::None && body.is_empty() {
            body = "success()".to_string();
        }

        let mut source = body;
        source.push_str("}}");
        let node = parser::parse(&source)
            .map_err(|err| EvalError::new(format!("Failed to parse: {}", err.message)))?;

        let node = match default_status_check {
            DefaultStatusCheck::None => node,
            check => {
                if has_status_check_function(&node) {
                    node
                } else {
                    ExprNode::LogicalOp {
                        kind: LogicalOpKind::And,
                        left: Box::new(ExprNode::FuncCall {
                            callee: check.as_str().to_string(),
                            args: Vec::new(),
                            token: Token::synthetic(),
                        }),
                        right: Box::new(node),
                        token: Token::synthetic(),
                    }
                }
            }
        };

        self.evaluate_node(&node)
    }

    fn evaluate_node(&self, node: &ExprNode) -> Result<Value, EvalError> {
        match node {
            ExprNode::Variable { name, .. } => self.evaluate_variable(name),
            ExprNode::Bool { value, .. } => Ok(Value::Bool(*value)),
            ExprNode::Null(_) => Ok(Value::Null),
            ExprNode::Int { value, .. } => Ok(Value::Int(*value)),
            ExprNode::Float { value, .. } => Ok(Value::Float(*value)),
            ExprNode::String { value, .. } => Ok(Value::String(value.clone())),
            ExprNode::IndexAccess { operand, index, .. } => {
                self.evaluate_index_access(operand, index)
            }
            ExprNode::ObjectDeref {
                receiver,
                property,
                ..
            } => {
                let left = self.evaluate_node(receiver)?;
                if matches!(**receiver, ExprNode::ArrayDeref { .. }) {
                    self.get_property_value_dereferenced(&left, property)
                } else {
                    self.get_property_value(&left, property)
                }
            }
            ExprNode::ArrayDeref { receiver, .. } => {
                let left = self.evaluate_node(receiver)?;
                Ok(safe_value(left))
            }
            ExprNode::NotOp { operand, .. } => {
                let value = self.evaluate_node(operand)?;
                Ok(Value::Bool(!is_truthy(&value)))
            }
            ExprNode::CompareOp {
                kind, left, right, ..
            } => {
                let left = self.evaluate_node(left)?;
                let right = self.evaluate_node(right)?;
                self.compare_values(&left, &right, *kind)
            }
            ExprNode::LogicalOp {
                kind, left, right, ..
            } => {
                let left = self.evaluate_node(left)?;
                if is_truthy(&left) == (*kind == LogicalOpKind::Or) {
                    return Ok(safe_value(left));
                }
                let right = self.evaluate_node(right)?;
                match kind {
                    LogicalOpKind::And | LogicalOpKind::Or => Ok(safe_value(right)),
                }
            }
            ExprNode::FuncCall { callee, args, .. } => self.evaluate_func_call(callee, args),
        }
    }

    fn evaluate_variable(&self, name: &str) -> Result<Value, EvalError> {
        self.env
            .context(name)
            .ok_or_else(|| EvalError::new(format!("Unavailable context: {name}")))
    }

    fn evaluate_index_access(
        &self,
        operand: &ExprNode,
        index: &ExprNode,
    ) -> Result<Value, EvalError> {
        let left = self.evaluate_node(operand)?;
        let right = self.evaluate_node(index)?;

        match right {
            Value::String(property) => self.get_property_value(&left, &property),
            Value::Int(position) => match &left {
                Value::Array(items) => {
                    if position < 0 || position as usize >= items.len() {
                        return Ok(Value::Null);
                    }
                    Ok(items[position as usize].clone())
                }
                _ => Ok(Value::Null),
            },
            _ => Ok(Value::Null),
        }
    }

    /// Reads `property` from a value, as `getPropertyValue` does.
    fn get_property_value(&self, left: &Value, property: &str) -> Result<Value, EvalError> {
        match left {
            Value::Object(_) => Ok(left.property(property).cloned().unwrap_or(Value::Null)),
            Value::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.get_property_value(item, property)?);
                }
                Ok(Value::Array(values))
            }
            _ => Ok(Value::Null),
        }
    }

    /// Reads `property` from an already-dereferenced value, as
    /// `getPropertyValueDereferenced` does.
    fn get_property_value_dereferenced(
        &self,
        left: &Value,
        property: &str,
    ) -> Result<Value, EvalError> {
        match left {
            Value::Object(map) => {
                let mut values = Vec::with_capacity(map.len());
                for value in map.values() {
                    values.push(self.get_property_value(value, property)?);
                }
                Ok(Value::Array(values))
            }
            Value::Array(_) => self.get_property_value(left, property),
            _ => self.get_property_value(left, property),
        }
    }

    fn compare_values(
        &self,
        left: &Value,
        right: &Value,
        kind: CompareOpKind,
    ) -> Result<Value, EvalError> {
        let (mut left, mut right) = (left.clone(), right.clone());

        if kind_of(&left) != kind_of(&right) {
            if !is_number(&left) {
                left = self.coerce_to_number(&left);
            }
            if !is_number(&right) {
                right = self.coerce_to_number(&right);
            }
        }

        match (&left, &right) {
            (Value::Bool(_), _) => {
                let l = to_f64(&self.coerce_to_number(&left));
                let r = to_f64(&self.coerce_to_number(&right));
                Ok(Value::Bool(compare_number(l, r, kind)))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(compare_string(
                &a.to_lowercase(),
                &b.to_lowercase(),
                kind,
            ))),
            (Value::Int(a), Value::Float(b)) => {
                Ok(Value::Bool(compare_number(*a as f64, *b, kind)))
            }
            (Value::Int(a), Value::Int(b)) => {
                Ok(Value::Bool(compare_number(*a as f64, *b as f64, kind)))
            }
            (Value::Float(a), Value::Int(b)) => {
                Ok(Value::Bool(compare_number(*a, *b as f64, kind)))
            }
            (Value::Float(a), Value::Float(b)) => {
                Ok(Value::Bool(compare_number(*a, *b, kind)))
            }
            (Value::Null, Value::Null) => Ok(Value::Bool(true)),
            (l, r) => Err(EvalError::new(format!(
                "Compare not implemented for types: left: {}, right: {}",
                l.kind_name(),
                r.kind_name()
            ))),
        }
    }

    /// Converts a value to a number, as `coerceToNumber` does.
    ///
    /// A non-empty string is re-evaluated as an expression first, which is why
    /// `'1' == 1` holds.
    fn coerce_to_number(&self, value: &Value) -> Value {
        match value {
            Value::Null => Value::Int(0),
            Value::Bool(true) => Value::Int(1),
            Value::Bool(false) => Value::Int(0),
            Value::Int(_) | Value::Float(_) => value.clone(),
            Value::String(text) => {
                if text.is_empty() {
                    return Value::Int(0);
                }
                match self.evaluate(text, DefaultStatusCheck::None) {
                    Ok(evaluated) if is_number(&evaluated) => evaluated,
                    _ => Value::Float(f64::NAN),
                }
            }
            _ => Value::Float(f64::NAN),
        }
    }

    /// Converts a value to a string, as `coerceToString` does.
    fn coerce_to_string(&self, value: &Value) -> String {
        match value {
            Value::Null => String::new(),
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            Value::String(text) => text.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float_g(*f),
            Value::Array(_) => "Array".to_string(),
            Value::Object(_) => "Object".to_string(),
        }
    }

    fn evaluate_func_call(&self, callee: &str, arg_nodes: &[ExprNode]) -> Result<Value, EvalError> {
        let mut args = Vec::with_capacity(arg_nodes.len());
        for node in arg_nodes {
            args.push(self.evaluate_node(node)?);
        }

        let name = callee.to_lowercase();
        match name.as_str() {
            "contains" => self.contains(&args, "contains"),
            "startswith" => self.contains(&args, "startswith"),
            "endswith" => self.contains(&args, "endswith"),
            "format" => {
                let Some(first) = args.first() else {
                    return Err(EvalError::new("format requires at least one argument"));
                };
                self.format(&self.coerce_to_string(first), &args[1..])
            }
            "join" => {
                let Some(first) = args.first() else {
                    return Err(EvalError::new("join requires at least one argument"));
                };
                let separator = match args.get(1) {
                    Some(value) => self.coerce_to_string(value),
                    None => ",".to_string(),
                };
                Ok(Value::String(self.join(first, &separator)))
            }
            "tojson" => {
                let Some(first) = args.first() else {
                    return Err(EvalError::new("toJSON requires an argument"));
                };
                self.to_json(first)
            }
            "fromjson" => {
                let Some(first) = args.first() else {
                    return Err(EvalError::new("fromJSON requires an argument"));
                };
                self.decode_json(first)
            }
            "hashfiles" => {
                if let Some(hash) = self.env.hash_files {
                    return hash(&args);
                }
                Err(EvalError::new(
                    "TODO: 'hashFiles' requires a workspace and is not yet wired",
                ))
            }
            "always" => Ok(Value::Bool(true)),
            "cancelled" => Ok(Value::Bool(self.status.cancelled())),
            "success" | "failure" => self.status_result(&name),
            other => Err(EvalError::new(format!("TODO: '{other}' not implemented"))),
        }
    }

    fn status_result(&self, name: &str) -> Result<Value, EvalError> {
        let outcome = match &self.context {
            EvaluationContext::Job => {
                if name == "success" {
                    self.status.job_success()
                } else {
                    self.status.job_failure()
                }
            }
            EvaluationContext::Step => {
                if name == "success" {
                    self.status.step_success()
                } else {
                    self.status.step_failure()
                }
            }
            EvaluationContext::Other(context) => {
                return Err(EvalError::new(format!(
                    "Context '{context}' must be one of 'job' or 'step'"
                )));
            }
        };
        Ok(Value::Bool(outcome))
    }

    /// `contains`, `startsWith` and `endsWith` all fold case.
    fn contains(&self, args: &[Value], which: &str) -> Result<Value, EvalError> {
        let (Some(search), Some(item)) = (args.first(), args.get(1)) else {
            return Err(EvalError::new(format!(
                "'{which}' requires two arguments"
            )));
        };

        match search {
            Value::Array(items) => {
                for candidate in items {
                    if let Value::Bool(true) =
                        self.compare_values(candidate, item, CompareOpKind::Eq)?
                    {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            Value::Object(_) | Value::Null => Ok(Value::Bool(false)),
            _ => {
                let haystack = self.coerce_to_string(search).to_lowercase();
                let needle = self.coerce_to_string(item).to_lowercase();
                let found = match which {
                    "contains" => haystack.contains(&needle),
                    "startswith" => haystack.starts_with(&needle),
                    _ => haystack.ends_with(&needle),
                };
                Ok(Value::Bool(found))
            }
        }
    }

    /// `format('{0} {1}', a, b)` with `{{` and `}}` escapes.
    fn format(&self, input: &str, replacements: &[Value]) -> Result<Value, EvalError> {
        let invalid = |reason: &str| {
            EvalError::new(format!(
                "The following format string is {reason}: '{input}'"
            ))
        };

        let mut output = String::with_capacity(input.len());
        let mut index = String::new();
        let mut state = FormatState::PassThrough;

        for character in input.chars() {
            match state {
                FormatState::PassThrough => match character {
                    '{' => state = FormatState::BracketOpen,
                    '}' => state = FormatState::BracketClose,
                    other => output.push(other),
                },
                FormatState::BracketOpen => match character {
                    '{' => {
                        output.push('{');
                        index.clear();
                        state = FormatState::PassThrough;
                    }
                    '}' => {
                        let parsed: i64 = index.parse().map_err(|_| invalid("invalid"))?;
                        index.clear();
                        let Some(value) = replacements.get(parsed.max(0) as usize) else {
                            return Err(invalid(
                                "references more arguments than were supplied",
                            ));
                        };
                        output.push_str(&self.coerce_to_string(value));
                        state = FormatState::PassThrough;
                    }
                    other => index.push(other),
                },
                FormatState::BracketClose => {
                    if character == '}' {
                        output.push('}');
                        index.clear();
                        state = FormatState::PassThrough;
                    } else {
                        return Err(invalid("invalid"));
                    }
                }
            }
        }

        match state {
            FormatState::PassThrough => {}
            FormatState::BracketOpen => return Err(invalid("invalid, unclosed brackets")),
            FormatState::BracketClose => {
                return Err(invalid("invalid, closing bracket without opening one"))
            }
        }

        Ok(Value::String(output))
    }

    fn join(&self, array: &Value, separator: &str) -> String {
        match array {
            Value::Array(items) => items
                .iter()
                .map(|item| self.coerce_to_string(item))
                .collect::<Vec<_>>()
                .join(separator),
            other => self.coerce_to_string(other),
        }
    }

    fn to_json(&self, value: &Value) -> Result<Value, EvalError> {
        let json_value = to_json_value(value);
        let rendered = serde_json::to_string_pretty(&json_value)
            .map_err(|err| EvalError::new(err.to_string()))?;
        Ok(Value::String(rendered))
    }

    fn decode_json(&self, value: &Value) -> Result<Value, EvalError> {
        let text = self.coerce_to_string(value);
        let parsed: JsonValue =
            serde_json::from_str(&text).map_err(|err| EvalError::new(err.to_string()))?;
        Ok(from_json_value(&parsed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatState {
    PassThrough,
    BracketOpen,
    BracketClose,
}

/// True when the tree already calls a status function, so act does not wrap it.
fn has_status_check_function(node: &ExprNode) -> bool {
    let mut found = false;
    crate::expr::ast::visit(node, &mut |node, entering| {
        if entering {
            if let ExprNode::FuncCall { callee, .. } = node {
                if matches!(
                    callee.to_lowercase().as_str(),
                    "success" | "always" | "cancelled" | "failure"
                ) {
                    found = true;
                }
            }
        }
        true
    });
    found
}

/// Normalises a value before returning it from `&&`, `||` or `.*`.
///
/// Upstream compares against a bare `0`, which in IEEE also matches `-0.0`,
/// so the check is written as a comparison rather than a pattern.
fn safe_value(value: Value) -> Value {
    if let Value::Float(f) = value {
        return if f == 0.0 { Value::Int(0) } else { Value::Float(f) };
    }
    value
}

fn is_number(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_))
}

fn to_f64(value: &Value) -> f64 {
    match value {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => f64::NAN,
    }
}

fn kind_of(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::String(_) => 2,
        Value::Int(_) => 3,
        Value::Float(_) => 4,
        Value::Array(_) => 5,
        Value::Object(_) => 6,
    }
}

fn compare_number(left: f64, right: f64, kind: CompareOpKind) -> bool {
    match kind {
        CompareOpKind::Less => left < right,
        CompareOpKind::LessEq => left <= right,
        CompareOpKind::Greater => left > right,
        CompareOpKind::GreaterEq => left >= right,
        CompareOpKind::Eq => left == right,
        CompareOpKind::NotEq => left != right,
    }
}

fn compare_string(left: &str, right: &str, kind: CompareOpKind) -> bool {
    match kind {
        CompareOpKind::Less => left < right,
        CompareOpKind::LessEq => left <= right,
        CompareOpKind::Greater => left > right,
        CompareOpKind::GreaterEq => left >= right,
        CompareOpKind::Eq => left == right,
        CompareOpKind::NotEq => left != right,
    }
}

/// Renders a float the way Go's `%.15G` does.
pub fn format_float_g(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Inf".to_string()
        } else {
            "+Inf".to_string()
        };
    }

    const PRECISION: usize = 15;
    let scientific = format!("{:.*e}", PRECISION - 1, value);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);

    if exponent < -4 || exponent >= PRECISION as i32 {
        let mantissa = trim_trailing_zeros(mantissa);
        format!("{mantissa}E{exponent:+03}")
    } else {
        let decimals = (PRECISION as i32 - 1 - exponent).max(0) as usize;
        trim_trailing_zeros(&format!("{value:.decimals$}"))
    }
}

fn trim_trailing_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    let trimmed = text.trim_end_matches('0');
    trimmed.trim_end_matches('.').to_string()
}

fn to_json_value(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Int(i) => json!(i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Array(items) => JsonValue::Array(items.iter().map(to_json_value).collect()),
        Value::Object(map) => JsonValue::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), to_json_value(value)))
                .collect(),
        ),
    }
}

pub fn from_json_value(value: &serde_json::Value) -> Value {
    match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(items) => Value::Array(items.iter().map(from_json_value).collect()),
        JsonValue::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), from_json_value(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> EvaluationEnvironment {
        EvaluationEnvironment {
            github: Some(Value::object([
                ("event_name".to_string(), Value::String("push".to_string())),
                (
                    "ref".to_string(),
                    Value::String("refs/heads/main".to_string()),
                ),
                (
                    "event".to_string(),
                    Value::object([(
                        "number".to_string(),
                        Value::String("42".to_string()),
                    )]),
                ),
            ])),
            env: [("FOO".to_string(), Value::String("bar".to_string()))]
                .into_iter()
                .collect(),
            matrix: [
                ("os".to_string(), Value::String("linux".to_string())),
                ("node".to_string(), Value::Int(20)),
            ]
            .into_iter()
            .collect(),
            steps: [(
                "setup".to_string(),
                Value::object([(
                    "outputs".to_string(),
                    Value::object([(
                        "id".to_string(),
                        Value::String("abc".to_string()),
                    )]),
                )]),
            )]
            .into_iter()
            .collect(),
            ..EvaluationEnvironment::default()
        }
    }

    fn eval(source: &str) -> Value {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        interpreter
            .evaluate(source, DefaultStatusCheck::None)
            .unwrap_or_else(|err| panic!("{source} must evaluate: {err}"))
    }

    fn eval_step(source: &str) -> Value {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        interpreter
            .evaluate(source, DefaultStatusCheck::Success)
            .unwrap_or_else(|err| panic!("{source} must evaluate: {err}"))
    }

    #[test]
    fn literals_evaluate() {
        assert_eq!(eval("'hello' }}"), Value::String("hello".to_string()));
        assert_eq!(eval("42 }}"), Value::Int(42));
        assert_eq!(eval("1.5 }}"), Value::Float(1.5));
        assert_eq!(eval("true }}"), Value::Bool(true));
        assert_eq!(eval("null }}"), Value::Null);
    }

    #[test]
    fn contexts_resolve() {
        assert_eq!(
            eval("github.event_name }}"),
            Value::String("push".to_string())
        );
        assert_eq!(eval("env.FOO }}"), Value::String("bar".to_string()));
        assert_eq!(eval("matrix.node }}"), Value::Int(20));
    }

    #[test]
    fn unknown_context_is_an_error() {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        let err = interpreter
            .evaluate("nope }}", DefaultStatusCheck::None)
            .expect_err("unknown context must fail");
        assert_eq!(err.message, "Unavailable context: nope");
    }

    #[test]
    fn context_lookups_are_case_insensitive() {
        assert_eq!(
            eval("GitHub.Event_Name }}"),
            Value::String("push".to_string())
        );
    }

    #[test]
    fn comparisons_work() {
        assert_eq!(eval("1 == 1 }}"), Value::Bool(true));
        assert_eq!(eval("1 != 2 }}"), Value::Bool(true));
        assert_eq!(eval("1 < 2 }}"), Value::Bool(true));
        assert_eq!(eval("'a' < 'b' }}"), Value::Bool(true));
    }

    #[test]
    fn string_comparison_folds_case() {
        assert_eq!(eval("'ABC' == 'abc' }}"), Value::Bool(true));
    }

    #[test]
    fn string_operands_are_re_evaluated_as_numbers() {
        // act coerces a non-empty string by evaluating it as an expression.
        assert_eq!(eval("'1' == 1 }}"), Value::Bool(true));
        assert_eq!(eval("'2' > 1 }}"), Value::Bool(true));
    }

    #[test]
    fn empty_string_coerces_to_zero() {
        assert_eq!(eval("'' == 0 }}"), Value::Bool(true));
    }

    #[test]
    fn non_numeric_string_comparison_yields_nan() {
        // `NaN == NaN` is false in Go, so this must be false, not true.
        assert_eq!(eval("'abc' == 'abc' }}"), Value::Bool(true));
        assert_eq!(eval("'abc' > 1 }}"), Value::Bool(false));
    }

    #[test]
    fn null_equals_null() {
        assert_eq!(eval("null == null }}"), Value::Bool(true));
    }

    #[test]
    fn truthiness_follows_the_rules() {
        assert!(!is_truthy(&Value::Null));
        assert!(!is_truthy(&Value::String(String::new())));
        assert!(is_truthy(&Value::String("x".to_string())));
        assert!(!is_truthy(&Value::Int(0)));
        assert!(is_truthy(&Value::Int(1)));
        assert!(!is_truthy(&Value::Float(f64::NAN)));
        assert!(!is_truthy(&Value::Float(0.0)));
        assert!(is_truthy(&Value::Array(Vec::new())));
        assert!(is_truthy(&Value::Object(BTreeMap::new())));
    }

    #[test]
    fn negation_uses_truthiness() {
        assert_eq!(eval("!'' }}"), Value::Bool(true));
        assert_eq!(eval("!'x' }}"), Value::Bool(false));
    }

    #[test]
    fn logical_operators_short_circuit() {
        assert_eq!(eval("true && 'x' }}"), Value::String("x".to_string()));
        assert_eq!(eval("false && 'x' }}"), Value::Bool(false));
        assert_eq!(eval("false || 'x' }}"), Value::String("x".to_string()));
        assert_eq!(eval("true || 'x' }}"), Value::Bool(true));
    }

    #[test]
    fn zero_floats_are_normalised_to_integer_zero() {
        // getSafeValue turns a 0.0 into an int, which is observable here.
        assert_eq!(eval("false || 0.0 }}"), Value::Int(0));
        assert_eq!(safe_value(Value::Float(-0.0)), Value::Int(0));
        assert_eq!(safe_value(Value::Float(1.5)), Value::Float(1.5));
        assert_eq!(safe_value(Value::Null), Value::Null);
    }

    #[test]
    fn index_access_on_arrays_and_objects() {
        assert_eq!(
            eval("fromJson('[1,2,3]')[1] }}"),
            Value::Int(2),
            "indexing works on parsed arrays"
        );
        assert_eq!(
            eval("steps.setup.outputs.id }}"),
            Value::String("abc".to_string())
        );
    }

    #[test]
    fn out_of_range_index_is_null() {
        assert_eq!(eval("fromJson('[1]')[9] }}"), Value::Null);
    }

    #[test]
    fn contains_folds_case() {
        assert_eq!(eval("contains('Hello World', 'WORLD') }}"), Value::Bool(true));
        assert_eq!(eval("contains('Hello', 'xyz') }}"), Value::Bool(false));
    }

    #[test]
    fn contains_on_arrays_compares_values() {
        assert_eq!(
            eval("contains(fromJson('[\"a\",\"b\"]'), 'b') }}"),
            Value::Bool(true)
        );
        assert_eq!(
            eval("contains(fromJson('[\"a\",\"b\"]'), 'c') }}"),
            Value::Bool(false)
        );
    }

    #[test]
    fn starts_with_and_ends_with_fold_case() {
        assert_eq!(eval("startsWith('Hello', 'HE') }}"), Value::Bool(true));
        assert_eq!(eval("endsWith('Hello', 'LO') }}"), Value::Bool(true));
        assert_eq!(eval("startsWith('Hello', 'LO') }}"), Value::Bool(false));
    }

    #[test]
    fn format_substitutes_positional_arguments() {
        assert_eq!(
            eval("format('{0}-{1}', 'a', 'b') }}"),
            Value::String("a-b".to_string())
        );
    }

    #[test]
    fn format_handles_brace_escapes() {
        assert_eq!(
            eval("format('{{{0}}}', 'x') }}"),
            Value::String("{x}".to_string())
        );
    }

    #[test]
    fn format_coerces_argument_types() {
        assert_eq!(
            eval("format('{0}', true) }}"),
            Value::String("true".to_string())
        );
        assert_eq!(eval("format('{0}', 42) }}"), Value::String("42".to_string()));
    }

    #[test]
    fn format_rejects_out_of_range_index() {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        assert!(interpreter
            .evaluate("format('{5}', 'a') }}", DefaultStatusCheck::None)
            .is_err());
    }

    #[test]
    fn join_uses_comma_by_default() {
        assert_eq!(
            eval("join(fromJson('[\"a\",\"b\"]')) }}"),
            Value::String("a,b".to_string())
        );
        assert_eq!(
            eval("join(fromJson('[\"a\",\"b\"]'), '-') }}"),
            Value::String("a-b".to_string())
        );
    }

    #[test]
    fn to_json_round_trips() {
        let value = eval("toJSON(fromJson('{\"a\":1}')) }}");
        let Value::String(text) = value else {
            panic!("expected a string, got {value:?}");
        };
        assert!(text.contains("\"a\""), "got {text}");
    }

    #[test]
    fn from_json_produces_values() {
        assert_eq!(eval("fromJson('1') }}"), Value::Int(1));
        assert_eq!(eval("fromJson('\"x\"') }}"), Value::String("x".to_string()));
        assert_eq!(eval("fromJson('true') }}"), Value::Bool(true));
    }

    #[test]
    fn from_json_rejects_garbage() {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        assert!(interpreter
            .evaluate("fromJson('not json') }}", DefaultStatusCheck::None)
            .is_err());
    }

    #[test]
    fn status_functions_evaluate() {
        assert_eq!(eval("success() }}"), Value::Bool(true));
        assert_eq!(eval("always() }}"), Value::Bool(true));
        assert_eq!(eval("failure() }}"), Value::Bool(false));
        assert_eq!(eval("cancelled() }}"), Value::Bool(false));
    }

    #[test]
    fn status_functions_reject_other_contexts() {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(
            &env,
            &status,
            EvaluationContext::Other("matrix".to_string()),
        );
        let err = interpreter
            .evaluate("success() }}", DefaultStatusCheck::None)
            .expect_err("other context must fail");
        assert!(err.message.contains("must be one of 'job' or 'step'"));
    }

    #[test]
    fn default_status_check_wraps_bare_conditions() {
        // With DefaultStatusCheck the expression is ANDed with success().
        assert_eq!(eval_step("'x' }}"), Value::String("x".to_string()));
    }

    #[test]
    fn default_status_check_is_not_applied_twice() {
        // An explicit status function must not be wrapped again.
        assert_eq!(eval_step("always() }}"), Value::Bool(true));
    }

    #[test]
    fn empty_expression_with_status_check_becomes_success() {
        assert_eq!(eval_step(""), Value::Bool(true));
    }

    #[test]
    fn infinity_and_nan_are_available() {
        assert_eq!(eval("infinity }}"), Value::Float(f64::INFINITY));
        assert!(matches!(eval("nan }}"), Value::Float(f) if f.is_nan()));
    }

    #[test]
    fn array_deref_returns_the_receiver_unchanged() {
        // act's `.*` does not itself collect anything; it marks the receiver so
        // that a following property access fans out over the values.
        let value = eval("fromJson('{\"a\":1,\"b\":2}').* }}");
        assert_eq!(
            value,
            Value::object([
                ("a".to_string(), Value::Int(1)),
                ("b".to_string(), Value::Int(2)),
            ])
        );
    }

    #[test]
    fn property_access_after_array_deref_fans_out() {
        assert_eq!(
            eval("fromJson('{\"x\":{\"a\":1},\"y\":{\"a\":2}}').*.a }}"),
            Value::Array(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn property_access_without_deref_does_not_fan_out() {
        // The same lookup without `.*` yields a single value.
        assert_eq!(
            eval("fromJson('{\"x\":{\"a\":1}}').x.a }}"),
            Value::Int(1)
        );
    }

    #[test]
    fn go_float_formatting() {
        assert_eq!(format_float_g(3.0), "3");
        assert_eq!(format_float_g(1.5), "1.5");
        assert_eq!(format_float_g(0.0), "0");
        assert_eq!(format_float_g(f64::NAN), "NaN");
        assert_eq!(format_float_g(f64::INFINITY), "+Inf");
    }

    #[test]
    fn unknown_function_is_an_error() {
        let env = env();
        let status = DefaultStatus;
        let interpreter = Interpreter::new(&env, &status, EvaluationContext::Step);
        let err = interpreter
            .evaluate("nope() }}", DefaultStatusCheck::None)
            .expect_err("unknown function must fail");
        assert!(err.message.contains("not implemented"), "got {}", err.message);
    }

    #[test]
    fn has_status_check_function_detects_all_four() {
        for source in ["success() }}", "always() }}", "cancelled() }}", "failure() }}"] {
            let mut text = source.to_string();
            text.push_str("}}");
            let node = parser::parse(&text).expect("parses");
            assert!(has_status_check_function(&node), "{source}");
        }
        let node = parser::parse("a }}").expect("parses");
        assert!(!has_status_check_function(&node));
    }
}
