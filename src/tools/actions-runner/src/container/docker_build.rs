//! `NewDockerBuildExecutor` and `createBuildContext`: `docker build`.
//!
//! Port of act's `pkg/container/docker_build.go` (123 lines) — the executor a
//! job uses when it needs an image that only exists after a build, and the
//! tar-packing half of it that decides what the daemon is even shown.
//!
//! ```go
//! func NewDockerBuildExecutor(input NewDockerBuildExecutorInput) common.Executor
//! func createBuildContext(ctx context.Context, contextDir string, relDockerfile string) (io.ReadCloser, error)
//! ```
//!
//! # ⚠ Upstream has no tests for this file
//!
//! There is no `docker_build_test.go`. So none of the tests below are ports;
//! they pin the pure half — the archive's membership and the platform split —
//! which is the half that can be checked without a daemon. See *What is not
//! tested* at the bottom for the rest.
//!
//! # The `Info` line is user-visible output, and it is not one string
//!
//! ```go
//! if input.Platform != "" {
//!     logger.Infof("%sdocker build -t %s --platform %s %s", logPrefix, …)
//! } else {
//!     logger.Infof("%sdocker build -t %s %s", logPrefix, …)
//! }
//! ```
//!
//! Two branches, one flag, and the flag is the *input's* platform — not the
//! split one. A platform that turns out to be unusable (see below) is still
//! printed, because the line is written before the platform is parsed. That
//! order is preserved here, and [`build_info_line`] is a pure function so the
//! exact text can be pinned without a daemon.
//!
//! The prefix is [`LOG_PREFIX`], which lives in [`super::docker_log`] because
//! three upstream files share it and this one is not the owner.
//!
//! # A bare platform is silently ignored here, and an error in `create`
//!
//! Upstream splits with `strings.SplitN(platform, "/", 2)` and applies the
//! result **only when there are exactly two parts**. So `linux` — a real thing
//! a user can write — is dropped without a word, and the build silently
//! produces a host-architecture image.
//!
//! [`super::docker_engine`] splits the *same string* the other way round: there
//! a bare platform is `incorrect container platform option`, and it is gated
//! on the daemon being new enough to accept `--platform` at all. That is not an
//! inconsistency to be tidied up — it is two different upstream files making two
//! different choices, and this port keeps both.
//!
//! # `createBuildContext` has two subtleties that are easy to drop
//!
//! **1. The Dockerfile path is made platform-independent before it is matched.**
//!
//! ```go
//! relDockerfile = filepath.ToSlash(relDockerfile)
//! ```
//!
//! It happens *before* the pattern match, so on Windows a job writing
//! `docker\Dockerfile` is tested against `.dockerignore` rules as
//! `docker/Dockerfile`. On Unix `filepath.ToSlash` is the identity, and that is
//! what [`to_slash`] does here — the test
//! `a_backslash_dockerfile_path_is_left_alone_on_unix` pins it, because a port
//! that "helpfully" normalised `\` everywhere would silently change which rule
//! a Windows-written path is tested against.
//!
//! **2. `includes` only ever grows, and only when the ignore rules exclude the
//! two files the daemon itself needs.**
//!
//! ```go
//! var includes = []string{"."}
//! keepThem1, _ := patternmatcher.Matches(".dockerignore", excludes)
//! keepThem2, _ := patternmatcher.Matches(relDockerfile, excludes)
//! if keepThem1 || keepThem2 {
//!     includes = append(includes, ".dockerignore", relDockerfile)
//! }
//! ```
//!
//! A context whose `.dockerignore` says `Dockerfile` must still ship the
//! Dockerfile: the daemon parses it and *then* decides whether to keep it. Both
//! paths come with `Exclusions` — if either one is excluded, **both** are sent,
//! because there is one code path and it does not look at which one matched.
//!
//! And an unreadable `.dockerignore` is not an error: `os.IsNotExist` is
//! swallowed, every other error is returned. A missing ignore file is the
//! normal case, not a broken one.
//!
//! # `.dockerignore` is read here, not by [`crate::gitignore`]
//!
//! Reusing the crate's gitignore reader was considered first and rejected,
//! for two independent reasons:
//!
//! * [`crate::gitignore::read_patterns`] hardcodes `.gitignore` and
//!   `.git/info/exclude` and takes no filename argument, so there is no way to
//!   point it at `.dockerignore`; and it **recurses into subdirectories**,
//!   collecting one `.gitignore` per directory. `ignorefile.ReadAll` reads
//!   exactly one file, at one level.
//! * The *pattern* grammar is not the same one. gitignore matches a pattern
//!   without a slash against **any path component** (go-git's
//!   `simpleNameMatch`); `patternmatcher` matches the **whole** slash-separated
//!   path. `Dockerfile` therefore ignores a file of that name at the context
//!   root under one and not the other.
//!
//! So this module ports the two libraries it actually needs:
//!
//! * [`read_dockerignore`] — `github.com/moby/patternmatcher/ignorefile.ReadAll`.
//! * [`PatternMatcher`] — `github.com/moby/patternmatcher` v0.6.0, including
//!   the `compile` step that picks between the exact / prefix / suffix /
//!   regular-expression forms.
//! * [`tar_context`] — the walk in `github.com/moby/go-archive`'s
//!   `TarWithOptions` for `Compression: archive.Uncompressed`,
//!   `ExcludePatterns: excludes`, `IncludeFiles: includes`.
//!
//! # Deviations, all of them forced by bollard or by Rust
//!
//! * **No `BuildContext io.Reader` input.** Upstream takes a pre-built context
//!   and falls back to `createBuildContext` when it is nil.
//!   [`NewDockerBuildExecutorInput`](super::NewDockerBuildExecutorInput) has no
//!   such field and is shared with the rest of the crate, so **only the
//!   `createBuildContext` path exists here.** A caller that already has a tar in
//!   hand cannot pass it in.
//! * **The archive is buffered, not streamed.** Upstream returns an
//!   `io.ReadCloser` fed by a goroutine through a pipe. This port builds the
//!   whole archive in memory and hands `body_full` the bytes — the same trade
//!   `super::docker_engine`'s `tar_directory` already makes.
//! * **`registry.AuthConfig` becomes `bollard::auth::DockerCredentials`.** Both
//!   carry username/password/serveraddress/auth; the conversion is
//!   [`docker_credentials`]. Credentials still travel in the
//!   `X-Registry-Config` header, which is where moby's `ImageBuild` puts them.
//! * **`specs.Platform` becomes a query string.** bollard takes
//!   `platform=os/arch[/variant]` on the URL where moby's client formats
//!   `options.Platforms[0]` with `formatPlatform`;
//!   [`BuildPlatform::query_value`] is that function.
//! * **Patterns are compiled when the matcher is built.** Go compiles lazily
//!   inside `Pattern.match`, so a pattern whose generated regular expression is
//!   invalid fails only when it is first used. Every pattern is used by every
//!   call here, and both callers turn a failure into `false`, so eager
//!   compilation is observationally the same and one error path shorter.
//! * **Hard links are archived as separate files.** go-archive rewrites the
//!   second link to a `TypeLink` header pointing at the first. `std` exposes
//!   neither the inode nor the link count on every target platform without a
//!   `cfg` per platform, so each link is written in full. The daemon accepts it;
//!   the archive is larger.
//! * **`os.PathSeparator` is `/`.** go-archive matches with the platform
//!   separator, and on Windows that is `\`. This port normalises to `/`
//!   throughout, which is also what tar member names must be.
//!
//! # What is not tested
//!
//! [`new_docker_build_executor`] past its `Info` line and its dry-run return:
//! the `build_image` call needs a Docker daemon, and there is none here, so the
//! daemon-facing half is ported and compiles but is **not** verified to work.
//! Nothing in this file should be read as claiming it does. The pieces it is
//! made of are [`docker_credentials`] and [`BuildPlatform::query_value`], both
//! pure, and [`super::docker_log::log_docker_response`], which has its own
//! tests.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use bollard::auth::DockerCredentials;
use bollard::body_full;
use bollard::query_parameters::BuildImageOptions;
use bollard::Docker;
use regex::Regex;

use crate::common::context::NullSink;
use crate::common::{Executor, LogSink, RunContext};
use crate::container::NewDockerBuildExecutorInput;
use crate::gomatch::{self, MatchResult};

use super::docker_auth::{load_docker_auth_configs, RegistryAuthConfig};
use super::docker_engine::{block_on, connect};
use super::docker_log::{log_docker_response, LOG_PREFIX};

/// The ignore file, read from the context root only.
const DOCKERIGNORE: &str = ".dockerignore";

/// `NewDockerBuildExecutor`: an executor that builds an image from `input`'s
/// context directory and tags it `input`'s tag.
///
/// The `Info` line is written first and unconditionally, then a dry run returns
/// before anything touches the daemon — upstream's order, and the reason the
/// line is the only thing a `--dryrun` shows.
pub fn new_docker_build_executor(input: NewDockerBuildExecutorInput) -> Executor {
    Arc::new(move |ctx: &RunContext| build_image(&input, ctx))
}

/// `NewDockerBuildExecutor`'s body: the `Info` line, the dry run, then
/// `cli.ImageBuild(ctx, buildContext, options)` and `logDockerResponse`.
fn build_image(input: &NewDockerBuildExecutorInput, ctx: &RunContext) -> Result<()> {
    // Written before anything is parsed and before the daemon is contacted, so
    // a dry run shows it and an unusable platform still shows it.
    ctx.log_info(&build_info_line(
        &input.image_tag,
        &input.platform,
        &input.context_dir,
    ));
    if ctx.dryrun() {
        return Ok(());
    }

    let client: Docker = connect()?;

    ctx.log_debug(&format!(
        "Building image from '{}'",
        input.context_dir.display()
    ));

    // Upstream builds the options first and the context second, so a bad
    // `.dockerignore` is reported after the `Building image from` line and not
    // before it — and `AuthConfigs` is one of the options, so the credentials
    // are resolved here too.
    let options = build_options(input);
    let credentials = auth_configs(ctx);
    let build_context = create_build_context(ctx, &input.context_dir, &input.dockerfile)?;

    ctx.log_debug(&format!(
        "Creating image from context dir '{}' with tag '{}' and platform '{}'",
        input.context_dir.display(),
        input.image_tag,
        input.platform
    ));

    let sink = build_log_sink(ctx);
    let body = body_full(bytes::Bytes::from(build_context));

    // bollard hands back a `Stream<Item = Result<BuildInfo, Error>>`: the
    // daemon's newline-delimited JSON, already decoded.
    let stream = client.build_image(options, credentials, Some(body));
    futures::pin_mut!(stream);
    let mut raw = Vec::new();
    block_on(async {
        use futures::StreamExt;
        while let Some(message) = stream.next().await {
            match message {
                Ok(info) => {
                    raw.extend_from_slice(encode_build_info(&info).as_bytes());
                    raw.push(b'\n');
                }
                // Upstream passes `err != nil` to `logDockerResponse`, whose job
                // is to decode the *response*. bollard reports a transport
                // failure as a stream item, and there are no response bytes to
                // decode when it does, so this returns the error instead of
                // logging nothing.
                Err(err) => return Err(anyhow!("{err}")),
            }
        }
        Ok(())
    })?;

    log_docker_response(&raw, false, sink.as_ref()).map_err(|err| anyhow!("{err}"))
}

/// One `BuildInfo` back into the JSON line the daemon sent.
///
/// `bollard::models::BuildInfo` is `Deserialize` and **not** `Serialize`, so
/// unlike `docker_engine`'s pull path this cannot round-trip the daemon's bytes;
/// the fields are put back by hand. Only the keys act's `dockerMessage` reads
/// are emitted, and each is emitted only when the daemon sent it — an absent
/// key stays absent, which is what keeps one line's fields out of the next.
///
/// **`progress` is the one field bollard's model does not carry.** Its
/// `ProgressDetail` keeps `current`/`total`, not the `[==>  ] 1.2MB/8MB` string
/// the daemon formatted, so a progress line arrives at the logger with its
/// status and its id but no bar. Everything else — `stream`, `status`, `id`,
/// `errorDetail.message` — survives, and those are the fields that decide
/// whether a build succeeds.
fn encode_build_info(info: &bollard::models::BuildInfo) -> String {
    let mut object = serde_json::Map::new();
    for (key, value) in [
        ("id", &info.id),
        ("stream", &info.stream),
        ("status", &info.status),
    ] {
        if let Some(value) = value {
            object.insert(key.to_string(), serde_json::Value::String(value.clone()));
        }
    }
    if let Some(detail) = &info.error_detail {
        object.insert(
            "errorDetail".to_string(),
            serde_json::json!({ "message": detail.message.clone().unwrap_or_default() }),
        );
    }
    serde_json::Value::Object(object).to_string()
}

/// The `Info` line [`new_docker_build_executor`] prints, as upstream does.
///
/// Pure, because it is user-visible output and the two branches are the whole
/// difference between them.
fn build_info_line(image_tag: &str, platform: &str, context_dir: &Path) -> String {
    let context = context_dir.display();
    if platform.is_empty() {
        format!("{LOG_PREFIX}docker build -t {image_tag} {context}")
    } else {
        format!("{LOG_PREFIX}docker build -t {image_tag} --platform {platform} {context}")
    }
}

/// Where `docker build`'s response goes.
///
/// The job's own sink, so a build shows the same progress lines a pull does. A
/// job that installed no sink gets a discarding one rather than an `unwrap`:
/// the build itself is the work, its output is commentary, and a job with no
/// log is not a failed job.
fn build_log_sink(ctx: &RunContext) -> Arc<dyn LogSink> {
    ctx.sink()
        .unwrap_or_else(|| Arc::new(NullSink) as Arc<dyn LogSink>)
}

/// `client.ImageBuildOptions`: the tag, the intermediates, the Dockerfile, the
/// registry credentials and — when there are exactly two parts — the platform.
///
/// bollard's generated type spells the scalars plain rather than optional, so
/// the four fields are always sent. `t` is a single tag there, which is all
/// act ever sends.
fn build_options(input: &NewDockerBuildExecutorInput) -> BuildImageOptions {
    BuildImageOptions {
        t: Some(input.image_tag.clone()),
        rm: true,
        dockerfile: input.dockerfile.clone(),
        platform: split_build_platform(&input.platform)
            .map_or(String::new(), |platform| platform.query_value()),
        ..Default::default()
    }
}

/// `strings.SplitN(platform, "/", 2)`, applied only at two parts.
///
/// `None` for `""` and for a bare `linux`, and that is the whole of upstream's
/// handling: no error, no default, no platform.
fn split_build_platform(platform: &str) -> Option<BuildPlatform> {
    let (os, architecture) = platform.split_once('/')?;
    Some(BuildPlatform {
        os: os.to_string(),
        architecture: architecture.to_string(),
    })
}

/// Upstream's `options.Platforms = []specs.Platform{{OS, Architecture}}`, as the
/// query value the daemon receives.
///
/// The split keeps everything after the **first** `/`, so `linux/arm/v7` is one
/// architecture string rather than a variant this port would have to split
/// again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildPlatform {
    /// The `OS` field. Empty when the platform string started with `/`.
    os: String,
    /// The `Architecture` field, which may itself contain a `/`.
    architecture: String,
}

impl BuildPlatform {
    /// `formatPlatform`: the `platform` query parameter's value.
    ///
    /// `path.Join(OS, Architecture, Variant)` — with no variant here — and an
    /// empty `OS` becomes the literal `"unknown"`, which is `formatPlatform`'s
    /// answer and not a guess.
    fn query_value(&self) -> String {
        if self.os.is_empty() {
            return "unknown".to_string();
        }
        let mut parts = vec![self.os.as_str()];
        if !self.architecture.is_empty() {
            parts.push(self.architecture.as_str());
        }
        path_clean(&parts.join("/"))
    }
}

/// `LoadDockerAuthConfigs(ctx)`: every credential in the docker config file,
/// keyed by its own server address.
///
/// A config directory that cannot be found, and a config file that cannot be
/// read, are both a warning and no credentials — upstream returns `nil` from
/// exactly those two paths, and a build from a public base image must work with
/// no credentials at all.
fn auth_configs(ctx: &RunContext) -> Option<HashMap<String, DockerCredentials>> {
    let directory = match super::docker_auth::docker_config_dir() {
        Ok(directory) => directory,
        Err(err) => {
            ctx.log_warning(&format!("Could not load docker config: {err}"));
            return None;
        }
    };
    match load_docker_auth_configs(&directory) {
        // Upstream hands the daemon an empty map here; bollard's
        // `Option<HashMap>` draws the same line between "no credentials" and
        // "empty credentials".
        Ok(configs) if configs.is_empty() => None,
        Ok(configs) => Some(
            configs
                .into_iter()
                .map(|(host, config)| (host, docker_credentials(config)))
                .collect(),
        ),
        Err(err) => {
            ctx.log_warning(&format!("Could not load docker config: {err}"));
            None
        }
    }
}

/// `registry.AuthConfig` as bollard's `DockerCredentials`.
///
/// `encoded()` borrows, so the server address is cloned out first — the same
/// dance as `super::docker_engine`'s pull credentials.
fn docker_credentials(config: RegistryAuthConfig) -> DockerCredentials {
    // `encoded()` borrows, so it is computed before the fields are moved out.
    let auth = config.encoded();
    let server_address = config.server_address.clone();
    DockerCredentials {
        username: Some(config.username),
        password: Some(config.password),
        serveraddress: Some(server_address),
        auth: Some(auth),
        ..Default::default()
    }
}


/// `createBuildContext`: the uncompressed tar the daemon unpacks as the build
/// context.
fn create_build_context(
    ctx: &RunContext,
    context_dir: &Path,
    rel_dockerfile: &str,
) -> Result<Vec<u8>> {
    ctx.log_debug(&format!(
        "Creating archive for build context dir '{}' with relative dockerfile '{}'",
        context_dir.display(),
        rel_dockerfile
    ));

    // Upstream canonicalises the Dockerfile name to a platform-independent one
    // before it is matched against the ignore rules.
    let rel_dockerfile = to_slash(rel_dockerfile).into_owned();

    let excludes = read_dockerignore(context_dir)?;

    // If `.dockerignore` mentions `.dockerignore` or the Dockerfile then make
    // sure we send both files over to the daemon, because the Dockerfile is
    // needed no matter what and `.dockerignore` is needed to know whether
    // either one has to be removed afterwards.
    let mut includes = vec![".".to_string()];
    if is_ignored(DOCKERIGNORE, &excludes) || is_ignored(&rel_dockerfile, &excludes) {
        includes.push(DOCKERIGNORE.to_string());
        includes.push(rel_dockerfile);
    }

    tar_context(context_dir, &excludes, &includes)
}

/// `ignorefile.ReadAll`: `.dockerignore` as a list of patterns.
///
/// A missing file is an empty list, not a failure; any other read error is
/// returned.
fn read_dockerignore(context_dir: &Path) -> Result<Vec<String>> {
    let path = context_dir.join(DOCKERIGNORE);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(anyhow!("Cannot read '{}': {err}", path.display())),
    };
    Ok(parse_dockerignore(&contents))
}

/// `ignorefile.ReadAll` over a string already in memory.
///
/// The rules, in upstream's order: a UTF-8 BOM is stripped from the first line,
/// a `#` line is a comment, surrounding whitespace goes, a `!` is held aside
/// while the rest is cleaned and unrooted, and the `!` goes back on.
fn parse_dockerignore(contents: &str) -> Vec<String> {
    let mut excludes = Vec::new();
    for (index, line) in contents.split('\n').enumerate() {
        // `bufio.Scanner` drops the carriage return of a CRLF file.
        let mut line = line.strip_suffix('\r').unwrap_or(line);
        if index == 0 {
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
        }
        // Comments are skipped before trimming, so an indented `#` is still a
        // comment.
        if line.starts_with('#') {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (invert, rest) = match trimmed.strip_prefix('!') {
            Some(rest) => (true, rest.trim()),
            None => (false, trimmed),
        };
        let pattern = if rest.is_empty() {
            String::new()
        } else {
            // A leading `/` is dropped, so `/some/path` and `some/path` are the
            // same rule.
            to_slash(&path_clean(rest))
                .trim_start_matches('/')
                .to_string()
        };
        excludes.push(if invert {
            format!("!{pattern}")
        } else {
            pattern
        });
    }
    excludes
}

/// `patternmatcher.Matches(file, patterns)`: does an ignore rule cover `file`?
///
/// `"."` is never excluded ("don't let them exclude everything, kind of
/// silly"), and a pattern list that does not compile is `false` — act writes
/// `keepThem1, _ :=` and throws the error away.
fn is_ignored(file: &str, patterns: &[String]) -> bool {
    let Ok(matcher) = PatternMatcher::new(patterns.to_vec()) else {
        return false;
    };
    let file = path_clean(file);
    if file == "." {
        return false;
    }
    matcher.matches(&file)
}

/// `archive.TarWithOptions(contextDir, &archive.TarOptions{…})` with
/// `Compression: archive.Uncompressed`.
///
/// Uncompressed because that is what act asks for: the daemon accepts a plain
/// tar as the request body, and compressing it first is work the transport
/// already does.
fn tar_context(src: &Path, excludes: &[String], includes: &[String]) -> Result<Vec<u8>> {
    let matcher = PatternMatcher::new(excludes.to_vec())?;
    let mut builder = tar::Builder::new(Vec::new());
    // `seen` is shared across includes, and that is what makes the explicit
    // `.dockerignore`/Dockerfile walk add only what the "." walk skipped.
    let mut seen: HashSet<String> = HashSet::new();

    for include in includes {
        let mut state = WalkState {
            include: include.clone(),
            parent_dirs: Vec::new(),
            parent_matches: Vec::new(),
        };
        walk_include(
            &mut builder,
            &matcher,
            src,
            &walk_root(src, include),
            &mut state,
            &mut seen,
        )?;
    }

    builder
        .into_inner()
        .map_err(|err| anyhow!("Cannot write the build context archive: {err}"))
}

/// `getWalkRoot`: `srcPath + "/" + include`.
///
/// `"."` walks the context root itself; anything else walks just that entry,
/// which is how an excluded Dockerfile gets in anyway.
fn walk_root(src: &Path, include: &str) -> PathBuf {
    if include == "." {
        src.to_path_buf()
    } else {
        src.join(include)
    }
}

/// What the walk remembers between two entries: the directories on the current
/// path, and per pattern, what the deepest of them decided.
///
/// go-archive threads the same two stacks so a directory that matched is not
/// re-tested for every file inside it.
struct WalkState {
    /// The include currently being walked, which is the one entry the exclude
    /// patterns are **not** consulted for.
    include: String,
    /// `parentDirs` upstream.
    parent_dirs: Vec<String>,
    /// `parentMatchInfo` upstream: one entry per pattern, per ancestor.
    parent_matches: Vec<Vec<bool>>,
}

/// `filepath.WalkDir` over one include, in the order Go visits it.
///
/// Directories are visited before their contents and siblings in name order,
/// which is what `WalkDir` does and what makes the archive reproducible.
fn walk_include(
    builder: &mut tar::Builder<Vec<u8>>,
    matcher: &PatternMatcher,
    src: &Path,
    root: &Path,
    state: &mut WalkState,
    seen: &mut HashSet<String>,
) -> Result<()> {
    // An include that names something other than a directory *is* that one
    // entry. This is the whole point of the explicit `.dockerignore` and
    // Dockerfile includes: the entry is an exact include, so no exclude pattern
    // is consulted for it at all.
    match fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.is_dir() => {
            let rel = rel_name(src, root);
            if state.include == rel && seen.insert(rel.clone()) {
                add_tar_file(builder, root, &rel)?;
            }
            return Ok(());
        }
        // A directory that cannot be listed is walked past, not fatal: a
        // context directory can change under a running build.
        Ok(_) => {}
        Err(_) => return Ok(()),
    }

    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    let mut names: Vec<(PathBuf, String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false);
        names.push((
            entry.path(),
            entry.file_name().to_string_lossy().into_owned(),
            is_dir,
        ));
    }
    names.sort_by(|left, right| left.1.cmp(&right.1));

    for (path, _name, is_dir) in names {
        let rel = rel_name(src, &path);
        let mut skip = false;

        // An exact include is asked for by name, so the exclude patterns are
        // not consulted at all. This is the rule that gets an ignored
        // Dockerfile and an ignored `.dockerignore` to the daemon.
        if state.include != rel {
            while let Some(parent) = state.parent_dirs.last() {
                if rel.starts_with(&format!("{parent}/")) {
                    break;
                }
                state.parent_dirs.pop();
                state.parent_matches.pop();
            }
            let parent = state.parent_matches.last().cloned().unwrap_or_default();
            let (matched, match_info) = matcher.matches_using_parent_results(&rel, &parent)?;
            if is_dir {
                state.parent_dirs.push(rel.clone());
                state.parent_matches.push(match_info);
            }
            skip = matched;
        }

        if skip {
            if !is_dir {
                continue;
            }
            // With no `!` rule anywhere, an excluded directory can be pruned
            // outright. With one, the directory stays in case something below
            // it is re-included.
            if !matcher.subtree_may_be_re_included(&rel) {
                continue;
            }
        }

        if !seen.insert(rel.clone()) {
            continue;
        }
        add_tar_file(builder, &path, &rel)?;

        if is_dir {
            walk_include(builder, matcher, src, &path, state, seen)?;
        }
    }

    Ok(())
}

/// `path` relative to `src`, with `/` separators — the tar member name.
fn rel_name(src: &Path, path: &Path) -> String {
    match path.strip_prefix(src) {
        Ok(rest) => rest
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// `addTarFile`: one header, plus the contents for a regular file.
///
/// A symlink is archived as a symlink and never followed. Anything that is not
/// a file, directory or symlink is dropped, which is what upstream does when
/// `FileInfoHeader` refuses the type: the error is logged and the walk carries
/// on.
fn add_tar_file(builder: &mut tar::Builder<Vec<u8>>, path: &Path, name: &str) -> Result<()> {
    // `os.Lstat`, so a symlink to a directory is a symlink.
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let file_type = metadata.file_type();

    let mut header = tar::Header::new_gnu();
    header.set_mtime(mtime_seconds(&metadata));
    // `tarheader.FileInfoHeader` fills these from `lstat` on Unix and leaves
    // them zero everywhere else, Windows included.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        header.set_uid(metadata.uid().into());
        header.set_gid(metadata.gid().into());
    }

    if file_type.is_symlink() {
        let Ok(target) = fs::read_link(path) else {
            return Ok(());
        };
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name(target.to_string_lossy().as_ref())?;
        return builder
            .append_data(&mut header, name, std::io::empty())
            .map_err(|err| archive_error(name, err));
    }

    if file_type.is_dir() {
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(permission_bits(&metadata));
        // Go's `FileInfoHeader` appends the separator to a directory's name.
        return builder
            .append_data(&mut header, format!("{name}/"), std::io::empty())
            .map_err(|err| archive_error(name, err));
    }

    if !file_type.is_file() {
        return Ok(());
    }

    let Ok(contents) = fs::read(path) else {
        return Ok(());
    };
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(contents.len() as u64);
    header.set_mode(permission_bits(&metadata));
    builder
        .append_data(&mut header, name, std::io::Cursor::new(contents))
        .map_err(|err| archive_error(name, err))
}

/// The mode bits `FileInfoHeader` copies from `lstat`.
#[cfg(unix)]
fn permission_bits(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o777
}

/// Windows has no permission bits to copy; the daemon does not act on them for
/// a build context.
#[cfg(not(unix))]
fn permission_bits(_metadata: &fs::Metadata) -> u32 {
    0o644
}

/// `hdr.ModTime` in the whole seconds a tar header stores.
///
/// A timestamp the filesystem cannot express — or one before the epoch — is
/// written as zero rather than dropped, because dropping it is not a thing Go's
/// header writer can do either.
fn mtime_seconds(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since_epoch| since_epoch.as_secs())
}

/// The error go-archive logs and carries on from.
fn archive_error(name: &str, err: std::io::Error) -> anyhow::Error {
    anyhow!("Cannot write '{name}' to the build context archive: {err}")
}

/// `patternmatcher.PatternMatcher` v0.6.0: the `.dockerignore` rules, and the
/// only thing that decides what is left out of the archive.
///
/// Ported whole because the semantics are not gitignore's: a pattern is
/// matched against the **whole** slash-separated path, a parent directory that
/// matched takes its subtree with it, and `**` is the one wildcard that crosses
/// a separator.
#[derive(Debug)]
struct PatternMatcher {
    /// `patterns`, in file order. The order is load-bearing: the first pattern
    /// that decides wins, so a later `!` can undo an earlier one.
    patterns: Vec<IgnorePattern>,
    /// `exclusions`: whether any pattern is a `!` rule. It is what turns a
    /// skipped directory from a prune into a walk.
    exclusions: bool,
}

impl PatternMatcher {
    /// `patternmatcher.New`.
    ///
    /// Every pattern is trimmed, dropped if empty, `path.Clean`ed, split into
    /// `!` + rest, and syntax-checked with `filepath.Match(p, ".")`. A bad
    /// pattern is an error, which is how a malformed `.dockerignore` reaches
    /// the user instead of being half-applied.
    fn new(patterns: Vec<String>) -> Result<Self> {
        let mut compiled = Vec::new();
        let mut exclusions = false;

        for raw in patterns {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut pattern = path_clean(trimmed);
            let mut exclusion = false;
            if pattern.starts_with('!') {
                if pattern.len() == 1 {
                    return Err(anyhow!("illegal exclusion pattern: \"!\""));
                }
                exclusion = true;
                exclusions = true;
                pattern = pattern[1..].to_string();
            }
            if matches!(gomatch::match_path(&pattern, "."), MatchResult::BadPattern) {
                return Err(anyhow!("syntax error in pattern: {pattern}"));
            }
            let dirs = pattern.split('/').map(str::to_string).collect();
            let match_kind = MatchKind::compile(&pattern)?;
            compiled.push(IgnorePattern {
                cleaned: pattern,
                dirs,
                exclusion,
                match_kind,
            });
        }

        Ok(Self {
            patterns: compiled,
            exclusions,
        })
    }

    /// `Matches`: the single-parent form, which is the **deprecated** one — and
    /// the one `docker_build.go` calls twice.
    ///
    /// ```go
    /// keepThem1, _ := patternmatcher.Matches(".dockerignore", excludes)
    /// ```
    ///
    /// Its parent check only re-tests as many leading components as the pattern
    /// has, so `Dockerfile` (one component) is tried against `sub` for a
    /// `sub/Dockerfile` and does not match. A nested Dockerfile excluded by name
    /// therefore does **not** get force-included, while a bare `Dockerfile` rule
    /// does. That is upstream, kept.
    fn matches(&self, file: &str) -> bool {
        let mut matched = false;
        let parent_path = path_dir(file);
        let parent_dirs: Vec<&str> = parent_path.split('/').collect();

        for pattern in &self.patterns {
            // An inclusion is skipped once something matched, and an exclusion is
            // skipped until something did.
            if pattern.exclusion != matched {
                continue;
            }
            let mut match_here = pattern.matches(file);
            // The single-parent probe: as many leading components as the
            // pattern has, and no deeper.
            if !match_here && parent_path != "." && pattern.dirs.len() <= parent_dirs.len() {
                match_here = pattern.matches(&parent_dirs[..pattern.dirs.len()].join("/"));
            }
            if match_here {
                matched = !pattern.exclusion;
            }
        }
        matched
    }

    /// `MatchesUsingParentResults`: the form the tar walk uses.
    ///
    /// `parent` is what the enclosing directory decided, one entry per pattern.
    /// Empty means "nothing known", and that is the only case where the parent
    /// components are re-tested from scratch.
    fn matches_using_parent_results(
        &self,
        file: &str,
        parent: &[bool],
    ) -> Result<(bool, Vec<bool>)> {
        if !parent.is_empty() && parent.len() != self.patterns.len() {
            return Err(anyhow!("wrong number of values in parentMatched"));
        }
        let mut matched = false;
        let mut match_info = vec![false; self.patterns.len()];

        for (index, pattern) in self.patterns.iter().enumerate() {
            let mut match_here = false;
            if !parent.is_empty() {
                match_here = parent[index];
            }
            if !match_here {
                if pattern.exclusion != matched {
                    continue;
                }
                match_here = pattern.matches(file);
                if !match_here && parent.is_empty() {
                    let parent_path = path_dir(file);
                    if parent_path != "." {
                        let dirs: Vec<&str> = parent_path.split('/').collect();
                        for depth in 0..dirs.len() {
                            match_here = pattern.matches(&dirs[..=depth].join("/"));
                            if match_here {
                                break;
                            }
                        }
                    }
                }
            }
            match_info[index] = match_here;
            if match_here {
                matched = !pattern.exclusion;
            }
        }
        Ok((matched, match_info))
    }

    /// `pm.Exclusions()` plus go-archive's "is any exclusion rule under this
    /// directory" probe.
    ///
    /// False means the walk may prune the directory outright. True means it
    /// must be entered anyway, because something below it might be
    /// re-included.
    fn subtree_may_be_re_included(&self, dir: &str) -> bool {
        if !self.exclusions {
            return false;
        }
        let dir_slash = format!("{dir}/");
        self.patterns
            .iter()
            .filter(|pattern| pattern.exclusion)
            .any(|pattern| format!("{}/", pattern.cleaned).starts_with(&dir_slash))
    }
}

/// `patternmatcher.Pattern`: one rule.
#[derive(Debug)]
struct IgnorePattern {
    /// `cleanedPattern`: `path.Clean`ed, `!` stripped.
    cleaned: String,
    /// `dirs`: `cleaned` split on `/`, used by the single-parent probe.
    dirs: Vec<String>,
    /// `exclusion`: a `!` rule, which re-includes rather than excludes.
    exclusion: bool,
    /// The compiled form. Every `Pattern.match` switch arm lives here.
    match_kind: MatchKind,
}

impl IgnorePattern {
    /// `Pattern.match`: one switch, four arms.
    fn matches(&self, path: &str) -> bool {
        match &self.match_kind {
            MatchKind::Exact => path == self.cleaned,
            MatchKind::Prefix => {
                path.starts_with(self.cleaned.strip_suffix("**").unwrap_or(&self.cleaned))
            }
            MatchKind::Suffix => {
                let suffix = self.cleaned.strip_prefix("**").unwrap_or(&self.cleaned);
                // `**/foo` has to match `foo` as well as `a/b/foo`, which is the
                // second arm of upstream's suffix case.
                path.ends_with(suffix)
                    || (suffix.starts_with('/') && path == suffix.trim_start_matches('/'))
            }
            MatchKind::Regex(re) => re.is_match(path),
        }
    }
}

/// `patternmatcher`'s `matchType`: the four shapes `Pattern.compile` picks.
///
/// The distinction is not cosmetic. A pattern with no wildcard is compared with
/// `==` and never reaches a regular expression, which is why `Dockerfile` costs
/// nothing to test against a hundred thousand paths, and why `patternmatcher`
/// behaves unlike gitignore on a name that merely *contains* a rule's text.
#[derive(Debug)]
enum MatchKind {
    /// `path == cleanedPattern`.
    Exact,
    /// The pattern ended in `**`: `strings.HasPrefix(path, pattern[:len-2])`.
    Prefix,
    /// The pattern started with `**`: a suffix test, plus the `**/foo` → `foo`
    /// special case.
    Suffix,
    /// Anything with `*`, `?`, `[`, `]` or `\` in it.
    Regex(Regex),
}

/// The three shorthand forms, as `matchType` holds them while a pattern is being
/// compiled — before it is known whether the pattern needs the regular
/// expression, which is what Go's `matchType = regexpMatch` flag tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shorthand {
    /// Nothing wildcard-ish seen yet.
    Exact,
    /// A trailing `**` with nothing wildcard-ish before it.
    Prefix,
    /// A leading `**`.
    Suffix,
}

impl MatchKind {
    /// `Pattern.compile`, including the bookkeeping that decides which of the
    /// four arms the pattern ends up in.
    ///
    /// `**` is the one wildcard that crosses a separator: `*` and `?` become
    /// `[^/]`, and `**/` eats its own slash so `a/**/b` matches `a/b`.
    fn compile(pattern: &str) -> Result<Self> {
        let mut shorthand = Shorthand::Exact;
        let mut needs_regex = false;
        let mut expression = String::from("^");
        let chars: Vec<char> = pattern.chars().collect();
        // `position` walks the pattern; `index` counts the characters the outer
        // loop has consumed, which is what the `i == 0` test below is about.
        let mut position = 0usize;
        let mut index = 0usize;

        while position < chars.len() {
            let ch = chars[position];
            position += 1;

            if ch == '*' {
                if chars.get(position) == Some(&'*') {
                    position += 1;
                    // Treat `**/` as `**`, so the slash is eaten.
                    if chars.get(position) == Some(&'/') {
                        position += 1;
                    }
                    if position >= chars.len() {
                        if shorthand == Shorthand::Exact {
                            shorthand = Shorthand::Prefix;
                        } else {
                            // Something wildcard-ish came first, so this is a
                            // real expression — and `**` at the end accepts
                            // everything, as .gitignore requires.
                            expression.push_str(".*");
                            needs_regex = true;
                        }
                    } else {
                        expression.push_str("(.*/)?");
                        needs_regex = true;
                    }
                    // A leading `**` makes it a suffix match, whatever came
                    // before it — which is only ever "nothing".
                    if index == 0 {
                        shorthand = Shorthand::Suffix;
                    }
                } else {
                    expression.push_str("[^/]*");
                    needs_regex = true;
                }
            } else if ch == '?' {
                expression.push_str("[^/]");
                needs_regex = true;
            } else if ch == '\\' {
                // A trailing backslash is kept and escaped; an escape consumes
                // the next character.
                match chars.get(position) {
                    Some(next) => {
                        expression.push('\\');
                        expression.push(*next);
                        position += 1;
                        needs_regex = true;
                    }
                    None => expression.push('\\'),
                }
            } else if should_escape(ch) {
                // Regexp metacharacters that `filepath.Match` treats as
                // ordinary characters.
                expression.push('\\');
                expression.push(ch);
            } else if ch == '[' || ch == ']' {
                expression.push(ch);
                needs_regex = true;
            } else {
                expression.push(ch);
            }
            index += 1;
        }

        if needs_regex {
            expression.push('$');
            let re = Regex::new(&expression)
                .map_err(|err| anyhow!("cannot compile pattern '{pattern}': {err}"))?;
            return Ok(MatchKind::Regex(re));
        }
        Ok(match shorthand {
            Shorthand::Exact => MatchKind::Exact,
            Shorthand::Prefix => MatchKind::Prefix,
            Shorthand::Suffix => MatchKind::Suffix,
        })
    }
}

/// The characters `shouldEscape` covers: regexp metacharacters that are *not*
/// `filepath.Match` metacharacters.
fn should_escape(ch: char) -> bool {
    matches!(ch, '.' | '+' | '(' | ')' | '|' | '{' | '}' | '$')
}

/// `filepath.ToSlash`: the platform separator becomes `/`, and on Unix that is
/// the identity.
fn to_slash(path: &str) -> Cow<'_, str> {
    if std::path::MAIN_SEPARATOR == '/' {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(path.replace('\\', "/"))
    }
}

/// `path.Clean`: the shortest equivalent path.
fn path_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let bytes = path.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    // Trimmed back to this index is the root (or nothing, unrooted), so a `..`
    // cannot escape it.
    let mut dotdot = if rooted { 1 } else { 0 };
    if rooted {
        out.push(b'/');
    }
    let mut read = 0usize;
    while read < bytes.len() {
        // Upstream's first two cases both advance by one and both mean "this
        // element does not survive": a separator, or a `.`.
        if bytes[read] == b'/'
            || (bytes[read] == b'.' && (read + 1 == bytes.len() || bytes[read + 1] == b'/'))
        {
            read += 1;
        } else if bytes[read] == b'.'
            && bytes.get(read + 1) == Some(&b'.')
            && (read + 2 == bytes.len() || bytes[read + 2] == b'/')
        {
            read += 2;
            if out.len() > dotdot {
                out.pop();
                while out.len() > dotdot && *out.last().expect("a non-empty path") != b'/' {
                    out.pop();
                }
            } else if !rooted {
                if !out.is_empty() {
                    out.push(b'/');
                }
                out.push(b'.');
                out.push(b'.');
                dotdot = out.len();
            }
        } else {
            if out.len() != usize::from(rooted) {
                out.push(b'/');
            }
            while read < bytes.len() && bytes[read] != b'/' {
                out.push(bytes[read]);
                read += 1;
            }
        }
    }
    if out.is_empty() {
        return ".".to_string();
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `path.Dir`: everything above the last separator.
fn path_dir(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut start = bytes.len();
    while start > 0 && bytes[start - 1] != b'/' {
        start -= 1;
    }
    path_clean(&path[..start])
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::common::{CollectingSink, Level};

    /// A context directory shaped like the one the tests need: a Dockerfile the
    /// ignore file excludes, a file it must keep, a file it must drop, and a
    /// subdirectory.
    fn context_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        fs::create_dir(root.join("sub")).expect("a subdirectory");
        fs::write(root.join("Dockerfile"), "FROM scratch\n").expect("a Dockerfile");
        fs::write(root.join("keep.txt"), "keep\n").expect("a kept file");
        fs::write(root.join("secret.env"), "TOKEN=x\n").expect("an ignored file");
        fs::write(root.join("sub").join("nested.txt"), "nested\n").expect("a nested file");
        fs::write(root.join(DOCKERIGNORE), "Dockerfile\nsecret.env\n").expect("a .dockerignore");
        dir
    }

    /// The member names in the archive, directories without their separator.
    fn member_names(archive: &[u8]) -> Vec<String> {
        let mut tar = tar::Archive::new(std::io::Cursor::new(archive));
        let mut names = Vec::new();
        for entry in tar.entries().expect("a readable archive") {
            let entry = entry.expect("a readable entry");
            let mut name = entry
                .path()
                .expect("a utf-8 member name")
                .to_string_lossy()
                .into_owned();
            if let Some(stripped) = name.strip_suffix('/') {
                name = stripped.to_string();
            }
            names.push(name);
        }
        names
    }

    #[test]
    fn an_excluded_dockerfile_is_still_sent_to_the_daemon() {
        let dir = context_dir();
        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");

        // `.dockerignore` excludes `Dockerfile` and it is in the archive anyway,
        // because the daemon needs it to know what to remove.
        assert!(
            member_names(&archive).contains(&"Dockerfile".to_string()),
            "{:?}",
            member_names(&archive)
        );
    }

    #[test]
    fn an_ignored_file_is_left_out_of_the_archive() {
        let dir = context_dir();
        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");
        let names = member_names(&archive);

        assert!(!names.contains(&"secret.env".to_string()), "{names:?}");
        // Everything the rules do not mention is in, subdirectory included.
        assert!(names.contains(&"keep.txt".to_string()), "{names:?}");
        assert!(names.contains(&"sub".to_string()), "{names:?}");
        assert!(names.contains(&"sub/nested.txt".to_string()), "{names:?}");
    }

    #[test]
    fn the_ignore_file_itself_is_sent_alongside_the_dockerfile() {
        let dir = context_dir();
        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");

        // Both are added when *either* one is excluded — and neither is added
        // when neither is, which the missing-ignore-file test covers.
        assert!(
            member_names(&archive).contains(&DOCKERIGNORE.to_string()),
            "{:?}",
            member_names(&archive)
        );
    }

    #[test]
    fn an_ignore_file_that_excludes_itself_is_still_sent_exactly_once() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::write(dir.path().join("Dockerfile"), "FROM scratch\n").expect("a Dockerfile");
        fs::write(dir.path().join(DOCKERIGNORE), ".dockerignore\nDockerfile\n")
            .expect("a .dockerignore");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");
        let names = member_names(&archive);

        // The "." walk skips it, the explicit include puts it back, and `seen`
        // keeps the explicit walk from adding it twice.
        assert_eq!(
            names.iter().filter(|name| *name == DOCKERIGNORE).count(),
            1,
            "{names:?}"
        );
        assert!(names.contains(&"Dockerfile".to_string()), "{names:?}");
    }

    #[test]
    fn a_missing_dockerignore_is_not_an_error() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::write(dir.path().join("Dockerfile"), "FROM scratch\n").expect("a Dockerfile");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");

        // With no rules at all `includes` stays `["."]` and nothing is forced.
        assert_eq!(member_names(&archive), ["Dockerfile"]);
    }

    #[test]
    fn a_backslash_dockerfile_path_is_left_alone_on_unix() {
        assert_eq!(
            to_slash(r"docker\Dockerfile"),
            if std::path::MAIN_SEPARATOR == '/' {
                Cow::Borrowed(r"docker\Dockerfile")
            } else {
                Cow::Owned("docker/Dockerfile".to_string())
            },
            "ToSlash replaces the platform separator and nothing else"
        );
    }

    #[test]
    fn only_a_platform_with_a_slash_reaches_the_daemon() {
        // A bare platform is dropped, not rejected — which is the difference
        // from `docker_engine`'s create.
        assert_eq!(split_build_platform(""), None);
        assert_eq!(split_build_platform("linux"), None);
        assert_eq!(
            split_build_platform("linux/amd64"),
            Some(BuildPlatform {
                os: "linux".to_string(),
                architecture: "amd64".to_string()
            })
        );
        // SplitN stops at the first `/`, so the variant stays inside the
        // architecture.
        assert_eq!(
            split_build_platform("linux/arm/v7"),
            Some(BuildPlatform {
                os: "linux".to_string(),
                architecture: "arm/v7".to_string()
            })
        );
    }

    #[test]
    fn the_platform_query_value_follows_format_platform() {
        let value =
            |platform: &str| split_build_platform(platform).map(|parsed| parsed.query_value());

        assert_eq!(value("linux/amd64").as_deref(), Some("linux/amd64"));
        assert_eq!(value("linux/arm/v7").as_deref(), Some("linux/arm/v7"));
        // An empty OS is `formatPlatform`'s "unknown", not an empty value.
        assert_eq!(value("/amd64").as_deref(), Some("unknown"));
        assert_eq!(value("linux/").as_deref(), Some("linux"));
    }

    #[test]
    fn the_build_line_says_what_the_daemon_was_asked() {
        let dir = Path::new("/tmp/context");

        assert_eq!(
            build_info_line("img:1", "", dir),
            format!("{LOG_PREFIX}docker build -t img:1 /tmp/context")
        );
        // The flag is present whenever the job named a platform, even one that
        // will be dropped later.
        assert_eq!(
            build_info_line("img:1", "linux/arm/v7", dir),
            format!("{LOG_PREFIX}docker build -t img:1 --platform linux/arm/v7 /tmp/context")
        );
    }

    #[test]
    fn a_dry_run_prints_the_command_and_stops() {
        let sink = Arc::new(CollectingSink::new());
        let ctx = RunContext::new().with_dryrun(true).with_sink(sink.clone());
        let executor = new_docker_build_executor(NewDockerBuildExecutorInput {
            context_dir: PathBuf::from("/tmp/context"),
            dockerfile: "Dockerfile".to_string(),
            image_tag: "img:1".to_string(),
            platform: String::new(),
        });

        executor(&ctx).expect("a dry run cannot fail");

        // No daemon and no daemon-facing work: the `Info` line is all of it.
        assert_eq!(
            sink.messages_at(Level::Info),
            [build_info_line("img:1", "", Path::new("/tmp/context"))]
        );
    }

    #[test]
    fn the_log_prefix_is_two_spaces_a_whale_and_two_spaces() {
        assert_eq!(LOG_PREFIX, "  \u{1F433}  ");
        assert_eq!(LOG_PREFIX.chars().count(), 5);
    }

    #[test]
    fn an_ignore_rule_excludes_the_whole_subtree_below_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::create_dir_all(dir.path().join("node_modules/pkg")).expect("a subtree");
        fs::write(dir.path().join("index.js"), "1\n").expect("a kept file");
        fs::write(dir.path().join("node_modules/pkg/i.js"), "1\n").expect("an ignored file");
        fs::write(dir.path().join(DOCKERIGNORE), "node_modules\n").expect("a .dockerignore");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");
        let names = member_names(&archive);

        assert!(names.contains(&"index.js".to_string()), "{names:?}");
        assert!(!names.contains(&"node_modules".to_string()), "{names:?}");
        assert!(
            !names.contains(&"node_modules/pkg/i.js".to_string()),
            "{names:?}"
        );
    }

    #[test]
    fn a_negated_rule_wins_when_it_is_later_in_the_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::write(dir.path().join("app.log"), "1\n").expect("an ignored file");
        fs::write(dir.path().join("keep.log"), "1\n").expect("a re-included file");
        fs::write(dir.path().join(DOCKERIGNORE), "*.log\n!keep.log\n").expect("a .dockerignore");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");
        let names = member_names(&archive);

        assert!(!names.contains(&"app.log".to_string()), "{names:?}");
        assert!(names.contains(&"keep.log".to_string()), "{names:?}");
    }

    #[test]
    fn a_double_star_rule_matches_at_any_depth() {
        let matcher = PatternMatcher::new(vec!["**/target".to_string()]).expect("a valid rule");
        let excluded = |name: &str| {
            matcher
                .matches_using_parent_results(name, &[])
                .expect("a match")
                .0
        };

        assert!(excluded("target"));
        assert!(excluded("crate/target"));
        assert!(excluded("a/b/c/target"));
        assert!(!excluded("crate/targets"));
        assert!(!excluded("targets"));
    }

    #[test]
    fn a_star_does_not_cross_a_separator() {
        let matcher = PatternMatcher::new(vec!["src/*.rs".to_string()]).expect("a valid rule");
        let excluded = |name: &str| {
            matcher
                .matches_using_parent_results(name, &[])
                .expect("a match")
                .0
        };

        assert!(excluded("src/main.rs"));
        assert!(!excluded("src/deep/main.rs"));
    }

    #[test]
    fn a_rule_without_a_slash_is_anchored_at_the_context_root() {
        // This is where the ported matcher and `crate::gitignore` differ: a
        // gitignore rule would match `sub/node_modules` too.
        let matcher = PatternMatcher::new(vec!["node_modules".to_string()]).expect("a valid rule");
        let excluded = |name: &str| {
            matcher
                .matches_using_parent_results(name, &[])
                .expect("a match")
                .0
        };

        assert!(excluded("node_modules"));
        assert!(!excluded("sub/node_modules"));
    }

    #[test]
    fn the_single_parent_probe_only_looks_at_one_ancestor() {
        let matcher = PatternMatcher::new(vec!["Dockerfile".to_string()]).expect("a valid rule");

        // The form `docker_build.go` calls: a nested Dockerfile is NOT matched,
        // because the pattern's one component is tested against `sub`.
        assert!(!matcher.matches("sub/Dockerfile"));
        assert!(matcher.matches("Dockerfile"));
    }

    #[test]
    fn a_rule_can_never_exclude_the_context_root_itself() {
        // "Don't let them exclude everything, kind of silly."
        assert!(!is_ignored(".", &["*".to_string()]));
    }

    #[test]
    fn a_malformed_rule_is_an_error_rather_than_a_half_applied_ignore() {
        let error = PatternMatcher::new(vec!["src/[unclosed".to_string()])
            .expect_err("an unterminated class is a bad pattern");

        assert!(
            error.to_string().contains("syntax error in pattern"),
            "{error}"
        );
        // And `is_ignored` swallows it, because act discards that error.
        assert!(!is_ignored("Dockerfile", &["src/[unclosed".to_string()]));
    }

    #[test]
    fn the_ignore_file_is_read_with_boms_comments_and_whitespace_handled() {
        let patterns =
            parse_dockerignore("\u{feff}# a comment\n  secret.env  \n\n/build\ndir/\n!keep\n");

        assert_eq!(patterns, ["secret.env", "build", "dir", "!keep"]);
    }

    #[test]
    fn path_clean_and_dir_are_go_semantics() {
        assert_eq!(path_clean(""), ".");
        assert_eq!(path_clean("a//b/"), "a/b");
        assert_eq!(path_clean("a/./b"), "a/b");
        assert_eq!(path_clean("a/../b"), "b");
        assert_eq!(path_clean("../b"), "../b");
        assert_eq!(path_clean("/a/../../b"), "/b");
        assert_eq!(path_dir("sub/Dockerfile"), "sub");
        assert_eq!(path_dir("Dockerfile"), ".");
        assert_eq!(path_dir("a/b/c"), "a/b");
    }

    #[test]
    fn an_excluded_directory_is_kept_when_a_negated_rule_reaches_into_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::create_dir(dir.path().join("node_modules")).expect("a directory");
        fs::write(dir.path().join("node_modules/keep.txt"), "1\n").expect("a re-included file");
        fs::write(dir.path().join("node_modules/junk.js"), "1\n").expect("an ignored file");
        fs::write(
            dir.path().join(DOCKERIGNORE),
            "node_modules\n!node_modules/keep.txt\n",
        )
        .expect("a .dockerignore");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");
        let names = member_names(&archive);

        // The directory is walked rather than pruned, because an exclusion rule
        // could still re-include something inside it.
        assert!(names.contains(&"node_modules".to_string()), "{names:?}");
        assert!(
            names.contains(&"node_modules/keep.txt".to_string()),
            "{names:?}"
        );
        assert!(
            !names.contains(&"node_modules/junk.js".to_string()),
            "{names:?}"
        );
    }

    #[test]
    fn a_build_line_carries_the_keys_the_daemon_logger_reads() {
        use bollard::models::{BuildInfo, ErrorDetail};

        let line = encode_build_info(&BuildInfo {
            id: Some("sha256:abc".to_string()),
            stream: Some("Step 1/2 : FROM scratch\n".to_string()),
            status: Some("Pulling from library/alpine".to_string()),
            ..Default::default()
        });
        let decoded: serde_json::Value = serde_json::from_str(&line).expect("the line is JSON");

        assert_eq!(decoded["id"], "sha256:abc");
        assert_eq!(decoded["stream"], "Step 1/2 : FROM scratch\n");
        assert_eq!(decoded["status"], "Pulling from library/alpine");
        // A field the daemon did not send stays absent rather than becoming an
        // empty string.
        assert!(decoded.get("errorDetail").is_none(), "{decoded}");

        // And the failure shape, which is the one that decides pass or fail.
        let line = encode_build_info(&BuildInfo {
            error_detail: Some(ErrorDetail {
                message: Some("no such file".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let decoded: serde_json::Value = serde_json::from_str(&line).expect("the line is JSON");

        assert_eq!(decoded["errorDetail"]["message"], "no such file");
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_is_archived_as_a_symlink_and_not_followed() {
        let dir = tempfile::tempdir().expect("a temp dir");
        fs::create_dir(dir.path().join("sub")).expect("a directory");
        fs::write(dir.path().join("sub").join("i.js"), "1\n").expect("a file");
        std::os::unix::fs::symlink("sub", dir.path().join("link"))
            .expect("a symlink to a directory");

        let archive =
            create_build_context(&RunContext::new(), dir.path(), "Dockerfile").expect("an archive");

        let mut tar = tar::Archive::new(std::io::Cursor::new(&archive));
        let entry = tar
            .entries()
            .expect("a readable archive")
            .find_map(|entry| {
                let entry = entry.expect("a readable entry");
                let name = entry.path().expect("a path").to_string_lossy().into_owned();
                (name == "link").then_some(entry)
            })
            .expect("the symlink is in the archive");

        // Following it would have written `link/i.js` instead.
        assert!(entry.header().entry_type().is_symlink());
        assert!(!member_names(&archive).contains(&"link/i.js".to_string()));
    }
}
