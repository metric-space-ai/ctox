use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use crossbeam_channel::after;
use crossbeam_channel::never;
use crossbeam_channel::select;
use crossbeam_channel::unbounded;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use nucleo::Config;
use nucleo::Injector;
use nucleo::Matcher;
use nucleo::Nucleo;
use nucleo::Utf32String;
use nucleo::pattern::CaseMatching;
use nucleo::pattern::Normalization;
use serde::Serialize;
use std::num::NonZero;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tokio::process::Command;

mod cli;

pub use cli::Cli;

/// A single match result returned from the search.
///
/// * `score` – Relevance score returned by `nucleo`.
/// * `path`  – Path to the matched file (relative to the search directory).
/// * `indices` – Optional list of character indices that matched the query.
///   These are only filled when the caller of [`run`] sets
///   `options.compute_indices` to `true`. The indices vector follows the
///   guidance from `nucleo::pattern::Pattern::indices`: they are
///   unique and sorted in ascending order so that callers can use
///   them directly for highlighting.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileMatch {
    pub score: u32,
    pub path: PathBuf,
    pub root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices: Option<Vec<u32>>, // Sorted & deduplicated when present
}

impl FileMatch {
    pub fn full_path(&self) -> PathBuf {
        self.root.join(&self.path)
    }
}

/// Returns the final path component for a matched path, falling back to the full path.
pub fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[derive(Debug)]
pub struct FileSearchResults {
    pub matches: Vec<FileMatch>,
    pub total_match_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct FileSearchSnapshot {
    pub query: String,
    pub matches: Vec<FileMatch>,
    pub total_match_count: usize,
    pub scanned_file_count: usize,
    pub walk_complete: bool,
}

#[derive(Debug, Clone)]
pub struct FileSearchOptions {
    pub limit: NonZero<usize>,
    pub exclude: Vec<String>,
    pub threads: NonZero<usize>,
    pub compute_indices: bool,
    /// Toggle ignore-file processing in the walker.
    ///
    /// When enabled, `.gitignore` files are scoped by
    /// `WalkBuilder::require_git(true)`, so they are honored only when the
    /// traversed path is inside a git repository. When disabled, the walker
    /// turns off `.gitignore`, git-global/exclude rules, `.ignore`, and
    /// parent-directory ignore scanning.
    pub respect_gitignore: bool,
}

impl Default for FileSearchOptions {
    fn default() -> Self {
        Self {
            #[expect(clippy::unwrap_used)]
            limit: NonZero::new(20).unwrap(),
            exclude: Vec::new(),
            #[expect(clippy::unwrap_used)]
            threads: NonZero::new(2).unwrap(),
            compute_indices: false,
            respect_gitignore: true,
        }
    }
}

pub trait SessionReporter: Send + Sync + 'static {
    /// Called when the debounced top-N changes.
    fn on_update(&self, snapshot: &FileSearchSnapshot);

    /// Called when the session becomes idle or is cancelled. Guaranteed to be called at least once per update_query.
    fn on_complete(&self);
}

pub struct FileSearchSession {
    inner: Arc<SessionInner>,
}

impl FileSearchSession {
    /// Update the query. This should be cheap relative to re-walking.
    pub fn update_query(&self, pattern_text: &str) {
        let _ = self
            .inner
            .work_tx
            .send(WorkSignal::QueryUpdated(pattern_text.to_string()));
    }
}

impl Drop for FileSearchSession {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
        let _ = self.inner.work_tx.send(WorkSignal::Shutdown);
    }
}

pub fn create_session(
    search_directories: Vec<PathBuf>,
    options: FileSearchOptions,
    reporter: Arc<dyn SessionReporter>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> anyhow::Result<FileSearchSession> {
    let FileSearchOptions {
        limit,
        exclude,
        threads,
        compute_indices,
        respect_gitignore,
    } = options;

    let Some(primary_search_directory) = search_directories.first() else {
        anyhow::bail!("at least one search directory is required");
    };
    let override_matcher = build_override_matcher(primary_search_directory, &exclude)?;
    let (work_tx, work_rx) = unbounded();

    let notify_tx = work_tx.clone();
    let notify = Arc::new(move || {
        let _ = notify_tx.send(WorkSignal::NucleoNotify);
    });
    let nucleo = Nucleo::new(
        Config::DEFAULT.match_paths(),
        notify,
        Some(threads.get()),
        1,
    );
    let injector = nucleo.injector();

    let cancelled = cancel_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

    let inner = Arc::new(SessionInner {
        search_directories,
        limit: limit.get(),
        threads: threads.get(),
        compute_indices,
        respect_gitignore,
        cancelled: cancelled.clone(),
        shutdown: Arc::new(AtomicBool::new(false)),
        reporter,
        work_tx: work_tx.clone(),
    });

    let matcher_inner = inner.clone();
    thread::spawn(move || matcher_worker(matcher_inner, work_rx, nucleo));

    let walker_inner = inner.clone();
    thread::spawn(move || walker_worker(walker_inner, override_matcher, injector));

    Ok(FileSearchSession { inner })
}

pub trait Reporter {
    fn report_match(&self, file_match: &FileMatch);
    fn warn_matches_truncated(&self, total_match_count: usize, shown_match_count: usize);
    fn warn_no_search_pattern(&self, search_directory: &Path);
}

pub async fn run_main<T: Reporter>(
    Cli {
        pattern,
        limit,
        cwd,
        compute_indices,
        json: _,
        exclude,
        threads,
    }: Cli,
    reporter: T,
) -> anyhow::Result<()> {
    let search_directory = match cwd {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let pattern_text = match pattern {
        Some(pattern) => pattern,
        None => {
            reporter.warn_no_search_pattern(&search_directory);
            #[cfg(unix)]
            Command::new("ls")
                .arg("-al")
                .current_dir(search_directory)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await?;
            #[cfg(windows)]
            {
                Command::new("cmd")
                    .arg("/c")
                    .arg(search_directory)
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
            }
            return Ok(());
        }
    };

    let FileSearchResults {
        total_match_count,
        matches,
    } = run(
        &pattern_text,
        vec![search_directory.to_path_buf()],
        FileSearchOptions {
            limit,
            exclude,
            threads,
            compute_indices,
            respect_gitignore: true,
        },
        /*cancel_flag*/ None,
    )?;
    let match_count = matches.len();
    let matches_truncated = total_match_count > match_count;

    for file_match in matches {
        reporter.report_match(&file_match);
    }
    if matches_truncated {
        reporter.warn_matches_truncated(total_match_count, match_count);
    }

    Ok(())
}

/// The worker threads will periodically check `cancel_flag` to see if they
/// should stop processing files.
pub fn run(
    pattern_text: &str,
    roots: Vec<PathBuf>,
    options: FileSearchOptions,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> anyhow::Result<FileSearchResults> {
    let reporter = Arc::new(RunReporter::default());
    let session = create_session(roots, options, reporter.clone(), cancel_flag)?;

    session.update_query(pattern_text);

    let snapshot = reporter.wait_for_complete();
    Ok(FileSearchResults {
        matches: snapshot.matches,
        total_match_count: snapshot.total_match_count,
    })
}

/// Returns a comparator closure suitable for `slice.sort_by(...)` that orders
/// items by descending score and then ascending path using the provided accessors.
pub fn cmp_by_score_desc_then_path_asc<T, FScore, FPath>(
    score_of: FScore,
    path_of: FPath,
) -> impl FnMut(&T, &T) -> std::cmp::Ordering
where
    FScore: Fn(&T) -> u32,
    FPath: Fn(&T) -> &str,
{
    use std::cmp::Ordering;
    move |a, b| match score_of(b).cmp(&score_of(a)) {
        Ordering::Equal => path_of(a).cmp(path_of(b)),
        other => other,
    }
}

struct SessionInner {
    search_directories: Vec<PathBuf>,
    limit: usize,
    threads: usize,
    compute_indices: bool,
    respect_gitignore: bool,
    cancelled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    reporter: Arc<dyn SessionReporter>,
    work_tx: Sender<WorkSignal>,
}

enum WorkSignal {
    QueryUpdated(String),
    NucleoNotify,
    WalkComplete,
    Shutdown,
}

fn build_override_matcher(
    search_directory: &Path,
    exclude: &[String],
) -> anyhow::Result<Option<ignore::overrides::Override>> {
    if exclude.is_empty() {
        return Ok(None);
    }
    let mut override_builder = OverrideBuilder::new(search_directory);
    for exclude in exclude {
        let exclude_pattern = format!("!{exclude}");
        override_builder.add(&exclude_pattern)?;
    }
    let matcher = override_builder.build()?;
    Ok(Some(matcher))
}

fn get_file_path<'a>(path: &'a Path, search_directories: &[PathBuf]) -> Option<(usize, &'a str)> {
    let mut best_match: Option<(usize, &Path)> = None;
    for (idx, root) in search_directories.iter().enumerate() {
        if let Ok(rel_path) = path.strip_prefix(root) {
            let root_depth = root.components().count();
            match best_match {
                Some((best_idx, _))
                    if search_directories[best_idx].components().count() >= root_depth => {}
                _ => {
                    best_match = Some((idx, rel_path));
                }
            }
        }
    }

    let (root_idx, rel_path) = best_match?;
    rel_path.to_str().map(|p| (root_idx, p))
}

/// Walks the search directories and feeds discovered file paths into `nucleo`
/// via the injector.
///
/// The walker uses `require_git(true)` to match git's own ignore semantics:
/// git never reads `.gitignore` files from directories above the repository
/// root. Without this flag, the `ignore` crate reads `.gitignore` files from
/// *all* ancestor directories—a deliberate divergence from git intended for
/// non-git use cases—allowing a broad parent ignore (e.g. `~/.gitignore`
/// containing `*`) to silently suppress every file in the walk.
///
/// When `respect_gitignore` is `false`, all git-related ignore processing is
/// disabled regardless of this flag.
fn walker_worker(
    inner: Arc<SessionInner>,
    override_matcher: Option<ignore::overrides::Override>,
    injector: Injector<Arc<str>>,
) {
    let Some(first_root) = inner.search_directories.first() else {
        let _ = inner.work_tx.send(WorkSignal::WalkComplete);
        return;
    };

    let mut walk_builder = WalkBuilder::new(first_root);
    for root in inner.search_directories.iter().skip(1) {
        walk_builder.add(root);
    }
    walk_builder
        .threads(inner.threads)
        // Allow hidden entries.
        .hidden(false)
        // Follow symlinks to search their contents.
        .follow_links(true)
        // Keep ignore behavior aligned with git repositories: only apply
        // gitignore rules when a git context exists.
        .require_git(true);
    if !inner.respect_gitignore {
        walk_builder
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false);
    }
    if let Some(override_matcher) = override_matcher {
        walk_builder.overrides(override_matcher);
    }

    let walker = walk_builder.build_parallel();

    walker.run(|| {
        const CHECK_INTERVAL: usize = 1024;
        let mut n = 0;
        let search_directories = inner.search_directories.clone();
        let injector = injector.clone();
        let cancelled = inner.cancelled.clone();
        let shutdown = inner.shutdown.clone();

        Box::new(move |entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => return ignore::WalkState::Continue,
            };
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                return ignore::WalkState::Continue;
            }
            let path = entry.path();
            let Some(full_path) = path.to_str() else {
                return ignore::WalkState::Continue;
            };
            if let Some((_, relative_path)) = get_file_path(path, &search_directories) {
                injector.push(Arc::from(full_path), |_, cols| {
                    cols[0] = Utf32String::from(relative_path);
                });
            }
            n += 1;
            if n >= CHECK_INTERVAL {
                if cancelled.load(Ordering::Relaxed) || shutdown.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                n = 0;
            }
            ignore::WalkState::Continue
        })
    });
    let _ = inner.work_tx.send(WorkSignal::WalkComplete);
}

fn matcher_worker(
    inner: Arc<SessionInner>,
    work_rx: Receiver<WorkSignal>,
    mut nucleo: Nucleo<Arc<str>>,
) -> anyhow::Result<()> {
    const TICK_TIMEOUT_MS: u64 = 10;
    let config = Config::DEFAULT.match_paths();
    let mut indices_matcher = inner.compute_indices.then(|| Matcher::new(config.clone()));
    let cancel_requested = || inner.cancelled.load(Ordering::Relaxed);
    let shutdown_requested = || inner.shutdown.load(Ordering::Relaxed);

    let mut last_query = String::new();
    let mut next_notify = never();
    let mut will_notify = false;
    let mut walk_complete = false;

    loop {
        select! {
            recv(work_rx) -> signal => {
                let Ok(signal) = signal else {
                    break;
                };
                match signal {
                    WorkSignal::QueryUpdated(query) => {
                        let append = query.starts_with(&last_query);
                        nucleo.pattern.reparse(
                            0,
                            &query,
                            CaseMatching::Smart,
                            Normalization::Smart,
                            append,
                        );
                        last_query = query;
                        will_notify = true;
                        next_notify = after(Duration::from_millis(0));
                    }
                    WorkSignal::NucleoNotify => {
                        if !will_notify {
                            will_notify = true;
                            next_notify = after(Duration::from_millis(TICK_TIMEOUT_MS));
                        }
                    }
                    WorkSignal::WalkComplete => {
                        walk_complete = true;
                        if !will_notify {
                            will_notify = true;
                            next_notify = after(Duration::from_millis(0));
                        }
                    }
                    WorkSignal::Shutdown => {
                        break;
                    }
                }
            }
            recv(next_notify) -> _ => {
                will_notify = false;
                let status = nucleo.tick(TICK_TIMEOUT_MS);
                if status.changed {
                    let snapshot = nucleo.snapshot();
                    let limit = inner.limit.min(snapshot.matched_item_count() as usize);
                    let pattern = snapshot.pattern().column_pattern(0);
                    let matches: Vec<_> = snapshot
                        .matches()
                        .iter()
                        .take(limit)
                        .filter_map(|match_| {
                            let item = snapshot.get_item(match_.idx)?;
                            let full_path = item.data.as_ref();
                            let (root_idx, relative_path) = get_file_path(Path::new(full_path), &inner.search_directories)?;
                            let indices = if let Some(indices_matcher) = indices_matcher.as_mut() {
                                let mut idx_vec = Vec::<u32>::new();
                                let haystack = item.matcher_columns[0].slice(..);
                                let _ = pattern.indices(haystack, indices_matcher, &mut idx_vec);
                                idx_vec.sort_unstable();
                                idx_vec.dedup();
                                Some(idx_vec)
                            } else {
                                None
                            };
                            Some(FileMatch {
                                score: match_.score,
                                path: PathBuf::from(relative_path),
                                root: inner.search_directories[root_idx].clone(),
                                indices,
                            })
                        })
                        .collect();

                    let snapshot = FileSearchSnapshot {
                        query: last_query.clone(),
                        matches,
                        total_match_count: snapshot.matched_item_count() as usize,
                        scanned_file_count: snapshot.item_count() as usize,
                        walk_complete,
                    };
                    inner.reporter.on_update(&snapshot);
                }
                if !status.running && walk_complete {
                    inner.reporter.on_complete();
                }
            }
            default(Duration::from_millis(100)) => {
                // Occasionally check the cancel flag.
            }
        }

        if cancel_requested() || shutdown_requested() {
            break;
        }
    }

    // If we cancelled or otherwise exited the loop, make sure the reporter is notified.
    inner.reporter.on_complete();

    Ok(())
}

#[derive(Default)]
struct RunReporter {
    snapshot: RwLock<FileSearchSnapshot>,
    completed: (Condvar, Mutex<bool>),
}

impl SessionReporter for RunReporter {
    fn on_update(&self, snapshot: &FileSearchSnapshot) {
        #[expect(clippy::unwrap_used)]
        let mut guard = self.snapshot.write().unwrap();
        *guard = snapshot.clone();
    }

    fn on_complete(&self) {
        let (cv, mutex) = &self.completed;
        let mut completed = mutex.lock().unwrap();
        *completed = true;
        cv.notify_all();
    }
}

impl RunReporter {
    fn wait_for_complete(&self) -> FileSearchSnapshot {
        let (cv, mutex) = &self.completed;
        let mut completed = mutex.lock().unwrap();
        while !*completed {
            completed = cv.wait(completed).unwrap();
        }
        self.snapshot.read().unwrap().clone()
    }
}
