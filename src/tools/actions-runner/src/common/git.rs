//! Reading a repository's state, and recognising where a remote URL points.
//!
//! Port of the lookup half of act's `pkg/common/git` (417 lines). The clone
//! executor (`CloneIfRequired` and `NewGitCloneExecutor`, 196 of those lines) is
//! **not** here: it belongs with `action_cache` and the remote-action steps, and
//! it arrives with them.
//!
//! Upstream reaches git through `go-git`; this port uses `gix`, the maintained
//! Rust implementation, which the crate already depends on for `git_index`. The
//! difference that matters is `DetectDotGit` / `EnableDotGitCommonDir`: upstream
//! walks up from the given path looking for a repository and follows a `.git`
//! *file* to the common directory, which is what makes a worktree or a
//! submodule resolve. [`gix::discover`] does both.
//!
//! # The slug rules, and why a greedy prefix matters
//!
//! [`find_git_slug`] recognises four URL shapes. The GitHub one looks harmless:
//!
//! ```text
//! ^https?://.*github.com.*/(.+)/(.+?)(?:.git)?$
//! ```
//!
//! It is not. Both `.*` are greedy, and the one after `github.com` swallows as
//! much as it can, so what is left for `(.+)` is the *shortest* possible owner.
//! Measured against the Go function:
//!
//! | URL | owner | repo |
//! |---|---|---|
//! | `https://github.com/a/b/c.git` | `a` | `b/c` |
//! | `https://github.com/a/b.git/c.git` | `a` | `b.git/c` |
//! | `https://github.com/a/b.c.git` | `a` | `b.c` |
//!
//! The GitHub Enterprise pattern has **no** `.*` in front, so there `(.+)` is
//! greedy and wins: the same `https://<instance>/a/b/c.git` gives `a/b`. Two
//! nearly identical patterns, two different owners, and a port that "cleans
//! this up" produces a different repository for every nested path. Both are
//! pinned below against measured output.
//!
//! Two more rules that a careful reader would get wrong:
//!
//! * The `.git` suffix is stripped **case-sensitively** and only **once**:
//!   `repo.GIT` keeps its suffix, `act.git.git` becomes `act.git`.
//! * A trailing slash is **not** stripped: `https://github.com/a/b/` is
//!   `a/b/`.
//!
//! # Tag wins over branch, and it is checked last
//!
//! [`find_git_ref`] cannot tell from HEAD alone whether a checkout is a branch
//! or a tag, because a tag checkout leaves HEAD pointing at a commit hash. It
//! therefore walks **all** references looking for one whose object matches, and
//! keeps looking after a branch matches — a tag with the same object may still
//! come later in the iteration. A tag therefore wins even when the branch was
//! found first.

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use std::sync::OnceLock;

fn code_commit_http_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^https?://git-codecommit\.(.+)\.amazonaws\.com/v1/repos/(.+)$")
            .expect("static regex must compile")
    })
}

fn code_commit_ssh_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"ssh://git-codecommit\.(.+)\.amazonaws\.com/v1/repos/(.+)$")
            .expect("static regex must compile")
    })
}

fn github_http_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^https?://.*github\.com.*/(.+)/(.+?)(?:\.git)?$")
            .expect("static regex must compile")
    })
}

fn github_ssh_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"github\.com[:/](.+)/(.+?)(?:\.git)?$").expect("static regex must compile")
    })
}

/// The provider a remote URL points at, and the `owner/repo` slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug {
    /// `CodeCommit`, `GitHub`, `GitHubEnterprise`, or empty when the URL matched
    /// no known shape.
    pub provider: String,
    /// The `owner/repo`, or the URL unchanged when nothing matched.
    pub slug: String,
}

impl Slug {
    /// True when a known provider recognised the URL.
    pub fn is_known(&self) -> bool {
        !self.provider.is_empty()
    }
}

/// `findGitSlug`: recognise a remote URL.
///
/// The order is load-bearing: CodeCommit is checked before GitHub because a
/// CodeCommit URL also contains `github`-free but could otherwise be caught by
/// a looser pattern. The GitHub Enterprise patterns are only compiled at all
/// when the instance is not `github.com` — so with the default instance a GHE
/// URL deliberately does **not** resolve, which is what stops an enterprise
/// remote from being reported as a github.com repository.
pub fn find_git_slug(url: &str, github_instance: &str) -> Slug {
    let group = |pattern: &Regex| -> Option<(String, String)> {
        let captures = pattern.captures(url)?;
        Some((captures[1].to_string(), captures[2].to_string()))
    };

    if let Some((_, repo)) = group(code_commit_http_regex()) {
        return Slug {
            provider: "CodeCommit".to_string(),
            slug: repo,
        };
    }
    if let Some((_, repo)) = group(code_commit_ssh_regex()) {
        return Slug {
            provider: "CodeCommit".to_string(),
            slug: repo,
        };
    }
    if let Some((owner, repo)) = group(github_http_regex()) {
        return Slug {
            provider: "GitHub".to_string(),
            slug: format!("{owner}/{repo}"),
        };
    }
    if let Some((owner, repo)) = group(github_ssh_regex()) {
        return Slug {
            provider: "GitHub".to_string(),
            slug: format!("{owner}/{repo}"),
        };
    }
    if github_instance != "github.com" {
        // Upstream builds these with `fmt.Sprintf`, so the instance is not a
        // regex-escaped literal there either. `regex::escape` is applied anyway:
        // an instance with a `.` in it is normal, and an instance with a `+` in
        // it should not silently change what matches.
        let instance = regex::escape(github_instance);
        let http = Regex::new(&format!(r"^https?://{instance}/(.+)/(.+?)(?:\.git)?$"))
            .expect("an instance with escaping cannot break the pattern");
        let ssh = Regex::new(&format!(r"{instance}[:/](.+)/(.+?)(?:\.git)?$"))
            .expect("an instance with escaping cannot break the pattern");
        for pattern in [&http, &ssh] {
            if let Some((owner, repo)) = group(pattern) {
                return Slug {
                    provider: "GitHubEnterprise".to_string(),
                    slug: format!("{owner}/{repo}"),
                };
            }
        }
    }
    Slug {
        provider: String::new(),
        slug: url.to_string(),
    }
}

/// `findGitRemoteURL`: the repository's first URL for `remote_name`.
pub fn find_git_remote_url(repo_path: &str, remote_name: &str) -> Result<String> {
    let repository = gix::discover(repo_path)
        .with_context(|| format!("opening the repository at {repo_path}"))?;
    let remote = repository
        .find_remote(remote_name)
        .with_context(|| format!("remote {remote_name:?} not found"))?;
    let url = remote
        .url(gix::remote::Direction::Fetch)
        .ok_or_else(|| anyhow!("remote '{remote_name}' exists but has no URL"))?;
    Ok(url.to_bstring().to_string())
}

/// `FindGithubRepo`: the `owner/repo` a repository's remote points at.
///
/// An empty `remote_name` means `origin`. A URL that matches no known shape
/// comes back as the URL itself, with an empty provider — which is what
/// `github.repository` then shows, and is better than an empty string.
pub fn find_github_repo(
    repo_path: &str,
    github_instance: &str,
    remote_name: &str,
) -> Result<String> {
    let remote_name = if remote_name.is_empty() {
        "origin"
    } else {
        remote_name
    };
    let url = find_git_remote_url(repo_path, remote_name)?;
    Ok(find_git_slug(&url, github_instance).slug)
}

/// `FindGitRevision`: the checked-out commit, short and full.
pub fn find_git_revision(repo_path: &str) -> Result<(String, String)> {
    let repository = gix::discover(repo_path)
        .with_context(|| format!("opening the repository at {repo_path}"))?;
    let head = repository.head_id().context("resolving HEAD")?.detach();
    let hash = head.to_hex().to_string();
    if hash.chars().all(|c| c == '0') {
        bail!("HEAD sha1 could not be resolved");
    }
    // Upstream returns the first seven characters. A SHA-1 is 40 and a SHA-256
    // is 64, so the same seven is a prefix of either.
    let short = hash.chars().take(7).collect();
    Ok((short, hash))
}

/// `FindGitRef`: the tag or branch the checked-out commit is.
///
/// HEAD alone cannot say: a tag checkout leaves HEAD at a bare commit hash, so
/// every reference with that object is a candidate. A **tag** wins over a
/// branch, and the search does not stop at the first branch — a tag pointing at
/// the same commit may come later in the iteration.
pub fn find_git_ref(repo_path: &str) -> Result<String> {
    let repository = gix::discover(repo_path)
        .with_context(|| format!("opening the repository at {repo_path}"))?;
    let head_id = repository.head_id().context("resolving HEAD")?.detach();
    let head = head_id.to_hex().to_string();
    if head.chars().all(|c| c == '0') {
        bail!("HEAD sha1 could not be resolved");
    }

    let mut tag: Option<String> = None;
    let mut branch: Option<String> = None;
    for reference in repository
        .references()
        .context("listing references")?
        .all()
        .context("reading references")?
    {
        let Ok(reference): std::result::Result<gix::Reference<'_>, _> = reference else {
            continue;
        };
        let name = reference.name().as_bstr().to_string();
        let is_tag = name.starts_with("refs/tags/");
        let is_branch = name.starts_with("refs/heads/");
        if !is_tag && !is_branch {
            continue;
        }
        let Ok(id) = reference.clone().into_fully_peeled_id() else {
            continue;
        };
        // Compared as bytes: `to_hex` allocates, and this runs once per
        // reference in the repository.
        if id.detach().to_hex() != head.as_str() {
            continue;
        }
        if is_tag {
            tag = Some(name);
        } else {
            branch = Some(name);
        }
        // Upstream stops here, but only because a tag found after a branch still
        // wins — and the loop above keeps looking, which is the point.
        if tag.is_some() && branch.is_some() {
            break;
        }
    }

    // The order matters, and it is the reason the loop above does not stop at
    // the first match.
    if let Some(tag) = tag {
        return Ok(tag);
    }
    if let Some(branch) = branch {
        return Ok(branch);
    }
    bail!("failed to identify reference (tag/branch) for the checked-out revision '{head}'")
}

/// Short SHA references are not supported.
///
/// Declared here for the message, which `NewGitCloneExecutor` raises when a
/// workflow asks for a sha prefix. The executor itself is not ported yet; the
/// wording is kept so a port of it does not have to rediscover it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortRefError;

impl std::fmt::Display for ShortRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("short SHA references are not supported")
    }
}

impl std::error::Error for ShortRefError {}

/// No repository was found at the given path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoRepoError;

impl std::fmt::Display for NoRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unable to find git repo")
    }
}

impl std::error::Error for NoRepoError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    /// The measured upstream table: `(url, provider, slug)` for the default
    /// `github.com` instance. Every expectation is the Go function's output.
    const SLUGS: &[(&str, &str, &str)] = &[
        // --- TestFindGitSlug ---
        (
            "https://git-codecommit.us-east-1.amazonaws.com/v1/repos/my-repo-name",
            "CodeCommit",
            "my-repo-name",
        ),
        (
            "ssh://git-codecommit.us-west-2.amazonaws.com/v1/repos/my-repo",
            "CodeCommit",
            "my-repo",
        ),
        ("git@github.com:nektos/act.git", "GitHub", "nektos/act"),
        ("git@github.com:nektos/act", "GitHub", "nektos/act"),
        ("https://github.com/nektos/act.git", "GitHub", "nektos/act"),
        ("http://github.com/nektos/act.git", "GitHub", "nektos/act"),
        ("https://github.com/nektos/act", "GitHub", "nektos/act"),
        ("http://github.com/nektos/act", "GitHub", "nektos/act"),
        (
            "git+ssh://git@github.com/owner/repo.git",
            "GitHub",
            "owner/repo",
        ),
        (
            "http://myotherrepo.com/act.git",
            "",
            "http://myotherrepo.com/act.git",
        ),
        // --- the greedy prefix, measured ---
        ("https://github.com/a/b/c.git", "GitHub", "b/c"),
        ("https://github.com/a/b.git/c.git", "GitHub", "b.git/c"),
        ("https://github.com/a/b.c.git", "GitHub", "a/b.c"),
        ("https://github.com/a/b/", "GitHub", "a/b/"),
        // One `/` is not enough: the pattern needs `owner/repo`.
        ("https://github.com/a.git", "", "https://github.com/a.git"),
        ("https://github.com/a/b", "GitHub", "a/b"),
        ("github.com:a/b", "GitHub", "a/b"),
        ("github.com:", "", "github.com:"),
        ("github.com/", "", "github.com/"),
        // `.git` is stripped case-sensitively, and only once.
        (
            "https://github.com/owner/repo.GIT",
            "GitHub",
            "owner/repo.GIT",
        ),
        (
            "git@github.com:nektos/act.git.git",
            "GitHub",
            "nektos/act.git",
        ),
        // The `.*` before `github.com` absorbs credentials.
        (
            "https://user:token@github.com/owner/repo.git",
            "GitHub",
            "owner/repo",
        ),
        // `github.example.com` does not contain `github.com`, so the default
        // instance must not claim it.
        (
            "https://github.example.com/owner/repo.git",
            "",
            "https://github.example.com/owner/repo.git",
        ),
        ("git@github.com:nektos/act/sub", "GitHub", "nektos/act/sub"),
        ("ssh://git@github.com/owner/repo", "GitHub", "owner/repo"),
        ("file:///some/local/path", "", "file:///some/local/path"),
        ("/just/a/path", "", "/just/a/path"),
        ("", "", ""),
    ];

    /// `TestFindGitSlug`, plus the measured greedy-prefix cases.
    #[test]
    fn a_slug_is_recognised_from_its_url_shape() {
        for (url, provider, slug) in SLUGS {
            let found = find_git_slug(url, "github.com");
            assert_eq!(found.provider, *provider, "provider for {url:?}");
            assert_eq!(found.slug, *slug, "slug for {url:?}");
        }
    }

    /// The same table, and the reason a GHE pattern is only compiled for a
    /// non-default instance.
    #[test]
    fn an_enterprise_instance_gets_its_own_patterns() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "https://github.example.com/owner/repo.git",
                "GitHubEnterprise",
                "owner/repo",
            ),
            (
                "git@github.example.com:owner/repo.git",
                "GitHubEnterprise",
                "owner/repo",
            ),
            (
                "https://github.example.com/owner/repo",
                "GitHubEnterprise",
                "owner/repo",
            ),
            (
                "git@github.example.com:owner/repo",
                "GitHubEnterprise",
                "owner/repo",
            ),
            // The GitHub pattern still wins for a github.com URL, because it is
            // checked first and does not depend on the instance.
            ("https://github.com/owner/repo.git", "GitHub", "owner/repo"),
            (
                "http://github.example.com/act.git",
                "",
                "http://github.example.com/act.git",
            ),
        ];
        for (url, provider, slug) in cases {
            let found = find_git_slug(url, "github.example.com");
            assert_eq!(found.provider, *provider, "provider for {url:?}");
            assert_eq!(found.slug, *slug, "slug for {url:?}");
        }
    }

    /// The asymmetry the module docs warn about, and the reason a cleanup of the
    /// GitHub pattern would be a behaviour change rather than a fix.
    #[test]
    fn the_github_patterns_choose_different_owners_for_the_same_nesting() {
        let url = "https://github.com/a/b/c.git";
        let github = find_git_slug(url, "github.com");
        let enterprise =
            find_git_slug("https://github.example.com/a/b/c.git", "github.example.com");
        assert_eq!(
            github.slug, "b/c",
            "the greedy .* leaves the shortest owner"
        );
        assert_eq!(
            enterprise.slug, "a/b/c",
            "the GHE pattern has no .*, so the greedy group wins"
        );
    }

    /// A directory git cannot open, or one with no remote, is an error rather
    /// than a silent empty string.
    #[test]
    fn a_directory_that_is_not_a_repository_is_an_error() {
        let temp = tempfile::tempdir().expect("a temp dir");
        assert!(find_git_remote_url(temp.path().to_str().expect("a path"), "origin").is_err());
        assert!(find_git_revision(temp.path().to_str().expect("a path")).is_err());
        assert!(find_git_ref(temp.path().to_str().expect("a path")).is_err());
    }

    /// Runs a git command in `dir`, failing the test with its output.
    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "act")
            .env("GIT_AUTHOR_EMAIL", "act@example.com")
            .env("GIT_COMMITTER_NAME", "act")
            .env("GIT_COMMITTER_EMAIL", "act@example.com")
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A fresh repository on `master`, with the sample hooks removed so nothing
    /// in the environment can run during a commit.
    fn repo_with(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::Builder::new()
            .prefix("act-test-")
            .tempdir()
            .expect("a temp dir");
        let dir = temp.path().join(name);
        std::fs::create_dir_all(&dir).expect("the repo dir");
        git(&dir, &["init", "--initial-branch=master"]);
        let hooks = dir.join(".git").join("hooks");
        if let Ok(entries) = std::fs::read_dir(&hooks) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        (temp, dir)
    }

    /// `TestGitFindRef`: HEAD cannot say whether a checkout is a branch or a tag,
    /// so a tag has to be found by looking at what else points at the commit.
    #[test]
    fn the_ref_follows_what_head_points_at() {
        // A repository with no commit has no revision to name.
        {
            let (_temp, dir) = repo_with("new_repo");
            let path = dir.to_str().expect("a path");
            assert!(find_git_ref(path).is_err(), "no commit, no ref");
        }
        // A commit on a branch.
        {
            let (_temp, dir) = repo_with("new_repo_with_commit");
            git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
            let path = dir.to_str().expect("a path");
            assert_eq!(find_git_ref(path).expect("a ref"), "refs/heads/master");
        }
        // Checked out at a tag.
        {
            let (_temp, dir) = repo_with("current_head_is_tag");
            git(&dir, &["commit", "--allow-empty", "-m", "commit msg"]);
            git(&dir, &["tag", "v1.2.3"]);
            git(&dir, &["checkout", "v1.2.3"]);
            let path = dir.to_str().expect("a path");
            assert_eq!(find_git_ref(path).expect("a ref"), "refs/tags/v1.2.3");
        }
        // A tag on the current commit wins even though the branch is also there.
        {
            let (_temp, dir) = repo_with("current_head_is_same_as_tag");
            git(&dir, &["commit", "--allow-empty", "-m", "1.4.2 release"]);
            git(&dir, &["tag", "v1.4.2"]);
            let path = dir.to_str().expect("a path");
            assert_eq!(find_git_ref(path).expect("a ref"), "refs/tags/v1.4.2");
        }
        // A tag on an *older* commit must not win: HEAD moved past it.
        {
            let (_temp, dir) = repo_with("current_head_is_not_tag");
            git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
            git(&dir, &["tag", "v1.4.2"]);
            git(&dir, &["commit", "--allow-empty", "-m", "msg2"]);
            let path = dir.to_str().expect("a path");
            assert_eq!(find_git_ref(path).expect("a ref"), "refs/heads/master");
        }
        // A different branch.
        {
            let (_temp, dir) = repo_with("current_head_is_another_branch");
            git(&dir, &["checkout", "-b", "mybranch"]);
            git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
            let path = dir.to_str().expect("a path");
            assert_eq!(find_git_ref(path).expect("a ref"), "refs/heads/mybranch");
        }
    }

    /// The revision comes back short and full, and the short one is a prefix of
    /// the long one.
    #[test]
    fn the_revision_is_reported_short_and_full() {
        let (_temp, dir) = repo_with("revision");
        git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
        let (short, full) = find_git_revision(dir.to_str().expect("a path")).expect("a revision");
        assert_eq!(short.len(), 7, "{short}");
        assert!(full.starts_with(&short), "{full} starts with {short}");
        assert_eq!(full.len(), 40, "a SHA-1 is 40 hex characters: {full}");
    }

    /// The remote's URL is what the slug is read from, and an empty remote name
    /// means `origin`.
    #[test]
    fn the_repository_slug_comes_from_the_remote() {
        let (_temp, dir) = repo_with("remote");
        git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
        git(
            &dir,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/nektos/act.git",
            ],
        );
        let path = dir.to_str().expect("a path");

        assert_eq!(
            find_github_repo(path, "github.com", "").expect("a slug"),
            "nektos/act",
            "an empty remote name means origin"
        );
        assert_eq!(
            find_github_repo(path, "github.com", "origin").expect("a slug"),
            "nektos/act"
        );
        assert_eq!(
            find_git_remote_url(path, "origin").expect("a url"),
            "https://github.com/nektos/act.git"
        );
        // A remote that does not exist is an error, not an empty slug.
        assert!(find_github_repo(path, "github.com", "nope").is_err());
    }

    /// An SSH remote resolves the same way an HTTPS one does.
    #[test]
    fn an_ssh_remote_gives_the_same_slug() {
        let (_temp, dir) = repo_with("ssh_remote");
        git(&dir, &["commit", "--allow-empty", "-m", "msg"]);
        git(
            &dir,
            &["remote", "add", "origin", "git@github.com:nektos/act.git"],
        );
        assert_eq!(
            find_github_repo(dir.to_str().expect("a path"), "github.com", "origin")
                .expect("a slug"),
            "nektos/act"
        );
    }

    /// Upstream distinguishes this case, and so does the error text: a short sha
    /// is not something a workflow may ask to be checked out.
    #[test]
    fn the_short_ref_rejection_message_is_what_upstream_uses() {
        assert_eq!(
            ShortRefError.to_string(),
            "short SHA references are not supported"
        );
        assert_eq!(NoRepoError.to_string(), "unable to find git repo");
    }
}
