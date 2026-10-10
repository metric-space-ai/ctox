//! What the runner needs that is not the runner: the pipeline algebra, the
//! cancellation policy, the job's log plumbing, the token a job authenticates
//! with, and a few file helpers.
//!
//! Port of act's `pkg/common`, 899 lines without tests. It is the least
//! self-contained package in act — a dozen small files that everything else
//! imports — and it is where the decisions that shape a *run* live rather than
//! a single component.
//!
//! | act file | module | notes |
//! |---|---|---|
//! | `executor.go` | [`executor`] | the combinators the whole runner is written in |
//! | `context.go`, `job_error.go`, `dryrun.go`, `logger.go` | [`context`] | Go's ambient context becomes [`RunContext`] |
//! | `auth.go` | [`auth`] | the `ACTIONS_RUNTIME_TOKEN` JWT |
//! | `line_writer.go` | [`line_writer`] | stdout chunks to log lines |
//! | `file.go` | [`file`] | workdir staging |
//! | `outbound_ip.go` | [`outbound_ip`] | the address a job is told to call back on |
//! | `draw.go` | [`draw`] | the `--graph` box drawing |
//! | `cartesian.go` | — | already in [`crate::model`], where the matrix is expanded |
//!
//! # Three upstream quirks kept on purpose
//!
//! * **`CopyFile` never sets the destination's mode** — the guard is inverted
//!   and the error is shadowed. See [`file`].
//! * **`on_error` joins its two errors as text**, and `finally` folds the
//!   original into its message, so neither result can be matched with
//!   `errors.Is` afterwards. See [`executor`].
//! * **`GetOutboundIP` refuses a single candidate.** See [`outbound_ip`].
//!
//! # What is not here
//!
//! `common`'s `ShellExecutor` and `ExecState` — the part that actually runs a
//! step's command — need a PTY on Windows, so they land with `container` and
//! the runner together rather than arriving as a shell that cannot work on
//! one of the three target platforms.

pub mod auth;
pub mod context;
pub mod draw;
pub mod executor;
pub mod file;
pub mod git;
pub mod line_writer;
pub mod outbound_ip;

pub use auth::{
    create_authorization_token, parse_authorization_token, verify, CachePermission, CacheScope,
    Claims, TOKEN_LIFETIME_SECONDS,
};
pub use context::{
    canceled, early_cancel, Cancellation, CollectingSink, DefaultSignalSource, Level, LogSink,
    RunContext, Scope, Signal, SignalSource,
};
pub use executor::{
    debug_executor, error_executor, finally, if_bool, if_not, if_then, info_executor, is_warning,
    on_error, parallel_executor, pipeline, then, then_error, warning, Conditional, Executor,
    Warning,
};
pub use file::{copy_dir, copy_file};
pub use git::{find_git_ref, find_git_revision, find_git_slug, find_github_repo, Slug};
pub use line_writer::{LineHandler, LineWriter};
pub use outbound_ip::{outbound_ip, OutboundIpError};
