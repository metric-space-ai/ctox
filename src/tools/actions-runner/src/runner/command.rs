//! The workflow commands: `::set-output::`, `##[add-path]`, and their cousins.
//!
//! An action cannot set an output or extend `PATH` by writing YAML — it writes
//! a line of a reserved shape to stdout and the runner notices. This module is
//! what notices.
//!
//! # Two grammars
//!
//! | shape | pattern | key/value separator |
//! |---|---|---|
//! | GitHub Actions | `^::([^ ]+)( (.+))?::([^\r\n]*)[\r\n]+$` | `,` |
//! | Azure DevOps | `^##\[([^ ]+)( (.+))?]([^\r\n]*)[\r\n]+$` | `;` |
//!
//! The GA one is tried first, and the two rules that decide real behaviour are
//! both about how `([^ ]+)` and the optional property group compete.
//!
//! ## A command name cannot contain a space, so the split point is a backtrack
//!
//! `([^ ]+)` is greedy: it takes everything up to the next space, and if what
//! follows cannot be completed the engine gives characters back. Where it gives
//! them back decides whether the line is a command with properties or one long
//! command name. Measured against Go's own `regexp` on the real patterns:
//!
//! | line | command | properties | data |
//! |---|---|---|---|
//! | `::set-output::name=x::value` | `set-output::name=x` | *(none)* | `value` |
//! | `::set-output name=x::value` | `set-output` | `name=x` | `value` |
//! | `::set-output:: token=secret` | `set-output` | *(none)* | `" token=secret"` |
//!
//! The first row is the one that looks like a bug and is not: with no space
//! anywhere on the line, the greedy group runs to the last `::` it can close on,
//! and the *whole* of `set-output::name=x` is the command name. So the unspaced
//! spelling carries no properties at all.
//!
//! The third row is the one to watch, because it is upstream's own
//! `TestAddmaskUsemask` line and the result surprises people: the space makes the
//! greedy group stop at `set-output::`, the property group then cannot close on
//! anything, and the engine backtracks the command group to `set-output` so the
//! `::` matches. The properties come out **empty** and the data is
//! `" token=secret"` — leading space included. The output is therefore written
//! under the empty name `""`, which is what makes that test's expected log line
//! read `::set-output:: = token=***`.
//!
//! ## A line without a trailing newline is not a command
//!
//! Both patterns end in `[\r\n]+`, so the last line of a step's output is not a
//! command. A step writing `::debug::x` with no trailing newline logs nothing.
//! That is upstream, and a port that "fixes" it changes behaviour.
//!
//! # `unescapeCommandData` has no defined output for a double escape
//!
//! Upstream unescapes by ranging over a Go **map**:
//!
//! ```go
//! escapeMap := map[string]string{"%25": "%", "%0D": "\r", "%0A": "\n"}
//! for k, v := range escapeMap { arg = strings.ReplaceAll(arg, k, v) }
//! ```
//!
//! Go randomises map iteration and the three replacements are **not**
//! order-independent. Replacing `%25` first turns `%250A` into `%0A`, which the
//! next pass then turns into a newline; replacing `%0A` first never sees it and
//! yields the literal `%0A`. Measured over 400 runs of the real function:
//! `"\n"` 294 times, `"%0A"` 106 times.
//!
//! So for a double-escaped sequence **upstream has no single answer** and no
//! port can be faithful to one. This implementation applies the escapes in the
//! order the map literal is written — `%25`, `%0D`, `%0A` — which is one of the
//! answers upstream produces and the only one that can be stated. A test pins
//! the choice and says what it is. Single escapes and escapes that do not chain
//! (`percent2%25%0Atest`, which every ordering reduces to `percent2%\ntest`) are
//! order-independent and are simply upstream's.
//!
//! # State, not the whole `RunContext`
//!
//! The commands touch a small, closed set of fields, so this module owns a
//! [`CommandContext`] with exactly those and the `RunContext` (ported later)
//! will hold one. Modelling it this way is what makes the module testable at
//! all: upstream's tests build a `RunContext` and a container to run commands
//! *in*, so they need a Docker daemon. Here the same ten test functions run
//! against four maps and a vector.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::common::context::Level;
use crate::common::LogSink;
use crate::model::StepResult;

/// `U+2699` — marks a command the runner acted on.
const MARK_HANDLED: char = '\u{2699}';
/// `U+1F4AC` speech balloon, in front of a `debug` line.
const MARK_DEBUG: char = '\u{1f4ac}';
/// `U+1F6A7` construction sign, in front of a `warning` line.
const MARK_WARNING: char = '\u{1f6a7}';
/// `U+2757` heavy exclamation, in front of an `error` line, and in front of the
/// "no outputs used step" message.
const MARK_ERROR: char = '\u{2757}';
/// `U+1F4BE` floppy disk, in front of a `save-state` line.
///
/// Upstream writes this escape as `\U0001f4be` with a **lowercase** `f4be`,
/// unlike every other code point in the file. Same character; noted because a
/// transcription that "tidied" the case would look right and be a different
/// byte sequence in the source.
const MARK_SAVE_STATE: char = '\u{1f4be}';
/// `U+2753` white four-pointed star, in front of anything else.
const MARK_OTHER: char = '\u{2753}';

/// The refusal `set-env` and `add-path` log without the opt-in.
const UNSECURE_COMMANDS_REFUSED: &str = "Please upgrade to using Environment Files or opt into unsafe commands by setting the `ACTIONS_ALLOW_UNSECURE_COMMANDS` environment variable to `true`.";

/// What a `set-output` did, so the handler can log it.
///
/// The distinction that matters is that upstream logs the **resolved** step and
/// name, not the ones the command was written with: an output mapping redirects
/// both, and the log line follows the redirection. Keeping the outcome in the
/// data rather than a log string makes that checkable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOutputOutcome {
    /// Written.
    Written {
        /// The step it landed on, after any output mapping.
        step_id: String,
        /// The output name it landed under, after any output mapping.
        output_name: String,
    },
    /// The step has no result to write into, so nothing was written and
    /// nothing was logged as a success.
    NoResult {
        /// The step that was looked up — again the resolved one.
        step_id: String,
    },
}

/// A parsed workflow-command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The command name, e.g. `set-output`.
    pub command: String,
    /// The `name=x` pairs, empty for a command written without them.
    pub kv_pairs: BTreeMap<String, String>,
    /// Everything after the closing `::`, unescaped.
    pub arg: String,
}

/// The state the workflow commands read and write.
///
/// This is the intersection of the `RunContext` fields `command.go` touches. It
/// is its own type so that the ten upstream test functions can run without a
/// `RunContext` and, upstream, without a Docker daemon.
#[derive(Debug, Clone, Default)]
pub struct CommandContext {
    /// The job's environment.
    pub env: BTreeMap<String, String>,
    /// The environment as the *next* step will see it. `set-env` writes both
    /// this and `env`; that they are separate is what lets a later step tell
    /// them apart.
    pub global_env: BTreeMap<String, String>,
    /// The extra `PATH` entries, most recently added first.
    pub extra_path: Vec<String>,
    /// `save-state` values, per step id.
    pub intra_action_state: BTreeMap<String, BTreeMap<String, String>>,
    /// The step currently running.
    pub current_step: String,
    /// The outputs of every step that has produced some.
    pub step_results: BTreeMap<String, StepResult>,
    /// Renames for outputs, so that a `set-output` in a reusable workflow lands
    /// on the step that consumed it. Keyed by `(step, output)`.
    pub output_mappings: BTreeMap<(String, String), (String, String)>,
    /// Whether the container's environment keys are case-insensitive, which
    /// decides how `set-env` merges. This is the Windows container case.
    pub environment_case_insensitive: bool,
    /// The values `add-mask` was given, to be redacted from the log.
    pub masks: Vec<String>,
}

impl CommandContext {
    /// Whether the unsafe commands have been opted into.
    fn allows_unsecure_commands(&self) -> bool {
        self.env
            .get("ACTIONS_ALLOW_UNSECURE_COMMANDS")
            .map(String::as_str)
            == Some("true")
    }

    /// `mergeIntoMapCaseInsensitive` from `step.go`, for one entry.
    ///
    /// Upstream folds keys with `strings.ToLower`, which is Unicode-aware, so
    /// this uses `to_lowercase` rather than an ASCII-only comparison: on a
    /// Windows container `İ` and `i̇` are the same variable, and an ASCII fold
    /// would file them as two.
    fn merge_case_insensitive(target: &mut BTreeMap<String, String>, name: &str, value: &str) {
        let folded = name.to_lowercase();
        let existing = target
            .keys()
            .find(|key| key.to_lowercase() == folded)
            .cloned();
        target.insert(
            existing.unwrap_or_else(|| name.to_string()),
            value.to_string(),
        );
    }

    /// `set-env`: write one variable into both the job and the global map.
    fn set_env(&mut self, name: &str, arg: &str) {
        let merge = |target: &mut BTreeMap<String, String>| {
            if self.environment_case_insensitive {
                Self::merge_case_insensitive(target, name, arg);
            } else {
                target.insert(name.to_string(), arg.to_string());
            }
        };
        merge(&mut self.env);
        merge(&mut self.global_env);
    }

    /// `setOutput`: record an output, and report which step and name it landed
    /// on so the handler can log *those* rather than the ones written.
    fn set_output(&mut self, kv_pairs: &BTreeMap<String, String>, arg: &str) -> SetOutputOutcome {
        let step_id = self.current_step.clone();
        let output_name = kv_pairs.get("name").cloned().unwrap_or_default();
        let mapping = self
            .output_mappings
            .get(&(step_id.clone(), output_name.clone()))
            .cloned();
        let (step_id, output_name) = match mapping {
            Some((mapped_step, mapped_name)) => (mapped_step, mapped_name),
            None => (step_id, output_name),
        };

        let Some(result) = self.step_results.get_mut(&step_id) else {
            return SetOutputOutcome::NoResult { step_id };
        };
        result.outputs.insert(output_name.clone(), arg.to_string());
        SetOutputOutcome::Written {
            step_id,
            output_name,
        }
    }

    /// `addPath`: put `arg` at the front, dropping any earlier copy of it.
    fn add_path(&mut self, arg: &str) {
        let mut extra_path = vec![arg.to_string()];
        for value in &self.extra_path {
            if value != arg {
                extra_path.push(value.clone());
            }
        }
        self.extra_path = extra_path;
    }

    /// `saveState`: a value for this step's `post` phase, and only while a step
    /// is running — outside one there is nothing to attach it to, and upstream
    /// drops it silently.
    fn save_state(&mut self, kv_pairs: &BTreeMap<String, String>, arg: &str) {
        if self.current_step.is_empty() {
            return;
        }
        let name = kv_pairs.get("name").cloned().unwrap_or_default();
        self.intra_action_state
            .entry(self.current_step.clone())
            .or_default()
            .insert(name, arg.to_string());
    }
}

/// The line handler, as a closure that owns its own `resumeCommand`.
///
/// The `resumeCommand` is what makes `stop-commands` work: after it, every
/// other command is ignored and logged as such until the named command comes
/// back. It is per-handler state, so it lives in the closure — a test that
/// built a fresh handler per line would never see a suspension.
///
/// The returned `bool` answers upstream's question: `true` means *pass this line
/// on to the log*, `false` means *it was a command and has been consumed*.
pub fn command_handler<'a>(
    context: &'a mut CommandContext,
    sink: Arc<dyn LogSink>,
) -> impl FnMut(&str) -> bool + 'a {
    let mut resume_command = String::new();
    move |line: &str| {
        let Some(command) = try_parse_raw_action_command(line) else {
            // Not a command: an ordinary line of output, pass it through.
            return true;
        };

        if !resume_command.is_empty() && command.command != resume_command {
            sink.log(Level::Info, &format!("  {MARK_HANDLED}  {line}"));
            return false;
        }

        let arg = unescape_command_data(&command.arg);
        let kv_pairs = unescape_kv_pairs(&command.kv_pairs);

        match command.command.as_str() {
            "set-env" => {
                if !context.allows_unsecure_commands() {
                    sink.log(
                        Level::Error,
                        &format!("The `set-env` command is disabled. {UNSECURE_COMMANDS_REFUSED}"),
                    );
                } else {
                    let name = kv_pairs.get("name").cloned().unwrap_or_default();
                    sink.log(
                        Level::Info,
                        &format!("  {MARK_HANDLED}  ::set-env:: {name}={arg}"),
                    );
                    context.set_env(&name, &arg);
                }
            }
            "set-output" => match context.set_output(&kv_pairs, &arg) {
                SetOutputOutcome::Written {
                    step_id: _,
                    output_name,
                } => sink.log(
                    Level::Info,
                    &format!("  {MARK_HANDLED}  ::set-output:: {output_name}={arg}"),
                ),
                SetOutputOutcome::NoResult { step_id } => {
                    sink.log(
                        Level::Info,
                        &format!("  {MARK_ERROR}  no outputs used step '{step_id}'"),
                    );
                }
            },
            "add-path" => {
                if !context.allows_unsecure_commands() {
                    sink.log(
                        Level::Error,
                        &format!("The `add-path` command is disabled. {UNSECURE_COMMANDS_REFUSED}"),
                    );
                } else {
                    sink.log(
                        Level::Info,
                        &format!("  {MARK_HANDLED}  ::add-path:: {arg}"),
                    );
                    context.add_path(&arg);
                }
            }
            "debug" => sink.log(Level::Debug, &format!("  {MARK_DEBUG}  {line}")),
            "warning" => sink.log(Level::Warn, &format!("  {MARK_WARNING}  {line}")),
            "error" => sink.log(Level::Error, &format!("  {MARK_ERROR}  {line}")),
            "add-mask" => {
                // A plain append, as `AddMask` is. The value is deliberately not
                // echoed: the log line says `***` and the substitution happens
                // later, in the job logger.
                context.masks.push(arg);
                sink.log(Level::Info, &format!("  {MARK_HANDLED}  ***"));
            }
            "stop-commands" => {
                resume_command = arg;
                sink.log(Level::Info, &format!("  {MARK_HANDLED}  {line}"));
            }
            // Upstream spells this `case resumeCommand:` — a *variable* case
            // sitting between `stop-commands` and `save-state`. The position is
            // load-bearing: a `::save-state::` line arriving while suspended on
            // `save-state` resumes here and does **not** save state, whereas an
            // arm placed after `save-state` would do both. It is also after the
            // literal arms, so a suspension on `set-env` can never be ended by a
            // `set-env` line — the literal arm takes it first.
            _ if !resume_command.is_empty() && command.command == resume_command => {
                resume_command.clear();
                sink.log(Level::Info, &format!("  {MARK_HANDLED}  {line}"));
            }
            "save-state" => {
                sink.log(Level::Info, &format!("  {MARK_SAVE_STATE}  {line}"));
                context.save_state(&kv_pairs, &arg);
            }
            // The one mark with a *single* space after it upstream.
            "add-matcher" => sink.log(Level::Info, &format!("  {MARK_OTHER} add-matcher {arg}")),
            _ => sink.log(Level::Info, &format!("  {MARK_OTHER}  {line}")),
        }

        // A recognised command is *consumed*: it does not reach the step's
        // output. That is what `false` means.
        false
    }
}

/// The GitHub Actions form: `::name key=value::data`.
fn command_pattern_ga() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(r"^::([^ ]+)( (.+))?::([^\r\n]*)[\r\n]+$")
            .expect("the GA workflow-command pattern is a constant")
    })
}

/// The Azure DevOps form: `##[name key=value]data`.
fn command_pattern_ado() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(r"^##\[([^ ]+)( (.+))?]([^\r\n]*)[\r\n]+$")
            .expect("the ADO workflow-command pattern is a constant")
    })
}

/// One capture group, or `""` when the group did not take part in the match.
///
/// This indirection is not cosmetic. Go's `FindStringSubmatch` fills a
/// non-participating group with `""`, and both patterns here contain
/// `( (.+))?` with a nested `(.+)` — so for any command written **without** a
/// space, group 3 does not participate at all. `::add-path::/zoo` is exactly
/// that case. Go reads it as `""`; Rust's `captures[3]` panics, because
/// `Captures::get` returns `None` and the `Index` impl treats "did not
/// participate" the same as "no such group".
pub fn group<'a>(captures: &'a regex::Captures<'a>, index: usize) -> &'a str {
    captures.get(index).map_or("", |m| m.as_str())
}

/// `tryParseRawActionCommand`: recognise a workflow command in one line.
///
/// The GA form is tried first. Both patterns are written out byte for byte as
/// upstream has them, and the capture groups are read the same way, because the
/// *way* the engine resolves the greedy command group against the optional
/// property group is what the module doc tabulates. Rust's `regex` crate uses
/// leftmost-first submatch semantics for this, which is why a test can pin
/// those results directly.
pub fn try_parse_raw_action_command(line: &str) -> Option<Command> {
    if let Some(captures) = command_pattern_ga().captures(line) {
        return Some(Command {
            command: group(&captures, 1).to_string(),
            kv_pairs: parse_key_value_pairs(group(&captures, 3), ','),
            arg: group(&captures, 4).to_string(),
        });
    }
    let captures = command_pattern_ado().captures(line)?;
    Some(Command {
        command: group(&captures, 1).to_string(),
        // The ADO form separates properties with `;`, the GA form with `,`.
        kv_pairs: parse_key_value_pairs(group(&captures, 3), ';'),
        arg: group(&captures, 4).to_string(),
    })
}

/// `parseKeyValuePairs`: `a=1,b=2`, keeping only the pairs that split into
/// **exactly** two parts.
///
/// That last part is not a simplification. Upstream splits on `=` and tests
/// `len(kv) == 2`, so `a=b=c` splits into three and is dropped entirely — not
/// kept as `a` → `b=c`. Measured: an empty input contributes nothing, `name=` is
/// kept with an empty value, and `=v` is a legitimate pair with an empty key.
pub fn parse_key_value_pairs(kv_pairs: &str, separator: char) -> BTreeMap<String, String> {
    kv_pairs
        .split(separator)
        .filter_map(|pair| {
            let parts: Vec<&str> = pair.split('=').collect();
            // Exactly two, so `a=b=c` is dropped whole.
            (parts.len() == 2).then(|| (parts[0].to_string(), parts[1].to_string()))
        })
        .collect()
}

/// `unescapeCommandData`: the three escapes in a command's **data**.
///
/// Applied in the order the Go map literal is written. See the module doc: for
/// a double-escaped sequence this is *one* of the answers upstream produces, and
/// upstream produces several.
pub fn unescape_command_data(arg: &str) -> String {
    let mut arg = arg.to_string();
    for (from, to) in [("%25", "%"), ("%0D", "\r"), ("%0A", "\n")] {
        arg = arg.replace(from, to);
    }
    arg
}

/// `unescapeCommandProperty`: the same three plus two more, for a property
/// value rather than the data.
///
/// `%3A` and `%2C` appear here and not in the data escapes because a property
/// value is itself comma-separated, so a literal comma in it has to be escaped
/// even where a literal newline in the data does not.
pub fn unescape_command_property(arg: &str) -> String {
    let mut arg = arg.to_string();
    for (from, to) in [
        ("%25", "%"),
        ("%0D", "\r"),
        ("%0A", "\n"),
        ("%3A", ":"),
        ("%2C", ","),
    ] {
        arg = arg.replace(from, to);
    }
    arg
}

/// `unescapeKvPairs`: unescape every property **value**, leaving the keys alone.
///
/// That is what makes upstream's `::set-output name=x%3A::…` write an output
/// called `x:` — the `name` property's *value* is the one unescaped.
pub fn unescape_kv_pairs(kv_pairs: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    kv_pairs
        .iter()
        .map(|(key, value)| (key.clone(), unescape_command_property(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::context::CollectingSink;

    /// A context, a sink the test reads, and the plumbing to drive the handler.
    struct Harness {
        context: CommandContext,
        sink: Arc<CollectingSink>,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                context: CommandContext::default(),
                sink: Arc::new(CollectingSink::new()),
            }
        }

        /// A context with the unsafe commands opted into, as six of the ten
        /// upstream tests set up.
        fn unsecure() -> Self {
            let mut harness = Self::new();
            harness.context.env.insert(
                "ACTIONS_ALLOW_UNSECURE_COMMANDS".to_string(),
                "true".to_string(),
            );
            harness
        }

        /// Feeds every line to **one** handler.
        ///
        /// The handler carries the `stop-commands` suspension, so a test that
        /// built a fresh one per line could not observe a suspension at all.
        /// Building it here and dropping it before returning is also what lets
        /// the caller read `context` afterwards: the handler borrows it mutably
        /// for its whole life.
        fn run_all(&mut self, lines: &[&str]) {
            let sink: Arc<dyn LogSink> = self.sink.clone();
            let mut handler = command_handler(&mut self.context, sink);
            for line in lines {
                handler(line);
            }
        }

        /// One line to a fresh handler. Returns whether the line was passed
        /// through to the log rather than consumed as a command.
        fn run(&mut self, line: &str) -> bool {
            let sink: Arc<dyn LogSink> = self.sink.clone();
            let mut handler = command_handler(&mut self.context, sink);
            handler(line)
        }

        fn messages_at(&self, level: Level) -> Vec<String> {
            self.sink.messages_at(level)
        }
    }

    /// `TestSetEnv`.
    #[test]
    fn set_env_writes_both_the_job_and_the_global_environment() {
        let mut h = Harness::unsecure();
        h.run("::set-env name=x::valz\n");
        assert_eq!(h.context.env.get("x").map(String::as_str), Some("valz"));
        assert_eq!(
            h.context.global_env.get("x").map(String::as_str),
            Some("valz")
        );
    }

    /// `TestSetEnvBlocked`.
    #[test]
    fn set_env_is_blocked_without_the_opt_in() {
        let mut h = Harness::new();
        h.run("::set-env name=x::valz\n");
        assert_eq!(h.context.env.get("x").map(String::as_str), None);
        assert_eq!(h.context.global_env.get("x").map(String::as_str), None);
    }

    /// On a case-insensitive container `set-env` merges case-insensitively, so
    /// `FOO` then `foo` is one variable and not two — the workflow would read
    /// one of them and the container would see the other.
    #[test]
    fn set_env_merges_case_insensitively_on_such_a_container() {
        let mut h = Harness::unsecure();
        h.context.environment_case_insensitive = true;
        h.run_all(&["::set-env name=FOO::one\n", "::set-env name=foo::two\n"]);
        assert_eq!(h.context.env.get("FOO").map(String::as_str), Some("two"));
        assert_eq!(h.context.env.len(), 2, "the opt-in plus exactly one FOO");
        assert_eq!(
            h.context.global_env.get("FOO").map(String::as_str),
            Some("two")
        );
    }

    /// The same two writes on a case-*sensitive* container are two variables.
    #[test]
    fn set_env_keeps_both_cases_on_a_case_sensitive_container() {
        let mut h = Harness::unsecure();
        h.run_all(&["::set-env name=FOO::one\n", "::set-env name=foo::two\n"]);
        assert_eq!(h.context.env.get("FOO").map(String::as_str), Some("one"));
        assert_eq!(h.context.env.get("foo").map(String::as_str), Some("two"));
    }

    /// `TestSetOutput`, in full: the plain write and then the whole escape
    /// battery, including the two cases where the **output name itself** carries
    /// escapes and the output has to land under the unescaped one.
    #[test]
    fn set_output_writes_the_current_steps_outputs_with_escapes() {
        let mut h = Harness::new();
        h.context.current_step = "my-step".to_string();
        h.context
            .step_results
            .insert("my-step".to_string(), StepResult::default());

        h.run("::set-output name=x::valz\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x")
                .map(String::as_str),
            Some("valz")
        );

        h.run("::set-output name=x::percent2%25\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x")
                .map(String::as_str),
            Some("percent2%")
        );

        h.run("::set-output name=x::percent2%25%0Atest\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x")
                .map(String::as_str),
            Some("percent2%\ntest")
        );

        h.run("::set-output name=x::percent2%25%0Atest another3%25test\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x")
                .map(String::as_str),
            Some("percent2%\ntest another3%test")
        );

        // The *name* property's value is unescaped too, so this writes an
        // output called `x:`.
        h.run("::set-output name=x%3A::percent2%25%0Atest\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x:")
                .map(String::as_str),
            Some("percent2%\ntest")
        );

        // All five property escapes at once, including `%25` immediately before
        // `%0D` and `%0A` — the shape that is ambiguous in the *data* escapes.
        // Here it is not, because a property value is unescaped one escape at a
        // time and every ordering reduces to the same string.
        h.run("::set-output name=x%3A%2C%0A%25%0D%3A::percent2%25%0Atest\n");
        assert_eq!(
            h.context.step_results["my-step"]
                .outputs
                .get("x:,\n%\r:")
                .map(String::as_str),
            Some("percent2%\ntest")
        );
    }

    /// A `set-output` for a step with no result writes nothing, and the message
    /// names the step that was *looked up*.
    #[test]
    fn set_output_for_a_step_with_no_result_writes_nothing() {
        let mut h = Harness::new();
        h.context.current_step = "missing".to_string();
        h.run("::set-output name=x::valz\n");
        assert!(h.context.step_results.is_empty());
        assert_eq!(
            h.messages_at(Level::Info),
            vec!["  \u{2757}  no outputs used step 'missing'".to_string()],
            "the error mark, and no success line"
        );
    }

    /// An output mapping redirects the write — and the log line follows the
    /// redirection, which is how a reusable workflow's output reaches its
    /// caller without a confusing log.
    #[test]
    fn an_output_mapping_redirects_set_output_and_its_log_line() {
        let mut h = Harness::new();
        h.context.current_step = "caller".to_string();
        h.context
            .step_results
            .insert("caller".to_string(), StepResult::default());
        h.context
            .step_results
            .insert("inner".to_string(), StepResult::default());
        h.context.output_mappings.insert(
            ("caller".to_string(), "x".to_string()),
            ("inner".to_string(), "real".to_string()),
        );
        h.run("::set-output name=x::valz\n");
        assert_eq!(
            h.context.step_results["inner"]
                .outputs
                .get("real")
                .map(String::as_str),
            Some("valz")
        );
        assert!(h.context.step_results["caller"].outputs.is_empty());
        assert_eq!(
            h.messages_at(Level::Info),
            vec!["  \u{2699}  ::set-output:: real=valz".to_string()],
            "the mapped name, not the written one"
        );
    }

    /// A mapping onto a step that has no result reports *that* step, not the one
    /// the command was written in.
    #[test]
    fn a_mapping_onto_a_missing_step_reports_the_mapped_name() {
        let mut h = Harness::new();
        h.context.current_step = "caller".to_string();
        h.context
            .step_results
            .insert("caller".to_string(), StepResult::default());
        h.context.output_mappings.insert(
            ("caller".to_string(), "x".to_string()),
            ("inner".to_string(), "real".to_string()),
        );
        h.run("::set-output name=x::valz\n");
        assert_eq!(
            h.messages_at(Level::Info),
            vec!["  \u{2757}  no outputs used step 'inner'".to_string()],
            "'inner', not the step the command was written in"
        );
    }

    /// `TestAddpath`.
    #[test]
    fn add_path_puts_the_new_entry_first() {
        let mut h = Harness::unsecure();
        h.run_all(&["::add-path::/zoo\n", "::add-path::/boo\n"]);
        assert_eq!(h.context.extra_path, vec!["/boo", "/zoo"]);
    }

    /// The same entry twice ends up once, at the front.
    #[test]
    fn add_path_deduplicates_only_the_value_being_added() {
        let mut h = Harness::unsecure();
        h.run_all(&[
            "::add-path::/one\n",
            "::add-path::/two\n",
            "::add-path::/one\n",
        ]);
        assert_eq!(h.context.extra_path, vec!["/one", "/two"]);
    }

    /// `TestAddPathBlocked`.
    #[test]
    fn add_path_is_blocked_without_the_opt_in() {
        let mut h = Harness::new();
        h.run("::add-path::/zoo\n");
        assert!(h.context.extra_path.is_empty());
    }

    /// `TestAddpathADO`: the Azure DevOps spelling of the same command, with no
    /// dot in the name — measured, not the `section.addPath` one might invent.
    #[test]
    fn the_ado_form_carries_the_same_command() {
        let mut h = Harness::unsecure();
        h.run_all(&["##[add-path]/zoo\n", "##[add-path]/boo\n"]);
        assert_eq!(h.context.extra_path, vec!["/boo", "/zoo"]);
    }

    /// `TestStopCommands`, including the log assertion: a suppressed command is
    /// still logged, as the handled mark over the raw line.
    #[test]
    fn stop_commands_suspends_until_the_named_command_returns() {
        let mut h = Harness::unsecure();
        h.run_all(&[
            "::set-env name=x::valz\n",
            "::stop-commands::my-end-token\n",
            "::set-env name=x::abcd\n",
            "::my-end-token::\n",
            "::set-env name=x::abcd\n",
        ]);
        assert_eq!(
            h.context.env.get("x").map(String::as_str),
            Some("abcd"),
            "the suspension lifted when the end token came back"
        );
        assert!(
            h.messages_at(Level::Info)
                .iter()
                .any(|line| line == "  \u{2699}  ::set-env name=x::abcd\n"),
            "the suppressed line is logged verbatim, newline included"
        );
    }

    /// The middle of `TestStopCommands` on its own: while suspended, a command
    /// is *logged* and dropped, and changes nothing.
    #[test]
    fn a_suspended_command_is_logged_and_dropped() {
        let mut h = Harness::unsecure();
        h.run_all(&[
            "::set-env name=x::valz\n",
            "::stop-commands::my-end-token\n",
            "::set-env name=x::abcd\n",
        ]);
        assert_eq!(
            h.context.env.get("x").map(String::as_str),
            Some("valz"),
            "a suspended command does not act"
        );
    }

    /// A suspension on a command that has a literal `switch` arm is **inert**,
    /// and that is the more surprising half of it.
    ///
    /// The guard lets the line through — the command *is* the resume command, so
    /// it is not "ignored" — and then the literal arm runs it, so the write
    /// happens. The suspension is neither honoured nor cleared. Measured against
    /// the real upstream switch: `env["x"] == "abcd"` and `resumeCommand` is
    /// still `"set-env"`. A port that reordered the arms to make such a
    /// suspension actually suppress the command would be a behaviour change.
    #[test]
    fn a_suspension_on_a_literal_command_does_not_suppress_it() {
        let mut h = Harness::unsecure();
        h.run_all(&[
            "::stop-commands::set-env\n",
            "::set-env name=x::abcd\n",
            "::debug::anything\n",
        ]);
        assert_eq!(
            h.context.env.get("x").map(String::as_str),
            Some("abcd"),
            "the literal arm ran; the guard did not fire because the names match"
        );
        assert!(
            h.messages_at(Level::Debug).is_empty(),
            "still suspended, so the debug line never reached the debug level"
        );
        assert!(
            h.messages_at(Level::Info)
                .iter()
                .any(|line| line == "  \u{2699}  ::debug::anything\n"),
            "it was logged as a suppressed command instead"
        );
    }

    /// The one place the order of the `resumeCommand` arm is observable:
    /// upstream's variable case sits *before* `save-state`, so a `save-state`
    /// line arriving under a `save-state` suspension resumes and does not save.
    /// Measured against the real upstream switch, which leaves the state empty
    /// and the suspension cleared.
    #[test]
    fn a_save_state_line_under_a_save_state_suspension_only_resumes() {
        let mut h = Harness::new();
        h.context.current_step = "step".to_string();
        h.run_all(&[
            "::stop-commands::save-state\n",
            "::save-state name=token::abc\n",
        ]);
        assert!(
            h.context.intra_action_state.is_empty(),
            "the resume arm comes first, so nothing was saved"
        );
        // Having resumed, the next `save-state` is an ordinary one again.
        h.run("::save-state name=token::abc\n");
        assert_eq!(
            h.context.intra_action_state["step"]
                .get("token")
                .map(String::as_str),
            Some("abc")
        );
    }

    /// `TestAddmask`.
    #[test]
    fn add_mask_records_the_value_and_prints_stars() {
        let mut h = Harness::new();
        h.run("::add-mask::my-secret-value\n");
        assert_eq!(h.context.masks, vec!["my-secret-value".to_string()]);
        assert_eq!(
            h.messages_at(Level::Info),
            vec!["  \u{2699}  ***".to_string()],
            "the exact upstream message, and never the value"
        );
    }

    /// The part of `TestAddmaskUsemask` that belongs to this module: the line
    /// upstream uses there is the *unspaced* form, and it parses to a command
    /// with no properties and a data value carrying its leading space. That is
    /// why the expected log in that test reads `::set-output:: = token=***` —
    /// the output name is the empty string.
    ///
    /// The substitution of the mask into the log happens in the job logger,
    /// which is not ported yet; what is pinned here is the parse and the write.
    #[test]
    fn the_unspaced_form_writes_an_output_named_nothing() {
        let mut h = Harness::new();
        h.context.current_step = "my-step".to_string();
        h.context
            .step_results
            .insert("my-step".to_string(), StepResult::default());
        h.run("::set-output:: token=secret\n");
        assert_eq!(
            h.context.step_results["my-step"].outputs[""], " token=secret",
            "the empty name, and the leading space kept"
        );
        assert_eq!(
            h.messages_at(Level::Info),
            vec!["  \u{2699}  ::set-output:: = token=secret".to_string()]
        );
    }

    /// `TestSaveState`.
    #[test]
    fn save_state_is_recorded_under_the_current_step() {
        let mut h = Harness::new();
        h.context.current_step = "step".to_string();
        h.run("::save-state name=state-name::state-value\n");
        assert_eq!(
            h.context.intra_action_state["step"]
                .get("state-name")
                .map(String::as_str),
            Some("state-value")
        );
    }

    /// Outside a step there is no state to attach to, and the value is dropped
    /// rather than filed under an empty step.
    #[test]
    fn save_state_outside_a_step_is_dropped() {
        let mut h = Harness::new();
        h.run("::save-state name=state-name::state-value\n");
        assert!(h.context.intra_action_state.is_empty());
    }

    /// A plain line of output is not a command and is passed through; a command
    /// is consumed.
    #[test]
    fn ordinary_output_is_passed_through_and_commands_are_consumed() {
        let mut h = Harness::new();
        assert!(h.run("just some output\n"), "passed through");
        assert!(!h.run("::debug::x\n"), "consumed");
    }

    /// The three measured spellings from the module doc, pinned. If a future
    /// release of the `regex` crate ever altered submatch selection, this is
    /// the test that would notice.
    #[test]
    fn the_three_measured_spellings_hold() {
        let command = try_parse_raw_action_command("::set-output::name=x::value\n")
            .expect("the GA form matches");
        assert_eq!(command.command, "set-output::name=x");
        assert!(command.kv_pairs.is_empty());
        assert_eq!(command.arg, "value");

        let command = try_parse_raw_action_command("::set-output name=x::value\n")
            .expect("the GA form matches");
        assert_eq!(command.command, "set-output");
        assert_eq!(command.kv_pairs.get("name").map(String::as_str), Some("x"));
        assert_eq!(command.arg, "value");

        let command = try_parse_raw_action_command("::set-output:: token=secret\n")
            .expect("the GA form matches");
        assert_eq!(command.command, "set-output");
        assert!(command.kv_pairs.is_empty());
        assert_eq!(command.arg, " token=secret");
    }

    /// The ADO form's `]` closes the property group just as `::` does, so a
    /// command written without a space has no properties and the rest is data.
    #[test]
    fn the_ado_form_parses_the_same_way() {
        let command =
            try_parse_raw_action_command("##[add-path]/zoo\n").expect("the ADO form matches");
        assert_eq!(command.command, "add-path");
        assert!(command.kv_pairs.is_empty());
        assert_eq!(command.arg, "/zoo");

        let command =
            try_parse_raw_action_command("##[debug]a=1;b=2::v\n").expect("the ADO form matches");
        assert_eq!(command.command, "debug");
        assert!(
            command.kv_pairs.is_empty(),
            "no space means the `]` closed the group, so `;` never applies"
        );
        assert_eq!(command.arg, "a=1;b=2::v");
    }

    /// A line without a trailing newline is **not** a command: both patterns end
    /// in `[\r\n]+`, so a step whose last line is `::debug::x` has it ignored.
    #[test]
    fn a_command_needs_its_trailing_newline() {
        assert!(try_parse_raw_action_command("::debug::hello\n").is_some());
        assert!(try_parse_raw_action_command("::debug::hello\r\n").is_some());
        assert!(
            try_parse_raw_action_command("::debug::hello").is_none(),
            "no newline, no command"
        );
    }

    /// The `stop-commands` end token is itself an ordinary command, with empty
    /// data. Its group 3 does not participate, which is the case Go reads as
    /// `""` and Rust's `Captures` would panic on.
    #[test]
    fn the_end_token_parses_as_an_empty_command() {
        let command =
            try_parse_raw_action_command("::my-end-token::\n").expect("the GA form matches");
        assert_eq!(command.command, "my-end-token");
        assert_eq!(command.arg, "");
    }

    /// A `key=value` only counts when it splits into exactly two parts.
    #[test]
    fn only_exactly_two_part_pairs_count() {
        assert_eq!(
            parse_key_value_pairs("a=b=c", ','),
            BTreeMap::new(),
            "three parts is not a pair"
        );
        assert_eq!(
            parse_key_value_pairs("name=x", ','),
            BTreeMap::from([("name".to_string(), "x".to_string())])
        );
        assert_eq!(
            parse_key_value_pairs("name=", ','),
            BTreeMap::from([("name".to_string(), String::new())]),
            "an empty value is a value"
        );
        assert_eq!(
            parse_key_value_pairs("=v", ','),
            BTreeMap::from([(String::new(), "v".to_string())]),
            "an empty key is a key"
        );
        assert!(parse_key_value_pairs("", ',').is_empty());
        assert_eq!(
            parse_key_value_pairs("name=x,y=2,z", ','),
            BTreeMap::from([
                ("name".to_string(), "x".to_string()),
                ("y".to_string(), "2".to_string()),
            ]),
            "a part with no `=` is skipped, the rest survive"
        );
    }

    /// The data escapes, in the fixed order this port chose. The double-escaped
    /// case is the one upstream has no answer for — measured 400 times as
    /// `"\n"` 294 times and `"%0A"` 106 times.
    #[test]
    fn the_data_escapes_are_applied_in_the_maps_literal_order() {
        assert_eq!(unescape_command_data("%25"), "%");
        assert_eq!(unescape_command_data("%0D"), "\r");
        assert_eq!(unescape_command_data("%0A"), "\n");
        assert_eq!(unescape_command_data("a%25b"), "a%b");
        assert_eq!(
            unescape_command_data("%250A"),
            "\n",
            "one of upstream's two answers, chosen and pinned"
        );
    }

    /// A property value gets two extra escapes, because it is itself a
    /// comma-separated list.
    #[test]
    fn a_property_value_gets_two_more_escapes_than_the_data() {
        assert_eq!(unescape_command_property("%3A"), ":");
        assert_eq!(unescape_command_property("%2C"), ",");
        assert_eq!(
            unescape_command_data("%3A"),
            "%3A",
            "the data form does not, so the two are not interchangeable"
        );
    }

    /// Keys are not unescaped, only values.
    #[test]
    fn only_property_values_are_unescaped() {
        let pairs = BTreeMap::from([
            ("na%3Ame".to_string(), "a%2Cb".to_string()),
            ("plain".to_string(), "b%0A".to_string()),
        ]);
        let unescaped = unescape_kv_pairs(&pairs);
        assert_eq!(unescaped.get("na%3Ame").map(String::as_str), Some("a,b"));
        assert_eq!(unescaped.get("plain").map(String::as_str), Some("b\n"));
    }

    /// The marks are user-visible and one of them is spelled in lowercase
    /// upstream. Pinned character for character.
    #[test]
    fn the_log_marks_are_the_upstream_code_points() {
        assert_eq!(MARK_HANDLED, '\u{2699}');
        assert_eq!(MARK_DEBUG, '\u{1f4ac}');
        assert_eq!(MARK_WARNING, '\u{1f6a7}');
        assert_eq!(MARK_ERROR, '\u{2757}');
        assert_eq!(MARK_SAVE_STATE, '\u{1f4be}');
        assert_eq!(MARK_OTHER, '\u{2753}');
    }
}
