//! Workjet Actions' in-tree Rust port of nektos/act (MIT).
//!
//! The default build exposes the native host backend, workflow/model and
//! expression APIs, file commands, artifact/cache protocols and pure parsers.
//! Docker execution is preserved behind the maintenance-only `docker` feature;
//! CTOX depends on this crate with that feature disabled.
//!
//! This is an engine library, not an admitted Workjet Actions service. Native
//! execution must be called only after the service has acquired its resource,
//! storage and source leases and installed OS limits (subsequent slices).
//! Workflow scheduling, action loading and complete step orchestration are not
//! implemented by this port. See README.md for the measured API inventory and
//! the distinction between parsing, preparation and end-to-end execution.
//!
//! Upstream-derived source comments describe act behavior and port provenance;
//! they are not claims that an entire GitHub Actions workflow runs unchanged.
pub mod host;

pub mod artifactcache;
pub mod artifacts;
pub mod base64url;
pub mod common;
pub mod container;
pub mod expr;
pub mod filecollector;
pub mod git_index;
pub mod gitignore;
pub mod gomatch;
pub mod gopath;
pub mod gostrconv;
pub mod http;
pub mod lookpath;
pub mod model;
pub mod runner;
pub mod validate;
pub mod schema;
pub mod workflow_pattern;
pub mod yaml_node;

pub use artifactcache::{Cache, Handler as ArtifactCacheHandler, Service as ArtifactCacheService};
pub use artifacts::{safe_resolve, Server as ArtifactServer, Service as ArtifactService};
pub use common::{RunContext, Scope, Warning};
pub use filecollector::{
    Cancellation, CopyCollector, DefaultFs, FileCollector, FileInfo, FileMode, Fs, Handler,
    TarCollector, WalkOutcome,
};
pub use gitignore::{IgnoreResult, Matcher, Pattern};
pub use lookpath::{look_path, look_path_in, Env, LookPathError, ProcessEnv};
pub use schema::{action_schema, workflow_schema, Definition, Schema, SchemaNode};
pub use workflow_pattern::{
    compile_pattern, compile_patterns, filter, skip, EmptyTraceWriter, StdOutTraceWriter,
    TraceWriter, WorkflowPattern,
};
