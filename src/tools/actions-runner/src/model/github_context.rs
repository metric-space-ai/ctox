//! The `github` context: what a workflow can read about the run that triggered
//! it.
//!
//! Port of act's `pkg/model/github_context.go` (217 lines). Every field here is
//! a key a workflow may reference as `github.<name>`, and several of them are
//! **derived** from the event payload rather than read from it — the ref of a
//! pull request, the branch of a `push`, the sha of a deployment.
//!
//! # The git seam, and why it is a parameter here
//!
//! `SetRef` and `SetSha` fall back on the repository when the event does not
//! carry the value. Upstream reaches git through two package-level `var`s,
//! `findGitRef` and `findGitRevision`, which its own test swaps out. That is a
//! good seam and this port keeps it, as a [`GitLookups`] argument: the decision
//! logic — the part that decides *which* event yields *which* ref — is what has
//! tests, and the concrete git implementation is a separate concern that plugs
//! into the same two slots. `pkg/common/git` is not ported yet.
//!
//! # `SetRef` shadows its own parameter, and it matters
//!
//! Inside the `default:` arm of the event switch there is a
//! `defaultBranch := asString(nestedMapLookup(…))` that declares a **new**
//! variable. The `defaultBranch` used further down, in the git fallback, is the
//! *parameter*. Two different values with one name, and the inner one must not
//! leak. In Rust that is two distinct bindings, named so the difference is
//! visible rather than implied by a block scope.
//!
//! # A pull request number is formatted, not parsed
//!
//! `refs/pull/%.0f/merge` formats the event's `number` as a float with no
//! decimals. `encoding/json` decodes every JSON number as `float64`, so this
//! works — and it rounds half to even, so `1.5` would become `refs/pull/2/merge`.
//! Go's `%.0f` and Rust's `{:.0}` were measured to agree on that and on every
//! other value tried, so the formatting is kept as it is rather than routed
//! through an integer.

use serde_json::{Map, Value};

/// The two git lookups `SetRef` and `SetSha` fall back on.
///
/// Upstream keeps these as package-level `var`s that its test replaces. Passing
/// them in keeps the same seam without global state.
pub struct GitLookups {
    /// The repository's current ref, as `git rev-parse --abbrev-ref HEAD`.
    pub find_ref: FindInRepository,
    /// The repository's current revision, as `git rev-parse HEAD`.
    pub find_revision: FindInRepository,
    /// The repository's `owner/name`, given the path, the instance and the
    /// remote name.
    ///
    /// **Not a seam upstream has.** act keeps `FindGithubRepo` a plain
    /// function, so its own `TestGetGitHubContext` passes only because it runs
    /// inside act's clone — the assertion `repository == "nektos/act"` is
    /// really "whatever the ambient checkout says". It is here because the
    /// alternative is a test that reads the developer's own repository, which
    /// is a test that fails on someone else's machine and tests nothing on
    /// theirs. The production value is the same function; only the ability to
    /// replace it is new.
    pub find_repo: FindGithubRepo,
}

/// A git lookup, given the repository path.
pub type FindInRepository = Box<dyn Fn(&str) -> anyhow::Result<String>>;

/// The repository lookup, which also needs the instance and the remote name.
pub type FindGithubRepo = Box<dyn Fn(&str, &str, &str) -> anyhow::Result<String>>;


impl std::fmt::Debug for GitLookups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitLookups")
    }
}

impl GitLookups {
    /// The lookups used in production, which shell out to git.
    pub fn new(
        find_ref: impl Fn(&str) -> anyhow::Result<String> + 'static,
        find_revision: impl Fn(&str) -> anyhow::Result<String> + 'static,
        find_repo: impl Fn(&str, &str, &str) -> anyhow::Result<String> + 'static,
    ) -> Self {
        Self {
            find_ref: Box::new(find_ref),
            find_revision: Box::new(find_revision),
            find_repo: Box::new(find_repo),
        }
    }
}

/// The `github` context.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GithubContext {
    /// The event payload, verbatim. Several fields below are read out of it.
    pub event: Map<String, Value>,
    /// Where `event.json` was written, inside the job container.
    pub event_path: String,
    /// The workflow's name.
    pub workflow: String,
    /// `github.run_attempt`, defaulting to `1`.
    pub run_attempt: String,
    /// `github.run_id`, defaulting to `1`.
    pub run_id: String,
    /// `github.run_number`, defaulting to `1`.
    pub run_number: String,
    /// `github.actor`, defaulting to `nektos/act`.
    pub actor: String,
    /// `owner/repo`.
    pub repository: String,
    /// The name of the event being run.
    pub event_name: String,
    /// The commit the run is for.
    pub sha: String,
    /// The full ref, `refs/heads/main` or `refs/pull/1234/merge`.
    pub ref_: String,
    /// The short ref, `main` or `1234/merge`.
    pub ref_name: String,
    /// `branch`, `tag`, or **empty** for a pull request.
    pub ref_type: String,
    /// The branch a pull request merges into.
    pub head_ref: String,
    /// The branch a pull request comes from.
    pub base_ref: String,
    /// The GitHub token.
    pub token: String,
    /// The workspace, as a path inside the container.
    pub workspace: String,
    /// The step currently running.
    pub action: String,
    /// The action's directory.
    pub action_path: String,
    /// The ref the action was referenced at.
    pub action_ref: String,
    /// The repository the action came from.
    pub action_repository: String,
    /// The job id.
    pub job: String,
    /// The job's name.
    pub job_name: String,
    /// The owner half of [`GithubContext::repository`].
    pub repository_owner: String,
    /// How long artifacts are kept, defaulting to `0`.
    pub retention_days: String,
    /// Where the runner writes its performance log, defaulting to `/dev/null`.
    pub runner_perflog: String,
    /// The runner tracking id.
    pub runner_tracking_id: String,
    /// `https://github.com`, or the instance's URL.
    pub server_url: String,
    /// The REST API URL.
    pub api_url: String,
    /// The GraphQL API URL.
    pub graphql_url: String,
}

impl GithubContext {
    /// The context as an expression sees it.
    ///
    /// The member names are act's **JSON tags**, not the Rust field names, and
    /// the difference is not cosmetic: a workflow writes `github.ref` and
    /// `github.run_id`, and the field is called `ref_` here only because `ref`
    /// is a Rust keyword. `Value`'s property lookup is case-insensitive, so
    /// `github.RUN_ID` resolves too, as it does upstream.
    pub fn to_value(&self) -> crate::expr::Value {
        use crate::expr::interpreter::from_json_value;
        use crate::expr::Value;
        let string = |value: &str| Value::String(value.to_string());
        // The event is a `serde_json::Map`; `Value::Object` wants a
        // `BTreeMap`, so the conversion happens here rather than in the
        // interpreter, which never has to know the event came from JSON.
        let event = Value::Object(
            self.event
                .iter()
                .map(|(key, value)| (key.clone(), from_json_value(value)))
                .collect(),
        );
        Value::object(
            [
            ("event", event),
            ("event_path", string(&self.event_path)),
            ("workflow", string(&self.workflow)),
            ("run_attempt", string(&self.run_attempt)),
            ("run_id", string(&self.run_id)),
            ("run_number", string(&self.run_number)),
            ("actor", string(&self.actor)),
            ("repository", string(&self.repository)),
            ("event_name", string(&self.event_name)),
            ("sha", string(&self.sha)),
            // The Rust field is `ref_`; the expression member is `ref`.
            ("ref", string(&self.ref_)),
            ("ref_name", string(&self.ref_name)),
            ("ref_type", string(&self.ref_type)),
            ("head_ref", string(&self.head_ref)),
            ("base_ref", string(&self.base_ref)),
            ("token", string(&self.token)),
            ("workspace", string(&self.workspace)),
            ("action", string(&self.action)),
            ("action_path", string(&self.action_path)),
            ("action_ref", string(&self.action_ref)),
            ("action_repository", string(&self.action_repository)),
            ("job", string(&self.job)),
            ("job_name", string(&self.job_name)),
            ("repository_owner", string(&self.repository_owner)),
            ("retention_days", string(&self.retention_days)),
            ("runner_perflog", string(&self.runner_perflog)),
            ("runner_tracking_id", string(&self.runner_tracking_id)),
            ("server_url", string(&self.server_url)),
            ("api_url", string(&self.api_url)),
            ("graphql_url", string(&self.graphql_url)),
            ]
        )
    }
}

/// `asString`: a JSON string, or `""` for anything else.
///
/// Note that a JSON **number** is not a string, so it becomes `""` — which is
/// what makes `SetRef` use `%.0f` rather than `asString` for a pull request
/// number.
pub fn as_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

/// `nestedMapLookup`: walk a decoded event by successive keys.
///
/// Upstream has this in **two** packages, `pkg/model` and `pkg/runner`, as two
/// copies. There is one here.
pub fn nested_map_lookup<'a>(map: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    let (first, rest) = keys.split_first()?;
    let mut current = map.get(*first)?;
    for key in rest {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

impl GithubContext {
    /// `withDefaultBranch`: put the default branch into the event if it is not
    /// already there.
    ///
    /// A **missing** `repository` gets one created — that is what makes a
    /// `no-event` payload end up on `refs/heads/master` rather than
    /// `refs/heads/`, and upstream's own `no-default-branch` subtest asserts
    /// exactly that. Only a `repository` of the **wrong type** is left alone:
    /// upstream logs a warning and returns the event untouched, because a
    /// payload that does not have a repository object is not one this code
    /// assumes.
    pub fn with_default_branch(event: &mut Map<String, Value>, branch: &str) {
        // Absent: create the object and fill it.
        let Some(existing) = event.get("repository").cloned() else {
            let mut repository = Map::new();
            repository.insert(
                "default_branch".to_string(),
                Value::String(branch.to_string()),
            );
            event.insert("repository".to_string(), Value::Object(repository));
            return;
        };
        // Wrong type: upstream warns and leaves the event untouched.
        let Some(mut repository) = existing.as_object().cloned() else {
            return;
        };
        // A branch already in the event wins over the one we would add.
        if repository.contains_key("default_branch") {
            return;
        }
        repository.insert(
            "default_branch".to_string(),
            Value::String(branch.to_string()),
        );
        event.insert("repository".to_string(), Value::Object(repository));
    }

    /// `SetBaseAndHeadRef`: the two halves of a pull request.
    ///
    /// Only for `pull_request` and `pull_request_target`; anything else leaves
    /// both empty. A value already set wins, so an explicit `GITHUB_BASE_REF` in
    /// the environment is not overwritten.
    pub fn set_base_and_head_ref(&mut self) {
        if self.event_name != "pull_request" && self.event_name != "pull_request_target" {
            return;
        }
        if self.base_ref.is_empty() {
            self.base_ref = as_string(nested_map_lookup(&self.event, &["pull_request", "base", "ref"]));
        }
        if self.head_ref.is_empty() {
            self.head_ref =
                as_string(nested_map_lookup(&self.event, &["pull_request", "head", "ref"]));
        }
    }

    /// `SetRef`: work out the ref this run is for.
    ///
    /// Which event yields which ref is the whole of this function:
    ///
    /// | event | ref |
    /// |---|---|
    /// | `pull_request_target` | `refs/heads/<base_ref>` |
    /// | `pull_request`, `pull_request_review`, `pull_request_review_comment` | `refs/pull/<number>/merge` |
    /// | `deployment`, `deployment_status` | `deployment.ref` |
    /// | `release` | `refs/tags/<release.tag_name>` |
    /// | `push`, `create`, `workflow_dispatch` | `event.ref` |
    /// | anything else | `refs/heads/<repository.default_branch>` |
    ///
    /// An empty result falls back to the repository's own ref, and only if that
    /// is empty too does it become `refs/heads/<default_branch>` — with `master`
    /// substituted when the caller supplied no default branch. The event is
    /// given that branch on the way out, so `github.event.repository.default_branch`
    /// resolves for a workflow that asks.
    pub fn set_ref(&mut self, default_branch: &str, repo_path: &str, git: &GitLookups) {
        match self.event_name.as_str() {
            "pull_request_target" => {
                self.ref_ = format!("refs/heads/{}", self.base_ref);
            }
            "pull_request" | "pull_request_review" | "pull_request_review_comment" => {
                // `%.0f` on the event's `number`. See the module docs: this is a
                // float with no decimals, rounded half to even, because that is
                // what `encoding/json` hands over.
                match self.event.get("number").and_then(Value::as_f64) {
                    Some(number) => self.ref_ = format!("refs/pull/{number:.0}/merge"),
                    // Upstream formats a missing or non-numeric `number` with
                    // Go's error verb and gets `refs/pull/%!f(<nil>)/merge`,
                    // which is non-empty, so the git fallback below is skipped
                    // and the garbage becomes the run's ref. Leaving it empty
                    // lets the fallback run and produce a real ref. This only
                    // differs where upstream produced `%!f(...)`.
                    None => self.ref_.clear(),
                }
            }
            "deployment" | "deployment_status" => {
                self.ref_ = as_string(nested_map_lookup(&self.event, &["deployment", "ref"]));
            }
            "release" => {
                self.ref_ = format!(
                    "refs/tags/{}",
                    as_string(nested_map_lookup(&self.event, &["release", "tag_name"]))
                );
            }
            "push" | "create" | "workflow_dispatch" => {
                self.ref_ = as_string(self.event.get("ref"));
            }
            _ => {
                // Deliberately a *different* binding from the parameter below,
                // shadowing exactly as upstream's `:=` inside this arm does.
                let branch_from_event =
                    as_string(nested_map_lookup(&self.event, &["repository", "default_branch"]));
                if !branch_from_event.is_empty() {
                    self.ref_ = format!("refs/heads/{branch_from_event}");
                }
            }
        }

        if self.ref_.is_empty() {
            // Upstream only warns on failure and carries on to the default
            // branch, so the error is dropped here rather than reported.
            if let Ok(found) = (git.find_ref)(repo_path) {
                self.ref_ = found;
            }

            if default_branch.is_empty() {
                GithubContext::with_default_branch(&mut self.event, "master");
            } else {
                GithubContext::with_default_branch(&mut self.event, default_branch);
            }

            if self.ref_.is_empty() {
                self.ref_ = format!(
                    "refs/heads/{}",
                    as_string(nested_map_lookup(&self.event, &["repository", "default_branch"]))
                );
            }
        }
    }

    /// `SetSha`: work out the commit this run is for.
    ///
    /// | event | sha |
    /// |---|---|
    /// | `pull_request_target` | `pull_request.base.sha` |
    /// | `deployment`, `deployment_status` | `deployment.sha` |
    /// | `push`, `create`, `workflow_dispatch` | `event.after`, but **not** when `deleted` |
    ///
    /// The `deleted` check is the part that is easy to miss: a deleted branch's
    /// push event still carries an `after`, and using it would pin the run to a
    /// commit that no longer exists. An empty result falls back to the
    /// repository's own revision.
    pub fn set_sha(&mut self, repo_path: &str, git: &GitLookups) {
        match self.event_name.as_str() {
            "pull_request_target" => {
                self.sha =
                    as_string(nested_map_lookup(&self.event, &["pull_request", "base", "sha"]));
            }
            "deployment" | "deployment_status" => {
                self.sha = as_string(nested_map_lookup(&self.event, &["deployment", "sha"]));
            }
            "push" | "create" | "workflow_dispatch" => {
                let deleted = self
                    .event
                    .get("deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !deleted {
                    self.sha = as_string(self.event.get("after"));
                }
            }
            _ => {}
        }

        if self.sha.is_empty() {
            if let Ok(found) = (git.find_revision)(repo_path) {
                self.sha = found;
            }
        }
    }

    /// `SetRepositoryAndOwner`: `owner/repo`, and the owner half of it.
    ///
    /// When git cannot say, upstream falls back to `nektos/act` — act is used as
    /// a default action, so why not as a default repository. Kept, because a
    /// workflow that reads `github.repository` in a directory that is not a
    /// clone gets *something* rather than an empty string.
    pub fn set_repository_and_owner(
        &mut self,
        find_repo: &dyn Fn(&str, &str, &str) -> anyhow::Result<String>,
        github_instance: &str,
        remote_name: &str,
        repo_path: &str,
    ) {
        if self.repository.is_empty() {
            match find_repo(repo_path, github_instance, remote_name) {
                Ok(repo) => self.repository = repo,
                Err(_) => self.repository = "nektos/act".to_string(),
            }
        }
        // Upstream splits on `/` and takes index 0, so a repository with no
        // slash at all yields the whole string as the owner.
        self.repository_owner = self
            .repository
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
    }

    /// `SetRefTypeAndName`: split the full ref into its type and short name.
    ///
    /// A pull request gets an **empty** `ref_type` and a `ref_name` of
    /// `<number>/merge` — a pull request is neither a branch nor a tag, and
    /// GitHub reports it that way. A ref matching none of the three prefixes
    /// leaves both empty. An already-set value wins in both cases, so an
    /// explicit `GITHUB_REF_TYPE` survives.
    pub fn set_ref_type_and_name(&mut self) {
        let (ref_type, ref_name) = if let Some(rest) = self.ref_.strip_prefix("refs/tags/") {
            ("tag", rest)
        } else if let Some(rest) = self.ref_.strip_prefix("refs/heads/") {
            ("branch", rest)
        } else if let Some(rest) = self.ref_.strip_prefix("refs/pull/") {
            ("", rest)
        } else {
            ("", "")
        };

        if self.ref_type.is_empty() {
            self.ref_type = ref_type.to_string();
        }
        if self.ref_name.is_empty() {
            self.ref_name = ref_name.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The lookups upstream's test injects: a repository on `master` at a fixed
    /// revision.
    fn git(ref_found: &'static str, sha: &'static str) -> GitLookups {
        GitLookups::new(
            move |path| {
                if path == "/fail" {
                    anyhow::bail!("no default branch")
                } else {
                    Ok(ref_found.to_string())
                }
            },
            move |_| Ok(sha.to_string()),
            |_, _, _| anyhow::bail!("no repository"),
        )
    }

    /// `TestSetRef`, whole.
    #[test]
    fn the_ref_follows_the_event() {
        // The table from `TestSetRef`.
        let cases: Vec<(&str, Value, &str, &str)> = vec![
            ("pull_request_target", json!({}), "refs/heads/master", "master"),
            (
                "pull_request",
                json!({"number": 1234.0}),
                "refs/pull/1234/merge",
                "1234/merge",
            ),
            (
                "deployment",
                json!({"deployment": {"ref": "refs/heads/somebranch"}}),
                "refs/heads/somebranch",
                "somebranch",
            ),
            (
                "release",
                json!({"release": {"tag_name": "v1.0.0"}}),
                "refs/tags/v1.0.0",
                "v1.0.0",
            ),
            (
                "push",
                json!({"ref": "refs/heads/somebranch"}),
                "refs/heads/somebranch",
                "somebranch",
            ),
            (
                "unknown",
                json!({"repository": {"default_branch": "main"}}),
                "refs/heads/main",
                "main",
            ),
            ("no-event", json!({}), "refs/heads/master", "master"),
        ];
        for (event_name, event, want_ref, want_ref_name) in &cases {
            let mut ghc = GithubContext {
                event_name: (*event_name).to_string(),
                base_ref: "master".to_string(),
                event: event.as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_ref("main", "/some/dir", &git("refs/heads/master", "1234fakesha"));
            ghc.set_ref_type_and_name();
            assert_eq!(ghc.ref_, *want_ref, "ref for {event_name}");
            assert_eq!(ghc.ref_name, *want_ref_name, "ref_name for {event_name}");
        }
    }

    /// The `no-default-branch` subtest: the git lookup fails, the caller
    /// supplied no default branch, and the run still ends up on `master`.
    #[test]
    fn a_failing_git_lookup_falls_back_to_master() {
        let mut ghc = GithubContext {
            event_name: "no-default-branch".to_string(),
            event: json!({}).as_object().cloned().unwrap_or_default(),
            ..GithubContext::default()
        };
        ghc.set_ref("", "/fail", &git("refs/heads/master", "1234fakesha"));
        assert_eq!(ghc.ref_, "refs/heads/master");
        // And the event is given that branch, so a workflow that asks for
        // `github.event.repository.default_branch` gets an answer.
        assert_eq!(
            nested_map_lookup(&ghc.event, &["repository", "default_branch"]),
            Some(&json!("master"))
        );
    }

    /// The two review events share the pull-request ref rule.
    #[test]
    fn the_review_events_use_the_pull_request_ref() {
        for event_name in [
            "pull_request_review",
            "pull_request_review_comment",
        ] {
            let mut ghc = GithubContext {
                event_name: event_name.to_string(),
                event: json!({"number": 42.0}).as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_ref("main", "/some/dir", &git("refs/heads/master", "sha"));
            assert_eq!(ghc.ref_, "refs/pull/42/merge", "ref for {event_name}");
        }
    }

    /// The number is formatted as a float with no decimals, because that is what
    /// `encoding/json` hands over. `1.5` rounds half to even, as both Go's
    /// `%.0f` and Rust's `{:.0}` do — measured, not assumed.
    #[test]
    fn the_pull_request_number_is_formatted_not_parsed() {
        let mut ghc = GithubContext {
            event_name: "pull_request".to_string(),
            event: json!({"number": 1.5}).as_object().cloned().unwrap_or_default(),
            ..GithubContext::default()
        };
        ghc.set_ref("main", "/some/dir", &git("refs/heads/master", "sha"));
        assert_eq!(ghc.ref_, "refs/pull/2/merge", "1.5 rounds to even, so 2");
    }

    /// A pull request with no usable `number` leaves the ref empty, so the git
    /// fallback can produce a real one. Upstream instead builds
    /// `refs/pull/%!f(<nil>)/merge` from Go's error verb, and because that is
    /// non-empty the fallback never runs and the garbage becomes the run's ref.
    #[test]
    fn a_pull_request_without_a_number_falls_back_to_git() {
        for event in [json!({}), json!({"number": "1234"}), json!({"number": true})] {
            let mut ghc = GithubContext {
                event_name: "pull_request".to_string(),
                event: event.as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_ref("main", "/some/dir", &git("refs/heads/master", "sha"));
            assert_eq!(
                ghc.ref_, "refs/heads/master",
                "a non-numeric number must not become the ref, event {event}"
            );
        }
    }

    /// `TestSetSha`, whole.
    #[test]
    fn the_sha_follows_the_event() {
        let cases: Vec<(&str, Value, &str)> = vec![
            (
                "pull_request_target",
                json!({"pull_request": {"base": {"sha": "pr-base-sha"}}}),
                "pr-base-sha",
            ),
            ("pull_request", json!({"number": 1234.0}), "1234fakesha"),
            (
                "deployment",
                json!({"deployment": {"sha": "deployment-sha"}}),
                "deployment-sha",
            ),
            ("release", json!({}), "1234fakesha"),
            (
                "push",
                json!({"after": "push-sha", "deleted": false}),
                "push-sha",
            ),
            ("unknown", json!({}), "1234fakesha"),
            ("no-event", json!({}), "1234fakesha"),
        ];
        for (event_name, event, want) in &cases {
            let mut ghc = GithubContext {
                event_name: (*event_name).to_string(),
                base_ref: "master".to_string(),
                event: event.as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_sha("/some/dir", &git("refs/heads/master", "1234fakesha"));
            assert_eq!(ghc.sha, *want, "sha for {event_name}");
        }
    }

    /// A deleted branch's push event still carries an `after`, and using it would
    /// pin the run to a commit that no longer exists.
    #[test]
    fn a_deleted_push_falls_back_to_the_repository() {
        let mut ghc = GithubContext {
            event_name: "push".to_string(),
            event: json!({"after": "gone-sha", "deleted": true})
                .as_object()
                .cloned()
                .unwrap_or_default(),
            ..GithubContext::default()
        };
        ghc.set_sha("/some/dir", &git("refs/heads/master", "1234fakesha"));
        assert_eq!(
            ghc.sha, "1234fakesha",
            "the `after` of a deleted ref is not a commit to build"
        );
    }

    /// The three ref shapes, and the two prefixes that match none of them.
    #[test]
    fn the_ref_type_and_name_split() {
        let cases: Vec<(&str, &str, &str)> = vec![
            ("refs/heads/main", "branch", "main"),
            ("refs/tags/v1.0.0", "tag", "v1.0.0"),
            // A pull request is neither, so the type is empty on purpose.
            ("refs/pull/1234/merge", "", "1234/merge"),
            ("refs/changes/1", "", ""),
            ("main", "", ""),
        ];
        for (reference, want_type, want_name) in &cases {
            let mut ghc = GithubContext {
                ref_: (*reference).to_string(),
                ..GithubContext::default()
            };
            ghc.set_ref_type_and_name();
            assert_eq!(ghc.ref_type, *want_type, "ref_type for {reference}");
            assert_eq!(ghc.ref_name, *want_name, "ref_name for {reference}");
        }
    }

    /// An explicit value in the environment wins over the derived one.
    #[test]
    fn a_ref_type_already_set_is_kept() {
        let mut ghc = GithubContext {
            ref_: "refs/heads/main".to_string(),
            ref_type: "tag".to_string(),
            ref_name: "pinned".to_string(),
            ..GithubContext::default()
        };
        ghc.set_ref_type_and_name();
        assert_eq!(ghc.ref_type, "tag");
        assert_eq!(ghc.ref_name, "pinned");
    }

    /// Base and head only exist for the two pull-request events.
    #[test]
    fn base_and_head_ref_are_only_for_pull_requests() {
        let event = json!({"pull_request": {"base": {"ref": "main"}, "head": {"ref": "feature"}}});

        for event_name in ["pull_request", "pull_request_target"] {
            let mut ghc = GithubContext {
                event_name: event_name.to_string(),
                event: event.as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_base_and_head_ref();
            assert_eq!(ghc.base_ref, "main", "base for {event_name}");
            assert_eq!(ghc.head_ref, "feature", "head for {event_name}");
        }

        for event_name in ["push", "release", ""] {
            let mut ghc = GithubContext {
                event_name: event_name.to_string(),
                event: event.as_object().cloned().unwrap_or_default(),
                ..GithubContext::default()
            };
            ghc.set_base_and_head_ref();
            assert!(ghc.base_ref.is_empty(), "base for {event_name:?}");
            assert!(ghc.head_ref.is_empty(), "head for {event_name:?}");
        }
    }

    /// The owner is the first `/`-separated half — and for a repository with no
    /// slash at all, that is the whole string.
    #[test]
    fn the_owner_is_the_first_half_of_the_repository() {
        let cases: Vec<(&str, &str)> = vec![
            ("owner/repo", "owner"),
            ("owner/repo/extra", "owner"),
            ("noslash", "noslash"),
        ];
        for (repository, want_owner) in &cases {
            let mut ghc = GithubContext {
                repository: (*repository).to_string(),
                ..GithubContext::default()
            };
            ghc.set_repository_and_owner(&|_, _, _| anyhow::bail!("unused"), "", "", "");
            assert_eq!(ghc.repository_owner, *want_owner, "owner of {repository}");
        }
    }

    /// act is used as a default action, so why not as a default repository.
    #[test]
    fn a_directory_that_is_not_a_clone_falls_back_to_nektos_act() {
        let mut ghc = GithubContext::default();
        ghc.set_repository_and_owner(
            &|_, _, _| anyhow::bail!("not a repository"),
            "github.com",
            "origin",
            "/some/dir",
        );
        assert_eq!(ghc.repository, "nektos/act");
        assert_eq!(ghc.repository_owner, "nektos");
    }

    /// A repository that git does know is used instead.
    #[test]
    fn a_known_repository_is_used_as_git_reports_it() {
        let mut ghc = GithubContext::default();
        ghc.set_repository_and_owner(
            &|path, instance, remote| {
                assert_eq!(path, "/some/dir");
                assert_eq!(instance, "github.com");
                assert_eq!(remote, "origin");
                Ok("nektos/act".to_string())
            },
            "github.com",
            "origin",
            "/some/dir",
        );
        assert_eq!(ghc.repository, "nektos/act");
    }

    /// A non-string is not a string, which is why the pull-request number is
    /// formatted rather than read with `as_string`.
    #[test]
    fn only_a_json_string_counts_as_a_string() {
        assert_eq!(as_string(None), "");
        assert_eq!(as_string(Some(&json!("x"))), "x");
        assert_eq!(as_string(Some(&json!(1))), "");
        assert_eq!(as_string(Some(&json!(1.5))), "");
        assert_eq!(as_string(Some(&json!(true))), "");
        assert_eq!(as_string(Some(&json!(null))), "");
        assert_eq!(as_string(Some(&json!({"a": 1}))), "");
        assert_eq!(as_string(Some(&json!([1]))), "");
    }

    /// A default branch already in the event is not overwritten, and an event
    /// with no `repository` object is left alone rather than given a synthetic
    /// one.
    #[test]
    fn the_default_branch_is_only_added_when_it_is_missing() {
        let mut present = json!({"repository": {"default_branch": "trunk"}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        GithubContext::with_default_branch(&mut present, "main");
        assert_eq!(
            nested_map_lookup(&present, &["repository", "default_branch"]),
            Some(&json!("trunk")),
            "an existing branch wins"
        );

        let mut absent = json!({"repository": {}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        GithubContext::with_default_branch(&mut absent, "main");
        assert_eq!(
            nested_map_lookup(&absent, &["repository", "default_branch"]),
            Some(&json!("main"))
        );

        // A missing `repository` gets one *created*. That is what makes an empty
        // event end up on `refs/heads/master` instead of `refs/heads/`.
        let mut no_repository = json!({"number": 1.0})
            .as_object()
            .cloned()
            .unwrap_or_default();
        GithubContext::with_default_branch(&mut no_repository, "main");
        assert_eq!(
            nested_map_lookup(&no_repository, &["repository", "default_branch"]),
            Some(&json!("main"))
        );

        // A `repository` of the wrong type is left alone.
        let mut wrong_type = json!({"repository": "nope"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        GithubContext::with_default_branch(&mut wrong_type, "main");
        assert_eq!(
            nested_map_lookup(&wrong_type, &["repository"]),
            Some(&json!("nope")),
            "upstream warns and returns the event untouched"
        );
    }

    /// The same lookup the `run_context` module needs, so the two agree.
    #[test]
    fn a_nested_lookup_stops_at_the_first_thing_that_is_not_a_map() {
        let event = json!({
            "deployment": {"ref": "refs/heads/x", "sha": "abc"},
            "n": 3
        });
        let map = event.as_object().expect("an object");
        assert_eq!(
            nested_map_lookup(map, &["deployment", "ref"]).and_then(Value::as_str),
            Some("refs/heads/x")
        );
        assert_eq!(nested_map_lookup(map, &["deployment", "missing"]), None);
        assert_eq!(nested_map_lookup(map, &["n", "deeper"]), None, "a number is not a map");
        assert_eq!(nested_map_lookup(map, &[]), None, "no keys at all");
    }
}
