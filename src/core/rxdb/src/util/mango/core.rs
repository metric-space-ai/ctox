//! Port of `mingo/src/core.ts` — minimal subset (Options, Context, operator
//! registration helpers) required by rx-query-mingo.ts.
//!
//! Pipeline/projection/accumulator/window operator groups are stubbed out as
//! `OperatorType` variants but unused; the only group rx-query-mingo
//! exercises at runtime is `Query`, plus `Pipeline` for `$sort` / `$project`.
//! Helpers like `computeValue` / `redact` from upstream are intentionally not
//! ported: rx-query-mingo never reaches expression evaluation since it does
//! not register expression operators.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

// ref: mingo/src/core.ts:50-59
/// Specifies how input and output documents are processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    /// Do not clone inputs or outputs. Resulting documents may share references.
    CloneOff = 0,
    /// Clone input documents to maintain immutability of original input.
    CloneInput = 1,
    /// Clone output documents to ensure distinct objects without shared references.
    CloneOutput = 2,
    /// Clone input and output documents.
    CloneAll = 3,
}

// ref: mingo/src/core.ts:236-243
/// The different groups of operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatorType {
    Accumulator,
    Expression,
    Pipeline,
    Projection,
    Query,
    Window,
}

impl OperatorType {
    /// Mirrors upstream string discriminator used by `useOperators(type, …)`.
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accumulator => "accumulator",
            Self::Expression => "expression",
            Self::Pipeline => "pipeline",
            Self::Projection => "projection",
            Self::Query => "query",
            Self::Window => "window",
        }
    }
}

// ref: mingo/src/core.ts:64-87
/// Generic options interface passed down to all operators.
///
/// Only the fields actually consulted by the ported operators (query+pipeline
/// subset) are kept. `collation`, `hashFunction`, `variables`, etc. have no
/// effect inside this subset and are omitted rather than carried as dead
/// fields.
#[derive(Debug, Clone)]
pub struct Options {
    // ref: mingo/src/core.ts:66
    /// The key that is used to lookup the ID value of a document. Default `_id`.
    pub id_key: String,
    // ref: mingo/src/core.ts:70
    /// Determines how to treat inputs and outputs. Default `CloneOff`.
    pub processing_mode: ProcessingMode,
    // ref: mingo/src/core.ts:72
    /// Enforces strict MongoDB compatibility. Default `true`.
    pub use_strict_mode: bool,
    // ref: mingo/src/core.ts:74
    /// Enable or disable custom script execution. Default `true`.
    pub script_enabled: bool,
    // ref: mingo/src/core.ts:76
    /// Enable or disable falling back to the global context for operators.
    /// Default `true`.
    pub use_global_context: bool,
    // ref: mingo/src/core.ts:86
    /// Extra references to operators to be used for processing.
    pub context: Context,
    // ref: rxdb-rs new code — selector traversal depth, replaces
    // `PredicateOptions.depth` which upstream layers on top of `Options`.
    pub depth: usize,
}

impl Options {
    // ref: mingo/src/core.ts:202-216
    /// Creates an Option from another where required keys are initialized.
    pub fn init(context: Context) -> Self {
        Self {
            id_key: "_id".to_string(),
            script_enabled: true,
            use_strict_mode: true,
            use_global_context: true,
            processing_mode: ProcessingMode::CloneOff,
            context,
            depth: 0,
        }
    }

    /// Returns a new `Options` with the same fields but `depth` overridden.
    pub fn with_depth(&self, depth: usize) -> Self {
        let mut next = self.clone();
        next.depth = depth;
        next
    }
}

/// Signature of a query operator after factory invocation.
///
/// Upstream `QueryOperator = (selector, value, options) => (obj) => boolean`.
/// In Rust we represent the closed-over predicate as a boxed `Fn` returning a
/// `bool` for a borrowed JSON value.
// ref: mingo/src/core.ts:270-274
pub type QueryPredicate = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

/// Query operator factory: `(selector, value, options) -> QueryPredicate`.
// ref: mingo/src/core.ts:270-274
pub type QueryOperatorFn = fn(selector: &str, value: &Value, options: &Options) -> QueryPredicate;

/// Pipeline operator: `(collection, expr, options) -> Vec<Value>`.
// ref: mingo/src/core.ts:257-261
pub type PipelineOperatorFn =
    fn(collection: Vec<Value>, expr: &Value, options: &Options) -> Vec<Value>;

/// Untyped operator handle stored in the `Context`.
///
/// We only need `Query` and `Pipeline` for the rx-query-mingo subset, but the
/// enum mirrors upstream `Operator = AccumulatorOperator | ExpressionOperator
/// | PipelineOperator | ProjectionOperator | QueryOperator | WindowOperator`.
// ref: mingo/src/core.ts:291-297
#[derive(Clone)]
pub enum Operator {
    Query(QueryOperatorFn),
    Pipeline(PipelineOperatorFn),
}

// ref: mingo/src/core.ts:315-372
/// Operator registry keyed by `OperatorType` then by operator name.
#[derive(Debug, Clone, Default)]
pub struct Context {
    operators: HashMap<OperatorType, HashMap<String, Operator>>,
}

impl std::fmt::Debug for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query(_) => f.write_str("Operator::Query(<fn>)"),
            Self::Pipeline(_) => f.write_str("Operator::Pipeline(<fn>)"),
        }
    }
}

impl Context {
    // ref: mingo/src/core.ts:320-322
    pub fn init() -> Self {
        Self::default()
    }

    // ref: mingo/src/core.ts:331-342
    fn add_operators(&mut self, ty: OperatorType, ops: Vec<(&str, Operator)>) -> &mut Self {
        let map = self.operators.entry(ty).or_default();
        for (name, fn_) in ops {
            // upstream guard: do not overwrite an existing registration.
            map.entry(name.to_string()).or_insert(fn_);
        }
        self
    }

    // ref: mingo/src/core.ts:344-347
    pub fn get_operator(&self, ty: OperatorType, name: &str) -> Option<Operator> {
        self.operators.get(&ty).and_then(|m| m.get(name)).cloned()
    }

    // ref: mingo/src/core.ts:357-359
    pub fn add_query_ops(&mut self, ops: Vec<(&str, QueryOperatorFn)>) -> &mut Self {
        self.add_operators(
            OperatorType::Query,
            ops.into_iter()
                .map(|(n, f)| (n, Operator::Query(f)))
                .collect(),
        )
    }

    // ref: mingo/src/core.ts:361-363
    pub fn add_pipeline_ops(&mut self, ops: Vec<(&str, PipelineOperatorFn)>) -> &mut Self {
        self.add_operators(
            OperatorType::Pipeline,
            ops.into_iter()
                .map(|(n, f)| (n, Operator::Pipeline(f)))
                .collect(),
        )
    }
}

// ref: mingo/src/core.ts:383-419
/// Register operators on the context.
///
/// In upstream this writes to a process-global `GLOBAL_CONTEXT`. The Rust port
/// keeps the same signature (a free function that mutates a `Context`) but
/// requires the caller to pass the context explicitly — there is no global
/// state inside the crate.
pub fn use_operators_query(ctx: &mut Context, ops: Vec<(&str, QueryOperatorFn)>) {
    ctx.add_query_ops(ops);
}

// ref: mingo/src/core.ts:383-419
pub fn use_operators_pipeline(ctx: &mut Context, ops: Vec<(&str, PipelineOperatorFn)>) {
    ctx.add_pipeline_ops(ops);
}

// ref: mingo/src/core.ts:427-435
/// Returns the operator function or `None` if it is not found.
pub fn get_operator(ctx: &Context, ty: OperatorType, name: &str) -> Option<Operator> {
    ctx.get_operator(ty, name)
}
